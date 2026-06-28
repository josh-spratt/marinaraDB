use marinaradb::{ColType, ColumnDef, Value};

fn tmp_path(tag: &str) -> String {
    format!("/tmp/marinara_test_{tag}_{}.pot",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap()
            .subsec_nanos())
}

fn user_cols() -> Vec<ColumnDef> {
    vec![
        ColumnDef { name: "name".into(), col_type: ColType::Text, nullable: true, strainer_root: None },
        ColumnDef { name: "age".into(), col_type: ColType::Integer, nullable: false, strainer_root: None },
    ]
}

#[test]
fn test_create_table_and_list_noodles() {
    let path = tmp_path("list");
    let mut pot = marinaradb::Pot::open(&path).unwrap();
    pot.create_table("users", user_cols()).unwrap();
    assert!(pot.noodles().contains(&"users".to_string()));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_recipe_returns_correct_columns() {
    let path = tmp_path("recipe");
    let mut pot = marinaradb::Pot::open(&path).unwrap();
    pot.create_table("users", user_cols()).unwrap();
    let recipe = pot.recipe("users").unwrap();
    assert_eq!(recipe.noodle_name, "users");
    assert_eq!(recipe.columns[0].name, "name");
    assert_eq!(recipe.columns[1].name, "age");
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_insert_returns_incrementing_keys() {
    let path = tmp_path("keys");
    let mut pot = marinaradb::Pot::open(&path).unwrap();
    pot.create_table("users", user_cols()).unwrap();
    let k1 = pot.insert_strand("users", vec![Value::Text("Tony".into()), Value::Integer(38)]).unwrap();
    let k2 = pot.insert_strand("users", vec![Value::Text("Maria".into()), Value::Integer(29)]).unwrap();
    assert_eq!(k1, 1);
    assert_eq!(k2, 2);
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_scan_returns_inserted_rows_in_order() {
    let path = tmp_path("scan");
    let mut pot = marinaradb::Pot::open(&path).unwrap();
    pot.create_table("users", user_cols()).unwrap();
    pot.insert_strand("users", vec![Value::Text("Tony".into()), Value::Integer(38)]).unwrap();
    pot.insert_strand("users", vec![Value::Text("Maria".into()), Value::Integer(29)]).unwrap();
    let rows = pot.scan_noodle("users").unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].1[0], Value::Text("Tony".into()));
    assert_eq!(rows[1].1[1], Value::Integer(29));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_not_null_violation_errors() {
    let path = tmp_path("notnull");
    let mut pot = marinaradb::Pot::open(&path).unwrap();
    pot.create_table("users", user_cols()).unwrap();
    // "age" is NOT NULL
    let result = pot.insert_strand("users", vec![Value::Text("Tony".into()), Value::Null]);
    assert!(matches!(result, Err(marinaradb::Scottatura::NullConstraintViolation { .. })));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_data_persists_across_reopen() {
    let path = tmp_path("persist");
    {
        let mut pot = marinaradb::Pot::open(&path).unwrap();
        pot.create_table("nums", vec![
            ColumnDef { name: "x".into(), col_type: ColType::Integer, nullable: false, strainer_root: None },
        ]).unwrap();
        pot.insert_strand("nums", vec![Value::Integer(99)]).unwrap();
    }
    let mut pot2 = marinaradb::Pot::open(&path).unwrap();
    let rows = pot2.scan_noodle("nums").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1[0], Value::Integer(99));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_drop_table_removes_noodle() {
    let path = tmp_path("drop");
    let mut pot = marinaradb::Pot::open(&path).unwrap();
    pot.create_table("tmp", user_cols()).unwrap();
    pot.drop_table("tmp").unwrap();
    assert!(!pot.noodles().contains(&"tmp".to_string()));
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_missing_noodle_errors() {
    let path = tmp_path("missing");
    let mut pot = marinaradb::Pot::open(&path).unwrap();
    let result = pot.scan_noodle("ghosts");
    assert!(matches!(result, Err(marinaradb::Scottatura::NoodleNotFound { .. })));
    std::fs::remove_file(&path).ok();
}
