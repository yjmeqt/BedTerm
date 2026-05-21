//! Warp-compatible DCS shell-integration protocol.
//!
//! Re-implementation of the on-the-wire format Warp uses to ship shell
//! lifecycle events from the remote shell to the terminal. **No code is
//! copied from Warp** (Warp's terminal sources are AGPL-3.0; BedTerm is
//! MIT). Only the wire shape — bytes-on-wire and JSON schema — is
//! reproduced here, which is the part that has to match for shell-side
//! interop.
//!
//! ## Wire frame
//!
//! ```text
//! ESC P $ d <hex-of-JSON> ST_8BIT
//!  │  │ │   │             │
//!  │  │ │   │             └ 0x9C — single-byte string terminator
//!  │  │ │   └ ASCII hex of the JSON body
//!  │  │ └ DCS final byte: 'd' = "hex-encoded JSON" (Warp's
//!  │  │   `HEX_ENCODED_JSON_MARKER`)
//!  │  └ DCS intermediate byte '$' (0x24)
//!  └ DCS introducer (ESC P)
//! ```
//!
//! Hex-encoding the entire JSON body sidesteps any DCS-internal byte that
//! could be mistaken for a terminator (0x9C, ESC) by simpler VTE parsers.
//!
//! ## JSON schema (tagged-enum dispatch)
//!
//! The body decodes to a tagged enum keyed by `"hook"`:
//!
//! ```json
//! {"hook":"Precmd",          "value": {"pwd":"/Users/alice"}}
//! {"hook":"Preexec",         "value": {"command":"ls"}}
//! {"hook":"CommandFinished", "value": {"exit_code":0}}
//! ```
//!
//! BedTerm only consumes these three variants; any other `hook` value
//! (Warp ships many more: `Bootstrapped`, `SSH`, `InitShell`, …) is
//! quietly ignored, leaving room for future expansion without breaking
//! the protocol.
//!
//! Command duration is **not** in the wire format — Warp doesn't ship it
//! either. We measure it on the Rust side as the wall-clock delta between
//! the `Preexec` and `CommandFinished` boundaries in `BlockStore::apply`.

use alacritty_terminal::vte::{Parser, Perform};
use std::collections::VecDeque;

/// One shell-integration event surfaced to the block store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DcsEvent {
    /// `{"hook":"Precmd","value":{"pwd":…,"git_branch":…}}` — shell is
    /// about to draw a prompt. `pwd` is the directory the upcoming
    /// command will run in. `git_branch` is the short branch name (or
    /// short SHA when detached); empty/absent when cwd is outside a
    /// repo or `git` is missing on the remote.
    Precmd {
        pwd: Option<String>,
        git_branch: Option<String>,
    },
    /// `{"hook":"Preexec","value":{"command":…}}` — user pressed Enter.
    Preexec { command: String },
    /// `{"hook":"CommandFinished","value":{"exit_code":N}}` — command
    /// returned. Fired before the precmd that opens the next block.
    CommandFinished { exit_code: i32 },
}

/// Owns a parallel VTE parser dedicated to DCS sniffing. Runs alongside
/// alacritty's main parser; both must see every byte.
pub struct DcsSniffer {
    parser: Parser,
    sink: Sink,
}

#[derive(Default)]
struct Sink {
    is_ours: bool,
    buf: Vec<u8>,
    events: VecDeque<DcsEvent>,
}

impl Default for DcsSniffer {
    fn default() -> Self {
        Self::new()
    }
}

