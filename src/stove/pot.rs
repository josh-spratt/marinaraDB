use crate::{ColumnDef, Recipe, Scottatura, Value};
use crate::prep_log::{recovery, PrepLog};
use crate::stove::catalog::{Catalog, NoodleEntry};
use crate::stove::heap::{encode_row, heap_delete, heap_insert, heap_scan};
use crate::stove::page::PageType;
use crate::stove::pot_file::PotFile;
use crate::warming_tray::WarmingTray;

pub struct Pot {
    pot_file: PotFile,
    tray: WarmingTray,
    catalog: Catalog,
    next_key: i64,
    prep_log: PrepLog,
}

fn encode_and_validate(entry: &NoodleEntry, key: i64, values: &[Value]) -> Result<Vec<u8>, Scottatura> {
    for (val, col) in values.iter().zip(entry.cols.iter()) {
        if matches!(val, Value::Null) && !col.nullable {
            return Err(Scottatura::NullConstraintViolation { col: col.name.clone() });
        }
    }
    encode_row(key, values, &entry.cols)
}

fn prep_log_path(pot_path: &str) -> String {
    match pot_path.strip_suffix(".pot") {
        Some(stripped) => format!("{stripped}.prep"),
        None => format!("{pot_path}.prep"),
    }
}

impl Pot {
    pub fn open(path: &str) -> Result<Self, Scottatura> {
        let pre_existing = std::path::Path::new(path).exists();
        let prep_path = prep_log_path(path);

        // Recovery must run against a cache-free PotFile, before the
        // Catalog (or anything else) populates the WarmingTray — otherwise
        // a replayed page write would be shadowed by a stale cache entry.
        let (mut pot_file, next_lsn) = if pre_existing {
            let mut f = PotFile::open(path)?;
            let next_lsn = recovery::recover(&mut f, &prep_path)?;
            (f, next_lsn)
        } else {
            (PotFile::create(path)?, 1)
        };

        let mut tray = WarmingTray::new(256);
        let catalog = if pre_existing {
            Catalog::load(&mut tray, &mut pot_file)?
        } else {
            Catalog::init(&mut tray, &mut pot_file)?
        };

        let prep_log = PrepLog::init(&prep_path, next_lsn)?;

        Ok(Self { pot_file, tray, catalog, next_key: 1, prep_log })
    }

    pub fn noodles(&self) -> Vec<String> {
        self.catalog.list_noodles()
    }

    pub fn recipe(&self, noodle: &str) -> Result<Recipe, Scottatura> {
        let e = self.catalog.get_recipe(noodle)
            .ok_or_else(|| Scottatura::NoodleNotFound { name: noodle.to_string() })?;
        Ok(Recipe { noodle_name: e.name.clone(), columns: e.cols.clone() })
    }

    pub fn create_table(&mut self, name: &str, cols: Vec<ColumnDef>) -> Result<(), Scottatura> {
        self.catalog.create_noodle(name, cols, &mut self.tray, &mut self.pot_file)?;
        Ok(())
    }

    pub fn drop_table(&mut self, name: &str) -> Result<(), Scottatura> {
        self.catalog.drop_noodle(name, &mut self.tray, &mut self.pot_file)
    }

    fn entry(&self, noodle: &str) -> Result<NoodleEntry, Scottatura> {
        self.catalog.get_recipe(noodle)
            .cloned()
            .ok_or_else(|| Scottatura::NoodleNotFound { name: noodle.to_string() })
    }

    pub fn insert_strand(&mut self, noodle: &str, values: Vec<Value>) -> Result<i64, Scottatura> {
        let entry = self.entry(noodle)?;

        if values.len() != entry.cols.len() {
            return Err(Scottatura::ValueCountMismatch { got: values.len(), expected: entry.cols.len() });
        }

        let key = self.next_key;
        let row_bytes = encode_and_validate(&entry, key, &values)?;
        self.heap_chain_insert(entry.root_heap_page, &row_bytes)?;
        self.next_key += 1;
        Ok(key)
    }

    pub(crate) fn insert_with_key(&mut self, noodle: &str, key: i64, values: &[Value]) -> Result<(), Scottatura> {
        let entry = self.entry(noodle)?;
        let row_bytes = encode_and_validate(&entry, key, values)?;
        self.heap_chain_insert(entry.root_heap_page, &row_bytes)
    }

    pub fn scan_noodle(&mut self, noodle: &str) -> Result<Vec<(i64, Vec<Value>)>, Scottatura> {
        let entry = self.entry(noodle)?;
        self.heap_chain_scan(entry.root_heap_page, &entry.cols)
    }

    pub(crate) fn delete_by_key(&mut self, noodle: &str, key: i64) -> Result<bool, Scottatura> {
        let entry = self.entry(noodle)?;
        self.heap_chain_delete(entry.root_heap_page, key, &entry.cols)
    }

    pub fn execute(&mut self, sauql: &str) -> Result<crate::ResultSet, Scottatura> {
        let stmt = crate::parser::parse_statement(sauql)?;
        let plan = crate::sous_chef::plan(stmt)?;
        crate::stove::executor::execute(plan, self)
    }

