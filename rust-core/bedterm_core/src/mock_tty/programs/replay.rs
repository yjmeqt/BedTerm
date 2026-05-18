//! Asciinema cast replay. The output stream is gated by a wall-clock tick
//! driven from the host (Swift uses a Timer; cargo tests pass synthetic times).
//!
//! v2 (absolute times):
//!   {"version": 2, "width": 80, "height": 24}
//!   [0.0, "o", "hello"]
//!   [0.5, "o", "\r\n"]
//!
//! v3 (per-event deltas):
//!   {"version": 3, "term": {...}}
//!   [0.055, "o", "hello"]   // 55 ms after start
//!   [0.500, "o", "\r\n"]    // 500 ms after the previous row
//!
//! Non-"o" rows (input, resize, marker, …) are skipped. The parser is
//! hand-rolled because we don't want a `serde` dependency for one tiny format.

use crate::mock_tty::program::{Program, ProgramKind};

pub struct Replay {
    events: Vec<(u64, Vec<u8>)>,
    next: usize,
    return_to_echo: bool,
}

impl Replay {
    pub fn from_cast_text(text: &str) -> Self {
        Self::from_cast_text_with_return(text, false)
    }

    pub fn from_cast_text_with_return(text: &str, return_to_echo: bool) -> Self {
        let mut lines = text.lines();
        let header = lines.next().unwrap_or("");
        let is_v3 = header_is_v3(header);

        let mut events = Vec::new();
        let mut accum_ms: u64 = 0;
        for line in lines {
            let line = line.trim();
            if line.is_empty() || !line.starts_with('[') {
                continue;
            }
            if let Some((t_ms, bytes)) = parse_event(line) {
                let abs_ms = if is_v3 {
                    accum_ms = accum_ms.saturating_add(t_ms);
                    accum_ms
                } else {
                    t_ms
                };
                events.push((abs_ms, bytes));
            }
        }
        Self {
            events,
            next: 0,
            return_to_echo,
        }
    }
}

/// Detect asciinema cast schema version. Defaults to v2 when the header is
/// missing or malformed so synthetic test fixtures without a version field
/// keep parsing as absolute times.
fn header_is_v3(header: &str) -> bool {
    // Tolerate optional whitespace between `:` and the digit.
    let needle = "\"version\"";
    let Some(start) = header.find(needle) else {
        return false;
    };
    let rest = &header[start + needle.len()..];
    let rest = rest.trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    rest.starts_with('3')
}

impl Default for Replay {
    fn default() -> Self {
        Self::from_cast_text("")
    }
}

impl Program for Replay {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn wants_raw(&self) -> bool {
        true
    }

    fn on_tick(&mut self, now_ms: u64, out: &mut Vec<u8>) {
        while let Some((t, bytes)) = self.events.get(self.next) {
            if *t > now_ms {
                break;
            }
            out.extend_from_slice(bytes);
            self.next += 1;
        }
    }

    fn pending_switch(&mut self) -> Option<ProgramKind> {
        if self.return_to_echo && self.next >= self.events.len() {
            self.return_to_echo = false;
            return Some(ProgramKind::EchoShell);
        }
        None
    }
}

/// Parses one row like `[0.5, "o", "hello\r\n"]`.
fn parse_event(s: &str) -> Option<(u64, Vec<u8>)> {
    let inner = s.strip_prefix('[')?.strip_suffix(']')?;
    let (t_str, rest) = split_first_comma(inner)?;
    let t_f: f64 = t_str.trim().parse().ok()?;
    let t_ms = (t_f * 1000.0) as u64;

    let (kind, rest) = split_first_comma(rest)?;
    if kind.trim().trim_matches('"') != "o" {
        return None;
    }

    let raw = rest.trim();
    let raw = raw.strip_prefix('"')?.strip_suffix('"')?;
    Some((t_ms, decode_json_string(raw)))
}

/// Find the first comma outside of a string literal.
fn split_first_comma(s: &str) -> Option<(&str, &str)> {
    let bytes = s.as_bytes();
    let mut in_string = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' && (i == 0 || bytes[i - 1] != b'\\') {
            in_string = !in_string;
        }
        if !in_string && c == b',' {
            return Some((&s[..i], &s[i + 1..]));
        }
        i += 1;
    }
    None
}

fn decode_json_string(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c != b'\\' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        if i >= bytes.len() {
            break;
        }
        match bytes[i] {
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'"' => out.push(b'"'),
            b'\\' => out.push(b'\\'),
            b'/' => out.push(b'/'),
            b'b' => out.push(0x08),
            b'f' => out.push(0x0C),
            b'u' => {
                if i + 4 < bytes.len() {
                    let hex = std::str::from_utf8(&bytes[i + 1..i + 5]).unwrap_or("0");
                    if let Ok(cp) = u32::from_str_radix(hex, 16) {
                        if let Some(c) = char::from_u32(cp) {
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                        }
                    }
                    i += 4;
                }
            }
            other => out.push(other),
        }
        i += 1;
    }
    out
}
