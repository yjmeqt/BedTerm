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

// ── Bug-fix regression tests ──────────────────────────────────────────────────

/// Bug 1 regression: commands / paths containing quotes or backslashes must
/// not break the DCS JSON parser during replay.
///
/// The fix in `persistence/ffi.rs` JSON-escapes `cwd` and `command` before
/// interpolating them into the synthesized Precmd/Preexec frames. This test
/// drives the path end-to-end through `bedterm_persistence_open_replay` and
/// asserts the returned terminal is non-null (would be null if the parser
/// failed) and contains at least one block with the correct command text.
#[test]
fn replay_handles_command_with_quotes_and_backslashes() {
    use bedterm_core::blocks_ffi::{bt_term_block_at, bt_term_block_count, BtBlockView};
    use bedterm_core::ffi::bt_term_free;
    use bedterm_core::persistence::ffi::{
        bedterm_persistence_close, bedterm_persistence_init, bedterm_persistence_open_replay,
    };
    use bedterm_core::persistence::{BlockRow, Database, SnapshotRow};
    use std::ffi::CString;
    use std::mem::MaybeUninit;

    // Prepare a temp DB with one block whose command and cwd contain JSON-
    // hostile characters: double-quotes and backslashes.
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap();
    {
        let db = Database::open(path).unwrap();
        db.insert_snapshot(&SnapshotRow {
            id: "s1".into(),
            host_id: "h1".into(),
            kill_reason: None,
            killed_at: None,
            last_cwd: None,
            last_command: None,
            last_exit_code: None,
            created_at: 0.0,
            owner_pid: None,
            owner_boot_time: None,
        })
        .unwrap();
        db.insert_block(&BlockRow {
            snapshot_id: "s1".into(),
            block_seq: 1,
            command: r#"echo "hello world""#.into(), // embedded double-quotes
            stylized_command: vec![],
            stylized_output: b"hello world\n".to_vec(),
            exit_code: Some(0),
            cwd: Some(r#"/path/with"weird\\stuff"#.into()), // quotes + backslash
            git_branch: None,
            started_at: 0.0,
            finished_at: 1.0,
        })
        .unwrap();
    }

    let cpath = CString::new(path).unwrap();
    let sid = CString::new("s1").unwrap();
    let h = unsafe { bedterm_persistence_init(cpath.as_ptr()) };
    assert!(!h.is_null());

    let term = unsafe { bedterm_persistence_open_replay(h, sid.as_ptr()) };
    assert!(
        !term.is_null(),
        "open_replay returned null — JSON injection likely broke the parser"
    );

    // Verify the block was reconstructed with the correct command text.
    let count = unsafe { bt_term_block_count(term) };
    assert_eq!(count, 1, "expected 1 replay block, got {count}");

    let mut view = MaybeUninit::<BtBlockView>::uninit();
    let rc = unsafe { bt_term_block_at(term, 0, view.as_mut_ptr()) };
    assert_eq!(rc, 0);
    let view = unsafe { view.assume_init() };
    let cmd_bytes = unsafe { std::slice::from_raw_parts(view.command, view.command_len) };
    let cmd_str = std::str::from_utf8(cmd_bytes).unwrap();
    assert_eq!(
        cmd_str.trim(),
        r#"echo "hello world""#,
        "command text was corrupted during replay"
    );

    unsafe { bt_term_free(term) };
    unsafe { bedterm_persistence_close(h) };
}

/// Bug 2 regression: a block stored with `exit_code: None` (Ctrl-C path)
/// must NOT produce a `CommandFinished` frame with `exit_code: 0` during
/// replay. That would falsify the command's outcome.
///
/// The fix omits the `CommandFinished` frame entirely when `exit_code` is
/// `None`. The next block's `Precmd` event seals the still-running block
/// with `exit_code: None` via `BlockStore::seal_open` — which is exactly
/// what happens in a live session when the user Ctrl-Cs.
///
/// For a single-block snapshot there is no following `Precmd`, so the block
/// stays open (is_running == true). We assert `has_exit_code == 0`.
#[test]
fn replay_block_with_no_exit_code_does_not_show_zero() {
    use bedterm_core::blocks_ffi::{bt_term_block_at, bt_term_block_count, BtBlockView};
    use bedterm_core::ffi::bt_term_free;
    use bedterm_core::persistence::ffi::{
        bedterm_persistence_close, bedterm_persistence_init, bedterm_persistence_open_replay,
    };
    use bedterm_core::persistence::{BlockRow, Database, SnapshotRow};
    use std::ffi::CString;
    use std::mem::MaybeUninit;

    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap();
    {
        let db = Database::open(path).unwrap();
        db.insert_snapshot(&SnapshotRow {
            id: "s1".into(),
            host_id: "h1".into(),
            kill_reason: None,
            killed_at: None,
            last_cwd: None,
            last_command: None,
            last_exit_code: None,
            created_at: 0.0,
            owner_pid: None,
            owner_boot_time: None,
        })
        .unwrap();
        db.insert_block(&BlockRow {
            snapshot_id: "s1".into(),
            block_seq: 1,
            command: "long-running-cmd".into(),
            stylized_command: vec![],
            stylized_output: b"some output\n".to_vec(),
            exit_code: None, // Ctrl-C — no exit code recorded
            cwd: None,
            git_branch: None,
            started_at: 0.0,
            finished_at: 1.0,
        })
        .unwrap();
    }

    let cpath = CString::new(path).unwrap();
    let sid = CString::new("s1").unwrap();
    let h = unsafe { bedterm_persistence_init(cpath.as_ptr()) };
    assert!(!h.is_null());

    let term = unsafe { bedterm_persistence_open_replay(h, sid.as_ptr()) };
    assert!(!term.is_null());

    let count = unsafe { bt_term_block_count(term) };
    assert_eq!(count, 1, "expected 1 replay block, got {count}");

    let mut view = MaybeUninit::<BtBlockView>::uninit();
    let rc = unsafe { bt_term_block_at(term, 0, view.as_mut_ptr()) };
    assert_eq!(rc, 0);
    let view = unsafe { view.assume_init() };

    // The block must NOT carry a false exit_code of 0.
    // Approach taken: skip CommandFinished entirely when exit_code is None.
    // Without a following Precmd to seal it, the block stays running
    // (is_running == 1) with no exit code recorded (has_exit_code == 0).
    assert_eq!(
        view.has_exit_code, 0,
        "block with exit_code: None must not expose exit_code 0 — \
         was falsely set to 0 before the fix"
    );

    unsafe { bt_term_free(term) };
    unsafe { bedterm_persistence_close(h) };
}
