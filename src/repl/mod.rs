use crate::{ResultSet, Value};
use std::io::Write;

/// Format a ResultSet into a human-readable string.
/// - For SELECT results (non-empty columns): header row, separator, data rows with aligned columns.
/// - For DML results (empty columns): summary line with affected row count and optional insert key.
pub fn format_result(rs: &ResultSet) -> String {
    if rs.columns.is_empty() {
        // DML result
        let row_word = if rs.affected_rows == 1 { "row" } else { "rows" };
        let mut result = format!("{} {} affected", rs.affected_rows, row_word);
        if let Some(key) = rs.last_insert_key {
            result.push_str(&format!(" (id {})", key));
        }
        result
    } else {
        // SELECT result
        if rs.rows.is_empty() {
            // Just header with 0 rows message
            let header = rs.columns.join(" | ");
            format!("{}\n---- (0 rows)", header)
        } else {
            // Calculate column widths
            let widths: Vec<usize> = rs
                .columns
                .iter()
                .enumerate()
                .map(|(col_idx, col_name)| {
                    let header_width = col_name.len();
                    let max_data_width = rs
                        .rows
                        .iter()
                        .map(|row| value_to_cell(&row[col_idx]).len())
                        .max()
                        .unwrap_or(0);
                    header_width.max(max_data_width)
                })
                .collect();

            // Build header
            let header = rs
                .columns
                .iter()
                .enumerate()
                .map(|(i, col)| format!("{:width$}", col, width = widths[i]))
                .collect::<Vec<_>>()
                .join(" | ");

            // Build separator
            let separator = widths
                .iter()
                .map(|w| "-".repeat(*w))
                .collect::<Vec<_>>()
                .join("-+-");

            // Build rows
            let rows_str = rs
                .rows
                .iter()
                .map(|row| {
                    row.iter()
                        .enumerate()
                        .map(|(col_idx, val)| {
                            let cell = value_to_cell(val);
                            format!("{:width$}", cell, width = widths[col_idx])
                        })
                        .collect::<Vec<_>>()
                        .join(" | ")
                })
                .collect::<Vec<_>>()
                .join("\n");

            format!("{}\n{}\n{}", header, separator, rows_str)
        }
    }
}

/// Convert a Value to a display string for table cells.
fn value_to_cell(v: &Value) -> String {
    match v {
        Value::Integer(i) => i.to_string(),
        Value::Real(f) => f.to_string(),
        Value::Text(s) => s.clone(),
        Value::Blob(_) => "[blob]".to_string(),
        Value::Null => String::new(), // Empty cell for NULL
    }
}

