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
