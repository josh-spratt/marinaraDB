pub mod ast;
pub mod lexer;

use ast::{BinOp, ColumnSpec, Expr, JoinClause, OrderByItem, SelectItem, SelectStatement, Statement, TableRef};
use lexer::Token;
use crate::{Scottatura, Value};

pub struct Parser {
    tokens: Vec<(Token, usize)>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<(Token, usize)>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos].0
    }

    fn peek_pos(&self) -> usize {
        self.tokens[self.pos].1
    }

    /// Peeks `offset` tokens ahead of the current position, clamped to the last token (Eof).
    fn peek_at(&self, offset: usize) -> &Token {
        let idx = (self.pos + offset).min(self.tokens.len() - 1);
        &self.tokens[idx].0
    }

    fn advance(&mut self) -> Token {
        let t = self.tokens[self.pos].0.clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn check(&self, t: &Token) -> bool {
        self.peek() == t
    }

    fn expect(&mut self, t: Token) -> Result<(), Scottatura> {
        if self.check(&t) {
            self.advance();
            Ok(())
        } else {
            Err(Scottatura::UnexpectedToken { token: format!("{:?}", self.peek()), pos: self.peek_pos() })
        }
    }

    fn expect_ident(&mut self) -> Result<String, Scottatura> {
        match self.peek().clone() {
            Token::Ident(s) => { self.advance(); Ok(s) }
            other => Err(Scottatura::UnexpectedToken { token: format!("{other:?}"), pos: self.peek_pos() }),
        }
    }

    fn parse_int_literal(&mut self) -> Result<i64, Scottatura> {
        match self.peek().clone() {
            Token::IntLiteral(n) => { self.advance(); Ok(n) }
            other => Err(Scottatura::UnexpectedToken { token: format!("{other:?}"), pos: self.peek_pos() }),
        }
    }

    pub fn parse_expr(&mut self) -> Result<Expr, Scottatura> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, Scottatura> {
        let mut left = self.parse_and()?;
        while self.check(&Token::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::BinaryOp { op: BinOp::Or, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, Scottatura> {
        let mut left = self.parse_not()?;
        while self.check(&Token::And) {
            self.advance();
            let right = self.parse_not()?;
            left = Expr::BinaryOp { op: BinOp::And, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<Expr, Scottatura> {
        if self.check(&Token::Not) {
            self.advance();
            let inner = self.parse_not()?;
            return Ok(Expr::UnaryNot(Box::new(inner)));
        }
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> Result<Expr, Scottatura> {
        let left = self.parse_additive()?;

        if self.check(&Token::Is) {
            self.advance();
            let negated = if self.check(&Token::Not) { self.advance(); true } else { false };
            self.expect(Token::Vuoto)?;
            return Ok(Expr::IsNull { expr: Box::new(left), negated });
        }
        if self.check(&Token::Between) {
            self.advance();
            let low = self.parse_additive()?;
            self.expect(Token::And)?;
            let high = self.parse_additive()?;
            return Ok(Expr::Between { expr: Box::new(left), low: Box::new(low), high: Box::new(high) });
        }
        if self.check(&Token::Like) {
            self.advance();
            let pattern = self.parse_additive()?;
            return Ok(Expr::Like { expr: Box::new(left), pattern: Box::new(pattern) });
        }
        if self.check(&Token::In) {
            self.advance();
            self.expect(Token::LParen)?;
            let mut list = Vec::new();
            loop {
                list.push(self.parse_additive()?);
                if self.check(&Token::Comma) { self.advance(); continue; }
                break;
            }
            self.expect(Token::RParen)?;
            return Ok(Expr::In { expr: Box::new(left), list });
        }

        let op = match self.peek() {
            Token::Eq => BinOp::Eq,
            Token::Neq => BinOp::Neq,
            Token::Lt => BinOp::Lt,
            Token::Gt => BinOp::Gt,
            Token::Lte => BinOp::Lte,
            Token::Gte => BinOp::Gte,
            _ => return Ok(left),
        };
        self.advance();
        let right = self.parse_additive()?;
        Ok(Expr::BinaryOp { op, left: Box::new(left), right: Box::new(right) })
    }

    fn parse_additive(&mut self) -> Result<Expr, Scottatura> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            left = Expr::BinaryOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, Scottatura> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::BinaryOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, Scottatura> {
        if self.check(&Token::Minus) {
            self.advance();
            let inner = self.parse_unary()?;
            return Ok(Expr::BinaryOp { op: BinOp::Sub, left: Box::new(Expr::Literal(Value::Integer(0))), right: Box::new(inner) });
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expr, Scottatura> {
        match self.peek().clone() {
            Token::IntLiteral(n) => { self.advance(); Ok(Expr::Literal(Value::Integer(n))) }
            Token::RealLiteral(f) => { self.advance(); Ok(Expr::Literal(Value::Real(f))) }
            Token::StringLiteral(s) => { self.advance(); Ok(Expr::Literal(Value::Text(s))) }
            Token::Vuoto => { self.advance(); Ok(Expr::Literal(Value::Null)) }
            Token::LParen => {
                self.advance();
                if self.check(&Token::Serve) {
                    return Err(Scottatura::SubServeNotSupported);
                }
                let inner = self.parse_expr()?;
                self.expect(Token::RParen)?;
                Ok(inner)
            }
            Token::Ident(name) => {
                self.advance();
                if self.check(&Token::LParen) && is_aggregate_fn(&name) {
                    self.advance();
                    let arg = if self.check(&Token::Star) {
                        self.advance();
                        None
                    } else {
                        Some(Box::new(self.parse_expr()?))
                    };
                    self.expect(Token::RParen)?;
                    return Ok(Expr::FunctionCall { name: name.to_ascii_uppercase(), arg });
                }
                if self.check(&Token::Dot) {
                    self.advance();
                    let col = self.expect_ident()?;
                    return Ok(Expr::Column { qualifier: Some(name), name: col });
                }
                Ok(Expr::Column { qualifier: None, name })
            }
            other => Err(Scottatura::UnexpectedToken { token: format!("{other:?}"), pos: self.peek_pos() }),
        }
    }

    pub fn parse_stmt(&mut self) -> Result<Statement, Scottatura> {
        match self.peek().clone() {
            Token::Plate => self.parse_create_table(),
            Token::Toss => self.parse_drop_table(),
            Token::Strain => self.parse_create_index(),
            Token::Remove => self.parse_drop_index(),
            Token::Serve => self.parse_select().map(Statement::Select),
            Token::Ladle => self.parse_insert(),
            Token::Season => self.parse_update(),
            Token::Discard => self.parse_delete(),
            Token::Begin => { self.advance(); self.expect(Token::Simmer)?; Ok(Statement::BeginSimmer) }
            Token::Finish => { self.advance(); self.expect(Token::Simmer)?; Ok(Statement::FinishSimmer) }
            Token::Spill => { self.advance(); self.expect(Token::Simmer)?; Ok(Statement::SpillSimmer) }
            _ => Err(Scottatura::StatementNotSupported),
        }
    }

    fn parse_insert(&mut self) -> Result<Statement, Scottatura> {
        self.expect(Token::Ladle)?;
        self.expect(Token::Into)?;
        let table = self.expect_ident()?;
        self.expect(Token::LParen)?;
        let mut columns = Vec::new();
        loop {
            let col = self.expect_ident()?;
            if col == "id" {
                return Err(Scottatura::ReservedIdColumn);
            }
            columns.push(col);
            if self.check(&Token::Comma) { self.advance(); continue; }
            break;
        }
        self.expect(Token::RParen)?;
        self.expect(Token::Portions)?;

        let mut rows = Vec::new();
        loop {
            self.expect(Token::LParen)?;
            let mut row = Vec::new();
            loop {
                row.push(self.parse_expr()?);
                if self.check(&Token::Comma) { self.advance(); continue; }
                break;
            }
            self.expect(Token::RParen)?;
            if row.len() != columns.len() {
                return Err(Scottatura::ValueCountMismatch { got: row.len(), expected: columns.len() });
            }
            rows.push(row);
            if self.check(&Token::Comma) { self.advance(); continue; }
            break;
        }
        Ok(Statement::Insert { table, columns, rows })
    }

    fn parse_update(&mut self) -> Result<Statement, Scottatura> {
        self.expect(Token::Season)?;
        let table = self.expect_ident()?;
        self.expect(Token::Set)?;
        let mut assignments = Vec::new();
        loop {
            let col = self.expect_ident()?;
            self.expect(Token::Eq)?;
            let value = self.parse_expr()?;
            assignments.push((col, value));
            if self.check(&Token::Comma) { self.advance(); continue; }
            break;
        }
        if !self.check(&Token::Strained) {
            return Err(Scottatura::SeasonRequiresFilter);
        }
        self.advance();
        self.expect(Token::By)?;
        let filter = self.parse_expr()?;
        Ok(Statement::Update { table, assignments, filter })
    }

    fn parse_delete(&mut self) -> Result<Statement, Scottatura> {
        self.expect(Token::Discard)?;
        self.expect(Token::From)?;
        let table = self.expect_ident()?;
        if !self.check(&Token::Strained) {
            return Err(Scottatura::DiscardRequiresFilter);
        }
        self.advance();
        self.expect(Token::By)?;
        let filter = self.parse_expr()?;
        Ok(Statement::Delete { table, filter })
    }

    fn parse_create_table(&mut self) -> Result<Statement, Scottatura> {
        self.expect(Token::Plate)?;
        self.expect(Token::Noodle)?;
        let name = self.expect_ident()?;
        self.expect(Token::LParen)?;
        let mut columns = Vec::new();
        loop {
            let col_name = self.expect_ident()?;
            if col_name == "id" {
                return Err(Scottatura::ReservedIdColumn);
            }
            let type_name = self.expect_ident()?;
            let col_type = match type_name.to_ascii_uppercase().as_str() {
                "INTEGER" | "INT" | "BIGINT" => crate::ColType::Integer,
                "REAL" | "FLOAT" => crate::ColType::Real,
                "TEXT" => crate::ColType::Text,
                "BLOB" => crate::ColType::Blob,
                _ => return Err(Scottatura::UnknownType { type_name }),
            };
            let mut not_null = false;
            if self.check(&Token::Not) {
                self.advance();
                self.expect(Token::Vuoto)?;
                not_null = true;
            }
            columns.push(ColumnSpec { name: col_name, col_type, not_null });
            if self.check(&Token::Comma) { self.advance(); continue; }
            break;
        }
        self.expect(Token::RParen)?;
        Ok(Statement::CreateTable { name, columns })
    }

    fn parse_drop_table(&mut self) -> Result<Statement, Scottatura> {
        self.expect(Token::Toss)?;
        self.expect(Token::Out)?;
        self.expect(Token::Noodle)?;
        let name = self.expect_ident()?;
        Ok(Statement::DropTable { name })
    }

    fn parse_create_index(&mut self) -> Result<Statement, Scottatura> {
        self.expect(Token::Strain)?;
        let index_name = self.expect_ident()?;
        self.expect(Token::On)?;
        let table = self.expect_ident()?;
        self.expect(Token::LParen)?;
        let column = self.expect_ident()?;
        if self.check(&Token::Comma) {
            return Err(Scottatura::MultiColumnStrainerNotSupported);
        }
        self.expect(Token::RParen)?;
        Ok(Statement::CreateIndex { index_name, table, column })
    }

    fn parse_drop_index(&mut self) -> Result<Statement, Scottatura> {
        self.expect(Token::Remove)?;
        self.expect(Token::Strainer)?;
        let index_name = self.expect_ident()?;
        Ok(Statement::DropIndex { index_name })
    }

    fn parse_select(&mut self) -> Result<SelectStatement, Scottatura> {
        self.expect(Token::Serve)?;
        let items = self.parse_select_items()?;
        self.expect(Token::From)?;
        let from = self.parse_table_ref()?;

        if let Token::Ident(w) = self.peek().clone() {
            if is_outer_join_modifier(&w) {
                return Err(Scottatura::OuterTossNotSupported);
            }
        }

        let mut join = None;
        if self.check(&Token::Toss) {
            self.advance();
            let table = self.parse_table_ref()?;
            self.expect(Token::On)?;
            let left_col = self.parse_qualified_column()?;
            if !self.check(&Token::Eq) {
                return Err(Scottatura::NonEquiJoinNotSupported);
            }
            self.advance();
            let right_col = self.parse_qualified_column()?;
            join = Some(JoinClause { table, left_col, right_col });
            if self.check(&Token::Toss) {
                return Err(Scottatura::MultipleTossNotSupported);
            }
        }

        let filter = if self.check(&Token::Strained) {
            self.advance();
            self.expect(Token::By)?;
            Some(self.parse_expr()?)
        } else {
            None
        };

        let mut group_by = Vec::new();
        if self.check(&Token::Bundled) {
            self.advance();
            self.expect(Token::By)?;
            loop {
                group_by.push(self.expect_ident()?);
                if self.check(&Token::Comma) { self.advance(); continue; }
                break;
            }
        }

        if matches!(self.peek(), Token::Having) {
            return Err(Scottatura::HavingNotSupported);
        }

        let mut order_by = Vec::new();
        if self.check(&Token::Arranged) {
            self.advance();
            self.expect(Token::By)?;
            loop {
                let column = self.expect_ident()?;
                let mut descending = false;
                if self.check(&Token::Desc) { self.advance(); descending = true; }
                else if self.check(&Token::Asc) { self.advance(); }
                order_by.push(OrderByItem { column, descending });
                if self.check(&Token::Comma) { self.advance(); continue; }
                break;
            }
        }

        let mut limit = None;
        let mut offset = None;
        if self.check(&Token::Only) {
            self.advance();
            limit = Some(self.parse_int_literal()?);
            if self.check(&Token::Skip) {
                self.advance();
                offset = Some(self.parse_int_literal()?);
            }
        } else if self.check(&Token::Skip) {
            return Err(Scottatura::SkipWithoutOnly);
        }

        Ok(SelectStatement { items, from, join, filter, group_by, order_by, limit, offset })
    }

    fn parse_select_items(&mut self) -> Result<Vec<SelectItem>, Scottatura> {
        if self.check(&Token::Star) {
            self.advance();
            return Ok(vec![SelectItem::Star]);
        }
        let mut items = Vec::new();
        loop {
            let expr = self.parse_expr()?;
            let alias = if self.check(&Token::As) {
                self.advance();
                Some(self.expect_ident()?)
            } else {
                None
            };
            items.push(SelectItem::Expr { expr, alias });
            if self.check(&Token::Comma) { self.advance(); continue; }
            break;
        }
        Ok(items)
    }

    fn parse_table_ref(&mut self) -> Result<TableRef, Scottatura> {
        let name = self.expect_ident()?;
        let alias = if self.check(&Token::As) {
            self.advance();
            Some(self.expect_ident()?)
        } else if let Token::Ident(a) = self.peek().clone() {
            // Don't swallow `LEFT` / `RIGHT` / `FULL` as an implicit alias when it's
            // actually introducing an (unsupported) outer TOSS — leave it unconsumed
            // so the outer-join check in `parse_select` can see it and reject it.
            if is_outer_join_modifier(&a) && self.peek_at(1) == &Token::Toss {
                None
            } else {
                self.advance();
                Some(a)
            }
        } else {
            None
        };
        Ok(TableRef { name, alias })
    }

    fn parse_qualified_column(&mut self) -> Result<(Option<String>, String), Scottatura> {
        let first = self.expect_ident()?;
        if self.check(&Token::Dot) {
            self.advance();
            let col = self.expect_ident()?;
            Ok((Some(first), col))
        } else {
            Ok((None, first))
        }
    }
}

fn is_aggregate_fn(name: &str) -> bool {
    matches!(name.to_ascii_uppercase().as_str(), "COUNT" | "SUM" | "MIN" | "MAX" | "AVG")
}

fn is_outer_join_modifier(word: &str) -> bool {
    matches!(word.to_ascii_lowercase().as_str(), "left" | "right" | "full")
}

/// Parses exactly one statement from `sql`, tolerating an optional trailing
/// semicolon. Errors if any tokens remain after that (the full token stream
/// must be consumed).
pub fn parse_statement(sql: &str) -> Result<Statement, Scottatura> {
    let tokens = lexer::tokenize(sql)?;
    let mut p = Parser::new(tokens);
    let stmt = p.parse_stmt()?;
    if p.check(&Token::Semicolon) {
        p.advance();
    }
    if !p.check(&Token::Eof) {
        return Err(Scottatura::UnexpectedToken { token: format!("{:?}", p.peek()), pos: p.peek_pos() });
    }
    Ok(stmt)
}

/// Parses multiple semicolon-separated statements until EOF.
pub fn parse_program(sql: &str) -> Result<Vec<Statement>, Scottatura> {
    let tokens = lexer::tokenize(sql)?;
    let mut p = Parser::new(tokens);
    let mut stmts = Vec::new();
    while !p.check(&Token::Eof) {
        stmts.push(p.parse_stmt()?);
        if p.check(&Token::Semicolon) {
            p.advance();
        }
    }
    Ok(stmts)
}

#[cfg(test)]
mod expr_tests {
    use super::*;
    use crate::Value;

    fn parse_expr_str(src: &str) -> Expr {
        let tokens = lexer::tokenize(src).unwrap();
        let mut p = Parser::new(tokens);
        p.parse_expr().unwrap()
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn test_parse_literal() {
        assert_eq!(parse_expr_str("42"), Expr::Literal(Value::Integer(42)));
        assert_eq!(parse_expr_str("3.14"), Expr::Literal(Value::Real(3.14)));
        assert_eq!(parse_expr_str("'hi'"), Expr::Literal(Value::Text("hi".into())));
        assert_eq!(parse_expr_str("VUOTO"), Expr::Literal(Value::Null));
    }

    #[test]
    fn test_parse_column_ref_qualified_and_unqualified() {
        assert_eq!(parse_expr_str("age"), Expr::Column { qualifier: None, name: "age".into() });
        assert_eq!(parse_expr_str("u.age"), Expr::Column { qualifier: Some("u".into()), name: "age".into() });
    }

    #[test]
    fn test_parse_arithmetic_precedence() {
        // 1 + 2 * 3 should parse as 1 + (2 * 3)
        let expr = parse_expr_str("1 + 2 * 3");
        assert_eq!(expr, Expr::BinaryOp {
            op: BinOp::Add,
            left: Box::new(Expr::Literal(Value::Integer(1))),
            right: Box::new(Expr::BinaryOp {
                op: BinOp::Mul,
                left: Box::new(Expr::Literal(Value::Integer(2))),
                right: Box::new(Expr::Literal(Value::Integer(3))),
            }),
        });
    }

    #[test]
    fn test_parse_and_or_precedence() {
        // a = 1 OR b = 2 AND c = 3  should parse as  (a=1) OR ((b=2) AND (c=3))
        let expr = parse_expr_str("a = 1 OR b = 2 AND c = 3");
        match expr {
            Expr::BinaryOp { op: BinOp::Or, right, .. } => {
                assert!(matches!(*right, Expr::BinaryOp { op: BinOp::And, .. }));
            }
            other => panic!("expected top-level OR, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_unary_minus() {
        let expr = parse_expr_str("-5");
        assert_eq!(expr, Expr::BinaryOp {
            op: BinOp::Sub,
            left: Box::new(Expr::Literal(Value::Integer(0))),
            right: Box::new(Expr::Literal(Value::Integer(5))),
        });
    }

    #[test]
    fn test_parse_is_vuoto_and_is_not_vuoto() {
        assert_eq!(parse_expr_str("age IS VUOTO"), Expr::IsNull {
            expr: Box::new(Expr::Column { qualifier: None, name: "age".into() }),
            negated: false,
        });
        assert_eq!(parse_expr_str("age IS NOT VUOTO"), Expr::IsNull {
            expr: Box::new(Expr::Column { qualifier: None, name: "age".into() }),
            negated: true,
        });
    }

    #[test]
    fn test_parse_between() {
        let expr = parse_expr_str("age BETWEEN 18 AND 30");
        assert_eq!(expr, Expr::Between {
            expr: Box::new(Expr::Column { qualifier: None, name: "age".into() }),
            low: Box::new(Expr::Literal(Value::Integer(18))),
            high: Box::new(Expr::Literal(Value::Integer(30))),
        });
    }

    #[test]
    fn test_parse_like() {
        let expr = parse_expr_str("name LIKE 'A%'");
        assert_eq!(expr, Expr::Like {
            expr: Box::new(Expr::Column { qualifier: None, name: "name".into() }),
            pattern: Box::new(Expr::Literal(Value::Text("A%".into()))),
        });
    }

    #[test]
    fn test_parse_in_list() {
        let expr = parse_expr_str("age IN (1, 2, 3)");
        assert_eq!(expr, Expr::In {
            expr: Box::new(Expr::Column { qualifier: None, name: "age".into() }),
            list: vec![
                Expr::Literal(Value::Integer(1)),
                Expr::Literal(Value::Integer(2)),
                Expr::Literal(Value::Integer(3)),
            ],
        });
    }

    #[test]
    fn test_parse_not() {
        let expr = parse_expr_str("NOT age IS VUOTO");
        assert!(matches!(expr, Expr::UnaryNot(_)));
    }

    #[test]
    fn test_parse_parenthesized_expr() {
        // (1 + 2) * 3 should parse as (1+2) * 3, not 1 + (2*3)
        let expr = parse_expr_str("(1 + 2) * 3");
        assert_eq!(expr, Expr::BinaryOp {
            op: BinOp::Mul,
            left: Box::new(Expr::BinaryOp {
                op: BinOp::Add,
                left: Box::new(Expr::Literal(Value::Integer(1))),
                right: Box::new(Expr::Literal(Value::Integer(2))),
            }),
            right: Box::new(Expr::Literal(Value::Integer(3))),
        });
    }

    #[test]
    fn test_parse_aggregate_function_calls() {
        assert_eq!(parse_expr_str("COUNT(*)"), Expr::FunctionCall { name: "COUNT".into(), arg: None });
        assert_eq!(parse_expr_str("SUM(total)"), Expr::FunctionCall {
            name: "SUM".into(),
            arg: Some(Box::new(Expr::Column { qualifier: None, name: "total".into() })),
        });
    }

    #[test]
    fn test_parse_sub_serve_rejected() {
        let tokens = lexer::tokenize("(SERVE * FROM users)").unwrap();
        let mut p = Parser::new(tokens);
        let err = p.parse_expr().unwrap_err();
        assert!(matches!(err, Scottatura::SubServeNotSupported));
    }
}

#[cfg(test)]
mod ddl_tests {
    use super::*;
    use crate::ColType;

    fn parse(src: &str) -> Statement {
        let tokens = lexer::tokenize(src).unwrap();
        let mut p = Parser::new(tokens);
        p.parse_stmt().unwrap()
    }

    fn parse_err(src: &str) -> Scottatura {
        let tokens = lexer::tokenize(src).unwrap();
        let mut p = Parser::new(tokens);
        p.parse_stmt().unwrap_err()
    }

    #[test]
    fn test_parse_create_table() {
        let stmt = parse("PLATE noodle users (name TEXT NOT VUOTO, age INT);");
        assert_eq!(stmt, Statement::CreateTable {
            name: "users".into(),
            columns: vec![
                ColumnSpec { name: "name".into(), col_type: ColType::Text, not_null: true },
                ColumnSpec { name: "age".into(), col_type: ColType::Integer, not_null: false },
            ],
        });
    }

    #[test]
    fn test_parse_create_table_rejects_id_column() {
        let err = parse_err("PLATE noodle users (id INT);");
        assert!(matches!(err, Scottatura::ReservedIdColumn));
    }

    #[test]
    fn test_parse_create_table_rejects_unknown_type() {
        let err = parse_err("PLATE noodle users (name VARCHAR);");
        assert!(matches!(err, Scottatura::UnknownType { .. }));
    }

    #[test]
    fn test_parse_drop_table() {
        let stmt = parse("TOSS OUT noodle users;");
        assert_eq!(stmt, Statement::DropTable { name: "users".into() });
    }

    #[test]
    fn test_parse_create_index() {
        let stmt = parse("STRAIN idx_age ON users (age);");
        assert_eq!(stmt, Statement::CreateIndex {
            index_name: "idx_age".into(),
            table: "users".into(),
            column: "age".into(),
        });
    }

    #[test]
    fn test_parse_create_index_rejects_multi_column() {
        let err = parse_err("STRAIN idx ON users (age, name);");
        assert!(matches!(err, Scottatura::MultiColumnStrainerNotSupported));
    }

    #[test]
    fn test_parse_drop_index() {
        let stmt = parse("REMOVE STRAINER idx_age;");
        assert_eq!(stmt, Statement::DropIndex { index_name: "idx_age".into() });
    }
}

#[cfg(test)]
mod select_tests {
    use super::*;
    use crate::Value;

    fn parse(src: &str) -> Statement {
        let tokens = lexer::tokenize(src).unwrap();
        let mut p = Parser::new(tokens);
        p.parse_stmt().unwrap()
    }

    fn parse_err(src: &str) -> Scottatura {
        let tokens = lexer::tokenize(src).unwrap();
        let mut p = Parser::new(tokens);
        p.parse_stmt().unwrap_err()
    }

    #[test]
    fn test_parse_select_star() {
        let stmt = parse("SERVE * FROM users;");
        match stmt {
            Statement::Select(s) => {
                assert_eq!(s.items, vec![ast::SelectItem::Star]);
                assert_eq!(s.from, ast::TableRef { name: "users".into(), alias: None });
                assert!(s.join.is_none());
                assert!(s.filter.is_none());
            }
            other => panic!("expected Select, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_select_with_strained_by() {
        let stmt = parse("SERVE name, age FROM users STRAINED BY age > 30;");
        match stmt {
            Statement::Select(s) => {
                assert!(s.filter.is_some());
                assert_eq!(s.items.len(), 2);
            }
            other => panic!("expected Select, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_select_with_toss_join() {
        let stmt = parse("SERVE u.name, o.total FROM users u TOSS orders o ON u.id = o.user_id;");
        match stmt {
            Statement::Select(s) => {
                let join = s.join.expect("expected join");
                assert_eq!(join.table, ast::TableRef { name: "orders".into(), alias: Some("o".into()) });
                assert_eq!(join.left_col, (Some("u".into()), "id".into()));
                assert_eq!(join.right_col, (Some("o".into()), "user_id".into()));
            }
            other => panic!("expected Select, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_select_rejects_second_toss() {
        let err = parse_err("SERVE * FROM a TOSS b ON a.id = b.id TOSS c ON a.id = c.id;");
        assert!(matches!(err, Scottatura::MultipleTossNotSupported));
    }

    #[test]
    fn test_parse_select_rejects_outer_toss() {
        let err = parse_err("SERVE * FROM a LEFT TOSS b ON a.id = b.id;");
        assert!(matches!(err, Scottatura::OuterTossNotSupported));
    }

    #[test]
    fn test_parse_select_rejects_non_equi_join() {
        let err = parse_err("SERVE * FROM a TOSS b ON a.id > b.id;");
        assert!(matches!(err, Scottatura::NonEquiJoinNotSupported));
    }

    #[test]
    fn test_parse_select_with_arranged_by_and_only() {
        let stmt = parse("SERVE * FROM users ARRANGED BY age DESC, name ONLY 10 SKIP 5;");
        match stmt {
            Statement::Select(s) => {
                assert_eq!(s.order_by, vec![
                    ast::OrderByItem { column: "age".into(), descending: true },
                    ast::OrderByItem { column: "name".into(), descending: false },
                ]);
                assert_eq!(s.limit, Some(10));
                assert_eq!(s.offset, Some(5));
            }
            other => panic!("expected Select, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_select_rejects_skip_without_only() {
        let err = parse_err("SERVE * FROM users SKIP 5;");
        assert!(matches!(err, Scottatura::SkipWithoutOnly));
    }

    #[test]
    fn test_parse_select_with_bundled_by() {
        let stmt = parse("SERVE age, COUNT(*) FROM users BUNDLED BY age;");
        match stmt {
            Statement::Select(s) => assert_eq!(s.group_by, vec!["age".to_string()]),
            other => panic!("expected Select, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_select_rejects_having() {
        let err = parse_err("SERVE age FROM users BUNDLED BY age HAVING age > 1;");
        assert!(matches!(err, Scottatura::HavingNotSupported));
    }

    #[test]
    fn test_parse_select_item_alias() {
        let stmt = parse("SERVE total * 2 AS doubled FROM orders;");
        match stmt {
            Statement::Select(s) => match &s.items[0] {
                ast::SelectItem::Expr { alias, .. } => assert_eq!(alias.as_deref(), Some("doubled")),
                other => panic!("expected Expr item, got {other:?}"),
            },
            other => panic!("expected Select, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_select_literal_in_filter() {
        let stmt = parse("SERVE * FROM t STRAINED BY x = 'y';");
        match stmt {
            Statement::Select(s) => {
                assert_eq!(s.filter, Some(ast::Expr::BinaryOp {
                    op: ast::BinOp::Eq,
                    left: Box::new(ast::Expr::Column { qualifier: None, name: "x".into() }),
                    right: Box::new(ast::Expr::Literal(Value::Text("y".into()))),
                }));
            }
            other => panic!("expected Select, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod dml_tests {
    use super::*;
    use crate::Value;

    fn parse(src: &str) -> Statement {
        let tokens = lexer::tokenize(src).unwrap();
        let mut p = Parser::new(tokens);
        p.parse_stmt().unwrap()
    }

    fn parse_err(src: &str) -> Scottatura {
        let tokens = lexer::tokenize(src).unwrap();
        let mut p = Parser::new(tokens);
        p.parse_stmt().unwrap_err()
    }

    #[test]
    fn test_parse_insert_single_row() {
        let stmt = parse("LADLE INTO orders (user_id, total) PORTIONS (1, 49.99);");
        assert_eq!(stmt, Statement::Insert {
            table: "orders".into(),
            columns: vec!["user_id".into(), "total".into()],
            rows: vec![vec![
                Expr::Literal(Value::Integer(1)),
                Expr::Literal(Value::Real(49.99)),
            ]],
        });
    }

    #[test]
    fn test_parse_insert_multi_row() {
        let stmt = parse("LADLE INTO t (a) PORTIONS (1), (2), (3);");
        match stmt {
            Statement::Insert { rows, .. } => assert_eq!(rows.len(), 3),
            other => panic!("expected Insert, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_insert_rejects_id_column() {
        let err = parse_err("LADLE INTO t (id, a) PORTIONS (1, 2);");
        assert!(matches!(err, Scottatura::ReservedIdColumn));
    }

    #[test]
    fn test_parse_insert_rejects_value_count_mismatch() {
        let err = parse_err("LADLE INTO t (a, b) PORTIONS (1);");
        assert!(matches!(err, Scottatura::ValueCountMismatch { got: 1, expected: 2 }));
    }

    #[test]
    fn test_parse_update() {
        let stmt = parse("SEASON users SET total = 59.99 STRAINED BY id = 1;");
        match stmt {
            Statement::Update { table, assignments, filter } => {
                assert_eq!(table, "users");
                assert_eq!(assignments, vec![("total".to_string(), Expr::Literal(Value::Real(59.99)))]);
                assert!(matches!(filter, Expr::BinaryOp { .. }));
            }
            other => panic!("expected Update, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_update_rejects_missing_strained_by() {
        let err = parse_err("SEASON users SET total = 59.99;");
        assert!(matches!(err, Scottatura::SeasonRequiresFilter));
    }

    #[test]
    fn test_parse_delete() {
        let stmt = parse("DISCARD FROM users STRAINED BY age < 18;");
        assert_eq!(stmt, Statement::Delete {
            table: "users".into(),
            filter: Expr::BinaryOp {
                op: BinOp::Lt,
                left: Box::new(Expr::Column { qualifier: None, name: "age".into() }),
                right: Box::new(Expr::Literal(Value::Integer(18))),
            },
        });
    }

    #[test]
    fn test_parse_delete_rejects_missing_strained_by() {
        let err = parse_err("DISCARD FROM users;");
        assert!(matches!(err, Scottatura::DiscardRequiresFilter));
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;

    #[test]
    fn test_parse_begin_finish_spill_simmer() {
        assert_eq!(parse_statement("BEGIN SIMMER;").unwrap(), Statement::BeginSimmer);
        assert_eq!(parse_statement("FINISH SIMMER;").unwrap(), Statement::FinishSimmer);
        assert_eq!(parse_statement("SPILL SIMMER;").unwrap(), Statement::SpillSimmer);
    }

    #[test]
    fn test_parse_statement_rejects_trailing_garbage() {
        let err = parse_statement("SERVE * FROM users; SERVE * FROM orders;").unwrap_err();
        assert!(matches!(err, Scottatura::UnexpectedToken { .. }));
    }

    #[test]
    fn test_parse_statement_unsupported_form() {
        let err = parse_statement("EXPLAIN SERVE * FROM users;").unwrap_err();
        assert!(matches!(err, Scottatura::StatementNotSupported));
    }

    #[test]
    fn test_parse_program_multiple_statements() {
        let stmts = parse_program("
            PLATE noodle users (name TEXT);
            LADLE INTO users (name) PORTIONS ('Tony');
            SERVE * FROM users;
        ").unwrap();
        assert_eq!(stmts.len(), 3);
        assert!(matches!(stmts[0], Statement::CreateTable { .. }));
        assert!(matches!(stmts[1], Statement::Insert { .. }));
        assert!(matches!(stmts[2], Statement::Select(_)));
    }

    #[test]
    fn test_parse_program_tolerates_trailing_semicolon_and_whitespace() {
        let stmts = parse_program("  BEGIN SIMMER;  \n\n  FINISH SIMMER;  ").unwrap();
        assert_eq!(stmts, vec![Statement::BeginSimmer, Statement::FinishSimmer]);
    }
}