impl DcsSniffer {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
            sink: Sink::default(),
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.sink, bytes);
    }

    /// Stateful feed that returns each newly-completed DCS event
    /// paired with its **end-of-frame byte offset** in `bytes` (1-past
    /// the terminator). Unlike the freestanding `scan_dcs_events`,
    /// this handles frames that span multiple `feed` calls — the VTE
    /// parser keeps DCS state between calls, so a `\eP$d` start in
    /// one feed and the matching `\e\` terminator in the next still
    /// emit a single event at the right position.
    ///
    /// Implementation feeds bytes one at a time and watches the
    /// sink's event queue grow. Byte-by-byte advance is allowed by
    /// the VTE parser by design — it's a finite state machine that
    /// handles any chunking.
    pub fn feed_with_positions(&mut self, bytes: &[u8]) -> Vec<(usize, DcsEvent)> {
        let mut located = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            let byte = bytes[i];
            let before = self.sink.events.len();
            self.parser
                .advance(&mut self.sink, std::slice::from_ref(&byte));
            if self.sink.events.len() > before {
                // Per VT500 state machine: a 7-bit ST is `ESC \`, and
                // `unhook` fires on the ESC byte while the trailing `\`
                // is still unconsumed (parser is now in Escape state, and
                // the next `\` would transition back to ground). If we
                // reported `i + 1` here the caller's
                // `dispatch_chunk(&bytes[..end_pos])` would feed only the
                // ESC, leaving the literal `\` to leak into whatever
                // target the next iteration routes to — observed as a
                // wild `\` at col 0 of every fresh BlockGrid in production.
                // Consume the trailing `\` through our parser too so its
                // state returns to ground for subsequent input.
                let end = if byte == 0x1B && bytes.get(i + 1) == Some(&b'\\') {
                    self.parser.advance(&mut self.sink, b"\\");
                    i + 2
                } else {
                    i + 1
                };
                for j in before..self.sink.events.len() {
                    located.push((end, self.sink.events[j].clone()));
                }
                i = end;
            } else {
                i += 1;
            }
        }
        located
    }

    pub fn pop(&mut self) -> Option<DcsEvent> {
        self.sink.events.pop_front()
    }

    pub fn pending(&self) -> usize {
        self.sink.events.len()
    }

    pub fn clear(&mut self) {
        self.sink.events.clear();
    }

    /// Borrow events queued at or after `start`. Used by `Terminal::feed` to
    /// apply only the events newly appended by the latest chunk without
    /// disturbing queue state observable to other drainers.
    pub fn events_from(&self, start: usize) -> impl Iterator<Item = &DcsEvent> {
        self.sink.events.range(start.min(self.sink.events.len())..)
    }
}

/// A located DCS event — the `(start, end_exclusive)` byte offsets the
/// frame occupies in the input slice plus the parsed event. Drives the
/// per-block byte-routing in `Terminal::feed`: bytes before `start` go
/// to the current target, the frame itself is fed through (alacritty
/// silently ignores our DCS), then `event` is applied (potentially
/// switching the target), then bytes from `end_exclusive` onward go to
/// the new target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedDcsEvent {
    pub start: usize,
    pub end_exclusive: usize,
    pub event: DcsEvent,
}

/// Scan a byte slice for BedTerm DCS frames (`ESC P $ d <hex> ST`) and
/// return their byte offsets + parsed events. Non-DCS bytes and
/// foreign DCS sequences (Sixel, kitty graphics, etc.) are silently
/// skipped — only the `$ d` final-byte family is ours.
///
/// Tolerates 7-bit (`ESC \`) and 8-bit (`\x9c`) string terminators.
/// Malformed frames (missing terminator before the slice ends; body
/// not pure hex; payload not a tagged-enum DcsEvent JSON) are dropped
/// silently to mirror the parser's behaviour.
pub fn scan_dcs_events(bytes: &[u8]) -> Vec<LocatedDcsEvent> {
    let mut out = Vec::new();
    let mut idx = 0;
    while idx + 4 <= bytes.len() {
        // Look for `ESC P $ d` — four-byte prefix unique to our DCS.
        if bytes[idx] != 0x1B || bytes[idx + 1] != b'P' {
            idx += 1;
            continue;
        }
        // Some DCS sequences have parameters before `$`, but ours is
        // bare. Require the next two bytes to be `$ d`.
        if bytes[idx + 2] != b'$' || bytes[idx + 3] != b'd' {
            // Foreign DCS — skip past its terminator and continue.
            match find_st(&bytes[idx + 2..]) {
                Some(rel) => {
                    let st_pos = idx + 2 + rel;
                    idx = match bytes.get(st_pos) {
                        Some(&0x9C) => st_pos + 1,
                        Some(&0x1B) => st_pos + 2,
                        _ => bytes.len(),
                    };
                }
                None => break, // truncated foreign DCS
            }
            continue;
        }
        let body_start = idx + 4;
        // Hex body until ST.
        let body_end = match find_st(&bytes[body_start..]) {
            Some(rel) => body_start + rel,
            None => break, // truncated frame; bail
        };
        let frame_end = match bytes.get(body_end) {
            Some(&0x9C) => body_end + 1, // 8-bit ST
            Some(&0x1B) if bytes.get(body_end + 1) == Some(&b'\\') => body_end + 2, // 7-bit ST
            _ => break,
        };
        let body = &bytes[body_start..body_end];
        if let Some(event) = parse_body(body) {
            out.push(LocatedDcsEvent {
                start: idx,
                end_exclusive: frame_end,
                event,
            });
        }
        idx = frame_end;
    }
    out
}

