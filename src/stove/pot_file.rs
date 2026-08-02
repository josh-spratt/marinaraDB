use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use crate::Scottatura;
use crate::stove::page::{Page, PageType, PAGE_SIZE};

const MAGIC: u32 = 0x4D52_4442;
const VERSION: u32 = 1;

pub struct PotFile {
    file: File,
    page_count: u64,
    root_catalog_page: u64,
}

impl PotFile {
    pub fn create(path: &str) -> Result<Self, Scottatura> {
        let mut file = OpenOptions::new()
            .read(true).write(true).create_new(true)
            .open(path)?;

        let mut header = [0u8; PAGE_SIZE];
        header[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        header[4..8].copy_from_slice(&VERSION.to_le_bytes());
        header[8..12].copy_from_slice(&(PAGE_SIZE as u32).to_le_bytes());
        header[12..16].copy_from_slice(&1u32.to_le_bytes()); // page_count = 1

        let crc = crc32fast::hash(&header[0..32]);
        header[32..36].copy_from_slice(&crc.to_le_bytes());

        file.write_all(&header)?;
        file.flush()?;
        Ok(Self { file, page_count: 1, root_catalog_page: 0 })
    }

    pub fn open(path: &str) -> Result<Self, Scottatura> {
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;

        // Check magic first; a short file or wrong bytes both mean NotAPotFile
        let mut magic_buf = [0u8; 4];
        if file.read_exact(&mut magic_buf).is_err() {
            return Err(Scottatura::NotAPotFile { path: path.to_string() });
        }
        let magic = u32::from_le_bytes(magic_buf);
        if magic != MAGIC {
            return Err(Scottatura::NotAPotFile { path: path.to_string() });
        }

        // Seek back and read the full header page
        file.seek(SeekFrom::Start(0))?;
        let mut header = [0u8; PAGE_SIZE];
        file.read_exact(&mut header)?;

        let page_count = u32::from_le_bytes(header[12..16].try_into().unwrap()) as u64;
        let root_catalog_page = u64::from_le_bytes(header[16..24].try_into().unwrap());
        Ok(Self { file, page_count, root_catalog_page })
    }

    pub fn page_count(&self) -> u64 { self.page_count }

    pub fn root_catalog_page(&self) -> u64 { self.root_catalog_page }

    pub fn set_root_catalog_page(&mut self, page_num: u64) -> Result<(), Scottatura> {
        self.root_catalog_page = page_num;
        self.flush_header()
    }

    pub fn ensure_page_count(&mut self, min_count: u64) -> Result<(), Scottatura> {
        if min_count > self.page_count {
            self.page_count = min_count;
            self.flush_header()?;
        }
        Ok(())
    }

    pub fn read_page(&mut self, page_num: u64) -> Result<Page, Scottatura> {
        self.file.seek(SeekFrom::Start(page_num * PAGE_SIZE as u64))?;
        let mut data = [0u8; PAGE_SIZE];
        self.file.read_exact(&mut data)?;
        Ok(Page { number: page_num, data })
    }

    pub fn write_page(&mut self, page: &Page) -> Result<(), Scottatura> {
        self.file.seek(SeekFrom::Start(page.number * PAGE_SIZE as u64))?;
        self.file.write_all(&page.data)?;
        Ok(())
    }

    pub fn allocate_page(&mut self, page_type: PageType) -> Result<Page, Scottatura> {
        let page_num = self.page_count;
        self.page_count += 1;
        self.flush_header()?;
        let page = Page::new(page_num, page_type);
        self.write_page(&page)?;
        Ok(page)
    }

    pub fn sync_all(&self) -> Result<(), Scottatura> {
        self.file.sync_all()?;
        Ok(())
    }

    fn flush_header(&mut self) -> Result<(), Scottatura> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut header = [0u8; PAGE_SIZE];
        self.file.read_exact(&mut header)?;

        header[12..16].copy_from_slice(&(self.page_count as u32).to_le_bytes());
        header[16..24].copy_from_slice(&self.root_catalog_page.to_le_bytes());

        let crc = crc32fast::hash(&header[0..32]);
        header[32..36].copy_from_slice(&crc.to_le_bytes());

        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&header)?;
        self.file.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn fresh_path() -> String {
        let f = NamedTempFile::new().unwrap();
        let p = f.path().to_str().unwrap().to_string();
        drop(f);
        p
    }

    #[test]
    fn test_create_and_reopen() {
        let path = fresh_path();
        {
            let pot = PotFile::create(&path).unwrap();
            assert_eq!(pot.page_count(), 1);
        }
        let pot2 = PotFile::open(&path).unwrap();
        assert_eq!(pot2.page_count(), 1);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_open_bad_magic_errors() {
        let f = NamedTempFile::new().unwrap();
        std::fs::write(f.path(), b"not a pot").unwrap();
        let result = PotFile::open(f.path().to_str().unwrap());
        assert!(matches!(result, Err(crate::Scottatura::NotAPotFile { .. })));
    }

    #[test]
    fn test_allocate_and_read_write_page() {
        let path = fresh_path();
        let mut pot = PotFile::create(&path).unwrap();
        let page = pot.allocate_page(PageType::Heap).unwrap();
        assert_eq!(page.number, 1);
        assert_eq!(pot.page_count(), 2);

        let mut p = Page::new(1, PageType::Heap);
        p.set_next_page(77);
        pot.write_page(&p).unwrap();

        let p2 = pot.read_page(1).unwrap();
        assert_eq!(p2.next_page(), 77);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_sync_all_succeeds() {
        let path = fresh_path();
        let pot = PotFile::create(&path).unwrap();
        pot.sync_all().unwrap();
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_root_catalog_page_persists() {
        let path = fresh_path();
        {
            let mut pot = PotFile::create(&path).unwrap();
            pot.set_root_catalog_page(5).unwrap();
        }
        let pot2 = PotFile::open(&path).unwrap();
        assert_eq!(pot2.root_catalog_page(), 5);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_ensure_page_count_raises_and_persists() {
        let path = fresh_path();
        {
            let mut pot = PotFile::create(&path).unwrap();
            assert_eq!(pot.page_count(), 1);
            pot.ensure_page_count(5).unwrap();
            assert_eq!(pot.page_count(), 5);
            pot.ensure_page_count(3).unwrap(); // lower value must not shrink it
            assert_eq!(pot.page_count(), 5);
        }
        let pot2 = PotFile::open(&path).unwrap();
        assert_eq!(pot2.page_count(), 5);
        std::fs::remove_file(&path).ok();
    }
}
