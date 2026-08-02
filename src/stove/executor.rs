use std::cmp::Ordering;
use crate::parser::ast::{BinOp, Expr, TableRef};
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

/// Compares two projected values for ORDER BY. VUOTO always sorts first,
/// regardless of ASC/DESC — only non-VUOTO pairs are affected by direction.
/// A type mismatch between two values in the same projected column can't
/// happen for a column sourced from one declared type; if it somehow did
/// (e.g. via a REAL NaN), treat the pair as equal rather than propagating
/// an error through a non-fallible sort comparator.
fn compare_vuoto_first(a: &Value, b: &Value, descending: bool) -> Ordering {
    match (matches!(a, Value::Null), matches!(b, Value::Null)) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => {
            let ord = compare_values(a, b).unwrap_or(Ordering::Equal);
            if descending { ord.reverse() } else { ord }
        }
    }
}

use crate::parser::ast::OrderByItem;

/// Sorts projected rows in place. `order_by` columns must appear in
/// `out_names` (the projected output) — ORDER BY on a column that wasn't
/// selected/aliased is a ColumnNotFound error, not a fallback to the
/// underlying table schema.
fn sort_rows(rows: &mut [Vec<Value>], order_by: &[OrderByItem], out_names: &[String]) -> Result<(), Scottatura> {
    let mut positions = Vec::with_capacity(order_by.len());
    for item in order_by {
        let matches: Vec<usize> = out_names.iter().enumerate()
            .filter(|(_, n)| *n == &item.column)
            .map(|(i, _)| i)
            .collect();
        let idx = match matches.as_slice() {
            [] => return Err(Scottatura::ColumnNotFound { col: item.column.clone(), name: "<projection>".to_string() }),
            [i] => *i,
            _ => return Err(Scottatura::AmbiguousColumn { name: item.column.clone() }),
        };
        positions.push((idx, item.descending));
    }
    rows.sort_by(|a, b| {
        for &(idx, desc) in &positions {
            let ord = compare_vuoto_first(&a[idx], &b[idx], desc);
            if ord != Ordering::Equal {
                return ord;
            }
        }
        Ordering::Equal
    });
    Ok(())
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
use crate::sous_chef::{JoinPlan, Plan};
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

fn item_has_aggregate(item: &SelectItem) -> bool {
    matches!(item, SelectItem::Expr { expr: Expr::FunctionCall { .. }, .. })
}

type Groups = Vec<(Vec<Value>, Vec<Vec<Value>>)>;

/// Buckets rows by their `group_by` column values. Empty `group_by` means
/// exactly one implicit group covering all rows (even zero rows) -- this
/// is what makes `SELECT COUNT(*) FROM t` with no matches yield one row
/// with `COUNT(*) = 0` rather than an empty result set.
fn build_groups(group_by: &[String], schema: &Schema, rows: Vec<Vec<Value>>) -> Result<Groups, Scottatura> {
    if group_by.is_empty() {
        return Ok(vec![(vec![], rows)]);
    }
    let mut groups: Groups = Vec::new();
    for row in rows {
        let key: Vec<Value> = group_by.iter()
            .map(|name| column_value(None, name, &row, schema))
            .collect::<Result<_, _>>()?;
        if let Some((_, bucket)) = groups.iter_mut().find(|(k, _)| *k == key) {
            bucket.push(row);
        } else {
            groups.push((key, vec![row]));
        }
    }
    Ok(groups)
}

fn eval_aggregate(name: &str, arg: Option<&Expr>, group_rows: &[Vec<Value>], schema: &Schema) -> Result<Value, Scottatura> {
    match name.to_uppercase().as_str() {
        "COUNT" => match arg {
            None => Ok(Value::Integer(group_rows.len() as i64)),
            Some(e) => {
                let mut n = 0i64;
                for row in group_rows {
                    if !matches!(eval_scalar(e, row, schema)?, Value::Null) {
                        n += 1;
                    }
                }
                Ok(Value::Integer(n))
            }
        },
        "SUM" | "AVG" => {
            let e = arg.ok_or_else(|| Scottatura::UnknownAggregateFunction { name: name.to_string() })?;
            let (mut sum, mut count, mut is_int) = (0f64, 0i64, true);
            for row in group_rows {
                match eval_scalar(e, row, schema)? {
                    Value::Null => {}
                    Value::Integer(i) => { sum += i as f64; count += 1; }
                    Value::Real(r) => { sum += r; count += 1; is_int = false; }
                    other => return Err(Scottatura::TypeMismatch {
                        got: format!("{other:?}"), col: "<aggregate>".to_string(), expected: "INTEGER or REAL".to_string(),
                    }),
                }
            }
            if count == 0 {
                return Ok(Value::Null);
            }
            if name.eq_ignore_ascii_case("SUM") {
                Ok(if is_int { Value::Integer(sum as i64) } else { Value::Real(sum) })
            } else {
                Ok(Value::Real(sum / count as f64))
            }
        }
        "MIN" | "MAX" => {
            let e = arg.ok_or_else(|| Scottatura::UnknownAggregateFunction { name: name.to_string() })?;
            let mut best: Option<Value> = None;
            for row in group_rows {
                let v = eval_scalar(e, row, schema)?;
                if matches!(v, Value::Null) {
                    continue;
                }
                best = Some(match best {
                    None => v,
                    Some(b) => {
                        let ord = compare_values(&v, &b)?;
                        let want_v = (name.eq_ignore_ascii_case("MIN") && ord == Ordering::Less)
                            || (name.eq_ignore_ascii_case("MAX") && ord == Ordering::Greater);
                        if want_v { v } else { b }
                    }
                });
            }
            Ok(best.unwrap_or(Value::Null))
        }
        other => Err(Scottatura::UnknownAggregateFunction { name: other.to_string() }),
    }
}

/// Structurally checks whether `qualifier`/`name` resolve unambiguously
/// against `schema`, independent of any particular row -- used for the
/// zero-row group case where there's no row to resolve a qualified column
/// value against, but the qualifier still needs validating.
fn column_resolves(qualifier: Option<&str>, name: &str, schema: &Schema) -> Result<(), Scottatura> {
    let matches = schema.iter()
        .filter(|sc| sc.col.name == name && qualifier.is_none_or(|q| sc.owner == q))
        .count();
    match matches {
        0 => Err(Scottatura::ColumnNotFound { col: name.to_string(), name: qualifier.unwrap_or("?").to_string() }),
        1 => Ok(()),
        _ => Err(Scottatura::AmbiguousColumn { name: name.to_string() }),
    }
}

fn project_grouped(items: &[SelectItem], group_by: &[String], schema: &Schema, key: &[Value], rows: &[Vec<Value>]) -> Result<Vec<Value>, Scottatura> {
    let mut out = Vec::new();
    for item in items {
        match item {
            SelectItem::Expr { expr: Expr::FunctionCall { name, arg }, .. } => {
                out.push(eval_aggregate(name, arg.as_deref(), rows, schema)?);
            }
            SelectItem::Expr { expr: Expr::Column { qualifier: None, name }, .. } => {
                // Unqualified name: resolve via the group_by index shortcut, as
                // before. This is safe even under a join because `build_groups`
                // already resolved this same bare name unambiguously via
                // `column_value(None, name, &row, schema)` when building the key.
                let idx = group_by.iter().position(|g| g == name)
                    .ok_or_else(|| Scottatura::ColumnNotGroupedOrAggregated { name: name.clone() })?;
                out.push(key[idx].clone());
            }
            SelectItem::Expr { expr: Expr::Column { qualifier: Some(q), name }, .. } => {
                // Qualified name: validate the qualifier against the schema
                // before falling back to the group_by-index shortcut, so a
                // wrong/nonexistent qualifier (or one that only matches a
                // column outside the group-by list) is caught rather than
                // silently ignored.
                match rows.first() {
                    Some(row) => out.push(column_value(Some(q.as_str()), name, row, schema)?),
                    None => {
                        column_resolves(Some(q.as_str()), name, schema)?;
                        let idx = group_by.iter().position(|g| g == name)
                            .ok_or_else(|| Scottatura::ColumnNotGroupedOrAggregated { name: name.clone() })?;
                        out.push(key[idx].clone());
                    }
                }
            }
            // sous_chef::validate_aggregate_context already rejects every other shape
            // (Star, arbitrary expressions) before this code runs in aggregate context.
            _ => return Err(Scottatura::ColumnNotGroupedOrAggregated { name: "<expression>".to_string() }),
        }
    }
    Ok(out)
}

/// Nested-loop inner equi-join. Resolves both join-condition columns
/// against the *combined* row/schema (not each side separately) so that
/// `ON a.x = b.y` and `ON b.y = a.x` both work regardless of which side
/// `left_col`/`right_col` names, and so qualifier ambiguity is caught the
/// same way it is for any other column reference.
fn scan_join(pot: &mut Pot, from: &TableRef, j: &JoinPlan) -> Result<(Schema, Vec<Vec<Value>>), Scottatura> {
    let left_recipe = pot.recipe(&from.name)?;
    let left_owner = from.alias.clone().unwrap_or_else(|| from.name.clone());
    let left_schema = full_schema(&left_owner, &left_recipe.columns);
    let left_rows: Vec<Vec<Value>> = pot.scan_noodle(&from.name)?
        .into_iter().map(|(k, v)| full_row(k, &v)).collect();

    let right_recipe = pot.recipe(&j.table.name)?;
    let right_owner = j.table.alias.clone().unwrap_or_else(|| j.table.name.clone());
    let right_schema = full_schema(&right_owner, &right_recipe.columns);
    let right_rows: Vec<Vec<Value>> = pot.scan_noodle(&j.table.name)?
        .into_iter().map(|(k, v)| full_row(k, &v)).collect();

    let mut schema = left_schema;
    schema.extend(right_schema);

    let mut out = Vec::new();
    for lrow in &left_rows {
        for rrow in &right_rows {
            let mut combined = lrow.clone();
            combined.extend(rrow.iter().cloned());
            let lv = column_value(j.left_col.0.as_deref(), &j.left_col.1, &combined, &schema)?;
            let rv = column_value(j.right_col.0.as_deref(), &j.right_col.1, &combined, &schema)?;
            if matches!(lv, Value::Null) || matches!(rv, Value::Null) {
                continue;
            }
            if values_equal(&lv, &rv)? {
                out.push(combined);
            }
        }
    }
    Ok((schema, out))
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
        Plan::Select { from, join, filter, group_by, columns, order_by, limit, offset } => {
            let (schema, rows): (Schema, Vec<Vec<Value>>) = if let Some(j) = &join {
                scan_join(pot, &from, j)?
            } else {
                let recipe = pot.recipe(&from.name)?;
                let owner = from.alias.clone().unwrap_or_else(|| from.name.clone());
                let s = full_schema(&owner, &recipe.columns);
                let r = pot.scan_noodle(&from.name)?.into_iter().map(|(k, v)| full_row(k, &v)).collect();
                (s, r)
            };
            let mut filtered = Vec::new();
            for row in rows {
                if let Some(f) = &filter {
                    if !eval_tri(f, &row, &schema)?.is_true() {
                        continue;
                    }
                }
                filtered.push(row);
            }
            let out_names = project_names(&columns, &schema);
            let is_aggregate_context = !group_by.is_empty() || columns.iter().any(item_has_aggregate);
            let mut out_rows = if is_aggregate_context {
                let groups = build_groups(&group_by, &schema, filtered)?;
                groups.iter()
                    .map(|(key, rows)| project_grouped(&columns, &group_by, &schema, key, rows))
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                filtered.iter()
                    .map(|row| project_values(&columns, &schema, row))
                    .collect::<Result<Vec<_>, _>>()?
            };
            if !order_by.is_empty() {
                sort_rows(&mut out_rows, &order_by, &out_names)?;
            }
            let skip_n = offset.unwrap_or(0).max(0) as usize;
            out_rows = out_rows.into_iter().skip(skip_n).collect();
            if let Some(l) = limit {
                out_rows.truncate(l.max(0) as usize);
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
