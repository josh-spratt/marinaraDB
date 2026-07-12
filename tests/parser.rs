use marinaradb::parser::{parse_program, parse_statement};
use marinaradb::parser::ast::Statement;
use marinaradb::Scottatura;

#[test]
fn test_full_crud_script_parses() {
    let script = "
        PLATE noodle users (
          name TEXT NOT VUOTO,
          age INT
        );
        LADLE INTO users (name, age) PORTIONS ('Tony', 38), ('Maria', 29);
        SEASON users SET age = 39 STRAINED BY name = 'Tony';
        SERVE name, age FROM users STRAINED BY age > 30 ARRANGED BY age DESC ONLY 10;
        DISCARD FROM users STRAINED BY age < 18;
        TOSS OUT noodle users;
    ";
    let stmts = parse_program(script).unwrap();
    assert_eq!(stmts.len(), 6);
    assert!(matches!(stmts[0], Statement::CreateTable { .. }));
    assert!(matches!(stmts[1], Statement::Insert { .. }));
    assert!(matches!(stmts[2], Statement::Update { .. }));
    assert!(matches!(stmts[3], Statement::Select(_)));
    assert!(matches!(stmts[4], Statement::Delete { .. }));
    assert!(matches!(stmts[5], Statement::DropTable { .. }));
}

#[test]
fn test_join_query_end_to_end() {
    let stmt = parse_statement(
        "SERVE u.name, o.total FROM users u TOSS orders o ON u.id = o.user_id STRAINED BY o.total > 10;"
    ).unwrap();
    assert!(matches!(stmt, Statement::Select(_)));
}

#[test]
fn test_transaction_script() {
    let stmts = parse_program("
        BEGIN SIMMER;
        LADLE INTO orders (user_id, total) PORTIONS (1, 49.99);
        DISCARD FROM users STRAINED BY age < 18;
        FINISH SIMMER;
    ").unwrap();
    assert_eq!(stmts.len(), 4);
}

#[test]
fn test_quoted_reserved_word_identifier() {
    // "serve" as a column name, quoted, must be usable as an identifier.
    let stmt = parse_statement("PLATE noodle t (\"serve\" TEXT);").unwrap();
    match stmt {
        Statement::CreateTable { columns, .. } => assert_eq!(columns[0].name, "serve"),
        other => panic!("expected CreateTable, got {other:?}"),
    }
}

#[test]
fn test_error_display_matches_scottatura_format() {
    let err = parse_statement("SERVE * FRM users;").unwrap_err();
    let msg = err.to_string();
    assert!(msg.starts_with("Scottatura [PARSE_001]"), "unexpected message: {msg}");
}

#[test]
fn test_scottatura_error_is_matchable_by_caller() {
    let err = parse_statement("DISCARD FROM users;").unwrap_err();
    assert!(matches!(err, Scottatura::DiscardRequiresFilter));
}
