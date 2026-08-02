use std::collections::HashMap;
use crate::Scottatura;
use crate::stove::{page::Page, pot_file::PotFile};

#[derive(Clone, Copy, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
}

struct CachedPage {
    page: Page,
    dirty: bool,
    simmer_id: Option<u64>,
    lru_tick: u64,
}

pub struct WarmingTray {
    capacity: usize,
    cache: HashMap<u64, CachedPage>,
    tick: u64,
    stats: CacheStats,
}

impl WarmingTray {
    pub fn new(capacity_pages: usize) -> Self {
        Self { capacity: capacity_pages, cache: HashMap::new(), tick: 0, stats: CacheStats::default() }
    }

    pub fn fetch(&mut self, page_num: u64, file: &mut PotFile) -> Result<&mut Page, Scottatura> {
        self.tick += 1;
        let tick = self.tick;
        if !self.cache.contains_key(&page_num) {
            self.stats.misses += 1;
            if self.cache.len() >= self.capacity {
                self.evict_one_clean();
            }
            let page = file.read_page(page_num)?;
            self.cache.insert(page_num, CachedPage { page, dirty: false, simmer_id: None, lru_tick: tick });
        } else {
            self.stats.hits += 1;
        }
        let entry = self.cache.get_mut(&page_num).unwrap();
        entry.lru_tick = tick;
        Ok(&mut entry.page)
    }

    pub fn mark_dirty(&mut self, page_num: u64, simmer_id: u64) {
        if let Some(entry) = self.cache.get_mut(&page_num) {
            entry.dirty = true;
            entry.simmer_id = Some(simmer_id);
        }
    }

    pub fn flush_simmer(&mut self, simmer_id: u64, file: &mut PotFile) -> Result<(), Scottatura> {
        for entry in self.cache.values_mut() {
            if entry.simmer_id == Some(simmer_id) && entry.dirty {
                file.write_page(&entry.page)?;
                entry.dirty = false;
                entry.simmer_id = None;
            }
        }
        Ok(())
    }

    pub fn flush_all(&mut self, file: &mut PotFile) -> Result<(), Scottatura> {
        for entry in self.cache.values_mut() {
            if entry.dirty {
                file.write_page(&entry.page)?;
                entry.dirty = false;
                entry.simmer_id = None;
            }
        }
        Ok(())
    }

    pub fn evict_simmer(&mut self, simmer_id: u64) {
        self.cache.retain(|_, e| e.simmer_id != Some(simmer_id));
    }

    pub fn invalidate(&mut self, page_num: u64) {
        self.cache.remove(&page_num);
    }

    pub fn stats(&self) -> CacheStats {
        self.stats
    }

    fn evict_one_clean(&mut self) {
        let to_evict = self.cache.iter()
            .filter(|(_, e)| !e.dirty)
            .min_by_key(|(_, e)| e.lru_tick)
            .map(|(k, _)| *k);
        if let Some(k) = to_evict {
            self.cache.remove(&k);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use crate::stove::{page::PageType, pot_file::PotFile};

    fn make_pot_with_page() -> (PotFile, String) {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_string();
        drop(f);
        let mut pot = PotFile::create(&path).unwrap();
        pot.allocate_page(PageType::Heap).unwrap(); // page 1
        (pot, path)
    }

    #[test]
    fn test_miss_then_hit() {
        let (mut pot, path) = make_pot_with_page();
        let mut tray = WarmingTray::new(16);
        tray.fetch(1, &mut pot).unwrap();
        assert_eq!(tray.stats().misses, 1);
        assert_eq!(tray.stats().hits, 0);
        tray.fetch(1, &mut pot).unwrap();
        assert_eq!(tray.stats().hits, 1);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_mark_dirty_and_flush_writes_to_disk() {
        let (mut pot, path) = make_pot_with_page();
        let mut tray = WarmingTray::new(16);
        let page = tray.fetch(1, &mut pot).unwrap();
        page.set_next_page(77);
        tray.mark_dirty(1, 42);
        tray.flush_simmer(42, &mut pot).unwrap();
        let on_disk = pot.read_page(1).unwrap();
        assert_eq!(on_disk.next_page(), 77);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_evict_simmer_discards_without_flush() {
        let (mut pot, path) = make_pot_with_page();
        let mut tray = WarmingTray::new(16);
        let page = tray.fetch(1, &mut pot).unwrap();
        page.set_next_page(99);
        tray.mark_dirty(1, 55);
        tray.evict_simmer(55);
        // Re-fetch from disk (was never flushed)
        let page = tray.fetch(1, &mut pot).unwrap();
        assert_eq!(page.next_page(), 0);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_invalidate_drops_page_without_flush() {
        let (mut pot, path) = make_pot_with_page();
        let mut tray = WarmingTray::new(16);
        let page = tray.fetch(1, &mut pot).unwrap();
        page.set_next_page(99);
        tray.invalidate(1);
        // Re-fetch from disk — the mutation was never flushed, so it's gone.
        let page = tray.fetch(1, &mut pot).unwrap();
        assert_eq!(page.next_page(), 0);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_flush_all_flushes_multiple_simmers() {
        let (mut pot, path) = make_pot_with_page(); // page 1
        pot.allocate_page(PageType::Heap).unwrap(); // page 2
        let mut tray = WarmingTray::new(16);
        tray.fetch(1, &mut pot).unwrap().set_next_page(11);
        tray.mark_dirty(1, 1);
        tray.fetch(2, &mut pot).unwrap().set_next_page(22);
        tray.mark_dirty(2, 2);

        tray.flush_all(&mut pot).unwrap();

        assert_eq!(pot.read_page(1).unwrap().next_page(), 11);
        assert_eq!(pot.read_page(2).unwrap().next_page(), 22);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_lru_evicts_clean_pages_at_capacity() {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_string();
        drop(f);
        let mut pot = PotFile::create(&path).unwrap();
        for _ in 0..5 {
            pot.allocate_page(PageType::Heap).unwrap();
        }
        let mut tray = WarmingTray::new(3); // capacity of 3
        tray.fetch(1, &mut pot).unwrap();
        tray.fetch(2, &mut pot).unwrap();
        tray.fetch(3, &mut pot).unwrap();
        // Page 1 is LRU; fetching page 4 should evict it
        tray.fetch(4, &mut pot).unwrap();
        assert_eq!(tray.stats().misses, 4);
        // Page 1 is now a miss again
        tray.fetch(1, &mut pot).unwrap();
        assert_eq!(tray.stats().misses, 5);
        std::fs::remove_file(&path).ok();
    }
}
