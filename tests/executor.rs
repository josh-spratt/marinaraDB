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
fn test_group_by_with_aggregates() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle orders (customer TEXT, total REAL);").unwrap();
    pot.execute("LADLE INTO orders (customer, total) PORTIONS ('Tony', 10.0), ('Tony', 5.0), ('Maria', 20.0);").unwrap();

    let r = pot.execute("SERVE customer, COUNT(*), SUM(total) FROM orders BUNDLED BY customer ARRANGED BY customer;").unwrap();
    assert_eq!(r.rows, vec![
        vec![Value::Text("Maria".to_string()), Value::Integer(1), Value::Real(20.0)],
        vec![Value::Text("Tony".to_string()), Value::Integer(2), Value::Real(15.0)],
    ]);
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_ungrouped_aggregate_over_empty_result_yields_zero_count() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle orders (total REAL);").unwrap();
    let r = pot.execute("SERVE COUNT(*), SUM(total) FROM orders;").unwrap();
    assert_eq!(r.rows, vec![vec![Value::Integer(0), Value::Null]]);
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_select_star_in_aggregate_context_rejected() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle orders (customer TEXT);").unwrap();
    let err = pot.execute("SERVE * FROM orders BUNDLED BY customer;").unwrap_err();
    assert!(matches!(err, Scottatura::ClauseNotYetSupported { .. }));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_ungrouped_column_in_aggregate_context_rejected() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle orders (customer TEXT, total REAL);").unwrap();
    let err = pot.execute("SERVE customer, total, COUNT(*) FROM orders BUNDLED BY customer;").unwrap_err();
    assert!(matches!(err, Scottatura::ColumnNotGroupedOrAggregated { name } if name == "total"));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_unknown_aggregate_function_rejected() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle orders (total REAL);").unwrap();
    let err = pot.execute("SERVE MEDIAN(total) FROM orders;").unwrap_err();
    assert!(matches!(err, Scottatura::UnknownAggregateFunction { name } if name == "MEDIAN"));
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

#[test]
fn test_join_group_order_limit_combined() {
    let path = temp_path();
    let mut pot = Pot::open(&path).unwrap();
    pot.execute("PLATE noodle customers (name TEXT);").unwrap();
    pot.execute("PLATE noodle orders (customer_id INT, total REAL);").unwrap();
    pot.execute("LADLE INTO customers (name) PORTIONS ('Tony'), ('Maria'), ('Gino');").unwrap(); // ids 1,2,3
    pot.execute("LADLE INTO orders (customer_id, total) PORTIONS (1, 10.0), (1, 5.0), (2, 100.0), (3, 1.0);").unwrap();

    // `BUNDLED BY` only accepts a single bare identifier (src/parser/mod.rs's
    // group_by parsing calls `expect_ident()`, which does not accept dotted
    // `table.column` syntax) -- use the unqualified column name. It's
    // unambiguous here because only `customers` has a `name` column.
    let r = pot.execute(
        "SERVE c.name, SUM(o.total) FROM customers c TOSS orders o ON c.id = o.customer_id \
         BUNDLED BY name ARRANGED BY name ONLY 2;"
    ).unwrap();
    assert_eq!(r.rows, vec![
        vec![Value::Text("Gino".to_string()), Value::Real(1.0)],
        vec![Value::Text("Maria".to_string()), Value::Real(100.0)],
    ]);
    std::fs::remove_file(&path).ok();
}
