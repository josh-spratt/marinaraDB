use std::cmp::Ordering;
use crate::parser::ast::{BinOp, Expr};
use crate::{ColType, ColumnDef, Scottatura, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tri {
    True,
    False,
    Unknown,
}

impl Tri {
    pub(crate) fn is_true(&self) -> bool {
        matches!(self, Tri::True)
    }

    fn from_bool(b: bool) -> Tri {
        if b { Tri::True } else { Tri::False }
    }

    fn not(&self) -> Tri {
        match self {
            Tri::True => Tri::False,
            Tri::False => Tri::True,
            Tri::Unknown => Tri::Unknown,
        }
    }

    fn and(&self, other: Tri) -> Tri {
        match (self, other) {
            (Tri::False, _) | (_, Tri::False) => Tri::False,
            (Tri::True, Tri::True) => Tri::True,
            _ => Tri::Unknown,
        }
    }

    fn or(&self, other: Tri) -> Tri {
        match (self, other) {
            (Tri::True, _) | (_, Tri::True) => Tri::True,
            (Tri::False, Tri::False) => Tri::False,
            _ => Tri::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SchemaColumn {
    pub owner: String,
    pub col: ColumnDef,
}

pub(crate) type Schema = Vec<SchemaColumn>;

pub(crate) fn full_schema(owner: &str, cols: &[ColumnDef]) -> Schema {
    let mut out = vec![SchemaColumn {
        owner: owner.to_string(),
        col: ColumnDef {
            name: "id".to_string(),
            col_type: ColType::Integer,
            nullable: false,
            strainer_root: None,
        },
    }];
    out.extend(cols.iter().cloned().map(|col| SchemaColumn { owner: owner.to_string(), col }));
    out
}

pub(crate) fn full_row(key: i64, values: &[Value]) -> Vec<Value> {
    let mut out = vec![Value::Integer(key)];
    out.extend(values.iter().cloned());
    out
}

fn column_value(qualifier: Option<&str>, name: &str, row: &[Value], schema: &Schema) -> Result<Value, Scottatura> {
    let matches: Vec<usize> = schema.iter().enumerate()
        .filter(|(_, sc)| sc.col.name == name && qualifier.is_none_or(|q| sc.owner == q))
        .map(|(i, _)| i)
        .collect();
    match matches.as_slice() {
        [] => Err(Scottatura::ColumnNotFound { col: name.to_string(), name: qualifier.unwrap_or("?").to_string() }),
        [i] => Ok(row[*i].clone()),
        _ => Err(Scottatura::AmbiguousColumn { name: name.to_string() }),
    }
}

fn values_equal(l: &Value, r: &Value) -> Result<bool, Scottatura> {
    match (l, r) {
        (Value::Integer(a), Value::Integer(b)) => Ok(a == b),
        (Value::Real(a), Value::Real(b)) => Ok(a == b),
        (Value::Integer(a), Value::Real(b)) => Ok(*a as f64 == *b),
        (Value::Real(a), Value::Integer(b)) => Ok(*a == *b as f64),
        (Value::Text(a), Value::Text(b)) => Ok(a == b),
        (Value::Blob(a), Value::Blob(b)) => Ok(a == b),
        (l, r) => Err(Scottatura::TypeMismatch {
            got: format!("{r:?}"), col: "<expr>".to_string(), expected: format!("{l:?}"),
        }),
    }
}

fn compare_values(l: &Value, r: &Value) -> Result<Ordering, Scottatura> {
    match (l, r) {
        (Value::Integer(a), Value::Integer(b)) => Ok(a.cmp(b)),
        (Value::Real(a), Value::Real(b)) => a.partial_cmp(b).ok_or_else(|| Scottatura::TypeMismatch {
            got: "NaN".to_string(), col: "<expr>".to_string(), expected: "orderable REAL".to_string(),
        }),
        (Value::Integer(a), Value::Real(b)) => (*a as f64).partial_cmp(b).ok_or_else(|| Scottatura::TypeMismatch {
            got: "NaN".to_string(), col: "<expr>".to_string(), expected: "orderable REAL".to_string(),
        }),
        (Value::Real(a), Value::Integer(b)) => a.partial_cmp(&(*b as f64)).ok_or_else(|| Scottatura::TypeMismatch {
            got: "NaN".to_string(), col: "<expr>".to_string(), expected: "orderable REAL".to_string(),
        }),
        (Value::Text(a), Value::Text(b)) => Ok(a.cmp(b)),
        (l, r) => Err(Scottatura::TypeMismatch {
            got: format!("{r:?}"), col: "<expr>".to_string(), expected: format!("orderable value matching {l:?}"),
        }),
    }
}

fn like_match(text: &str, pattern: &str) -> bool {
    fn rec(t: &[u8], p: &[u8]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some(b'%') => rec(t, &p[1..]) || (!t.is_empty() && rec(&t[1..], p)),
            Some(b'_') => !t.is_empty() && rec(&t[1..], &p[1..]),
            Some(c) => !t.is_empty() && t[0] == *c && rec(&t[1..], &p[1..]),
        }
    }
    rec(text.as_bytes(), pattern.as_bytes())
}

fn eval_arith(op: &BinOp, l: Value, r: Value) -> Result<Value, Scottatura> {
    if matches!(l, Value::Null) || matches!(r, Value::Null) {
        return Ok(Value::Null);
    }
    let (a, b): (f64, f64) = match (&l, &r) {
        (Value::Integer(a), Value::Integer(b)) => {
            let (a, b) = (*a, *b);
            return Ok(match op {
                BinOp::Add => Value::Integer(a + b),
                BinOp::Sub => Value::Integer(a - b),
                BinOp::Mul => Value::Integer(a * b),
                BinOp::Div => {
                    if b == 0 { Value::Null } else { Value::Integer(a / b) }
                }
                _ => unreachable!("eval_arith only called with arithmetic ops"),
            });
        }
        (Value::Integer(a), Value::Real(b)) => (*a as f64, *b),
        (Value::Real(a), Value::Integer(b)) => (*a, *b as f64),
        (Value::Real(a), Value::Real(b)) => (*a, *b),
        _ => return Err(Scottatura::TypeMismatch {
            got: format!("{r:?}"), col: "<expr>".to_string(), expected: "INTEGER or REAL".to_string(),
        }),
    };
    Ok(match op {
        BinOp::Add => Value::Real(a + b),
        BinOp::Sub => Value::Real(a - b),
        BinOp::Mul => Value::Real(a * b),
        BinOp::Div => if b == 0.0 { Value::Null } else { Value::Real(a / b) },
        _ => unreachable!("eval_arith only called with arithmetic ops"),
    })
}

pub(crate) fn eval_scalar(expr: &Expr, row: &[Value], schema: &Schema) -> Result<Value, Scottatura> {
    match expr {
        Expr::Literal(v) => Ok(v.clone()),
        Expr::Column { qualifier, name } => column_value(qualifier.as_deref(), name, row, schema),
        Expr::BinaryOp { op: op @ (BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div), left, right } => {
            let l = eval_scalar(left, row, schema)?;
            let r = eval_scalar(right, row, schema)?;
            eval_arith(op, l, r)
        }
        Expr::FunctionCall { .. } => Err(Scottatura::ClauseNotYetSupported { clause: "aggregate functions".to_string() }),
        _ => Err(Scottatura::ClauseNotYetSupported { clause: "boolean expression in value position".to_string() }),
    }
}

pub(crate) fn eval_tri(expr: &Expr, row: &[Value], schema: &Schema) -> Result<Tri, Scottatura> {
    match expr {
        Expr::BinaryOp { op: BinOp::And, left, right } => {
            Ok(eval_tri(left, row, schema)?.and(eval_tri(right, row, schema)?))
        }
        Expr::BinaryOp { op: BinOp::Or, left, right } => {
            Ok(eval_tri(left, row, schema)?.or(eval_tri(right, row, schema)?))
        }
        Expr::BinaryOp { op: cmp_op @ (BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte), left, right } => {
            let l = eval_scalar(left, row, schema)?;
            let r = eval_scalar(right, row, schema)?;
            if matches!(l, Value::Null) || matches!(r, Value::Null) {
                return Ok(Tri::Unknown);
            }
            let result = match cmp_op {
                BinOp::Eq => values_equal(&l, &r)?,
                BinOp::Neq => !values_equal(&l, &r)?,
                BinOp::Lt => compare_values(&l, &r)? == Ordering::Less,
                BinOp::Gt => compare_values(&l, &r)? == Ordering::Greater,
                BinOp::Lte => compare_values(&l, &r)? != Ordering::Greater,
                BinOp::Gte => compare_values(&l, &r)? != Ordering::Less,
                _ => unreachable!(),
            };
            Ok(Tri::from_bool(result))
        }
        Expr::UnaryNot(inner) => Ok(eval_tri(inner, row, schema)?.not()),
        Expr::IsNull { expr, negated } => {
            let is_null = matches!(eval_scalar(expr, row, schema)?, Value::Null);
            Ok(Tri::from_bool(is_null != *negated))
        }
        Expr::Between { expr, low, high } => {
            let v = eval_scalar(expr, row, schema)?;
            let lo = eval_scalar(low, row, schema)?;
            let hi = eval_scalar(high, row, schema)?;
            if matches!(v, Value::Null) || matches!(lo, Value::Null) || matches!(hi, Value::Null) {
                return Ok(Tri::Unknown);
            }
            let ge_lo = compare_values(&v, &lo)? != Ordering::Less;
            let le_hi = compare_values(&v, &hi)? != Ordering::Greater;
            Ok(Tri::from_bool(ge_lo && le_hi))
        }
        Expr::Like { expr, pattern } => {
            let v = eval_scalar(expr, row, schema)?;
            let p = eval_scalar(pattern, row, schema)?;
            if matches!(v, Value::Null) || matches!(p, Value::Null) {
                return Ok(Tri::Unknown);
            }
            match (v, p) {
                (Value::Text(t), Value::Text(pat)) => Ok(Tri::from_bool(like_match(&t, &pat))),
                (got, _) => Err(Scottatura::TypeMismatch {
                    got: format!("{got:?}"), col: "<expr>".to_string(), expected: "TEXT".to_string(),
                }),
            }
        }
        Expr::In { expr, list } => {
            let v = eval_scalar(expr, row, schema)?;
            if matches!(v, Value::Null) {
                return Ok(Tri::Unknown);
            }
            for item in list {
                let item_val = eval_scalar(item, row, schema)?;
                if !matches!(item_val, Value::Null) && values_equal(&v, &item_val)? {
                    return Ok(Tri::True);
                }
            }
            Ok(Tri::False)
        }
        _ => Err(Scottatura::ClauseNotYetSupported { clause: "non-boolean expression in filter position".to_string() }),
    }
}

use crate::parser::ast::SelectItem;
use crate::sous_chef::Plan;
use crate::stove::pot::Pot;
use crate::ResultSet;

/// Resolve the projected column names for a SELECT item list against a table
/// schema. This does not depend on any particular row, so it can (and should)
/// be computed once regardless of how many rows match the filter -- including
/// zero rows.
fn project_names(items: &[SelectItem], schema: &Schema) -> Vec<String> {
    let mut names = Vec::new();
    for (i, item) in items.iter().enumerate() {
        match item {
            SelectItem::Star => {
                for sc in schema {
                    names.push(sc.col.name.clone());
                }
            }
            SelectItem::Expr { expr, alias } => {
                let name = alias.clone().unwrap_or_else(|| match expr {
                    Expr::Column { name, .. } => name.clone(),
                    _ => format!("expr{}", i + 1),
                });
                names.push(name);
            }
        }
    }
    names
}

/// Resolve the projected values for a SELECT item list against a single row.
fn project_values(items: &[SelectItem], schema: &Schema, row: &[Value]) -> Result<Vec<Value>, Scottatura> {
    let mut vals = Vec::new();
    for item in items {
        match item {
            SelectItem::Star => {
                for val in row.iter().take(schema.len()) {
                    vals.push(val.clone());
                }
            }
            SelectItem::Expr { expr, .. } => {
                vals.push(eval_scalar(expr, row, schema)?);
            }
        }
    }
    Ok(vals)
}

fn resolve_insert_values(table: &str, table_cols: &[ColumnDef], columns: &[String], row_exprs: &[Expr]) -> Result<Vec<Value>, Scottatura> {
    for name in columns {
        if !table_cols.iter().any(|c| &c.name == name) {
            return Err(Scottatura::ColumnNotFound { col: name.clone(), name: table.to_string() });
        }
    }
    let mut by_name: std::collections::HashMap<&str, Value> = std::collections::HashMap::new();
    let empty_schema: Schema = Vec::new();
    for (name, expr) in columns.iter().zip(row_exprs.iter()) {
        by_name.insert(name.as_str(), eval_scalar(expr, &[], &empty_schema)?);
    }
    Ok(table_cols.iter().map(|col| by_name.remove(col.name.as_str()).unwrap_or(Value::Null)).collect())
}

pub(crate) fn execute(plan: Plan, pot: &mut Pot) -> Result<ResultSet, Scottatura> {
    match plan {
        Plan::CreateTable { name, columns } => {
            let cols = columns.into_iter().map(|c| ColumnDef {
                name: c.name, col_type: c.col_type, nullable: !c.not_null, strainer_root: None,
            }).collect();
            pot.create_table(&name, cols)?;
            Ok(ResultSet::default())
        }
        Plan::DropTable { name } => {
            pot.drop_table(&name)?;
            Ok(ResultSet::default())
        }
        Plan::Insert { table, columns, rows } => {
            let recipe = pot.recipe(&table)?;
            let mut last_key = None;
            let n = rows.len();
            for row_exprs in &rows {
                let values = resolve_insert_values(&table, &recipe.columns, &columns, row_exprs)?;
                last_key = Some(pot.insert_strand(&table, values)?);
            }
            Ok(ResultSet { columns: vec![], rows: vec![], affected_rows: n as u64, last_insert_key: last_key })
        }
        Plan::SeqScan { table, columns, filter } => {
            let recipe = pot.recipe(&table)?;
            let schema = full_schema(&table, &recipe.columns);
            let raw_rows = pot.scan_noodle(&table)?;
            let out_names = project_names(&columns, &schema);
            let mut out_rows = Vec::new();
            for (key, values) in raw_rows {
                let row = full_row(key, &values);
                if let Some(f) = &filter {
                    if !eval_tri(f, &row, &schema)?.is_true() {
                        continue;
                    }
                }
                out_rows.push(project_values(&columns, &schema, &row)?);
            }
            let n = out_rows.len() as u64;
            Ok(ResultSet { columns: out_names, rows: out_rows, affected_rows: n, last_insert_key: None })
        }
        Plan::Update { table, assignments, filter } => {
            let recipe = pot.recipe(&table)?;
            let schema = full_schema(&table, &recipe.columns);
            for (name, _) in &assignments {
                if name == "id" {
                    return Err(Scottatura::AlDenteKeyNotUpdatable);
                }
            }
            let raw_rows = pot.scan_noodle(&table)?;
            let mut affected = 0u64;
            for (key, values) in raw_rows {
                let row = full_row(key, &values);
                if !eval_tri(&filter, &row, &schema)?.is_true() {
                    continue;
                }
                let mut new_values = values.clone();
                for (name, expr) in &assignments {
                    let idx = recipe.columns.iter().position(|c| &c.name == name)
                        .ok_or_else(|| Scottatura::ColumnNotFound { col: name.clone(), name: table.clone() })?;
                    new_values[idx] = eval_scalar(expr, &row, &schema)?;
                }
                pot.delete_by_key(&table, key)?;
                pot.insert_with_key(&table, key, &new_values)?;
                affected += 1;
            }
            Ok(ResultSet { columns: vec![], rows: vec![], affected_rows: affected, last_insert_key: None })
        }
        Plan::Delete { table, filter } => {
            let recipe = pot.recipe(&table)?;
            let schema = full_schema(&table, &recipe.columns);
            let raw_rows = pot.scan_noodle(&table)?;
            let mut affected = 0u64;
            for (key, values) in raw_rows {
                let row = full_row(key, &values);
                if !eval_tri(&filter, &row, &schema)?.is_true() {
                    continue;
                }
                pot.delete_by_key(&table, key)?;
                affected += 1;
            }
            Ok(ResultSet { columns: vec![], rows: vec![], affected_rows: affected, last_insert_key: None })
        }
    }
}

#[cfg(test)]
mod execute_tests {
    use super::*;
    use crate::sous_chef::plan;
    use crate::parser::parse_statement;

    fn temp_pot() -> Pot {
        let f = tempfile::NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_string();
        drop(f);
        std::fs::remove_file(&path).ok();
        Pot::open(&path).unwrap()
    }

    fn run(pot: &mut Pot, sql: &str) -> Result<ResultSet, Scottatura> {
        execute(plan(parse_statement(sql).unwrap()).unwrap(), pot)
    }

    #[test]
    fn test_create_insert_serve() {
        let mut pot = temp_pot();
        run(&mut pot, "PLATE noodle users (name TEXT, age INT);").unwrap();
        let r = run(&mut pot, "LADLE INTO users (name, age) PORTIONS ('Tony', 38);").unwrap();
        assert_eq!(r.last_insert_key, Some(1));
        let r = run(&mut pot, "SERVE * FROM users STRAINED BY age = 38;").unwrap();
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.columns, vec!["id", "name", "age"]);
    }

    #[test]
    fn test_insert_with_partial_columns_defaults_to_vuoto() {
        let mut pot = temp_pot();
        run(&mut pot, "PLATE noodle users (name TEXT, age INT);").unwrap();
        run(&mut pot, "LADLE INTO users (name) PORTIONS ('Tony');").unwrap();
        let r = run(&mut pot, "SERVE * FROM users;").unwrap();
        assert_eq!(r.rows[0][2], Value::Null);
    }

    #[test]
    fn test_update_moves_row_keeping_key() {
        let mut pot = temp_pot();
        run(&mut pot, "PLATE noodle users (name TEXT, age INT);").unwrap();
        run(&mut pot, "LADLE INTO users (name, age) PORTIONS ('Tony', 38);").unwrap();
        run(&mut pot, "SEASON users SET age = 39 STRAINED BY name = 'Tony';").unwrap();
        let r = run(&mut pot, "SERVE * FROM users;").unwrap();
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.rows[0][0], Value::Integer(1)); // same id
        assert_eq!(r.rows[0][2], Value::Integer(39));
    }

    #[test]
    fn test_update_id_rejected() {
        let mut pot = temp_pot();
        run(&mut pot, "PLATE noodle users (name TEXT, age INT);").unwrap();
        run(&mut pot, "LADLE INTO users (name, age) PORTIONS ('Tony', 38);").unwrap();
        let err = run(&mut pot, "SEASON users SET id = 5 STRAINED BY name = 'Tony';").unwrap_err();
        assert!(matches!(err, Scottatura::AlDenteKeyNotUpdatable));
    }

    #[test]
    fn test_delete_removes_matching_row() {
        let mut pot = temp_pot();
        run(&mut pot, "PLATE noodle users (name TEXT, age INT);").unwrap();
        run(&mut pot, "LADLE INTO users (name, age) PORTIONS ('Tony', 38), ('Maria', 12);").unwrap();
        let r = run(&mut pot, "DISCARD FROM users STRAINED BY age < 18;").unwrap();
        assert_eq!(r.affected_rows, 1);
        let r = run(&mut pot, "SERVE * FROM users;").unwrap();
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.rows[0][1], Value::Text("Tony".to_string()));
    }

    #[test]
    fn test_drop_table() {
        let mut pot = temp_pot();
        run(&mut pot, "PLATE noodle users (name TEXT);").unwrap();
        run(&mut pot, "TOSS OUT noodle users;").unwrap();
        assert!(pot.noodles().is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_statement;
    use crate::parser::ast::Statement;

    fn schema() -> Schema {
        full_schema("t", &[
            ColumnDef { name: "name".into(), col_type: ColType::Text, nullable: true, strainer_root: None },
            ColumnDef { name: "age".into(), col_type: ColType::Integer, nullable: true, strainer_root: None },
        ])
    }

    fn filter_of(sql: &str) -> Expr {
        match parse_statement(sql).unwrap() {
            Statement::Select(s) => s.filter.unwrap(),
            _ => panic!("expected SERVE"),
        }
    }

    #[test]
    fn test_eq_true() {
        let row = vec![Value::Integer(1), Value::Text("Tony".into()), Value::Integer(38)];
        let f = filter_of("SERVE * FROM t STRAINED BY age = 38;");
        assert_eq!(eval_tri(&f, &row, &schema()).unwrap(), Tri::True);
    }

    #[test]
    fn test_vuoto_equality_is_unknown() {
        let row = vec![Value::Integer(1), Value::Text("Tony".into()), Value::Null];
        let f = filter_of("SERVE * FROM t STRAINED BY age = 38;");
        assert_eq!(eval_tri(&f, &row, &schema()).unwrap(), Tri::Unknown);
    }

    #[test]
    fn test_is_vuoto() {
        let row = vec![Value::Integer(1), Value::Text("Tony".into()), Value::Null];
        let f = filter_of("SERVE * FROM t STRAINED BY age IS VUOTO;");
        assert_eq!(eval_tri(&f, &row, &schema()).unwrap(), Tri::True);
    }

    #[test]
    fn test_and_short_circuits_to_false() {
        let row = vec![Value::Integer(1), Value::Text("Tony".into()), Value::Null];
        let f = filter_of("SERVE * FROM t STRAINED BY age = 38 AND name = 'nobody';");
        assert_eq!(eval_tri(&f, &row, &schema()).unwrap(), Tri::False);
    }

    #[test]
    fn test_between_inclusive() {
        let row = vec![Value::Integer(1), Value::Text("Tony".into()), Value::Integer(38)];
        let f = filter_of("SERVE * FROM t STRAINED BY age BETWEEN 38 AND 40;");
        assert_eq!(eval_tri(&f, &row, &schema()).unwrap(), Tri::True);
    }

    #[test]
    fn test_like_wildcards() {
        let row = vec![Value::Integer(1), Value::Text("Tony".into()), Value::Integer(38)];
        let f = filter_of("SERVE * FROM t STRAINED BY name LIKE 'T_n%';");
        assert_eq!(eval_tri(&f, &row, &schema()).unwrap(), Tri::True);
    }

    #[test]
    fn test_in_list() {
        let row = vec![Value::Integer(1), Value::Text("Tony".into()), Value::Integer(38)];
        let f = filter_of("SERVE * FROM t STRAINED BY age IN (1, 2, 38);");
        assert_eq!(eval_tri(&f, &row, &schema()).unwrap(), Tri::True);
    }

    #[test]
    fn test_integer_promotes_to_real_in_comparison() {
        let row = vec![Value::Integer(1), Value::Text("Tony".into()), Value::Integer(38)];
        let f = filter_of("SERVE * FROM t STRAINED BY age > 37.5;");
        assert_eq!(eval_tri(&f, &row, &schema()).unwrap(), Tri::True);
    }

    #[test]
    fn test_arithmetic_with_vuoto_is_vuoto() {
        let row = vec![Value::Integer(1), Value::Text("Tony".into()), Value::Null];
        let expr = Expr::BinaryOp {
            op: BinOp::Add,
            left: Box::new(Expr::Column { qualifier: None, name: "age".to_string() }),
            right: Box::new(Expr::Literal(Value::Integer(1))),
        };
        assert_eq!(eval_scalar(&expr, &row, &schema()).unwrap(), Value::Null);
    }

    #[test]
    fn test_text_ordering_is_lexicographic() {
        let row = vec![Value::Integer(1), Value::Text("banana".into()), Value::Integer(38)];
        let f = filter_of("SERVE * FROM t STRAINED BY name > 'apple';");
        assert_eq!(eval_tri(&f, &row, &schema()).unwrap(), Tri::True);
    }

    fn two_table_schema() -> Schema {
        let mut s = full_schema("users", &[
            ColumnDef { name: "name".into(), col_type: ColType::Text, nullable: true, strainer_root: None },
        ]);
        s.extend(full_schema("orders", &[
            ColumnDef { name: "name".into(), col_type: ColType::Text, nullable: true, strainer_root: None },
        ]));
        s
    }

    #[test]
    fn test_qualified_column_resolves_correct_side() {
        // schema: [users.id, users.name, orders.id, orders.name]
        let row = vec![
            Value::Integer(1), Value::Text("Tony".into()),
            Value::Integer(9), Value::Text("Pizza".into()),
        ];
        let schema = two_table_schema();
        let v = column_value(Some("orders"), "name", &row, &schema).unwrap();
        assert_eq!(v, Value::Text("Pizza".into()));
    }

    #[test]
    fn test_unqualified_ambiguous_column_errors() {
        let row = vec![
            Value::Integer(1), Value::Text("Tony".into()),
            Value::Integer(9), Value::Text("Pizza".into()),
        ];
        let schema = two_table_schema();
        let err = column_value(None, "name", &row, &schema).unwrap_err();
        assert!(matches!(err, Scottatura::AmbiguousColumn { name } if name == "name"));
    }
}
