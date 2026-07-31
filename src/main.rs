use std::io::{BufReader, Write};

fn main() -> std::io::Result<()> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "marinara.pot".to_string());
    let mut pot = match marinaradb::Pot::open(&path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Could not open {path}: {e}");
            std::process::exit(1);
        }
    };
    println!("MarinaraDB v0.1.0 — Buon appetito!");
    println!("Connected to {path}. Type .help for commands, .quit to exit.");
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    marinaradb::repl::run(&mut pot, BufReader::new(stdin.lock()), &mut stdout)?;
    stdout.flush()
}
