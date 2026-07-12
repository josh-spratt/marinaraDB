# MarinaraDB Phase 2a: SaucQL Parser Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the SaucQL lexer, AST, and recursive-descent parser so that any statement from `docs/sql-semantics.md` turns into a validated AST, with `PARSE_*`/`TIPO_003`/`TIPO_004`/`RICETTA_006`/`RICETTA_009` Scottatura codes raised exactly where the spec requires.

**Architecture:** A hand-rolled lexer (`src/parser/lexer.rs`) turns SaucQL source into a flat `Vec<(Token, usize)>` (token + byte offset, for error messages). A hand-rolled recursive-descent `Parser` (`src/parser/mod.rs`) consumes that token stream and produces `Statement`/`Expr` AST nodes (`src/parser/ast.rs`) using precedence climbing for expressions (`OR` < `AND` < `NOT` < comparison < `+`/`-` < `*`/`/` < unary < primary). No parser-level dependency on the storage engine — this plan only produces and validates AST; execution against a `Pot` is Phase 2b.

**Tech Stack:** Pure Rust, no parser-generator crate (per `SPEC.md`: "hand-rolled recursive-descent (no external parser library)"). `thiserror` for `Scottatura` variants already used elsewhere in the crate.

## Global Constraints

- Rust edition 2021, no `unsafe`.
- No new crate dependencies — this plan uses only `std` plus the existing `thiserror` dependency already in `Cargo.toml`.
- All identifiers case-insensitive, normalized to lowercase at parse time (`docs/sql-semantics.md` → Identifiers).
- Maximum identifier length: 128 characters → `PARSE_012`.
- SaucQL reserved words used as identifiers must be quoted with double-quotes (e.g. `"serve"`).
- `--` starts a line comment; block comments are not supported in v1.
- Error codes must match `docs/error-taxonomy.md` exactly — every `Scottatura` variant added in this plan carries the code from that table in its `#[error(...)]` message.
- Every public parsing entry point returns `Result<T, Scottatura>` — never panics on malformed input.

---

## Task 1: Lexer — tokens and tokenizer

**Files:**
- Create: `src/parser/lexer.rs`
- Modify: `src/lib.rs:53` (append new `Scottatura` variants after `NullConstraintViolation`)
- Modify: `src/parser/mod.rs` (replace stub comment with `pub mod lexer;`)

**Interfaces:**
- Produces: `pub enum Token` (all SaucQL keyword tokens as single words, `Ident(String)`, `IntLiteral(i64)`, `RealLiteral(f64)`, `StringLiteral(String)`, punctuation, `Eof`), `pub fn tokenize(src: &str) -> Result<Vec<(Token, usize)>, Scottatura>`.
- Consumes: `crate::Scottatura` (existing enum in `src/lib.rs`).

- [ ] **Step 1: Write the failing test**

