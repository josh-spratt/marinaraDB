use crate::{ColumnDef, Recipe, Scottatura, Value};
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
}

fn encode_and_validate(entry: &NoodleEntry, key: i64, values: &[Value]) -> Result<Vec<u8>, Scottatura> {
    for (val, col) in values.iter().zip(entry.cols.iter()) {
        if matches!(val, Value::Null) && !col.nullable {
            return Err(Scottatura::NullConstraintViolation { col: col.name.clone() });
        }
    }
    encode_row(key, values, &entry.cols)
}

impl Pot {
    pub fn open(path: &str) -> Result<Self, Scottatura> {
        let (pot_file, tray, catalog) = if std::path::Path::new(path).exists() {
            let mut f = PotFile::open(path)?;
            let mut t = WarmingTray::new(256);
            let c = Catalog::load(&mut t, &mut f)?;
            (f, t, c)
        } else {
            let mut f = PotFile::create(path)?;
            let mut t = WarmingTray::new(256);
            let c = Catalog::init(&mut t, &mut f)?;
            (f, t, c)
        };
        Ok(Self { pot_file, tray, catalog, next_key: 1 })
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

    pub fn insert_with_key(&mut self, noodle: &str, key: i64, values: &[Value]) -> Result<(), Scottatura> {
        let entry = self.entry(noodle)?;
        let row_bytes = encode_and_validate(&entry, key, values)?;
        self.heap_chain_insert(entry.root_heap_page, &row_bytes)
    }

    pub fn scan_noodle(&mut self, noodle: &str) -> Result<Vec<(i64, Vec<Value>)>, Scottatura> {
        let entry = self.entry(noodle)?;
        self.heap_chain_scan(entry.root_heap_page, &entry.cols)
    }

    pub fn delete_by_key(&mut self, noodle: &str, key: i64) -> Result<bool, Scottatura> {
        let entry = self.entry(noodle)?;
        self.heap_chain_delete(entry.root_heap_page, key, &entry.cols)
    }

    fn heap_chain_insert(&mut self, root_page: u64, row_bytes: &[u8]) -> Result<(), Scottatura> {
        let mut page_num = root_page;
        loop {
            let next_page_num = {
                let page = self.tray.fetch(page_num, &mut self.pot_file)?;
                if heap_insert(page, row_bytes).is_some() {
                    self.tray.mark_dirty(page_num, 0);
                    self.tray.flush_simmer(0, &mut self.pot_file)?;
                    return Ok(());
                }
                page.next_page()
            };
            if next_page_num != 0 {
                page_num = next_page_num;
                continue;
            }
            let new_page = self.pot_file.allocate_page(PageType::Heap)?;
            let new_page_num = new_page.number;
            let page = self.tray.fetch(page_num, &mut self.pot_file)?;
            page.set_next_page(new_page_num);
            self.tray.mark_dirty(page_num, 0);
            self.tray.flush_simmer(0, &mut self.pot_file)?;
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
            let (deleted, next) = {
                let page = self.tray.fetch(page_num, &mut self.pot_file)?;
                let deleted = heap_delete(page, key, cols);
                (deleted, page.next_page())
            };
            if deleted {
                self.tray.mark_dirty(page_num, 0);
                self.tray.flush_simmer(0, &mut self.pot_file)?;
                return Ok(true);
            }
            if next == 0 { return Ok(false); }
            page_num = next;
        }
    }
}
