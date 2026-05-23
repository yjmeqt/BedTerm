use bedterm_core::persistence::{unix_seconds_now, BlockRow, Database, KillReason, SnapshotRow};

#[test]
fn open_sweeps_orphans_as_app_relaunch() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap();
    {
        let db = Database::open(path).unwrap();
        db.insert_snapshot(&SnapshotRow {
            id: "orphan1".into(),
            host_id: "h1".into(),
            kill_reason: None,
            killed_at: None,
            last_cwd: None,
            last_command: None,
            last_exit_code: None,
            created_at: 1_700_000_000.0,
            owner_pid: Some(99),
            owner_boot_time: Some(500.0),
        })
        .unwrap();
        db.insert_block(&BlockRow {
            snapshot_id: "orphan1".into(),
            block_seq: 1,
            command: "ls".into(),
            stylized_command: b"$ ls\n".to_vec(),
            stylized_output: b"a b c\n".to_vec(),
            exit_code: Some(0),
            cwd: Some("/tmp".into()),
            git_branch: None,
            started_at: 1_700_000_001.0,
            finished_at: 1_700_000_002.0,
        })
        .unwrap();
    }
    let db2 = Database::open(path).unwrap();
    let row = db2.list_snapshots("h1").unwrap().pop().unwrap();
    assert_eq!(row.kill_reason, Some(KillReason::AppRelaunch));
    assert!(row.killed_at.is_some());
    assert!((row.killed_at.unwrap() - 1_700_000_002.0).abs() < 1e-6);
}

#[test]
fn open_with_no_orphans_is_noop() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap();
    {
        let db = Database::open(path).unwrap();
        db.insert_snapshot(&SnapshotRow {
            id: "s1".into(),
            host_id: "h1".into(),
            kill_reason: Some(KillReason::UserKilled),
            killed_at: Some(unix_seconds_now()),
            last_cwd: None,
            last_command: None,
            last_exit_code: None,
            created_at: unix_seconds_now(),
            owner_pid: None,
            owner_boot_time: None,
        })
        .unwrap();
    }
    let db2 = Database::open(path).unwrap();
    let rows = db2.list_snapshots("h1").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kill_reason, Some(KillReason::UserKilled));
}