/// Find the next string-terminator byte. Returns the offset to the ST's
/// first byte (`\x9c` for 8-bit, `\x1b` for the start of `ESC \`).
fn find_st(bytes: &[u8]) -> Option<usize> {
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x9C {
            return Some(i);
        }
        if bytes[i] == 0x1B && bytes.get(i + 1) == Some(&b'\\') {
            return Some(i);
        }
        i += 1;
    }
    None
}

impl Perform for Sink {
    fn hook(
        &mut self,
        _params: &alacritty_terminal::vte::Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        // Only `ESC P $ d` is ours. Any other DCS (Sixel='q', DECRQSS='$|',
        // kitty graphics='G', …) is third-party and passes through silently.
        self.is_ours = action == 'd' && intermediates == [b'$'];
        self.buf.clear();
    }

    fn put(&mut self, byte: u8) {
        if self.is_ours {
            self.buf.push(byte);
        }
    }

    fn unhook(&mut self) {
        if !self.is_ours {
            return;
        }
        self.is_ours = false;
        if let Some(event) = parse_body(&self.buf) {
            self.events.push_back(event);
        }
    }
}

/// Hex-decode the body and dispatch on the `"hook"` discriminator.
/// Returns `None` for malformed input or unknown hooks — both are
/// silently dropped to keep the protocol forward-compatible.
fn parse_body(body: &[u8]) -> Option<DcsEvent> {
    let json = hex_decode(body)?;
    let hook = find_string(&json, b"hook")?;
    match hook.as_slice() {
        b"Precmd" => Some(DcsEvent::Precmd {
            pwd: find_string_in_value(&json, b"pwd").and_then(|b| String::from_utf8(b).ok()),
            git_branch: find_string_in_value(&json, b"git_branch")
                .and_then(|b| String::from_utf8(b).ok())
                .filter(|s| !s.is_empty()),
        }),
        b"Preexec" => Some(DcsEvent::Preexec {
            command: find_string_in_value(&json, b"command")
                .and_then(|b| String::from_utf8(b).ok())
                .unwrap_or_default(),
        }),
        b"CommandFinished" => Some(DcsEvent::CommandFinished {
            exit_code: find_int_in_value(&json, b"exit_code").map(|n| n as i32)?,
        }),
        _ => None,
    }
}

/// Minimal lowercase-hex decoder. The producer (`bedterm-integration.sh`)
/// only emits lowercase hex; uppercase is accepted for resilience. Odd
/// length or any non-hex byte returns `None`.
fn hex_decode(input: &[u8]) -> Option<Vec<u8>> {
    if !input.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(input.len() / 2);
    let nibble = |b: u8| -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    };
    for pair in input.chunks(2) {
        out.push((nibble(pair[0])? << 4) | nibble(pair[1])?);
    }
    Some(out)
}

/// Locate `"<key>":` at the top level of the JSON byte slice and return
/// the byte offset immediately after the colon (skipping whitespace).
/// Naive — we own both ends of the wire and the producer only emits the
/// known shape.
fn find_value_start(json: &[u8], key: &[u8]) -> Option<usize> {
    let needle_len = key.len() + 3;
    let mut i = 0;
    while i + needle_len <= json.len() {
        if json[i] == b'"'
            && json[i + 1..].starts_with(key)
            && json[i + 1 + key.len()] == b'"'
            && json[i + 2 + key.len()] == b':'
        {
            let mut start = i + 3 + key.len();
            while start < json.len() && matches!(json[start], b' ' | b'\t' | b'\n' | b'\r') {
                start += 1;
            }
            return Some(start);
        }
        i += 1;
    }
    None
}

