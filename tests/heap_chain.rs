use marinaradb::{ColType, ColumnDef, Pot};

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
        pot.insert_strand("nums", vec![marinaradb::Value::Integer(i)]).unwrap();
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
        pot.insert_strand("nums", vec![marinaradb::Value::Integer(i)]).unwrap();
    }
    // Key 500 (the 500th insert, i.e. last_insert_key of the loop's last call) lives on a later page.
    assert!(pot.delete_by_key("nums", 500).unwrap());
    assert!(!pot.delete_by_key("nums", 500).unwrap()); // already gone

    pot.insert_with_key("nums", 500, &[marinaradb::Value::Integer(999)]).unwrap();
    let rows = pot.scan_noodle("nums").unwrap();
    let reinserted = rows.iter().find(|(k, _)| *k == 500).unwrap();
    assert_eq!(reinserted.1[0], marinaradb::Value::Integer(999));
    std::fs::remove_file(&path).ok();
}
