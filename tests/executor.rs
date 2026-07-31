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

use marinaradb::{Scottatura, Value};

#[test]
fn test_full_crud_lifecycle_through_execute() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (name TEXT NOT VUOTO, age INT);").unwrap();
    pot.execute("LADLE INTO users (name, age) PORTIONS ('Tony', 38), ('Maria', 29);").unwrap();
    pot.execute("SEASON users SET age = 39 STRAINED BY name = 'Tony';").unwrap();
    let r = pot.execute("SERVE name, age FROM users STRAINED BY age > 30;").unwrap();
    assert_eq!(r.rows.len(), 1);
    pot.execute("DISCARD FROM users STRAINED BY age < 35;").unwrap();
    let r = pot.execute("SERVE * FROM users;").unwrap();
    assert_eq!(r.rows.len(), 1);
    pot.execute("TOSS OUT noodle users;").unwrap();
    assert!(pot.noodles().is_empty());
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_multi_page_insert_via_execute() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle nums (n INT);").unwrap();
    for i in 0..500 {
        pot.execute(&format!("LADLE INTO nums (n) PORTIONS ({i});")).unwrap();
    }
    let r = pot.execute("SERVE * FROM nums;").unwrap();
    assert_eq!(r.rows.len(), 500);
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_join_returns_clause_not_yet_supported() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (name TEXT);").unwrap();
    pot.execute("PLATE noodle orders (user_id INT);").unwrap();
    let err = pot.execute("SERVE * FROM users u TOSS orders o ON u.id = o.user_id;").unwrap_err();
    assert!(matches!(err, Scottatura::ClauseNotYetSupported { .. }));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_order_by_limit_bundled_by_all_return_clause_not_yet_supported() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (age INT);").unwrap();
    assert!(matches!(
        pot.execute("SERVE * FROM users ARRANGED BY age;").unwrap_err(),
        Scottatura::ClauseNotYetSupported { .. }
    ));
    assert!(matches!(
        pot.execute("SERVE * FROM users ONLY 10;").unwrap_err(),
        Scottatura::ClauseNotYetSupported { .. }
    ));
    assert!(matches!(
        pot.execute("SERVE age FROM users BUNDLED BY age;").unwrap_err(),
        Scottatura::ClauseNotYetSupported { .. }
    ));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_transactions_return_transaction_not_supported() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    assert!(matches!(pot.execute("BEGIN SIMMER;").unwrap_err(), Scottatura::TransactionNotSupported));
    assert!(matches!(pot.execute("FINISH SIMMER;").unwrap_err(), Scottatura::TransactionNotSupported));
    assert!(matches!(pot.execute("SPILL SIMMER;").unwrap_err(), Scottatura::TransactionNotSupported));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_strain_and_remove_strainer_return_strainer_not_supported() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (age INT);").unwrap();
    assert!(matches!(
        pot.execute("STRAIN idx1 ON users (age);").unwrap_err(),
        Scottatura::StrainerNotSupported
    ));
    assert!(matches!(
        pot.execute("REMOVE STRAINER idx1;").unwrap_err(),
        Scottatura::StrainerNotSupported
    ));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_vuoto_three_valued_logic_excludes_unknown_rows() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (name TEXT, age INT);").unwrap();
    pot.execute("LADLE INTO users (name) PORTIONS ('NoAge');").unwrap(); // age is VUOTO
    pot.execute("LADLE INTO users (name, age) PORTIONS ('HasAge', 20);").unwrap();
    let r = pot.execute("SERVE * FROM users STRAINED BY age = 20;").unwrap();
    assert_eq!(r.rows.len(), 1);
    let r = pot.execute("SERVE * FROM users STRAINED BY age IS VUOTO;").unwrap();
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0][1], Value::Text("NoAge".to_string()));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_not_null_violation_through_execute() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (name TEXT NOT VUOTO);").unwrap();
    let err = pot.execute("LADLE INTO users () PORTIONS ();").unwrap_err();
    assert!(matches!(err, Scottatura::NullConstraintViolation { .. }) || matches!(err, Scottatura::UnexpectedToken { .. }));
    std::fs::remove_file(&path).ok();
}