    fn heap_chain_insert(&mut self, root_page: u64, row_bytes: &[u8]) -> Result<(), Scottatura> {
        let mut page_num = root_page;
        loop {
            let inserted_image = {
                let page = self.tray.fetch(page_num, &mut self.pot_file)?;
                if heap_insert(page, row_bytes).is_some() {
                    Some(page.data.to_vec())
                } else {
                    None
                }
            };
            if let Some(after_image) = inserted_image {
                self.log_and_flush(page_num, after_image)?;
                return Ok(());
            }

            let next_page_num = {
                let page = self.tray.fetch(page_num, &mut self.pot_file)?;
                page.next_page()
            };
            if next_page_num != 0 {
                page_num = next_page_num;
                continue;
            }

            let new_page = self.pot_file.allocate_page(PageType::Heap)?;
            let new_page_num = new_page.number;
            let linked_image = {
                let page = self.tray.fetch(page_num, &mut self.pot_file)?;
                page.set_next_page(new_page_num);
                page.data.to_vec()
            };
            self.log_and_flush(page_num, linked_image)?;
            page_num = new_page_num;
        }
    }

    fn heap_chain_scan(&mut self, root_page: u64, cols: &[ColumnDef]) -> Result<Vec<(i64, Vec<Value>)>, Scottatura> {
        let mut rows = Vec::new();
        let mut page_num = root_page;
        loop {
            let (page_rows, next) = {
                let page = self.tray.fetch(page_num, &mut self.pot_file)?;
                (heap_scan(page, cols), page.next_page())
            };
            rows.extend(page_rows);
            if next == 0 { break; }
            page_num = next;
        }
        Ok(rows)
    }

    fn heap_chain_delete(&mut self, root_page: u64, key: i64, cols: &[ColumnDef]) -> Result<bool, Scottatura> {
        let mut page_num = root_page;
        loop {
            let (deleted_image, next) = {
                let page = self.tray.fetch(page_num, &mut self.pot_file)?;
                let deleted = heap_delete(page, key, cols);
                let image = if deleted { Some(page.data.to_vec()) } else { None };
                (image, page.next_page())
            };
            if let Some(after_image) = deleted_image {
                self.log_and_flush(page_num, after_image)?;
                return Ok(true);
            }
            if next == 0 { return Ok(false); }
            page_num = next;
        }
    }

    fn log_and_flush(&mut self, page_num: u64, after_image: Vec<u8>) -> Result<(), Scottatura> {
        if let Err(e) = self.prep_log.append_write(0, page_num, 0, &after_image) {
            self.tray.invalidate(page_num);
            return Err(e);
        }
        if let Err(e) = self.prep_log.append_commit(0) {
            self.tray.invalidate(page_num);
            return Err(e);
        }
        self.tray.mark_dirty(page_num, 0);
        self.tray.flush_simmer(0, &mut self.pot_file)?;
        if self.prep_log.should_checkpoint() {
            self.prep_log.checkpoint(&mut self.tray, &mut self.pot_file)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ColType;

    fn temp_path() -> String {
        let f = tempfile::NamedTempFile::new().unwrap();
        let p = f.path().to_str().unwrap().to_string();
        drop(f);
        p
    }

    #[test]
    fn test_insert_beyond_one_page_spans_multiple_pages() {
        let path = temp_path();
        let mut pot = Pot::open(&path).unwrap();
        pot.create_table("nums", vec![
            ColumnDef { name: "n".into(), col_type: ColType::Integer, nullable: false, strainer_root: None },
        ]).unwrap();

        // Each row is ~17 bytes + an 8-byte slot; a 4KB page holds well under 300.
        // 500 rows guarantees at least one page split.
        for i in 0..500 {
            pot.insert_strand("nums", vec![Value::Integer(i)]).unwrap();
        }

        let rows = pot.scan_noodle("nums").unwrap();
        assert_eq!(rows.len(), 500);
        assert_eq!(rows[0].0, 1);
        assert_eq!(rows[499].0, 500);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_delete_and_reinsert_across_chain() {
        let path = temp_path();
        let mut pot = Pot::open(&path).unwrap();
        pot.create_table("nums", vec![
            ColumnDef { name: "n".into(), col_type: ColType::Integer, nullable: false, strainer_root: None },
        ]).unwrap();
        for i in 0..500 {
            pot.insert_strand("nums", vec![Value::Integer(i)]).unwrap();
        }
        // Key 500 (the 500th insert, i.e. last_insert_key of the loop's last call) lives on a later page.
        assert!(pot.delete_by_key("nums", 500).unwrap());
        assert!(!pot.delete_by_key("nums", 500).unwrap()); // already gone

        pot.insert_with_key("nums", 500, &[Value::Integer(999)]).unwrap();
        let rows = pot.scan_noodle("nums").unwrap();
        let reinserted = rows.iter().find(|(k, _)| *k == 500).unwrap();
        assert_eq!(reinserted.1[0], Value::Integer(999));
        std::fs::remove_file(&path).ok();
    }
}