/// Read a JSON string value at `start`. Handles `\\`, `\"`, `\n`, `\r`,
/// `\t`, and `\uXXXX` escape sequences — the minimum the shell side
/// `__bedterm_json_escape` produces.
fn read_json_string(json: &[u8], start: usize) -> Option<Vec<u8>> {
    if json.get(start) != Some(&b'"') {
        return None;
    }
    let mut i = start + 1;
    let mut out = Vec::new();
    while i < json.len() {
        match json[i] {
            b'"' => return Some(out),
            b'\\' => {
                let esc = *json.get(i + 1)?;
                match esc {
                    b'"' | b'\\' | b'/' => {
                        out.push(esc);
                        i += 2;
                    }
                    b'n' => {
                        out.push(b'\n');
                        i += 2;
                    }
                    b'r' => {
                        out.push(b'\r');
                        i += 2;
                    }
                    b't' => {
                        out.push(b'\t');
                        i += 2;
                    }
                    b'b' => {
                        out.push(0x08);
                        i += 2;
                    }
                    b'f' => {
                        out.push(0x0C);
                        i += 2;
                    }
                    b'u' => {
                        let hex = json.get(i + 2..i + 6)?;
                        let mut code: u32 = 0;
                        for b in hex {
                            let v = match *b {
                                b'0'..=b'9' => b - b'0',
                                b'a'..=b'f' => b - b'a' + 10,
                                b'A'..=b'F' => b - b'A' + 10,
                                _ => return None,
                            };
                            code = (code << 4) | v as u32;
                        }
                        // BMP only — sufficient for the control-byte
                        // escapes the shell emits. Surrogate pairs are
                        // never produced (we hex shell-escape ASCII < 0x20).
                        let c = char::from_u32(code)?;
                        let mut buf = [0u8; 4];
                        out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                        i += 6;
                    }
                    _ => return None,
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    None
}

/// Top-level `"<key>":"string"` lookup. Used for the `hook` discriminator.
fn find_string(json: &[u8], key: &[u8]) -> Option<Vec<u8>> {
    read_json_string(json, find_value_start(json, key)?)
}

/// Lookup `"<key>":"…"` inside the `value` object. We don't actually scope
/// to the nested object — we just search the whole body for the key. This
/// is safe because the keys we care about (`pwd`, `command`, `exit_code`)
/// never appear elsewhere in the payload (the outer object only has `hook`
/// and `value`).
fn find_string_in_value(json: &[u8], key: &[u8]) -> Option<Vec<u8>> {
    read_json_string(json, find_value_start(json, key)?)
}

fn find_int_in_value(json: &[u8], key: &[u8]) -> Option<i64> {
    let mut start = find_value_start(json, key)?;
    let negative = json.get(start) == Some(&b'-');
    if negative {
        start += 1;
    }
    let mut end = start;
    while end < json.len() && json[end].is_ascii_digit() {
        end += 1;
    }
    if end == start {
        return None;
    }
    let n: i64 = std::str::from_utf8(&json[start..end]).ok()?.parse().ok()?;
    Some(if negative { -n } else { n })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_of(input: &str) -> String {
        let mut out = String::with_capacity(input.len() * 2);
        for b in input.bytes() {
            use std::fmt::Write;
            write!(out, "{b:02x}").unwrap();
        }
        out
    }

    /// `ESC P $ d <hex> 0x9C` — match Warp's exact framing.
    fn dcs(hex_body: &str) -> Vec<u8> {
        let mut v = vec![0x1B, b'P', b'$', b'd'];
        v.extend_from_slice(hex_body.as_bytes());
        v.push(0x9C);
        v
    }

    fn precmd_bytes(pwd: &str) -> Vec<u8> {
        let json = format!(r#"{{"hook":"Precmd","value":{{"pwd":"{pwd}"}}}}"#);
        dcs(&hex_of(&json))
    }

    fn preexec_bytes(cmd: &str) -> Vec<u8> {
        let json = format!(r#"{{"hook":"Preexec","value":{{"command":"{cmd}"}}}}"#);
        dcs(&hex_of(&json))
    }

    fn command_finished_bytes(exit: i32) -> Vec<u8> {
        let json = format!(r#"{{"hook":"CommandFinished","value":{{"exit_code":{exit}}}}}"#);
        dcs(&hex_of(&json))
    }

    #[test]
    fn parses_precmd() {
        let mut s = DcsSniffer::new();
        s.feed(&precmd_bytes("/Users/alice"));
        assert_eq!(
            s.pop(),
            Some(DcsEvent::Precmd {
                pwd: Some("/Users/alice".into()),
                git_branch: None,
            })
        );
    }

    #[test]
    fn parses_preexec() {
        let mut s = DcsSniffer::new();
        s.feed(&preexec_bytes("ls -la"));
        assert_eq!(
            s.pop(),
            Some(DcsEvent::Preexec {
                command: "ls -la".into(),
            })
        );
    }

    #[test]
    fn parses_command_finished_with_nonzero_exit() {
        let mut s = DcsSniffer::new();
        s.feed(&command_finished_bytes(127));
        assert_eq!(s.pop(), Some(DcsEvent::CommandFinished { exit_code: 127 }));
    }

    #[test]
    fn parses_command_finished_with_negative_exit() {
        let mut s = DcsSniffer::new();
        s.feed(&command_finished_bytes(-1));
        assert_eq!(s.pop(), Some(DcsEvent::CommandFinished { exit_code: -1 }));
    }

    #[test]
    fn parses_full_prompt_cycle() {
        let mut s = DcsSniffer::new();
        // First prompt — no preceding command.
        s.feed(&precmd_bytes("/tmp"));
        // User runs `ls`.
        s.feed(&preexec_bytes("ls"));
        // Output prints, then shell fires CommandFinished + Precmd.
        s.feed(&command_finished_bytes(0));
        s.feed(&precmd_bytes("/tmp"));
        let mut events = Vec::new();
        while let Some(e) = s.pop() {
            events.push(e);
        }
        assert_eq!(
            events,
            vec![
                DcsEvent::Precmd {
                    pwd: Some("/tmp".into()),
                    git_branch: None,
                },
                DcsEvent::Preexec {
                    command: "ls".into()
                },
                DcsEvent::CommandFinished { exit_code: 0 },
                DcsEvent::Precmd {
                    pwd: Some("/tmp".into()),
                    git_branch: None,
                },
            ]
        );
    }

    #[test]
    fn decodes_json_string_escapes() {
        // command containing `"` and `\` — properly JSON-escaped on the wire.
        let json = r#"{"hook":"Preexec","value":{"command":"echo \"hi\\there\""}}"#;
        let mut s = DcsSniffer::new();
        s.feed(&dcs(&hex_of(json)));
        assert_eq!(
            s.pop(),
            Some(DcsEvent::Preexec {
                command: r#"echo "hi\there""#.into(),
            })
        );
    }

    #[test]
    fn decodes_unicode_escape() {
        // < (less-than). Verifies the BMP \uXXXX path.
        let json = r#"{"hook":"Preexec","value":{"command":"a<b"}}"#;
        let mut s = DcsSniffer::new();
        s.feed(&dcs(&hex_of(json)));
        assert_eq!(
            s.pop(),
            Some(DcsEvent::Preexec {
                command: "a<b".into(),
            })
        );
    }

    #[test]
    fn ignores_dcs_with_other_final_byte() {
        // Sixel uses `q`; DECRQSS uses `$|`. Neither is ours.
        let mut s = DcsSniffer::new();
        s.feed(&[0x1B, b'P', b'q', b'1', b';', 0x9C]);
        s.feed(&[0x1B, b'P', b'$', b'|', b'r', 0x9C]);
        assert_eq!(s.pending(), 0);
    }

    #[test]
    fn ignores_unknown_hook_variants() {
        // Warp ships many more hooks (Bootstrapped, SSH, InitShell, …);
        // we silently drop everything except the three we consume.
        let mut s = DcsSniffer::new();
        let json = r#"{"hook":"Bootstrapped","value":{"shell":"zsh"}}"#;
        s.feed(&dcs(&hex_of(json)));
        assert_eq!(s.pending(), 0);
    }

    #[test]
    fn malformed_hex_drops_event() {
        let mut s = DcsSniffer::new();
        s.feed(&dcs("zzznothex"));
        assert_eq!(s.pending(), 0);
    }

    #[test]
    fn split_input_still_parses() {
        let mut s = DcsSniffer::new();
        let full = preexec_bytes("date");
        let (a, b) = full.split_at(full.len() / 2);
        s.feed(a);
        s.feed(b);
        assert_eq!(
            s.pop(),
            Some(DcsEvent::Preexec {
                command: "date".into(),
            })
        );
    }

    #[test]
    fn st_seven_bit_terminator_also_works() {
        // VTE accepts both 0x9C (8-bit ST) and ESC `\` (7-bit ST). Our
        // bundled shell emits 8-bit; remote terminals that strip the
        // high bit on the wire need the 7-bit form to still parse.
        let mut s = DcsSniffer::new();
        let json = r#"{"hook":"Precmd","value":{"pwd":"/x"}}"#;
        let mut v = vec![0x1B, b'P', b'$', b'd'];
        v.extend_from_slice(hex_of(json).as_bytes());
        v.extend_from_slice(&[0x1B, b'\\']);
        s.feed(&v);
        assert_eq!(
            s.pop(),
            Some(DcsEvent::Precmd {
                pwd: Some("/x".into()),
                git_branch: None,
            })
        );
    }

    // --- scan_dcs_events ---

    fn dcs_8bit(json: &str) -> Vec<u8> {
        let mut v = vec![0x1B, b'P', b'$', b'd'];
        v.extend_from_slice(hex_of(json).as_bytes());
        v.push(0x9C);
        v
    }

    #[test]
    fn scanner_finds_single_event() {
        let frame = dcs_8bit(r#"{"hook":"Precmd","value":{"pwd":"/tmp"}}"#);
        let located = scan_dcs_events(&frame);
        assert_eq!(located.len(), 1);
        assert_eq!(located[0].start, 0);
        assert_eq!(located[0].end_exclusive, frame.len());
        assert_eq!(
            located[0].event,
            DcsEvent::Precmd {
                pwd: Some("/tmp".into()),
                git_branch: None,
            }
        );
    }

    #[test]
    fn scanner_handles_pre_and_post_text() {
        let pre = b"prompt> ";
        let frame = dcs_8bit(r#"{"hook":"Preexec","value":{"command":"ls"}}"#);
        let post = b"output line";
        let mut bytes = Vec::new();
        bytes.extend_from_slice(pre);
        bytes.extend_from_slice(&frame);
        bytes.extend_from_slice(post);
        let located = scan_dcs_events(&bytes);
        assert_eq!(located.len(), 1);
        assert_eq!(located[0].start, pre.len());
        assert_eq!(located[0].end_exclusive, pre.len() + frame.len());
    }

    #[test]
    fn scanner_handles_back_to_back_events() {
        // CommandFinished immediately followed by Precmd — happens
        // every prompt cycle once a command has finished running.
        let cf = dcs_8bit(r#"{"hook":"CommandFinished","value":{"exit_code":0}}"#);
        let pc = dcs_8bit(r#"{"hook":"Precmd","value":{"pwd":"/x"}}"#);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&cf);
        bytes.extend_from_slice(&pc);
        let located = scan_dcs_events(&bytes);
        assert_eq!(located.len(), 2);
        assert_eq!(located[0].end_exclusive, cf.len());
        assert_eq!(located[1].start, cf.len());
    }

    #[test]
    fn scanner_accepts_7bit_terminator() {
        let json = r#"{"hook":"Precmd","value":{"pwd":"/x"}}"#;
        let mut bytes = vec![0x1B, b'P', b'$', b'd'];
        bytes.extend_from_slice(hex_of(json).as_bytes());
        bytes.extend_from_slice(&[0x1B, b'\\']);
        let located = scan_dcs_events(&bytes);
        assert_eq!(located.len(), 1);
        assert_eq!(located[0].end_exclusive, bytes.len());
    }

    #[test]
    fn scanner_skips_foreign_dcs() {
        // A Sixel-shaped DCS (final byte `q`) followed by our frame.
        let mut bytes = vec![0x1B, b'P', b'q', b'1', b';', 0x9C];
        let frame = dcs_8bit(r#"{"hook":"Precmd","value":{"pwd":"/x"}}"#);
        bytes.extend_from_slice(&frame);
        let located = scan_dcs_events(&bytes);
        assert_eq!(located.len(), 1);
        assert_eq!(located[0].event.precmd_pwd().as_deref(), Some("/x"));
    }

    #[test]
    fn scanner_ignores_truncated_frame() {
        // Frame starts but no terminator before slice ends.
        let mut bytes = vec![0x1B, b'P', b'$', b'd'];
        bytes.extend_from_slice(hex_of(r#"{"hook":"Precmd"}"#).as_bytes());
        // no ST
        let located = scan_dcs_events(&bytes);
        assert!(located.is_empty());
    }

    impl DcsEvent {
        fn precmd_pwd(&self) -> Option<String> {
            if let DcsEvent::Precmd { pwd, .. } = self {
                pwd.clone()
            } else {
                None
            }
        }
    }
}
