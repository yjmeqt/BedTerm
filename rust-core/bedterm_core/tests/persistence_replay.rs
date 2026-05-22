//! Replay path: stored `stylized_output` bytes reconstruct the block list when
//! fed back into a `Terminal::new_replay` instance.
//!
//! Protocol note: BedTerm uses DCS JSON frames (not OSC 133). See
//! `tests/persistence_capture_e2e.rs` for the format rationale.

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

/// Assemble one complete block: Precmd → Preexec → output bytes → CommandFinished.
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
fn replay_reconstructs_block_list() {
    // 1. Build a live terminal, persist two blocks.
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

    let mut live = Terminal::new(80, 24);
    live.attach_persistence(&db, "s1");
    live.feed(&dcs_block("/tmp", "ls", b"a b c\n", 0));
    live.feed(&dcs_block("/tmp", "pwd", b"/tmp\n", 0));

    let rows = db.load_blocks("s1").unwrap();
    assert_eq!(
        rows.len(),
        2,
        "expected 2 persisted blocks, got {}",
        rows.len()
    );

    // 2. Build a replay terminal and feed it the stored bytes.
    //    `stylized_command` is always empty for the DCS protocol (Preexec
    //    delivers the command text via JSON, not as a separate byte stream).
    //    `stylized_output` holds all output bytes between Preexec and
    //    CommandFinished — but NOT the DCS frames themselves (excluded by
    //    the capture logic).
    //
    //    To reconstruct complete blocks we must wrap each stored output chunk
    //    in the DCS envelope again, because the replay terminal needs the full
    //    Precmd/Preexec/CommandFinished protocol to advance its block state
    //    machine. We synthesize minimal DCS frames from the persisted command
    //    and exit_code metadata and sandwich the raw output bytes in between.
    let mut replay = Terminal::new_replay(80, 24);
    for row in &rows {
        let mut block_bytes = Vec::new();
        block_bytes.extend_from_slice(&dcs(&format!(
            r#"{{"hook":"Precmd","value":{{"pwd":"{}"}}}}"#,
            row.cwd.as_deref().unwrap_or("/")
        )));
        block_bytes.extend_from_slice(&dcs(&format!(
            r#"{{"hook":"Preexec","value":{{"command":"{}"}}}}"#,
            row.command
        )));
        block_bytes.extend_from_slice(&row.stylized_output);
        let exit = row.exit_code.unwrap_or(0);
        block_bytes.extend_from_slice(&dcs(&format!(
            r#"{{"hook":"CommandFinished","value":{{"exit_code":{exit}}}}}"#
        )));
        replay.feed(&block_bytes);
    }

    assert!(
        !replay.blocks().is_empty(),
        "replay produced no blocks from stored bytes"
    );
    assert_eq!(
        replay.blocks().len(),
        rows.len(),
        "replay block count mismatch: got {}, want {}",
        replay.blocks().len(),
        rows.len()
    );
    assert_eq!(replay.blocks()[0].command.trim(), "ls");
    assert_eq!(replay.blocks()[1].command.trim(), "pwd");
}

#[test]
fn replay_terminal_has_no_persistence_sink() {
    // A replay terminal must not persist anything — it has no attached DB.
    // Feeding DCS blocks should populate `blocks()` without any side-effects.
    let mut replay = Terminal::new_replay(80, 24);
    replay.feed(&dcs_block("/", "echo hi", b"hi\n", 0));
    assert_eq!(replay.blocks().len(), 1);
    assert_eq!(replay.blocks()[0].command.trim(), "echo hi");
}
