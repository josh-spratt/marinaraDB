use marinaradb::Pot;

fn temp_path() -> String {
    let f = tempfile::NamedTempFile::new().unwrap();
    let p = f.path().to_str().unwrap().to_string();
    drop(f);
    p
}

#[test]
fn test_pot_execute_smoke() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (name TEXT NOT VUOTO, age INT);").unwrap();
    let r = pot.execute("LADLE INTO users (name, age) PORTIONS ('Tony', 38);").unwrap();
    assert_eq!(r.last_insert_key, Some(1));
    let r = pot.execute("SERVE * FROM users STRAINED BY age = 38;").unwrap();
    assert_eq!(r.rows.len(), 1);
    std::fs::remove_file(&path).ok();
}