Create `src/parser/lexer.rs` with just the test module (no implementation yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<Token> {
        tokenize(src).unwrap().into_iter().map(|(t, _)| t).collect()
    }

    #[test]
    fn test_tokenize_keywords_and_ident() {
        let toks = kinds("SERVE * FROM users");
        assert_eq!(toks, vec![Token::Serve, Token::Star, Token::From, Token::Ident("users".into()), Token::Eof]);
    }

    #[test]
    fn test_tokenize_is_case_insensitive_and_lowercases_idents() {
        let toks = kinds("serve Name from USERS");
        assert_eq!(toks, vec![Token::Serve, Token::Ident("name".into()), Token::From, Token::Ident("users".into()), Token::Eof]);
    }

    #[test]
    fn test_tokenize_literals() {
        let toks = kinds("42 3.14 'hello'");
        assert_eq!(toks, vec![
            Token::IntLiteral(42),
            Token::RealLiteral(3.14),
            Token::StringLiteral("hello".into()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_tokenize_string_with_escaped_quote() {
        let toks = kinds("'it''s'");
        assert_eq!(toks, vec![Token::StringLiteral("it's".into()), Token::Eof]);
    }

    #[test]
    fn test_tokenize_quoted_identifier() {
        let toks = kinds("\"serve\"");
        assert_eq!(toks, vec![Token::Ident("serve".into()), Token::Eof]);
    }

    #[test]
    fn test_tokenize_punctuation_and_operators() {
        let toks = kinds("(a, b) = != < > <= >= + - * / .");
        assert_eq!(toks, vec![
            Token::LParen, Token::Ident("a".into()), Token::Comma, Token::Ident("b".into()), Token::RParen,
            Token::Eq, Token::Neq, Token::Lt, Token::Gt, Token::Lte, Token::Gte,
            Token::Plus, Token::Minus, Token::Star, Token::Slash, Token::Dot,
            Token::Eof,
        ]);
    }

    #[test]
    fn test_tokenize_skips_line_comments() {
        let toks = kinds("SERVE * -- this is a comment\nFROM users");
        assert_eq!(toks, vec![Token::Serve, Token::Star, Token::From, Token::Ident("users".into()), Token::Eof]);
    }

    #[test]
    fn test_tokenize_rejects_unknown_char() {
        let err = tokenize("SERVE @ FROM users").unwrap_err();
        assert!(matches!(err, Scottatura::UnexpectedToken { .. }));
    }

    #[test]
    fn test_tokenize_rejects_identifier_over_128_chars() {
        let long_name = "a".repeat(129);
        let err = tokenize(&long_name).unwrap_err();
        assert!(matches!(err, Scottatura::IdentifierTooLong { .. }));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib parser::lexer -- --nocapture`
Expected: compile error — `Token`, `tokenize`, and the new `Scottatura` variants don't exist yet.

- [ ] **Step 3: Add the new `Scottatura` variants**

In `src/lib.rs`, after the `NullConstraintViolation` variant (the last one in the enum), add:

```rust
    #[error("Scottatura [PARSE_001]: Unexpected token `{token}` at position {pos}.")]
    UnexpectedToken { token: String, pos: usize },

    #[error("Scottatura [PARSE_012]: Identifier `{name}` exceeds maximum length of 128 characters.")]
    IdentifierTooLong { name: String },

    #[error("Scottatura [TIPO_004]: Invalid literal `{literal}` for type {type_name}.")]
    InvalidLiteral { literal: String, type_name: String },
```

- [ ] **Step 4: Implement the lexer**

Prepend to `src/parser/lexer.rs` (above the `#[cfg(test)]` module):

```rust
use crate::Scottatura;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // statement keywords
    Serve, From, Strained, By, Ladle, Into, Portions, Season, Set, Discard,
    Plate, Noodle, Toss, Out, On, As, Begin, Simmer, Finish, Spill,
    Strain, Remove, Strainer,
    // expression keywords
    And, Or, Not, In, Between, Like, Is, Vuoto,
    Asc, Desc, Arranged, Only, Skip, Bundled, Having,
    Al, Dente, Key,
    // literals & identifiers
    Ident(String),
    IntLiteral(i64),
    RealLiteral(f64),
    StringLiteral(String),
    // punctuation
    LParen, RParen, Comma, Semicolon, Dot,
    Eq, Neq, Lt, Gt, Lte, Gte,
    Plus, Minus, Star, Slash,
    Eof,
}

pub fn tokenize(src: &str) -> Result<Vec<(Token, usize)>, Scottatura> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    let len = bytes.len();

    while i < len {
        let c = bytes[i] as char;

        if c.is_whitespace() {
            i += 1;
            continue;
        }

        if c == '-' && i + 1 < len && bytes[i + 1] as char == '-' {
            while i < len && bytes[i] as char != '\n' {
                i += 1;
            }
            continue;
        }

        if c == '\'' {
            let start = i;
            i += 1;
            let mut s = String::new();
            loop {
                if i >= len {
                    return Err(Scottatura::UnexpectedToken { token: "<eof>".into(), pos: start });
                }
                let ch = bytes[i] as char;
                if ch == '\'' {
                    if i + 1 < len && bytes[i + 1] as char == '\'' {
                        s.push('\'');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                s.push(ch);
                i += 1;
            }
            tokens.push((Token::StringLiteral(s), start));
            continue;
        }

        if c == '"' {
            let start = i;
            i += 1;
            let mut s = String::new();
            loop {
                if i >= len {
                    return Err(Scottatura::UnexpectedToken { token: "<eof>".into(), pos: start });
                }
                let ch = bytes[i] as char;
                if ch == '"' {
                    i += 1;
                    break;
                }
                s.push(ch);
                i += 1;
            }
            tokens.push((Token::Ident(s.to_lowercase()), start));
            continue;
        }

        if c.is_ascii_digit() {
            let start = i;
            let mut is_real = false;
            while i < len && (bytes[i] as char).is_ascii_digit() {
                i += 1;
            }
            if i < len && bytes[i] as char == '.' && i + 1 < len && (bytes[i + 1] as char).is_ascii_digit() {
                is_real = true;
                i += 1;
                while i < len && (bytes[i] as char).is_ascii_digit() {
                    i += 1;
                }
            }
            let text = &src[start..i];
            if is_real {
                tokens.push((Token::RealLiteral(text.parse().unwrap()), start));
            } else {
                match text.parse::<i64>() {
                    Ok(n) => tokens.push((Token::IntLiteral(n), start)),
                    Err(_) => return Err(Scottatura::InvalidLiteral { literal: text.to_string(), type_name: "INTEGER".into() }),
                }
            }
            continue;
        }

        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < len && ((bytes[i] as char).is_ascii_alphanumeric() || bytes[i] as char == '_') {
                i += 1;
            }
            let text = &src[start..i];
            if text.len() > 128 {
                return Err(Scottatura::IdentifierTooLong { name: text.to_string() });
            }
            tokens.push((keyword_or_ident(text), start));
            continue;
        }

        let start = i;
        let tok = match c {
            '(' => { i += 1; Token::LParen }
            ')' => { i += 1; Token::RParen }
            ',' => { i += 1; Token::Comma }
            ';' => { i += 1; Token::Semicolon }
            '.' => { i += 1; Token::Dot }
            '+' => { i += 1; Token::Plus }
            '-' => { i += 1; Token::Minus }
            '*' => { i += 1; Token::Star }
            '/' => { i += 1; Token::Slash }
            '=' => { i += 1; Token::Eq }
            '!' if i + 1 < len && bytes[i + 1] as char == '=' => { i += 2; Token::Neq }
            '<' if i + 1 < len && bytes[i + 1] as char == '=' => { i += 2; Token::Lte }
            '<' => { i += 1; Token::Lt }
            '>' if i + 1 < len && bytes[i + 1] as char == '=' => { i += 2; Token::Gte }
            '>' => { i += 1; Token::Gt }
            other => return Err(Scottatura::UnexpectedToken { token: other.to_string(), pos: start }),
        };
        tokens.push((tok, start));
    }

    tokens.push((Token::Eof, len));
    Ok(tokens)
}

fn keyword_or_ident(text: &str) -> Token {
    match text.to_ascii_uppercase().as_str() {
        "SERVE" => Token::Serve,
        "FROM" => Token::From,
        "STRAINED" => Token::Strained,
        "BY" => Token::By,
        "LADLE" => Token::Ladle,
        "INTO" => Token::Into,
        "PORTIONS" => Token::Portions,
        "SEASON" => Token::Season,
        "SET" => Token::Set,
        "DISCARD" => Token::Discard,
        "PLATE" => Token::Plate,
        "NOODLE" => Token::Noodle,
        "TOSS" => Token::Toss,
        "OUT" => Token::Out,
        "ON" => Token::On,
        "AS" => Token::As,
        "BEGIN" => Token::Begin,
        "SIMMER" => Token::Simmer,
        "FINISH" => Token::Finish,
        "SPILL" => Token::Spill,
        "STRAIN" => Token::Strain,
        "REMOVE" => Token::Remove,
        "STRAINER" => Token::Strainer,
        "AND" => Token::And,
        "OR" => Token::Or,
        "NOT" => Token::Not,
        "IN" => Token::In,
        "BETWEEN" => Token::Between,
        "LIKE" => Token::Like,
        "IS" => Token::Is,
        "VUOTO" => Token::Vuoto,
        "ASC" => Token::Asc,
        "DESC" => Token::Desc,
        "ARRANGED" => Token::Arranged,
        "ONLY" => Token::Only,
        "SKIP" => Token::Skip,
        "BUNDLED" => Token::Bundled,
        "HAVING" => Token::Having,
        "AL" => Token::Al,
        "DENTE" => Token::Dente,
        "KEY" => Token::Key,
        _ => Token::Ident(text.to_lowercase()),
    }
}
```

Update `src/parser/mod.rs` (currently just `// SaucQL recursive-descent parser — Phase 2`) to:

```rust
pub mod lexer;
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --lib parser::lexer -- --nocapture`
Expected: all 9 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add src/parser/lexer.rs src/parser/mod.rs src/lib.rs
git commit -m "feat: SaucQL lexer — tokens, keywords, literals, comments"
```

---

## Task 2: AST types

**Files:**
- Create: `src/parser/ast.rs`
- Modify: `src/parser/mod.rs` (add `pub mod ast;`)

**Interfaces:**
- Consumes: `crate::{ColType, Value}` (existing types in `src/lib.rs`).
- Produces: `pub enum Statement`, `pub struct ColumnSpec`, `pub struct SelectStatement`, `pub enum SelectItem`, `pub struct TableRef`, `pub struct JoinClause`, `pub struct OrderByItem`, `pub enum Expr`, `pub enum BinOp` — all consumed by the parser built in Tasks 3–7.

This task is data-only (no parsing logic), so there is no separate test step — its shape is exercised by the parser tests in Tasks 3–7. Skipping tests here would violate TDD in spirit, so the "test" for this task is that the crate still compiles with the new types in scope (`cargo check`), which doubles as the verification step.

- [ ] **Step 1: Write the AST types**

Create `src/parser/ast.rs`:

```rust
use crate::{ColType, Value};

#[derive(Debug, Clone, PartialEq)]
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
```

Update `src/parser/mod.rs`:

```rust
pub mod ast;
pub mod lexer;
```

- [ ] **Step 2: Run to verify it compiles**

Run: `cargo check`
Expected: no errors (new types are unused outside the crate but that produces no warning for `pub` items in a lib crate re-exported from `mod.rs`).

- [ ] **Step 3: Commit**

```bash
git add src/parser/ast.rs src/parser/mod.rs
git commit -m "feat: SaucQL AST types — Statement, Expr, and supporting structs"
```

---

## Task 3: Expression parser

**Files:**
- Modify: `src/parser/mod.rs` (add `Parser` struct and expression-parsing methods)

**Interfaces:**
- Consumes: `lexer::{Token, tokenize}`, `ast::{Expr, BinOp}`, `crate::{Value, Scottatura}`.
- Produces: `pub struct Parser { .. }` with `pub fn new(tokens: Vec<(Token, usize)>) -> Self` and `pub fn parse_expr(&mut self) -> Result<Expr, Scottatura>` — consumed by every statement-parsing method added in Tasks 4–7.

- [ ] **Step 1: Write the failing test**

Append to `src/parser/mod.rs`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib parser::expr_tests -- --nocapture`
Expected: compile error — `Parser` and `Scottatura::SubServeNotSupported` don't exist yet.

- [ ] **Step 3: Add the `SubServeNotSupported` variant**

In `src/lib.rs`, add after `InvalidLiteral` (added in Task 1):

```rust
    #[error("Scottatura [PARSE_003]: Sub-serves are not supported in v1.")]
    SubServeNotSupported,
```

- [ ] **Step 4: Implement `Parser` and expression parsing**

Prepend to `src/parser/mod.rs` (above the `pub mod ast;` / `pub mod lexer;` lines, then keep those lines, then the new code below them):

```rust
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
```

Note: `parse_int_literal` carries `#[allow(dead_code)]` because it's unused until Task 5 (`ONLY`/`SKIP`) calls it.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --lib parser:: -- --nocapture`
Expected: all lexer tests (Task 1) and all `expr_tests` PASS.

- [ ] **Step 6: Commit**

```bash
git add src/parser/mod.rs src/lib.rs
git commit -m "feat: SaucQL expression parser — precedence climbing for arithmetic, boolean, IS/BETWEEN/LIKE/IN"
```

---

## Task 4: DDL statement parsing — PLATE, TOSS OUT, STRAIN, REMOVE STRAINER

**Files:**
- Modify: `src/parser/mod.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `Parser::{peek, check, advance, expect, expect_ident, parse_expr}` (Task 3), `ast::{Statement, ColumnSpec}` (Task 2).
- Produces: `Parser::parse_create_table`, `Parser::parse_drop_table`, `Parser::parse_create_index`, `Parser::parse_drop_index` — each `fn(&mut self) -> Result<Statement, Scottatura>`, consumed by the top-level dispatcher in Task 7.

- [ ] **Step 1: Write the failing test**

Append to `src/parser/mod.rs`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib parser::ddl_tests -- --nocapture`
Expected: compile error — `parse_stmt` and the new `Scottatura` variants (`ReservedIdColumn`, `UnknownType`, `MultiColumnStrainerNotSupported`) don't exist yet.

- [ ] **Step 3: Add the new `Scottatura` variants**

In `src/lib.rs`, add after `SubServeNotSupported` (added in Task 3):

```rust
    #[error("Scottatura [PARSE_004]: Multi-column strainers are not supported in v1.")]
    MultiColumnStrainerNotSupported,

    #[error("Scottatura [TIPO_003]: Unknown type `{type_name}`. Supported types: INTEGER / INT / BIGINT, REAL / FLOAT, TEXT, BLOB.")]
    UnknownType { type_name: String },

    #[error("Scottatura [RICETTA_006]: Column `id` is reserved for the Al Dente Key; remove it from the column list.")]
    ReservedIdColumn,
```

- [ ] **Step 4: Implement DDL parsing**

Append to the `impl Parser` block in `src/parser/mod.rs` (after `parse_primary` and before the closing `}` of `impl Parser`, i.e. insert these methods as new members of the same `impl Parser { ... }` block):

```rust
    pub fn parse_stmt(&mut self) -> Result<Statement, Scottatura> {
        match self.peek().clone() {
            Token::Plate => self.parse_create_table(),
            Token::Toss => self.parse_drop_table(),
            Token::Strain => self.parse_create_index(),
            Token::Remove => self.parse_drop_index(),
            other => Err(Scottatura::UnexpectedToken { token: format!("{other:?}"), pos: self.peek_pos() }),
        }
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
            columns.push(ast::ColumnSpec { name: col_name, col_type, not_null });
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
```

Also change `use ast::{BinOp, Expr};` near the top of `src/parser/mod.rs` to `use ast::{BinOp, Expr, Statement};` (needed by the return types above).

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --lib parser:: -- --nocapture`
Expected: all tests from Tasks 1, 3, and the new `ddl_tests` PASS.

- [ ] **Step 6: Commit**

```bash
git add src/parser/mod.rs src/lib.rs
git commit -m "feat: SaucQL DDL parsing — PLATE, TOSS OUT, STRAIN, REMOVE STRAINER"
```

---

## Task 5: SERVE (SELECT) statement parsing

**Files:**
- Modify: `src/parser/mod.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `Parser` primitives (Task 3), `ast::{SelectStatement, SelectItem, TableRef, JoinClause, OrderByItem}` (Task 2).
- Produces: `Parser::parse_select(&mut self) -> Result<SelectStatement, Scottatura>`, consumed by `parse_stmt` (extended in this task) and by the Task 7 dispatcher.

- [ ] **Step 1: Write the failing test**

Append to `src/parser/mod.rs`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib parser::select_tests -- --nocapture`
Expected: compile error — `Statement::Select`, `parse_select`, and several `Scottatura` variants don't exist on the dispatch path yet.

- [ ] **Step 3: Add the new `Scottatura` variants**

In `src/lib.rs`, add after `ReservedIdColumn` (added in Task 4):

```rust
    #[error("Scottatura [PARSE_005]: LEFT / RIGHT / FULL TOSS is not supported in v1; use TOSS (inner equi-join).")]
    OuterTossNotSupported,

    #[error("Scottatura [PARSE_006]: Only equi-joins (ON a.col = b.col) are supported.")]
    NonEquiJoinNotSupported,

    #[error("Scottatura [PARSE_009]: SKIP without ONLY is not supported.")]
    SkipWithoutOnly,

    #[error("Scottatura [PARSE_010]: HAVING is not supported in v1.")]
    HavingNotSupported,

    #[error("Scottatura [PARSE_011]: At most one TOSS per serve is supported in v1.")]
    MultipleTossNotSupported,
```

- [ ] **Step 4: Implement SELECT parsing**

In `src/parser/mod.rs`, change the `use ast::{BinOp, Expr, Statement};` line to:

```rust
use ast::{BinOp, Expr, JoinClause, OrderByItem, SelectItem, SelectStatement, Statement, TableRef};
```

Extend `parse_stmt`'s match arms (added in Task 4) to add a `Serve` arm:

```rust
            Token::Plate => self.parse_create_table(),
            Token::Toss => self.parse_drop_table(),
            Token::Strain => self.parse_create_index(),
            Token::Remove => self.parse_drop_index(),
            Token::Serve => self.parse_select().map(Statement::Select),
```

Add these methods to `impl Parser`:

```rust
    fn parse_select(&mut self) -> Result<SelectStatement, Scottatura> {
        self.expect(Token::Serve)?;
        let items = self.parse_select_items()?;
        self.expect(Token::From)?;
        let from = self.parse_table_ref()?;

        if let Token::Ident(w) = self.peek().clone() {
            if matches!(w.as_str(), "left" | "right" | "full") {
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
            self.advance();
            Some(a)
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
```

Remove the `#[allow(dead_code)]` attribute above `parse_int_literal` (added in Task 3) — it's now called from `parse_select`.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --lib parser:: -- --nocapture`
Expected: all tests from Tasks 1, 3, 4, and the new `select_tests` PASS.

- [ ] **Step 6: Commit**

```bash
git add src/parser/mod.rs src/lib.rs
git commit -m "feat: SaucQL SERVE parsing — items, TOSS join, STRAINED BY, BUNDLED BY, ARRANGED BY, ONLY/SKIP"
```

---

## Task 6: LADLE / SEASON / DISCARD statement parsing

**Files:**
- Modify: `src/parser/mod.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `Parser` primitives (Task 3), `ast::Statement` variants `Insert`/`Update`/`Delete` (Task 2).
- Produces: `Parser::{parse_insert, parse_update, parse_delete}`, each `fn(&mut self) -> Result<Statement, Scottatura>`, consumed by the Task 7 dispatcher.

- [ ] **Step 1: Write the failing test**

Append to `src/parser/mod.rs`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib parser::dml_tests -- --nocapture`
Expected: compile error — `Statement::Insert`/`Update`/`Delete` dispatch and new `Scottatura` variants don't exist yet.

- [ ] **Step 3: Add the new `Scottatura` variants**

In `src/lib.rs`, add after `MultipleTossNotSupported` (added in Task 5):

```rust
    #[error("Scottatura [RICETTA_009]: Value count ({got}) does not match column count ({expected}).")]
    ValueCountMismatch { got: usize, expected: usize },

    #[error("Scottatura [PARSE_007]: SEASON requires a STRAINED BY clause.")]
    SeasonRequiresFilter,

    #[error("Scottatura [PARSE_008]: DISCARD FROM requires a STRAINED BY clause.")]
    DiscardRequiresFilter,
```

- [ ] **Step 4: Implement LADLE/SEASON/DISCARD parsing**

Extend `parse_stmt`'s match arms to add `Ladle`, `Season`, `Discard`:

```rust
            Token::Serve => self.parse_select().map(Statement::Select),
            Token::Ladle => self.parse_insert(),
            Token::Season => self.parse_update(),
            Token::Discard => self.parse_delete(),
```

Add these methods to `impl Parser`:

```rust
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
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --lib parser:: -- --nocapture`
Expected: all tests from Tasks 1, 3, 4, 5, and the new `dml_tests` PASS.

- [ ] **Step 6: Commit**

```bash
git add src/parser/mod.rs src/lib.rs
git commit -m "feat: SaucQL DML parsing — LADLE INTO, SEASON, DISCARD FROM"
```

---

## Task 7: Transaction statements, top-level dispatch, and multi-statement programs

**Files:**
- Modify: `src/parser/mod.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: everything from Tasks 1–6.
- Produces: `pub fn parse_statement(sql: &str) -> Result<Statement, Scottatura>`, `pub fn parse_program(sql: &str) -> Result<Vec<Statement>, Scottatura>` — the public entry points Phase 2b's executor and the REPL (Phase 2d) will call.

- [ ] **Step 1: Write the failing test**

Append to `src/parser/mod.rs`:

```rust
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
        assert!(matches!(err, Scottatura::StatementNotSupported { .. }));
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib parser::dispatch_tests -- --nocapture`
Expected: compile error — `parse_statement`, `parse_program`, and `Scottatura::StatementNotSupported` don't exist yet.

- [ ] **Step 3: Add the `StatementNotSupported` variant**

In `src/lib.rs`, add after `DiscardRequiresFilter` (added in Task 6):

```rust
    #[error("Scottatura [PARSE_002]: Statement type not supported.")]
    StatementNotSupported,
```

- [ ] **Step 4: Implement BEGIN/FINISH/SPILL SIMMER, top-level dispatch, and program parsing**

Extend `parse_stmt`'s match arms — replace the final `other => Err(...)` fallback arm with the transaction-statement arms followed by the fallback:

```rust
            Token::Discard => self.parse_delete(),
            Token::Begin => { self.advance(); self.expect(Token::Simmer)?; Ok(Statement::BeginSimmer) }
            Token::Finish => { self.advance(); self.expect(Token::Simmer)?; Ok(Statement::FinishSimmer) }
            Token::Spill => { self.advance(); self.expect(Token::Simmer)?; Ok(Statement::SpillSimmer) }
            _ => Err(Scottatura::StatementNotSupported),
```

(This replaces the `other => Err(Scottatura::UnexpectedToken { .. })` fallback that `parse_stmt` had since Task 4 — unrecognized statement starts now report `StatementNotSupported` per `PARSE_002`, matching `docs/sql-semantics.md`: "All other statement forms produce a `PARSE_002` Scottatura.")

Add two free functions at the bottom of `src/parser/mod.rs` (outside `impl Parser`):

```rust
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
```

Since `parse_statement`/`parse_program` call `p.check(...)` and `p.peek()`/`p.advance()` from outside `impl Parser`'s own methods but within the same module, no visibility change is needed — `check`, `peek`, `peek_pos`, and `advance` are private (`fn`, no `pub`) but this code lives in the same module (`src/parser/mod.rs`), so it can call them directly.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --lib parser:: -- --nocapture`
Expected: all tests from Tasks 1, 3–6, and the new `dispatch_tests` PASS.

- [ ] **Step 6: Commit**

```bash
git add src/parser/mod.rs src/lib.rs
git commit -m "feat: SaucQL transaction statements, top-level dispatch, multi-statement programs"
```

---

## Task 8: Integration tests and self-review

**Files:**
- Create: `tests/parser.rs`

**Interfaces:**
- Consumes: `marinaradb::parser::{parse_statement, parse_program}`, `marinaradb::parser::ast::*`, `marinaradb::Scottatura` (all public as of Task 7).

- [ ] **Step 1: Write the integration test**

Create `tests/parser.rs`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test parser -- --nocapture`
Expected: compile error if `parser` or `ast` modules aren't `pub` from `src/lib.rs`. If it fails this way, add `pub mod ast;` re-export — check `src/lib.rs:1` already has `pub mod parser;`, and `src/parser/mod.rs` already has `pub mod ast;` (from Task 2) and `pub mod lexer;` (from Task 1), so both should already be reachable as `marinaradb::parser::ast::Statement` and `marinaradb::parser::lexer::Token`. No source change expected here — this step just confirms the module tree is public end-to-end.

- [ ] **Step 3: Run full test suite**

Run: `cargo test`
Expected: every test in `src/parser/mod.rs` (lexer, expr, ddl, select, dml, dispatch) and `tests/parser.rs` PASSES; pre-existing tests in `src/stove/`, `src/warming_tray/`, `tests/foundation.rs` are unaffected and still PASS.

- [ ] **Step 4: Run `cargo build` and `cargo clippy`**

Run: `cargo build && cargo clippy --all-targets -- -D warnings`
Expected: no errors, no warnings. If clippy flags anything (e.g. `needless_return`, `redundant_clone`), fix it in place before moving on.

- [ ] **Step 5: Commit**

```bash
git add tests/parser.rs
git commit -m "test: SaucQL parser integration tests — full CRUD script, joins, transactions, quoted identifiers"
```

---

## Self-Review Checklist (run before calling Phase 2a complete)

- [ ] `cargo test` passes with zero failures.
- [ ] `cargo build` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] Every statement form in `docs/sql-semantics.md` → "Supported Statements" has a `parse_*` method and at least one test: `PLATE noodle` ✓, `TOSS OUT noodle` ✓, `STRAIN` ✓, `REMOVE STRAINER` ✓, `SERVE` ✓, `LADLE INTO` ✓, `SEASON` ✓, `DISCARD FROM` ✓, `BEGIN SIMMER` ✓, `FINISH SIMMER` ✓, `SPILL SIMMER` ✓.
- [ ] Every `PARSE_*`, relevant `TIPO_*`/`RICETTA_*` code referenced by `docs/error-taxonomy.md` that is parser-detectable (not executor-detectable) is implemented and tested: `PARSE_001`–`PARSE_012`, `TIPO_003`, `TIPO_004`, `RICETTA_006`, `RICETTA_009`.
- [ ] No `todo!()`, `unimplemented!()`, or placeholder strings remain in `src/parser/`.

---

## What Phase 2b Adds

Phase 2b (Sous Chef planner + executor) will add:
- Multi-page heap traversal (following `next_page` pointers in `src/stove/heap.rs` / `src/stove/pot.rs`)
- Sous Chef query planner (`src/sous_chef/`) — turns `ast::Statement` into an execution `Plan`, selects SeqScan vs. Strainer index scan
- Stove executor — walks `Plan`, executes `SERVE`/`LADLE`/`SEASON`/`DISCARD` against a `Pot`, evaluates `Expr` (including VUOTO three-valued logic, type coercion `INTEGER → REAL`)
- `Pot::execute(sauql: &str) -> Result<ResultSet, Scottatura>`, built on `parser::parse_statement` from this plan

Deliverable: single-table `SERVE`/`LADLE`/`SEASON`/`DISCARD` with `STRAINED BY` execute against a real `.pot` file end-to-end (joins, `ARRANGED BY`/`ONLY`, and aggregates land in Phase 2c).
