use crate::{ColumnDef, Recipe, Scottatura, Value};
use crate::stove::catalog::Catalog;
use crate::stove::heap::{encode_row, heap_insert, heap_scan};
use crate::stove::pot_file::PotFile;
use crate::warming_tray::WarmingTray;

pub struct Pot {
    pot_file: PotFile,
    tray: WarmingTray,
    catalog: Catalog,
    next_key: i64,
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

    pub fn insert_strand(&mut self, noodle: &str, values: Vec<Value>) -> Result<i64, Scottatura> {
        let entry = self.catalog.get_recipe(noodle)
            .ok_or_else(|| Scottatura::NoodleNotFound { name: noodle.to_string() })?
            .clone();

        for (val, col) in values.iter().zip(entry.cols.iter()) {
            if matches!(val, Value::Null) && !col.nullable {
                return Err(Scottatura::NullConstraintViolation { col: col.name.clone() });
            }
        }

        let key = self.next_key;
        self.next_key += 1;
        let row_bytes = encode_row(key, &values, &entry.cols)?;

        let page_num = entry.root_heap_page;
        let page = self.tray.fetch(page_num, &mut self.pot_file)?;
        if heap_insert(page, &row_bytes).is_none() {
            return Err(Scottatura::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                "heap page full — multi-page heap implemented in Phase 2",
            )));
        }
        self.tray.mark_dirty(page_num, 0);
        self.tray.flush_simmer(0, &mut self.pot_file)?;
        Ok(key)
    }

    pub fn scan_noodle(&mut self, noodle: &str) -> Result<Vec<(i64, Vec<Value>)>, Scottatura> {
        let entry = self.catalog.get_recipe(noodle)
            .ok_or_else(|| Scottatura::NoodleNotFound { name: noodle.to_string() })?
            .clone();
        let page = self.tray.fetch(entry.root_heap_page, &mut self.pot_file)?;
        Ok(heap_scan(page, &entry.cols))
    }
}
