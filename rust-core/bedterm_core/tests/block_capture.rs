use bedterm_core::term::Terminal;

/// Build a hex-encoded DCS frame: `ESC P $ d <hex(json)> 0x9C`.
/// This is the Warp-compatible wire format the DCS sniffer expects.
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

/// Assemble a complete block sequence: Precmd → Preexec (with `command`)
/// → `output` bytes → CommandFinished (with `exit`).
fn dcs_block(command: &str, output: &[u8], exit: i32) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&dcs(r#"{"hook":"Precmd","value":{"pwd":"/tmp"}}"#));
    v.extend_from_slice(&dcs(&format!(
        r#"{{"hook":"Preexec","value":{{"command":"{command}"}}}}"#
    )));
    v.extend_from_slice(output);
    v.extend_from_slice(&dcs(&format!(
        r#"{{"hook":"CommandFinished","value":{{"exit_code":{exit}}}}}"#
    )));
    v
}

/// Note on `stylized_command`:
/// The DCS-based protocol used by BedTerm has no separate byte stream for
/// the "command echo" phase: `Preexec` delivers the command text via the
/// JSON payload, and all bytes between `Preexec` and `CommandFinished` are
/// command *output* bytes. Therefore `stylized_command` is always empty in
/// this implementation — it is reserved for future protocol extensions
/// (e.g. an iTerm2 OSC 133;B/C split). `stylized_output` holds all
/// byte-level content the command produced.
#[test]
fn block_captures_stylized_output() {
    let mut term = Terminal::new(80, 24);
    term.feed(&dcs_block("ls", b"a b c\n", 0));
    let block = term.blocks().first().expect("block exists");
    assert_eq!(block.command.trim(), "ls");
    // stylized_command is empty because this DCS protocol does not produce
    // a separate command-phase byte stream (see module comment above).
    assert!(
        block.stylized_command.is_empty(),
        "stylized_command should be empty for DCS protocol"
    );
    // stylized_output must contain the raw output bytes.
    assert!(
        block.stylized_output.windows(5).any(|w| w == b"a b c"),
        "stylized_output missing expected content; got {:?}",
        String::from_utf8_lossy(&block.stylized_output)
    );
}

#[test]
fn block_output_capped_at_5000_lines() {
    let mut term = Terminal::new(80, 24);
    let mut payload = Vec::new();
    payload.extend_from_slice(&dcs(r#"{"hook":"Precmd","value":{"pwd":"/tmp"}}"#));
    payload.extend_from_slice(&dcs(r#"{"hook":"Preexec","value":{"command":"cmd"}}"#));
    // Write 6000 lines of output (each is "x\n").
    for _ in 0..6000 {
        payload.push(b'x');
        payload.push(b'\n');
    }
    payload.extend_from_slice(&dcs(
        r#"{"hook":"CommandFinished","value":{"exit_code":0}}"#,
    ));
    term.feed(&payload);
    let block = term.blocks().first().expect("block exists");
    let newlines = block
        .stylized_output
        .iter()
        .filter(|&&b| b == b'\n')
        .count();
    assert_eq!(newlines, 5000, "capture stops at 5000 newlines");
}

#[test]
fn block_stylized_output_empty_for_no_output_command() {
    let mut term = Terminal::new(80, 24);
    term.feed(&dcs_block("true", b"", 0));
    let block = term.blocks().first().expect("block exists");
    assert!(
        block.stylized_output.is_empty(),
        "expected empty stylized_output for command with no output; got {:?}",
        block.stylized_output
    );
}

#[test]
fn stylized_output_preserves_ansi_escapes() {
    let mut term = Terminal::new(80, 24);
    // A bold "hello" in ANSI: ESC[1mhello\nESC[0m
    let output = b"\x1b[1mhello\n\x1b[0m";
    term.feed(&dcs_block("echo", output, 0));
    let block = term.blocks().first().expect("block exists");
    assert!(
        block.stylized_output.windows(5).any(|w| w == b"hello"),
        "ANSI output should preserve raw bytes including escape sequences"
    );
    // The ESC byte should be present in the raw capture.
    assert!(
        block.stylized_output.contains(&0x1B),
        "ESC byte should be present in stylized_output (verbatim capture)"
    );
}
