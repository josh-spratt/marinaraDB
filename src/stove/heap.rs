use crate::{ColType, ColumnDef, Scottatura, Value};
use crate::stove::encoding::{decode_value, encode_value};
use crate::stove::page::{Page, PAGE_SIZE, SLOT_SIZE, TOMBSTONE};

pub fn encode_row(al_dente_key: i64, values: &[Value], cols: &[ColumnDef]) -> Result<Vec<u8>, Scottatura> {
    let mut out = Vec::new();
    out.extend_from_slice(&al_dente_key.to_le_bytes());

    let bitmap_bytes = (cols.len() + 7) / 8;
    let mut bitmap = vec![0u8; bitmap_bytes];
    for (i, val) in values.iter().enumerate() {
        if matches!(val, Value::Null) {
            bitmap[i / 8] |= 1 << (i % 8);
        }
    }
    out.extend_from_slice(&bitmap);

    for (val, col) in values.iter().zip(cols.iter()) {
        out.extend(encode_value(val, &col.col_type)?);
    }
    Ok(out)
}

pub fn decode_row(data: &[u8], cols: &[ColumnDef]) -> (i64, Vec<Value>) {
    let key = i64::from_le_bytes(data[0..8].try_into().unwrap());
    let bitmap_bytes = (cols.len() + 7) / 8;
    let bitmap = &data[8..8 + bitmap_bytes];
    let mut cursor = 8 + bitmap_bytes;

    let mut values = Vec::with_capacity(cols.len());
    for (i, col) in cols.iter().enumerate() {
        let is_null = (bitmap[i / 8] >> (i % 8)) & 1 == 1;
        let val = decode_value(&data[cursor..], &col.col_type, is_null);
        cursor += match col.col_type {
            ColType::Integer | ColType::Real => 8,
            ColType::Text | ColType::Blob => {
                if is_null {
                    4 // just the sentinel
                } else {
                    4 + u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize
                }
            }
        };
        values.push(val);
    }
    (key, values)
}

pub fn heap_insert(page: &mut Page, row_bytes: &[u8]) -> Option<u32> {
    let slot_count = page.slot_count();
    let free_off = page.free_space_offset() as usize;
    let row_end = if slot_count == 0 {
        PAGE_SIZE
    } else {
        page.slot(slot_count - 1).0 as usize
    };
    if free_off + SLOT_SIZE + row_bytes.len() > row_end {
        return None;
    }
    let row_start = row_end - row_bytes.len();
    page.data[row_start..row_end].copy_from_slice(row_bytes);
    page.set_slot(slot_count, row_start as u32, row_bytes.len() as u32);
    page.set_slot_count(slot_count + 1);
    page.set_free_space_offset((free_off + SLOT_SIZE) as u16);
    Some(slot_count)
}

pub fn heap_scan(page: &Page, cols: &[ColumnDef]) -> Vec<(i64, Vec<Value>)> {
    (0..page.slot_count())
        .filter_map(|i| {
            let (off, len) = page.slot(i);
            if len == TOMBSTONE { return None; }
            Some(decode_row(&page.data[off as usize..(off + len) as usize], cols))
        })
        .collect()
}

pub fn heap_delete(page: &mut Page, al_dente_key: i64, cols: &[ColumnDef]) -> bool {
    for i in 0..page.slot_count() {
        let (off, len) = page.slot(i);
        if len == TOMBSTONE { continue; }
        let (key, _) = decode_row(&page.data[off as usize..(off + len) as usize], cols);
        if key == al_dente_key {
            page.set_slot(i, off, TOMBSTONE);
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ColType, ColumnDef, Value};
    use crate::stove::page::{Page, PageType};

    fn two_cols() -> Vec<ColumnDef> {
        vec![
            ColumnDef { name: "name".into(), col_type: ColType::Text, nullable: true, strainer_root: None },
            ColumnDef { name: "age".into(), col_type: ColType::Integer, nullable: false, strainer_root: None },
        ]
    }

    #[test]
    fn test_encode_decode_row_roundtrip() {
        let cols = two_cols();
        let vals = vec![Value::Text("Tony".into()), Value::Integer(38)];
        let bytes = encode_row(1, &vals, &cols).unwrap();
        let (key, decoded) = decode_row(&bytes, &cols);
        assert_eq!(key, 1);
        assert_eq!(decoded, vals);
    }

    #[test]
    fn test_encode_decode_row_with_null() {
        let cols = two_cols();
        let vals = vec![Value::Null, Value::Integer(25)];
        let bytes = encode_row(2, &vals, &cols).unwrap();
        let (key, decoded) = decode_row(&bytes, &cols);
        assert_eq!(key, 2);
        assert_eq!(decoded[0], Value::Null);
        assert_eq!(decoded[1], Value::Integer(25));
    }

    #[test]
    fn test_heap_insert_and_scan() {
        let cols = two_cols();
        let mut page = Page::new(1, PageType::Heap);
        let r1 = encode_row(1, &[Value::Text("Tony".into()), Value::Integer(38)], &cols).unwrap();
        let r2 = encode_row(2, &[Value::Text("Maria".into()), Value::Integer(29)], &cols).unwrap();
        heap_insert(&mut page, &r1).unwrap();
        heap_insert(&mut page, &r2).unwrap();
        let rows = heap_scan(&page, &cols);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, 1);
        assert_eq!(rows[1].0, 2);
        assert_eq!(rows[0].1[0], Value::Text("Tony".into()));
    }

    #[test]
    fn test_heap_delete_tombstones_correct_row() {
        let cols = two_cols();
        let mut page = Page::new(1, PageType::Heap);
        heap_insert(&mut page, &encode_row(1, &[Value::Text("Tony".into()), Value::Integer(38)], &cols).unwrap()).unwrap();
        heap_insert(&mut page, &encode_row(2, &[Value::Text("Maria".into()), Value::Integer(29)], &cols).unwrap()).unwrap();
        assert!(heap_delete(&mut page, 1, &cols));
        let rows = heap_scan(&page, &cols);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, 2);
    }

    #[test]
    fn test_heap_delete_nonexistent_returns_false() {
        let cols = two_cols();
        let mut page = Page::new(1, PageType::Heap);
        heap_insert(&mut page, &encode_row(1, &[Value::Text("Tony".into()), Value::Integer(38)], &cols).unwrap()).unwrap();
        assert!(!heap_delete(&mut page, 99, &cols));
    }

    #[test]
    fn test_heap_returns_none_when_full() {
        // A large row that fills the page quickly
        let cols = vec![ColumnDef { name: "data".into(), col_type: ColType::Blob, nullable: false, strainer_root: None }];
        let big = Value::Blob(vec![0u8; 512]);
        let mut page = Page::new(1, PageType::Heap);
        let row = encode_row(1, &[big], &cols).unwrap();
        // Fill the page
        let mut inserted = 0;
        while heap_insert(&mut page, &row).is_some() {
            inserted += 1;
            if inserted > 100 { break; } // safety valve
        }
        // One more insert should return None
        assert!(heap_insert(&mut page, &row).is_none());
    }
}
