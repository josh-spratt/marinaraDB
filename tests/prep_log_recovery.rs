use marinaradb::stove::page::{Page, PageType};
use marinaradb::stove::pot_file::PotFile;
use marinaradb::{ColType, ColumnDef, Pot, Value};

fn temp_path() -> String {
    let f = tempfile::NamedTempFile::new().unwrap();
    let p = f.path().to_str().unwrap().to_string();
    drop(f);
    p
}

fn nums_col() -> Vec<ColumnDef> {
    vec![ColumnDef { name: "n".into(), col_type: ColType::Integer, nullable: false, strainer_root: None }]
}

#[test]
fn test_committed_write_survives_simulated_pot_write_loss() {
    let path = temp_path();
    let prep_path = format!("{path}.prep");
    {
        let mut pot = Pot::open(&path).unwrap();
        pot.create_table("nums", nums_col()).unwrap();
        pot.insert_strand("nums", vec![Value::Integer(42)]).unwrap();
        // A fresh single-table database always gets catalog root = page 1,
        // heap root = page 2 (PotFile::create leaves page_count at 1 for the
        // header page; Catalog::init claims page 1, create_table claims page 2).
    }

    // Simulate a crash where the .pot write was lost but the Prep Log's
    // commit record made it to disk: blank out the heap page directly.
    {
        let mut raw = PotFile::open(&path).unwrap();
        raw.write_page(&Page::new(2, PageType::Heap)).unwrap();
    }

    let mut pot = Pot::open(&path).unwrap();
    let rows = pot.scan_noodle("nums").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1[0], Value::Integer(42));

    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&prep_path).ok();
}

#[test]
fn test_torn_tail_discarded_without_losing_prior_committed_write() {
    let path = temp_path();
    let prep_path = format!("{path}.prep");
    {
        let mut pot = Pot::open(&path).unwrap();
        pot.create_table("nums", nums_col()).unwrap();
        pot.insert_strand("nums", vec![Value::Integer(7)]).unwrap();
    }

    // Simulate the .pot write being lost...
    {
        let mut raw = PotFile::open(&path).unwrap();
        raw.write_page(&Page::new(2, PageType::Heap)).unwrap();
    }

    // ...and a torn/partial record dangling at the end of the prep log, as
    // if a crash interrupted mid-write of the next statement's record.
    {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new().append(true).open(&prep_path).unwrap();
        file.write_all(&[0xAB; 20]).unwrap(); // shorter than the 36-byte header
    }

    let mut pot = Pot::open(&path).unwrap();
    let rows = pot.scan_noodle("nums").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1[0], Value::Integer(7));

    // Prove the reopened Pot is still fully writable after a torn-tail recovery.
    pot.insert_strand("nums", vec![Value::Integer(8)]).unwrap();
    let rows = pot.scan_noodle("nums").unwrap();
    assert_eq!(rows.len(), 2);

    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&prep_path).ok();
}

#[test]
fn test_checkpoint_shrinks_prep_log_file() {
    let path = temp_path();
    let prep_path = format!("{path}.prep");
    let mut pot = Pot::open(&path).unwrap();
    pot.create_table("nums", nums_col()).unwrap();

    for i in 0..520 {
        pot.insert_strand("nums", vec![Value::Integer(i)]).unwrap();
    }

    let size_after = std::fs::metadata(&prep_path).unwrap().len();
    // Each insert logs one full 4096-byte page image plus a commit record
    // (~4176 bytes/insert). 520 inserts uncheckpointed would be ~2.1MB; a
    // checkpoint fires automatically at record 1000 (insert 500), truncating
    // the log back to just the checkpoint record plus the ~20 records from
    // inserts 501-520 (~84KB) — well under a tenth of the pre-checkpoint size.
    assert!(size_after < 500_000, "expected checkpoint to shrink the log, got {size_after} bytes");

    drop(pot);

    let mut reopened = Pot::open(&path).unwrap();
    let rows = reopened.scan_noodle("nums").unwrap();
    assert_eq!(rows.len(), 520);

    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&prep_path).ok();
}

#[test]
fn test_uncommitted_write_after_committed_predecessor_is_not_replayed() {
    use marinaradb::prep_log::record::LogRecord;

    let path = temp_path();
    let prep_path = format!("{path}.prep");

    // Both inserts must happen in ONE session: `Pot::open` reinitializes the
    // .prep file after recovery, so a second session's log would not contain
    // the first insert's Commit — and it's precisely that earlier Commit under
    // the reused autocommit Simmer ID `0` that used to poison the replay.
    let snapshot_after_first = {
        let mut pot = Pot::open(&path).unwrap();
        pot.create_table("nums", nums_col()).unwrap();
        pot.insert_strand("nums", vec![Value::Integer(111)]).unwrap();

        // Snapshot page 2 right after the first (committed) insert, before the
        // second insert touches it.
        let snapshot = {
            let mut raw = PotFile::open(&path).unwrap();
            raw.read_page(2).unwrap().data
        };

        pot.insert_strand("nums", vec![Value::Integer(222)]).unwrap();
        snapshot
    };

    // Simulate a crash between the second insert's write record and its
    // commit record: truncate the .prep file right before the fourth
    // record (the second insert's Commit — records on disk are Write1,
    // Commit1, Write2, Commit2), and roll .pot's page 2 back to its
    // pre-second-insert state, as if that flush never happened either.
    let prep_bytes = std::fs::read(&prep_path).unwrap();
    let mut cursor = 0usize;
    for _ in 0..3 {
        let (_, consumed) = LogRecord::decode(&prep_bytes[cursor..]).unwrap().unwrap();
        cursor += consumed;
    }
    std::fs::write(&prep_path, &prep_bytes[..cursor]).unwrap();

    {
        let mut raw = PotFile::open(&path).unwrap();
        raw.write_page(&Page { number: 2, data: snapshot_after_first }).unwrap();
    }

    let mut pot = Pot::open(&path).unwrap();
    let rows = pot.scan_noodle("nums").unwrap();
    assert_eq!(rows.len(), 1, "the second insert never committed and must not be replayed, even though Simmer ID 0 committed earlier in the log");
    assert_eq!(rows[0].1[0], Value::Integer(111));

    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&prep_path).ok();
}
