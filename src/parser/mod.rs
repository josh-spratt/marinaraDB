pub mod ast;
pub mod lexer;

use ast::{BinOp, Expr};
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

    #[allow(dead_code)] // consumed by parse_select in Task 5
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
}

fn is_aggregate_fn(name: &str) -> bool {
    matches!(name.to_ascii_uppercase().as_str(), "COUNT" | "SUM" | "MIN" | "MAX" | "AVG")
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
