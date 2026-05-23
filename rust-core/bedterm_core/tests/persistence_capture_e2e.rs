//! End-to-end test: `Terminal::attach_persistence` → `Database::insert_block`.
//!
//! # Protocol note
//!
//! BedTerm's shell integration uses **DCS JSON** frames (not OSC 133 byte
//! sequences). The `command` field on a `Block` is populated by the
//! `Preexec` DCS event handler. OSC 133 is not parsed by this codebase.
//! All fixtures here use the DCS wire format: `ESC P $ d <hex(JSON)> 0x9C`.

use bedterm_core::persistence::{unix_seconds_now, Database, SnapshotRow};
use bedterm_core::term::Terminal;

/// Build a hex-encoded DCS frame: `ESC P $ d <hex(json)> 0x9C`.
fn dcs(json: &str) -> Vec<u8> {
    use std::fmt::Write as _;
    let mut hex = String::with_capacity(json.len() * 2);
    for b in json.bytes() {
        write!(hex, "{b:02x}").unwrap();
    }
    let mut v = vec![0x1B, b'P', b'$', b'd'];
    v.extend_from_slice(hex.as_bytes());
    v.push(0x9C);
    v
}

/// Assemble one complete block sequence via DCS frames:
///   Precmd (pwd) → Preexec (command) → output bytes → CommandFinished (exit).
fn dcs_block(pwd: &str, command: &str, output: &[u8], exit: i32) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&dcs(&format!(
        r#"{{"hook":"Precmd","value":{{"pwd":"{pwd}"}}}}"#
    )));
    v.extend_from_slice(&dcs(&format!(
        r#"{{"hook":"Preexec","value":{{"command":"{command}"}}}}"#
    )));
    v.extend_from_slice(output);
    v.extend_from_slice(&dcs(&format!(
        r#"{{"hook":"CommandFinished","value":{{"exit_code":{exit}}}}}"#
    )));
    v
}

#[test]
fn finalized_block_lands_in_sqlite() {
    let db = Database::open_in_memory().unwrap();
    db.insert_snapshot(&SnapshotRow {
        id: "s1".into(),
        host_id: "h1".into(),
        kill_reason: None,
        killed_at: None,
        last_cwd: None,
        last_command: None,
        last_exit_code: None,
        created_at: unix_seconds_now(),
        owner_pid: None,
        owner_boot_time: None,
    })
    .unwrap();

    let mut term = Terminal::new(80, 24);
    term.attach_persistence(&db, "s1");

    // Feed two complete blocks.
    term.feed(&dcs_block("/tmp", "ls", b"a b c\n", 0));
    term.feed(&dcs_block("/tmp", "pwd", b"/tmp\n", 0));

    let rows = db.load_blocks("s1").unwrap();
    assert_eq!(
        rows.len(),
        2,
        "expected 2 blocks in SQLite, got {}",
        rows.len()
    );
    assert_eq!(rows[0].exit_code, Some(0), "first block exit_code");
    assert_eq!(
        rows[0].command.trim(),
        "ls",
        "first block command; got {:?}",
        rows[0].command
    );
    assert_eq!(rows[1].command.trim(), "pwd", "second block command");
    assert!(
        rows[1].stylized_output.windows(4).any(|w| w == b"/tmp"),
        "second block stylized_output should contain /tmp; got {:?}",
        String::from_utf8_lossy(&rows[1].stylized_output)
    );
}

#[test]
fn started_at_lte_finished_at() {
    let db = Database::open_in_memory().unwrap();
    db.insert_snapshot(&SnapshotRow {
        id: "s2".into(),
        host_id: "h1".into(),
        kill_reason: None,
        killed_at: None,
        last_cwd: None,
        last_command: None,
        last_exit_code: None,
        created_at: unix_seconds_now(),
        owner_pid: None,
        owner_boot_time: None,
    })
    .unwrap();

    let mut term = Terminal::new(80, 24);
    term.attach_persistence(&db, "s2");
    term.feed(&dcs_block("/home", "echo hi", b"hi\n", 0));

    let rows = db.load_blocks("s2").unwrap();
    assert_eq!(rows.len(), 1);
    assert!(
        rows[0].started_at <= rows[0].finished_at,
        "started_at ({}) must be <= finished_at ({})",
        rows[0].started_at,
        rows[0].finished_at
    );
    // finished_at must be a reasonable Unix timestamp (after year 2020).
    assert!(
        rows[0].finished_at > 1_577_836_800.0,
        "finished_at looks wrong: {}",
        rows[0].finished_at
    );
}

#[test]
fn ctrl_c_block_persisted_without_exit_code() {
    // Ctrl-C path: shell sends Precmd → Preexec → then another Precmd
    // without a CommandFinished. The first block should be sealed and
    // persisted with exit_code = NULL.
    let db = Database::open_in_memory().unwrap();
    db.insert_snapshot(&SnapshotRow {
        id: "s3".into(),
        host_id: "h1".into(),
        kill_reason: None,
        killed_at: None,
        last_cwd: None,
        last_command: None,
        last_exit_code: None,
        created_at: unix_seconds_now(),
        owner_pid: None,
        owner_boot_time: None,
    })
    .unwrap();

    let mut term = Terminal::new(80, 24);
    term.attach_persistence(&db, "s3");

    // First block: killed before CommandFinished.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&dcs(r#"{"hook":"Precmd","value":{"pwd":"/"}}"#));
    bytes.extend_from_slice(&dcs(
        r#"{"hook":"Preexec","value":{"command":"sleep 100"}}"#,
    ));
    // Second Precmd seals the first block (Ctrl-C path).
    bytes.extend_from_slice(&dcs(r#"{"hook":"Precmd","value":{"pwd":"/"}}"#));
    // Second block completes normally.
    bytes.extend_from_slice(&dcs(r#"{"hook":"Preexec","value":{"command":"date"}}"#));
    bytes.extend_from_slice(&dcs(
        r#"{"hook":"CommandFinished","value":{"exit_code":0}}"#,
    ));
    term.feed(&bytes);

    let rows = db.load_blocks("s3").unwrap();
    assert_eq!(rows.len(), 2, "expected 2 blocks; got {}", rows.len());
    // First block (sleep 100) was killed — no exit code.
    assert_eq!(rows[0].command, "sleep 100");
    assert!(
        rows[0].exit_code.is_none(),
        "killed block should have NULL exit_code"
    );
    // Second block completed normally.
    assert_eq!(rows[1].command, "date");
    assert_eq!(rows[1].exit_code, Some(0));
}
