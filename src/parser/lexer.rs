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
    let chars: Vec<char> = src.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    let len = chars.len();

    while i < len {
        let c = chars[i];

        if c.is_whitespace() {
            i += 1;
            continue;
        }

        if c == '-' && i + 1 < len && chars[i + 1] == '-' {
            while i < len && chars[i] != '\n' {
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
                let ch = chars[i];
                if ch == '\'' {
                    if i + 1 < len && chars[i + 1] == '\'' {
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
                let ch = chars[i];
                if ch == '"' {
                    i += 1;
                    break;
                }
                s.push(ch);
                i += 1;
            }
            if s.chars().count() > 128 {
                return Err(Scottatura::IdentifierTooLong { name: s });
            }
            tokens.push((Token::Ident(s.to_lowercase()), start));
            continue;
        }

        if c.is_ascii_digit() {
            let start = i;
            let mut is_real = false;
            while i < len && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i < len && chars[i] == '.' && i + 1 < len && chars[i + 1].is_ascii_digit() {
                is_real = true;
                i += 1;
                while i < len && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let text: String = chars[start..i].iter().collect();
            if is_real {
                match text.parse::<f64>() {
                    Ok(f) => tokens.push((Token::RealLiteral(f), start)),
                    Err(_) => return Err(Scottatura::InvalidLiteral { literal: text.to_string(), type_name: "REAL".to_string() }),
                }
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
            while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            if text.len() > 128 {
                return Err(Scottatura::IdentifierTooLong { name: text });
            }
            tokens.push((keyword_or_ident(&text), start));
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
            '!' if i + 1 < len && chars[i + 1] == '=' => { i += 2; Token::Neq }
            '<' if i + 1 < len && chars[i + 1] == '=' => { i += 2; Token::Lte }
            '<' => { i += 1; Token::Lt }
            '>' if i + 1 < len && chars[i + 1] == '=' => { i += 2; Token::Gte }
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
        let toks = kinds("42 3.5 'hello'");
        assert_eq!(toks, vec![
            Token::IntLiteral(42),
            Token::RealLiteral(3.5),
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

    #[test]
    fn test_tokenize_rejects_quoted_identifier_over_128_chars() {
        let long_name = format!("\"{}\"", "a".repeat(129));
        let err = tokenize(&long_name).unwrap_err();
        assert!(matches!(err, Scottatura::IdentifierTooLong { .. }));
    }

    #[test]
    fn test_tokenize_real_literal_does_not_panic_and_parses() {
        // The scanning logic only admits digit sequences into the real-literal
        // parse path, so a genuinely unparsable "REAL" text is unreachable via
        // normal tokenizing today. This test locks in that well-formed reals
        // still parse correctly via the non-panicking `.map_err` path (rather
        // than the old `.unwrap()`), so a future change to the digit scan
        // can't silently reintroduce a panic on malformed input.
        let toks = kinds("123.456");
        assert_eq!(toks, vec![Token::RealLiteral(123.456), Token::Eof]);
    }

    #[test]
    fn test_tokenize_rejects_integer_literal_that_overflows_i64() {
        // Unlike the REAL path above, an all-digit sequence *can* fail to
        // parse as i64 once it exceeds i64::MAX, so this path is reachable
        // via normal tokenizing and must surface TIPO_004, not panic.
        let too_big = "99999999999999999999";
        let err = tokenize(too_big).unwrap_err();
        assert!(matches!(err, Scottatura::InvalidLiteral { ref type_name, .. } if type_name == "INTEGER"));
    }

    #[test]
    fn test_tokenize_string_literal_with_multibyte_utf8() {
        let toks = kinds("'café naïve'");
        assert_eq!(toks, vec![Token::StringLiteral("café naïve".into()), Token::Eof]);
    }

    #[test]
    fn test_tokenize_quoted_identifier_with_multibyte_utf8() {
        let toks = kinds("\"café\"");
        assert_eq!(toks, vec![Token::Ident("café".into()), Token::Eof]);
    }
}
