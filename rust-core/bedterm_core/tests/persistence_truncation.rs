use bedterm_core::persistence::{Database, KillReason, SnapshotRow};

fn snap(id: &str, host: &str, created: f64) -> SnapshotRow {
    SnapshotRow {
        id: id.into(),
        host_id: host.into(),
        kill_reason: None,
        killed_at: None,
        last_cwd: None,
        last_command: None,
        last_exit_code: None,
        created_at: created,
        owner_pid: None,
        owner_boot_time: None,
    }
}

#[test]
fn per_host_cap_keeps_newest_10() {
    let db = Database::open_in_memory().unwrap();
    for i in 0..12 {
        let id = format!("s{i:02}");
        db.insert_snapshot(&snap(&id, "h1", 1.0 + i as f64))
            .unwrap();
        db.record_kill(&id, KillReason::UserKilled, None, None, None)
            .unwrap();
    }
    db.enforce_host_cap("h1").unwrap();
    let kept = db.list_snapshots("h1").unwrap();
    assert_eq!(kept.len(), 10);
    let ids: Vec<&str> = kept.iter().map(|r| r.id.as_str()).collect();
    assert!(!ids.contains(&"s00"));
    assert!(!ids.contains(&"s01"));
    assert!(ids.contains(&"s11"));
}

#[test]
fn record_kill_runs_enforcement() {
    // record_kill should call enforce_host_cap implicitly so we never
    // grow past 10 snapshots per host even if callers forget.
    let db = Database::open_in_memory().unwrap();
    for i in 0..12 {
        let id = format!("s{i:02}");
        db.insert_snapshot(&snap(&id, "h1", 1.0 + i as f64))
            .unwrap();
        db.record_kill(&id, KillReason::UserKilled, None, None, None)
            .unwrap();
    }
    let kept = db.list_snapshots("h1").unwrap();
    assert_eq!(kept.len(), 10);
}
