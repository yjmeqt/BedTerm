use bedterm_core::persistence::{unix_seconds_now, Database, KillReason, SnapshotRow};

fn fixture(id: &str, host: &str) -> SnapshotRow {
    SnapshotRow {
        id: id.into(),
        host_id: host.into(),
        kill_reason: None,
        killed_at: None,
        last_cwd: None,
        last_command: None,
        last_exit_code: None,
        created_at: unix_seconds_now(),
        owner_pid: Some(42),
        owner_boot_time: Some(100.0),
    }
}

#[test]
fn insert_then_list_for_host() {
    let db = Database::open_in_memory().unwrap();
    db.insert_snapshot(&fixture("s1", "h1")).unwrap();
    db.insert_snapshot(&fixture("s2", "h1")).unwrap();
    db.insert_snapshot(&fixture("s3", "h2")).unwrap();
    let rows = db.list_snapshots("h1").unwrap();
    assert_eq!(rows.len(), 2);
    let ids: Vec<_> = rows.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&"s1") && ids.contains(&"s2"));
}

#[test]
fn record_kill_updates_row() {
    let db = Database::open_in_memory().unwrap();
    db.insert_snapshot(&fixture("s1", "h1")).unwrap();
    db.record_kill(
        "s1",
        KillReason::UserKilled,
        Some("/tmp"),
        Some("ls"),
        Some(0),
    )
    .unwrap();
    let row = db.list_snapshots("h1").unwrap().pop().unwrap();
    assert_eq!(row.kill_reason, Some(KillReason::UserKilled));
    assert!(row.killed_at.is_some());
    assert_eq!(row.last_cwd.as_deref(), Some("/tmp"));
    assert_eq!(row.last_command.as_deref(), Some("ls"));
    assert_eq!(row.last_exit_code, Some(0));
}

#[test]
fn discard_removes_snapshot() {
    let db = Database::open_in_memory().unwrap();
    db.insert_snapshot(&fixture("s1", "h1")).unwrap();
    db.discard_snapshot("s1").unwrap();
    assert!(db.list_snapshots("h1").unwrap().is_empty());
}

#[test]
fn discard_for_host_clears_only_that_host() {
    let db = Database::open_in_memory().unwrap();
    db.insert_snapshot(&fixture("s1", "h1")).unwrap();
    db.insert_snapshot(&fixture("s2", "h2")).unwrap();
    db.discard_for_host("h1").unwrap();
    assert!(db.list_snapshots("h1").unwrap().is_empty());
    assert_eq!(db.list_snapshots("h2").unwrap().len(), 1);
}
