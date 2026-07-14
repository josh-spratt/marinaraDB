use std::cmp::Ordering;
use crate::parser::ast::{BinOp, Expr};
use crate::{ColType, ColumnDef, Scottatura, Value};

// Currently exercised only by tests below; intended for Task 5 executor use.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tri {
    True,
    False,
    Unknown,
}

impl Tri {
    #[allow(dead_code)]
    pub(crate) fn is_true(&self) -> bool {
        matches!(self, Tri::True)
    }

    #[allow(dead_code)]
    fn from_bool(b: bool) -> Tri {
        if b { Tri::True } else { Tri::False }
    }

    #[allow(dead_code)]
    fn not(&self) -> Tri {
        match self {
            Tri::True => Tri::False,
            Tri::False => Tri::True,
            Tri::Unknown => Tri::Unknown,
        }
    }

    #[allow(dead_code)]
    fn and(&self, other: Tri) -> Tri {
        match (self, other) {
            (Tri::False, _) | (_, Tri::False) => Tri::False,
            (Tri::True, Tri::True) => Tri::True,
            _ => Tri::Unknown,
        }
    }

    #[allow(dead_code)]
    fn or(&self, other: Tri) -> Tri {
        match (self, other) {
            (Tri::True, _) | (_, Tri::True) => Tri::True,
            (Tri::False, Tri::False) => Tri::False,
            _ => Tri::Unknown,
        }
    }
}

// Intended for Task 5 executor use.
#[allow(dead_code)]
pub(crate) fn full_schema(cols: &[ColumnDef]) -> Vec<ColumnDef> {
    let mut out = vec![ColumnDef {
        name: "id".to_string(),
        col_type: ColType::Integer,
        nullable: false,
        strainer_root: None,
    }];
    out.extend(cols.iter().cloned());
    out
}

// Intended for Task 5 executor use.
#[allow(dead_code)]
pub(crate) fn full_row(key: i64, values: &[Value]) -> Vec<Value> {
    let mut out = vec![Value::Integer(key)];
    out.extend(values.iter().cloned());
    out
}

#[allow(dead_code)]
fn column_value(name: &str, row: &[Value], schema: &[ColumnDef]) -> Result<Value, Scottatura> {
    schema.iter().position(|c| c.name == name)
        .map(|i| row[i].clone())
        .ok_or_else(|| Scottatura::ColumnNotFound { col: name.to_string(), name: "?".to_string() })
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

// Intended for Task 5 executor use.
#[allow(dead_code)]
pub(crate) fn eval_scalar(expr: &Expr, row: &[Value], schema: &[ColumnDef]) -> Result<Value, Scottatura> {
    match expr {
        Expr::Literal(v) => Ok(v.clone()),
        Expr::Column { name, .. } => column_value(name, row, schema),
        Expr::BinaryOp { op: op @ (BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div), left, right } => {
            let l = eval_scalar(left, row, schema)?;
            let r = eval_scalar(right, row, schema)?;
            eval_arith(op, l, r)
        }
        Expr::FunctionCall { .. } => Err(Scottatura::ClauseNotYetSupported { clause: "aggregate functions".to_string() }),
        _ => Err(Scottatura::ClauseNotYetSupported { clause: "boolean expression in value position".to_string() }),
    }
}

// Intended for Task 5 executor use.
#[allow(dead_code)]
pub(crate) fn eval_tri(expr: &Expr, row: &[Value], schema: &[ColumnDef]) -> Result<Tri, Scottatura> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_statement;
    use crate::parser::ast::Statement;

    fn schema() -> Vec<ColumnDef> {
        vec![
            ColumnDef { name: "id".into(), col_type: ColType::Integer, nullable: false, strainer_root: None },
            ColumnDef { name: "name".into(), col_type: ColType::Text, nullable: true, strainer_root: None },
            ColumnDef { name: "age".into(), col_type: ColType::Integer, nullable: true, strainer_root: None },
        ]
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
}
