pub const PAGE_SIZE: usize = 4096;
pub const PAGE_HEADER_SIZE: usize = 16;
pub const SLOT_SIZE: usize = 8;
pub const TOMBSTONE: u32 = 0xFFFF_FFFF;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageType {
    Heap = 0x01,
    BtreeInternal = 0x02,
    BtreeLeaf = 0x03,
    Overflow = 0x04,
    Free = 0x05,
}

impl PageType {
    pub fn from_u8(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(Self::Heap),
            0x02 => Some(Self::BtreeInternal),
            0x03 => Some(Self::BtreeLeaf),
            0x04 => Some(Self::Overflow),
            0x05 => Some(Self::Free),
            _ => None,
        }
    }
}

pub struct Page {
    pub number: u64,
    pub data: [u8; PAGE_SIZE],
}

impl Page {
    pub fn new(number: u64, page_type: PageType) -> Self {
        let mut data = [0u8; PAGE_SIZE];
        data[0] = page_type as u8;
        data[2..4].copy_from_slice(&(PAGE_HEADER_SIZE as u16).to_le_bytes());
        Self { number, data }
    }

    pub fn page_type(&self) -> PageType {
        PageType::from_u8(self.data[0]).unwrap_or(PageType::Heap)
    }

    pub fn free_space_offset(&self) -> u16 {
        u16::from_le_bytes(self.data[2..4].try_into().unwrap())
    }

    pub fn set_free_space_offset(&mut self, offset: u16) {
        self.data[2..4].copy_from_slice(&offset.to_le_bytes());
    }

    pub fn slot_count(&self) -> u32 {
        u32::from_le_bytes(self.data[4..8].try_into().unwrap())
    }

    pub fn set_slot_count(&mut self, count: u32) {
        self.data[4..8].copy_from_slice(&count.to_le_bytes());
    }

    pub fn next_page(&self) -> u64 {
        u64::from_le_bytes(self.data[8..16].try_into().unwrap())
    }

    pub fn set_next_page(&mut self, page: u64) {
        self.data[8..16].copy_from_slice(&page.to_le_bytes());
    }

    pub fn slot(&self, idx: u32) -> (u32, u32) {
        let start = PAGE_HEADER_SIZE + (idx as usize) * SLOT_SIZE;
        let offset = u32::from_le_bytes(self.data[start..start + 4].try_into().unwrap());
        let length = u32::from_le_bytes(self.data[start + 4..start + 8].try_into().unwrap());
        (offset, length)
    }

    pub fn set_slot(&mut self, idx: u32, offset: u32, length: u32) {
        let start = PAGE_HEADER_SIZE + (idx as usize) * SLOT_SIZE;
        self.data[start..start + 4].copy_from_slice(&offset.to_le_bytes());
        self.data[start + 4..start + 8].copy_from_slice(&length.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_heap_page_header() {
        let page = Page::new(1, PageType::Heap);
        assert_eq!(page.page_type(), PageType::Heap);
        assert_eq!(page.free_space_offset(), PAGE_HEADER_SIZE as u16);
        assert_eq!(page.slot_count(), 0);
        assert_eq!(page.next_page(), 0);
    }

    #[test]
    fn test_set_and_read_slot() {
        let mut page = Page::new(1, PageType::Heap);
        page.set_slot(0, 4000, 80);
        let (off, len) = page.slot(0);
        assert_eq!(off, 4000);
        assert_eq!(len, 80);
    }

    #[test]
    fn test_tombstone_constant() {
        let mut page = Page::new(1, PageType::Heap);
        page.set_slot(0, 100, TOMBSTONE);
        let (_, len) = page.slot(0);
        assert_eq!(len, TOMBSTONE);
    }

    #[test]
    fn test_free_space_offset_roundtrip() {
        let mut page = Page::new(1, PageType::Heap);
        page.set_free_space_offset(256);
        assert_eq!(page.free_space_offset(), 256);
    }

    #[test]
    fn test_next_page_roundtrip() {
        let mut page = Page::new(5, PageType::Heap);
        page.set_next_page(42);
        assert_eq!(page.next_page(), 42);
    }

    #[test]
    fn test_slot_count_roundtrip() {
        let mut page = Page::new(1, PageType::Heap);
        page.set_slot_count(17);
        assert_eq!(page.slot_count(), 17);
    }

    #[test]
    fn test_multiple_slots_independent() {
        let mut page = Page::new(1, PageType::Heap);
        page.set_slot(0, 4000, 50);
        page.set_slot(1, 3940, 60);
        assert_eq!(page.slot(0), (4000, 50));
        assert_eq!(page.slot(1), (3940, 60));
    }
}
