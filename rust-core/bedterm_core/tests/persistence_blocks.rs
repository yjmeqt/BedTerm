use bedterm_core::persistence::{unix_seconds_now, BlockRow, Database, SnapshotRow};

fn snap(id: &str) -> SnapshotRow {
    SnapshotRow {
        id: id.into(),
        host_id: "h1".into(),
        kill_reason: None,
        killed_at: None,
        last_cwd: None,
        last_command: None,
        last_exit_code: None,
        created_at: unix_seconds_now(),
        owner_pid: None,
        owner_boot_time: None,
    }
}

fn block(snap_id: &str, seq: i64) -> BlockRow {
    BlockRow {
        snapshot_id: snap_id.into(),
        block_seq: seq,
        command: format!("cmd-{seq}"),
        stylized_command: format!("\x1b[1m$ cmd-{seq}\x1b[0m\n").into_bytes(),
        stylized_output: format!("out-{seq}\n").into_bytes(),
        exit_code: Some(0),
        cwd: Some("/tmp".into()),
        git_branch: Some("main".into()),
        started_at: 1_700_000_000.0 + seq as f64,
        finished_at: 1_700_000_000.0 + seq as f64 + 0.5,
    }
}

#[test]
fn insert_then_load_blocks() {
    let db = Database::open_in_memory().unwrap();
    db.insert_snapshot(&snap("s1")).unwrap();
    db.insert_block(&block("s1", 1)).unwrap();
    db.insert_block(&block("s1", 2)).unwrap();
    let loaded = db.load_blocks("s1").unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].block_seq, 1);
    assert_eq!(loaded[1].block_seq, 2);
    assert_eq!(loaded[1].stylized_output, b"out-2\n");
}

#[test]
fn block_truncation_keeps_last_100() {
    let db = Database::open_in_memory().unwrap();
    db.insert_snapshot(&snap("s1")).unwrap();
    for seq in 1..=150 {
        db.insert_block(&block("s1", seq)).unwrap();
    }
    let rows = db.load_blocks("s1").unwrap();
    assert_eq!(rows.len(), 100);
    assert_eq!(rows.first().unwrap().block_seq, 51);
    assert_eq!(rows.last().unwrap().block_seq, 150);
}

#[test]
fn discarding_snapshot_cascades_blocks() {
    let db = Database::open_in_memory().unwrap();
    db.insert_snapshot(&snap("s1")).unwrap();
    db.insert_block(&block("s1", 1)).unwrap();
    db.discard_snapshot("s1").unwrap();
    assert!(db.load_blocks("s1").unwrap().is_empty());
}
