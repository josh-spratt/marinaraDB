use crate::{ColType, Value};

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)] // SelectStatement is inherently larger; boxing would ripple indirection into every parse/match site for no measurable benefit in this toy DB
pub enum Statement {
    CreateTable { name: String, columns: Vec<ColumnSpec> },
    DropTable { name: String },
    CreateIndex { index_name: String, table: String, column: String },
    DropIndex { index_name: String },
    Select(SelectStatement),
    Insert { table: String, columns: Vec<String>, rows: Vec<Vec<Expr>> },
    Update { table: String, assignments: Vec<(String, Expr)>, filter: Expr },
    Delete { table: String, filter: Expr },
    BeginSimmer,
    FinishSimmer,
    SpillSimmer,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnSpec {
    pub name: String,
    pub col_type: ColType,
    pub not_null: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectStatement {
    pub items: Vec<SelectItem>,
    pub from: TableRef,
    pub join: Option<JoinClause>,
    pub filter: Option<Expr>,
    pub group_by: Vec<String>,
    pub order_by: Vec<OrderByItem>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SelectItem {
    Star,
    Expr { expr: Expr, alias: Option<String> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableRef {
    pub name: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JoinClause {
    pub table: TableRef,
    pub left_col: (Option<String>, String),
    pub right_col: (Option<String>, String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderByItem {
    pub column: String,
    pub descending: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Value),
    Column { qualifier: Option<String>, name: String },
    BinaryOp { op: BinOp, left: Box<Expr>, right: Box<Expr> },
    UnaryNot(Box<Expr>),
    IsNull { expr: Box<Expr>, negated: bool },
    Between { expr: Box<Expr>, low: Box<Expr>, high: Box<Expr> },
    Like { expr: Box<Expr>, pattern: Box<Expr> },
    In { expr: Box<Expr>, list: Vec<Expr> },
    FunctionCall { name: String, arg: Option<Box<Expr>> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add, Sub, Mul, Div,
    Eq, Neq, Lt, Gt, Lte, Gte,
    And, Or,
}
