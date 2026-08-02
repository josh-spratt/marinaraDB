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
fn test_inner_join_combines_matching_rows() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (name TEXT);").unwrap();
    pot.execute("PLATE noodle orders (user_id INT, total REAL);").unwrap();
    pot.execute("LADLE INTO users (name) PORTIONS ('Tony'), ('Maria');").unwrap(); // ids 1, 2
    pot.execute("LADLE INTO orders (user_id, total) PORTIONS (1, 12.5), (1, 3.0), (2, 7.0);").unwrap();

    let r = pot.execute("SERVE u.name, o.total FROM users u TOSS orders o ON u.id = o.user_id STRAINED BY u.name = 'Tony';").unwrap();
    assert_eq!(r.rows.len(), 2);
    for row in &r.rows {
        assert_eq!(row[0], Value::Text("Tony".to_string()));
    }
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_join_unqualified_ambiguous_column_errors() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (name TEXT);").unwrap();
    pot.execute("PLATE noodle orders (user_id INT);").unwrap();
    pot.execute("LADLE INTO users (name) PORTIONS ('Tony');").unwrap(); // id 1
    pot.execute("LADLE INTO orders (user_id) PORTIONS (1);").unwrap();
    // both sides have an `id` column (the Al Dente Key) -- unqualified `id` in STRAINED BY is ambiguous.
    let err = pot.execute("SERVE * FROM users u TOSS orders o ON u.id = o.user_id STRAINED BY id = 1;").unwrap_err();
    assert!(matches!(err, Scottatura::AmbiguousColumn { name } if name == "id"));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_bundled_by_still_returns_clause_not_yet_supported() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (age INT);").unwrap();
    assert!(matches!(
        pot.execute("SERVE age FROM users BUNDLED BY age;").unwrap_err(),
        Scottatura::ClauseNotYetSupported { .. }
    ));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_only_and_skip_apply_after_ordering() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle nums (n INT);").unwrap();
    for i in 0..10 {
        pot.execute(&format!("LADLE INTO nums (n) PORTIONS ({i});")).unwrap();
    }
    let r = pot.execute("SERVE n FROM nums ARRANGED BY n ONLY 3 SKIP 2;").unwrap();
    assert_eq!(r.rows, vec![vec![Value::Integer(2)], vec![Value::Integer(3)], vec![Value::Integer(4)]]);

    let r = pot.execute("SERVE n FROM nums ARRANGED BY n ONLY 0;").unwrap();
    assert!(r.rows.is_empty());

    let r = pot.execute("SERVE n FROM nums ARRANGED BY n ONLY 10 SKIP 100;").unwrap();
    assert!(r.rows.is_empty());
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_order_by_sorts_ascending_and_descending_with_vuoto_first() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle users (name TEXT, age INT);").unwrap();
    pot.execute("LADLE INTO users (name, age) PORTIONS ('Carol', 40), ('Alice', 20), ('Bob', VUOTO) , ('Dave', 20);").unwrap();
    // Bob has VUOTO age.
    let r = pot.execute("SERVE name, age FROM users ARRANGED BY age;").unwrap();
    let names: Vec<String> = r.rows.iter().map(|row| match &row[0] { Value::Text(s) => s.clone(), _ => panic!() }).collect();
    assert_eq!(names[0], "Bob"); // VUOTO first regardless of direction
    let ages: Vec<Value> = r.rows[1..].iter().map(|row| row[1].clone()).collect();
    assert_eq!(ages, vec![Value::Integer(20), Value::Integer(20), Value::Integer(40)]);

    let r = pot.execute("SERVE name, age FROM users ARRANGED BY age DESC;").unwrap();
    let names: Vec<String> = r.rows.iter().map(|row| match &row[0] { Value::Text(s) => s.clone(), _ => panic!() }).collect();
    assert_eq!(names[0], "Bob"); // still first in DESC
    let ages: Vec<Value> = r.rows[1..].iter().map(|row| row[1].clone()).collect();
    assert_eq!(ages, vec![Value::Integer(40), Value::Integer(20), Value::Integer(20)]);
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