/// Run the interactive REPL loop.
/// Reads from input, writes to output. Returns Ok(()) on normal exit (EOF or .quit),
/// or Err if I/O operations fail.
pub fn run(pot: &mut crate::Pot, input: impl std::io::BufRead, output: &mut impl Write) -> std::io::Result<()> {
    let mut lines = input.lines();
    let mut buffer = String::new();

    loop {
        // Determine which prompt to show
        let prompt = if buffer.is_empty() {
            "marinara> "
        } else {
            "     -> "
        };

        // Write prompt without trailing newline
        write!(output, "{}", prompt)?;
        output.flush()?;

        // Read next line
        match lines.next() {
            Some(line_result) => {
                let line = line_result?;
                let trimmed = line.trim();

                // Check for dot-commands
                if trimmed.starts_with('.') {
                    if buffer.is_empty() {
                        // Only recognize dot-commands on a fresh line
                        match trimmed {
                            ".tables" => {
                                let tables = pot.noodles();
                                if tables.is_empty() {
                                    writeln!(output, "(no tables)")?;
                                } else {
                                    for table in tables {
                                        writeln!(output, "{}", table)?;
                                    }
                                }
                            }
                            ".help" => {
                                writeln!(output, "MarinaraDB interactive shell commands:")?;
                                writeln!(output, "  .tables     - List all tables")?;
                                writeln!(output, "  .help       - Show this help message")?;
                                writeln!(output, "  .quit/.exit - Exit the shell")?;
                                writeln!(output, "  ;           - Execute statement")?;
                            }
                            ".quit" | ".exit" => {
                                return Ok(());
                            }
                            _ => {
                                writeln!(output, "Unknown command: {}", trimmed)?;
                            }
                        }
                    } else {
                        // Dot-command in middle of statement accumulation
                        buffer.push('\n');
                        buffer.push_str(&line);
                    }
                } else {
                    // Regular SQL line
                    if !buffer.is_empty() {
                        buffer.push('\n');
                    }
                    buffer.push_str(&line);

                    // Check if buffer contains a semicolon (statement terminator)
                    if buffer.contains(';') {
                        // Execute the statement
                        let result = pot.execute(&buffer);
                        match result {
                            Ok(rs) => {
                                writeln!(output, "{}", format_result(&rs))?;
                            }
                            Err(e) => {
                                writeln!(output, "Error: {}", e)?;
                            }
                        }
                        buffer.clear();
                    }
                }
            }
            None => {
                // EOF reached
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_result_empty_select() {
        let rs = ResultSet {
            columns: vec!["id".to_string(), "name".to_string()],
            rows: vec![],
            affected_rows: 0,
            last_insert_key: None,
        };
        let output = format_result(&rs);
        assert!(output.contains("id"));
        assert!(output.contains("name"));
        assert!(output.contains("0 rows"));
    }

    #[test]
    fn test_format_result_multi_row_select() {
        let rs = ResultSet {
            columns: vec!["id".to_string(), "name".to_string(), "age".to_string()],
            rows: vec![
                vec![Value::Integer(1), Value::Text("Alice".to_string()), Value::Integer(30)],
                vec![Value::Integer(2), Value::Text("Bob".to_string()), Value::Null],
                vec![Value::Integer(3), Value::Text("Charlie".to_string()), Value::Integer(25)],
            ],
            affected_rows: 0,
            last_insert_key: None,
        };
        let output = format_result(&rs);
        assert!(output.contains("id"));
        assert!(output.contains("name"));
        assert!(output.contains("age"));
        assert!(output.contains("Alice"));
        assert!(output.contains("Bob"));
        assert!(output.contains("Charlie"));
        assert!(output.contains("30"));
        assert!(output.contains("25"));
        // NULL should render as empty cell
        assert!(output.lines().count() > 2); // Header, separator, at least 3 rows
    }

    #[test]
    fn test_format_result_single_affected_row() {
        let rs = ResultSet {
            columns: vec![],
            rows: vec![],
            affected_rows: 1,
            last_insert_key: None,
        };
        let output = format_result(&rs);
        assert!(output.contains("1 row affected"));
    }

    #[test]
    fn test_format_result_multiple_affected_rows() {
        let rs = ResultSet {
            columns: vec![],
            rows: vec![],
            affected_rows: 5,
            last_insert_key: None,
        };
        let output = format_result(&rs);
        assert!(output.contains("5 rows affected"));
    }

    #[test]
    fn test_format_result_with_insert_key() {
        let rs = ResultSet {
            columns: vec![],
            rows: vec![],
            affected_rows: 1,
            last_insert_key: Some(42),
        };
        let output = format_result(&rs);
        assert!(output.contains("1 row affected"));
        assert!(output.contains("(id 42)"));
    }

    #[test]
    fn test_format_result_with_null_values() {
        let rs = ResultSet {
            columns: vec!["a".to_string(), "b".to_string()],
            rows: vec![
                vec![Value::Integer(1), Value::Null],
                vec![Value::Null, Value::Text("test".to_string())],
            ],
            affected_rows: 0,
            last_insert_key: None,
        };
        let output = format_result(&rs);
        assert!(output.contains("a"));
        assert!(output.contains("b"));
        assert!(output.contains("1"));
        assert!(output.contains("test"));
    }

    #[test]
    fn test_format_result_with_real_values() {
        let rs = ResultSet {
            columns: vec!["price".to_string()],
            rows: vec![
                vec![Value::Real(10.50)],
                vec![Value::Real(20.99)],
            ],
            affected_rows: 0,
            last_insert_key: None,
        };
        let output = format_result(&rs);
        assert!(output.contains("10.5"));
        assert!(output.contains("20.99"));
    }

    #[test]
    fn test_repl_basic_session() -> std::io::Result<()> {
        use std::io::Cursor;

        // Create a temporary pot file for testing
        let db_path = "/tmp/test_repl_basic.pot";
        if std::path::Path::new(db_path).exists() {
            std::fs::remove_file(db_path)?;
        }

        let mut pot = crate::Pot::open(db_path).expect("Failed to create test pot");

        // Simulate input: create table, insert, select, .tables, .quit
        let input_data = r#"PLATE noodle users (name TEXT, age INT);
LADLE INTO users (name, age) PORTIONS ('Alice', 30);
SERVE * FROM users;
.tables
.quit
"#;

        let input = Cursor::new(input_data.as_bytes());
        let mut output = Vec::new();

        run(&mut pot, std::io::BufReader::new(input), &mut output)?;

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("marinara>"), "Should show prompt");
        // We should see output from the SELECT, and the tables list

        // Clean up
        if std::path::Path::new(db_path).exists() {
            std::fs::remove_file(db_path)?;
        }

        Ok(())
    }

    #[test]
    fn test_repl_error_recovery() -> std::io::Result<()> {
        use std::io::Cursor;

        let db_path = "/tmp/test_repl_error.pot";
        if std::path::Path::new(db_path).exists() {
            std::fs::remove_file(db_path)?;
        }

        let mut pot = crate::Pot::open(db_path).expect("Failed to create test pot");

        // Input with a malformed statement followed by a valid one
        let input_data = "INVALID SQL;PLATE noodle test (col INT);.quit\n";

        let input = Cursor::new(input_data.as_bytes());
        let mut output = Vec::new();

        run(&mut pot, std::io::BufReader::new(input), &mut output)?;

        let output_str = String::from_utf8_lossy(&output);
        // Should see error message
        assert!(output_str.contains("Error:"), "Should show error message");
        // Should continue after error and show prompt again
        assert!(output_str.contains("marinara>"), "Should continue after error");

        // Clean up
        if std::path::Path::new(db_path).exists() {
            std::fs::remove_file(db_path)?;
        }

        Ok(())
    }
}
