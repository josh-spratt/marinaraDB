use serde::{Deserialize, Serialize};
use crate::{ColumnDef, Scottatura};
use crate::stove::page::{Page, PageType, PAGE_HEADER_SIZE, PAGE_SIZE};
use crate::stove::pot_file::PotFile;
use crate::warming_tray::WarmingTray;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoodleEntry {
    pub name: String,
    pub root_heap_page: u64,
    pub root_strainer_page: u64,
    pub cols: Vec<ColumnDef>,
}

pub struct Catalog {
    entries: Vec<NoodleEntry>,
    catalog_page_num: u64,
}

impl Catalog {
    pub fn init(tray: &mut WarmingTray, pot: &mut PotFile) -> Result<Self, Scottatura> {
        let page = pot.allocate_page(PageType::Heap)?;
        let page_num = page.number;
        pot.set_root_catalog_page(page_num)?;
        tray.fetch(page_num, pot)?;
        Ok(Self { entries: Vec::new(), catalog_page_num: page_num })
    }

    pub fn load(tray: &mut WarmingTray, pot: &mut PotFile) -> Result<Self, Scottatura> {
        let page_num = pot.root_catalog_page();
        let page = tray.fetch(page_num, pot)?;
        let entries = Self::deserialize_entries(page);
        Ok(Self { entries, catalog_page_num: page_num })
    }

    pub fn list_noodles(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.name.clone()).collect()
    }

    pub fn get_recipe(&self, name: &str) -> Option<&NoodleEntry> {
        self.entries.iter().find(|e| e.name == name)
    }

    pub fn get_recipe_mut(&mut self, name: &str) -> Option<&mut NoodleEntry> {
        self.entries.iter_mut().find(|e| e.name == name)
    }

    pub fn create_noodle(
        &mut self, name: &str, cols: Vec<ColumnDef>,
        tray: &mut WarmingTray, pot: &mut PotFile,
    ) -> Result<u64, Scottatura> {
        if self.entries.iter().any(|e| e.name == name) {
            return Err(Scottatura::NoodleAlreadyExists { name: name.to_string() });
        }
        let heap_page = pot.allocate_page(PageType::Heap)?;
        self.entries.push(NoodleEntry {
            name: name.to_string(),
            root_heap_page: heap_page.number,
            root_strainer_page: 0,
            cols,
        });
        self.persist(tray, pot)?;
        Ok(heap_page.number)
    }

    pub fn drop_noodle(
        &mut self, name: &str,
        tray: &mut WarmingTray, pot: &mut PotFile,
    ) -> Result<(), Scottatura> {
        let pos = self.entries.iter().position(|e| e.name == name)
            .ok_or_else(|| Scottatura::NoodleNotFound { name: name.to_string() })?;
        self.entries.remove(pos);
        self.persist(tray, pot)
    }

    fn persist(&mut self, tray: &mut WarmingTray, pot: &mut PotFile) -> Result<(), Scottatura> {
        let encoded = bincode::serialize(&self.entries)
            .map_err(|e| Scottatura::IoError(std::io::Error::other(e)))?;
        if encoded.len() > (PAGE_SIZE - PAGE_HEADER_SIZE - 4) {
            return Err(Scottatura::IoError(std::io::Error::other(
                "catalog too large for single page",
            )));
        }
        let page = tray.fetch(self.catalog_page_num, pot)?;
        let len = encoded.len() as u32;
        page.data[PAGE_HEADER_SIZE..PAGE_HEADER_SIZE + 4].copy_from_slice(&len.to_le_bytes());
        page.data[PAGE_HEADER_SIZE + 4..PAGE_HEADER_SIZE + 4 + encoded.len()]
            .copy_from_slice(&encoded);
        tray.mark_dirty(self.catalog_page_num, 0);
        tray.flush_simmer(0, pot)
    }

    fn deserialize_entries(page: &Page) -> Vec<NoodleEntry> {
        let len = u32::from_le_bytes(
            page.data[PAGE_HEADER_SIZE..PAGE_HEADER_SIZE + 4].try_into().unwrap()
        ) as usize;
        if len == 0 { return Vec::new(); }
        let data = &page.data[PAGE_HEADER_SIZE + 4..PAGE_HEADER_SIZE + 4 + len];
        bincode::deserialize(data).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use crate::{ColType, ColumnDef};
    use crate::stove::pot_file::PotFile;
    use crate::warming_tray::WarmingTray;

    fn setup() -> (PotFile, WarmingTray, String) {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_string();
        drop(f);
        (PotFile::create(&path).unwrap(), WarmingTray::new(64), path)
    }

    fn user_cols() -> Vec<ColumnDef> {
        vec![
            ColumnDef { name: "name".into(), col_type: ColType::Text, nullable: true, strainer_root: None },
            ColumnDef { name: "age".into(), col_type: ColType::Integer, nullable: false, strainer_root: None },
        ]
    }

    #[test]
    fn test_create_and_list() {
        let (mut pot, mut tray, path) = setup();
        let mut cat = Catalog::init(&mut tray, &mut pot).unwrap();
        cat.create_noodle("users", user_cols(), &mut tray, &mut pot).unwrap();
        assert_eq!(cat.list_noodles(), vec!["users"]);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_get_recipe_returns_correct_columns() {
        let (mut pot, mut tray, path) = setup();
        let mut cat = Catalog::init(&mut tray, &mut pot).unwrap();
        cat.create_noodle("users", user_cols(), &mut tray, &mut pot).unwrap();
        let entry = cat.get_recipe("users").unwrap();
        assert_eq!(entry.cols[0].name, "name");
        assert_eq!(entry.cols[1].name, "age");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_duplicate_name_errors() {
        let (mut pot, mut tray, path) = setup();
        let mut cat = Catalog::init(&mut tray, &mut pot).unwrap();
        cat.create_noodle("users", user_cols(), &mut tray, &mut pot).unwrap();
        assert!(matches!(
            cat.create_noodle("users", user_cols(), &mut tray, &mut pot),
            Err(crate::Scottatura::NoodleAlreadyExists { .. })
        ));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_drop_removes_entry() {
        let (mut pot, mut tray, path) = setup();
        let mut cat = Catalog::init(&mut tray, &mut pot).unwrap();
        cat.create_noodle("users", user_cols(), &mut tray, &mut pot).unwrap();
        cat.drop_noodle("users", &mut tray, &mut pot).unwrap();
        assert!(cat.list_noodles().is_empty());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_persists_across_reopen() {
        let (mut pot, mut tray, path) = setup();
        {
            let mut cat = Catalog::init(&mut tray, &mut pot).unwrap();
            cat.create_noodle("orders", user_cols(), &mut tray, &mut pot).unwrap();
        }
        drop(pot);
        let mut pot2 = PotFile::open(&path).unwrap();
        let mut tray2 = WarmingTray::new(64);
        let cat2 = Catalog::load(&mut tray2, &mut pot2).unwrap();
        assert_eq!(cat2.list_noodles(), vec!["orders"]);
        std::fs::remove_file(&path).ok();
    }
}
