use crate::parser::ast::{ColumnSpec, Expr, JoinClause, OrderByItem, SelectItem, Statement, TableRef};
use crate::Scottatura;

#[derive(Debug, Clone)]
pub struct JoinPlan {
    pub table: TableRef,
    pub left_col: (Option<String>, String),
    pub right_col: (Option<String>, String),
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)] // Select is inherently larger; boxing would ripple indirection into every match site
pub enum Plan {
    Select {
        from: TableRef,
        join: Option<JoinPlan>,
        filter: Option<Expr>,
        group_by: Vec<String>,
        columns: Vec<SelectItem>,
        order_by: Vec<OrderByItem>,
        limit: Option<i64>,
        offset: Option<i64>,
    },
    Insert { table: String, columns: Vec<String>, rows: Vec<Vec<Expr>> },
    Update { table: String, assignments: Vec<(String, Expr)>, filter: Expr },
    Delete { table: String, filter: Expr },
    CreateTable { name: String, columns: Vec<ColumnSpec> },
    DropTable { name: String },
}

fn item_is_aggregate(item: &SelectItem) -> bool {
    matches!(item, SelectItem::Expr { expr: Expr::FunctionCall { .. }, .. })
}

fn validate_aggregate_context(sel: &crate::parser::ast::SelectStatement) -> Result<(), Scottatura> {
    let is_aggregate_context = !sel.group_by.is_empty() || sel.items.iter().any(item_is_aggregate);
    if !is_aggregate_context {
        return Ok(());
    }
    for item in &sel.items {
        match item {
            SelectItem::Star => {
                return Err(Scottatura::ClauseNotYetSupported { clause: "* with BUNDLED BY/aggregates".to_string() });
            }
            SelectItem::Expr { expr: Expr::FunctionCall { name, .. }, .. } => {
                if !matches!(name.to_uppercase().as_str(), "COUNT" | "SUM" | "AVG" | "MIN" | "MAX") {
                    return Err(Scottatura::UnknownAggregateFunction { name: name.clone() });
                }
            }
            SelectItem::Expr { expr: Expr::Column { name, .. }, .. } => {
                if !sel.group_by.iter().any(|g| g == name) {
                    return Err(Scottatura::ColumnNotGroupedOrAggregated { name: name.clone() });
                }
            }
            SelectItem::Expr { .. } => {
                return Err(Scottatura::ColumnNotGroupedOrAggregated { name: "<expression>".to_string() });
            }
        }
    }
    Ok(())
}

pub fn plan(stmt: Statement) -> Result<Plan, Scottatura> {
    match stmt {
        Statement::Select(sel) => {
            validate_aggregate_context(&sel)?;
            let join = sel.join.map(|j: JoinClause| JoinPlan {
                table: j.table,
                left_col: j.left_col,
                right_col: j.right_col,
            });
            Ok(Plan::Select {
                from: sel.from,
                join,
                filter: sel.filter,
                group_by: sel.group_by,
                columns: sel.items,
                order_by: sel.order_by,
                limit: sel.limit,
                offset: sel.offset,
            })
        }
        Statement::Insert { table, columns, rows } => Ok(Plan::Insert { table, columns, rows }),
        Statement::Update { table, assignments, filter } => Ok(Plan::Update { table, assignments, filter }),
        Statement::Delete { table, filter } => Ok(Plan::Delete { table, filter }),
        Statement::CreateTable { name, columns } => Ok(Plan::CreateTable { name, columns }),
        Statement::DropTable { name } => Ok(Plan::DropTable { name }),
        Statement::CreateIndex { .. } | Statement::DropIndex { .. } => Err(Scottatura::StrainerNotSupported),
        Statement::BeginSimmer | Statement::FinishSimmer | Statement::SpillSimmer => {
            Err(Scottatura::TransactionNotSupported)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_statement;

    #[test]
    fn test_select_becomes_select_plan() {
        let stmt = parse_statement("SERVE * FROM users STRAINED BY age > 18;").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::Select { from, .. } if from.name == "users"));
    }

    #[test]
    fn test_select_without_filter() {
        let stmt = parse_statement("SERVE * FROM users;").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::Select { filter: None, .. }));
    }

    #[test]
    fn test_join_passthrough() {
        let stmt = parse_statement("SERVE * FROM users u TOSS orders o ON u.id = o.user_id;").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::Select { join: Some(j), .. } if j.table.name == "orders"));
    }

    #[test]
    fn test_order_by_passthrough() {
        let stmt = parse_statement("SERVE * FROM users ARRANGED BY age;").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::Select { order_by, .. } if order_by.len() == 1 && order_by[0].column == "age" && !order_by[0].descending));
    }

    #[test]
    fn test_limit_offset_passthrough() {
        let stmt = parse_statement("SERVE * FROM users ONLY 10 SKIP 5;").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::Select { limit: Some(10), offset: Some(5), .. }));
    }

    #[test]
    fn test_insert_passthrough() {
        let stmt = parse_statement("LADLE INTO users (name) PORTIONS ('Tony');").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::Insert { table, columns, .. } if table == "users" && columns == vec!["name".to_string()]));
    }

    #[test]
    fn test_update_passthrough() {
        let stmt = parse_statement("SEASON users SET age = 39 STRAINED BY name = 'Tony';").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::Update { table, .. } if table == "users"));
    }

    #[test]
    fn test_delete_passthrough() {
        let stmt = parse_statement("DISCARD FROM users STRAINED BY age < 18;").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::Delete { table, .. } if table == "users"));
    }

    #[test]
    fn test_create_table_passthrough() {
        let stmt = parse_statement("PLATE noodle t (name TEXT);").unwrap();
        assert!(matches!(plan(stmt).unwrap(), Plan::CreateTable { name, .. } if name == "t"));
    }

    #[test]
    fn test_drop_table_passthrough() {
        let stmt = parse_statement("TOSS OUT noodle t;").unwrap();
        assert!(matches!(plan(stmt).unwrap(), Plan::DropTable { name } if name == "t"));
    }

    #[test]
    fn test_create_index_rejected() {
        let stmt = parse_statement("STRAIN idx1 ON users (age);").unwrap();
        assert!(matches!(plan(stmt).unwrap_err(), Scottatura::StrainerNotSupported));
    }

    #[test]
    fn test_drop_index_rejected() {
        let stmt = parse_statement("REMOVE STRAINER idx1;").unwrap();
        assert!(matches!(plan(stmt).unwrap_err(), Scottatura::StrainerNotSupported));
    }

    #[test]
    fn test_begin_simmer_rejected() {
        let stmt = parse_statement("BEGIN SIMMER;").unwrap();
        assert!(matches!(plan(stmt).unwrap_err(), Scottatura::TransactionNotSupported));
    }

    #[test]
    fn test_group_by_passthrough() {
        let stmt = parse_statement("SERVE age, COUNT(*) FROM users BUNDLED BY age;").unwrap();
        let p = plan(stmt).unwrap();
        assert!(matches!(p, Plan::Select { group_by, .. } if group_by == vec!["age".to_string()]));
    }

    #[test]
    fn test_star_with_group_by_rejected_at_plan_time() {
        let stmt = parse_statement("SERVE * FROM users BUNDLED BY age;").unwrap();
        assert!(matches!(plan(stmt).unwrap_err(), Scottatura::ClauseNotYetSupported { .. }));
    }
}
