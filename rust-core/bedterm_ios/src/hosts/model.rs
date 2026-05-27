//! Pure-logic Hosts list model + JSON marshalling helpers.
//!
//! The Swift `HostsBridge` serialises the current `[SavedHost]` array to
//! a UTF-8 JSON string; Rust parses it via [`parse_entries_json`] and
//! retains the result inside the VC. Marshalling one blob keeps the FFI
//! seam narrow — no per-field length / capacity dances.

/// One row in the Rust-rendered hosts list. Mirrors the subset of
/// `SavedHost` the list cell needs: identity (UUID string), display
/// label, and the resolved `username@host[:port]` tuple. Authentication
/// kind is reduced to a bool (`auth_is_key`) so the cell can draw the
/// matching badge without seeing the secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostListEntry {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_is_key: bool,
}

impl HostListEntry {
    /// Primary line displayed in the cell. Mirrors `HostRow.primaryLabel`
    /// in the SwiftUI implementation: prefer the user-supplied label,
    /// fall back to `username@host`.
    pub fn primary_label(&self) -> String {
        if !self.label.is_empty() {
            self.label.clone()
        } else {
            format!("{}@{}", self.username, formatted_host(&self.host))
        }
    }

    /// Optional subtitle. Hidden when the entry has no custom label *and*
    /// the port is the SSH default (22) — matches the SwiftUI rule so
    /// the visual rhythm stays identical when the flag is flipped.
    pub fn subtitle(&self) -> Option<String> {
        if self.label.is_empty() && self.port == 22 {
            return None;
        }
        let mut s = format!("{}@{}", self.username, formatted_host(&self.host));
        if self.port != 22 {
            s.push(':');
            s.push_str(&self.port.to_string());
        }
        Some(s)
    }
}

/// IPv6 literals get bracketed (`[::1]`) so the `host[:port]` form
/// remains unambiguous. Matches `HostRow.formattedHost`.
fn formatted_host(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

/// Parse the snapshot JSON Swift hands the bridge. Returns an empty
/// vector for blank / null / malformed input — UI never gets a partial
/// crash on a bad blob, just an empty list. Detailed error reporting
/// isn't needed: the bridge owns the round-trip and can log on its
/// side.
///
/// JSON shape:
/// ```text
/// [
///   { "id": "UUID-STRING",
///     "label": "Personal Mac",
///     "host":  "10.0.0.5",
///     "port":  22,
///     "username": "yi",
///     "authIsKey": true },
///   ...
/// ]
/// ```
pub fn parse_entries_json(blob: &str) -> Vec<HostListEntry> {
    let trimmed = blob.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    parse_array(trimmed).unwrap_or_default()
}

// Minimal JSON-array reader. Avoids pulling `serde_json` into the
// crate's build — the shape is fixed by our own writer in Swift, so a
// hand rolled tokenizer is fine.
fn parse_array(blob: &str) -> Option<Vec<HostListEntry>> {
    let bytes = blob.as_bytes();
    let mut i = skip_ws(bytes, 0);
    if i >= bytes.len() || bytes[i] != b'[' {
        return None;
    }
    i += 1;
    let mut entries = Vec::new();
    loop {
        i = skip_ws(bytes, i);
        if i >= bytes.len() {
            return None;
        }
        if bytes[i] == b']' {
            return Some(entries);
        }
        let (entry, ni) = parse_entry(blob, i)?;
        entries.push(entry);
        i = skip_ws(bytes, ni);
        if i >= bytes.len() {
            return None;
        }
        match bytes[i] {
            b',' => {
                i += 1;
            }
            b']' => return Some(entries),
            _ => return None,
        }
    }
}

fn parse_entry(blob: &str, start: usize) -> Option<(HostListEntry, usize)> {
    let bytes = blob.as_bytes();
    let mut i = skip_ws(bytes, start);
    if i >= bytes.len() || bytes[i] != b'{' {
        return None;
    }
    i += 1;
    let mut id = String::new();
    let mut label = String::new();
    let mut host = String::new();
    let mut username = String::new();
    let mut port: u16 = 22;
    let mut auth_is_key = false;
    loop {
        i = skip_ws(bytes, i);
        if i >= bytes.len() {
            return None;
        }
        if bytes[i] == b'}' {
            i += 1;
            return Some((
                HostListEntry {
                    id,
                    label,
                    host,
                    port,
                    username,
                    auth_is_key,
                },
                i,
            ));
        }
        let (key, ni) = parse_string(blob, i)?;
        i = skip_ws(bytes, ni);
        if i >= bytes.len() || bytes[i] != b':' {
            return None;
        }
        i += 1;
        i = skip_ws(bytes, i);
        match key.as_str() {
            "id" => {
                let (v, ni) = parse_string(blob, i)?;
                id = v;
                i = ni;
            }
            "label" => {
                let (v, ni) = parse_string(blob, i)?;
                label = v;
                i = ni;
            }
            "host" => {
                let (v, ni) = parse_string(blob, i)?;
                host = v;
                i = ni;
            }
            "username" => {
                let (v, ni) = parse_string(blob, i)?;
                username = v;
                i = ni;
            }
            "port" => {
                let (v, ni) = parse_number(blob, i)?;
                port = v as u16;
                i = ni;
            }
            "authIsKey" => {
                let (v, ni) = parse_bool(blob, i)?;
                auth_is_key = v;
                i = ni;
            }
            _ => {
                // Unknown field — skip its value. Accept strings, numbers,
                // bools, null. Nested objects/arrays aren't expected.
                i = skip_value(bytes, i)?;
            }
        }
        i = skip_ws(bytes, i);
        if i >= bytes.len() {
            return None;
        }
        match bytes[i] {
            b',' => {
                i += 1;
            }
            b'}' => {}
            _ => return None,
        }
    }
}

fn parse_string(blob: &str, start: usize) -> Option<(String, usize)> {
    let bytes = blob.as_bytes();
    if start >= bytes.len() || bytes[start] != b'"' {
        return None;
    }
    let mut i = start + 1;
    let mut out = String::new();
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\\' && i + 1 < bytes.len() {
            let nxt = bytes[i + 1];
            match nxt {
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'/' => out.push('/'),
                b'n' => out.push('\n'),
                b't' => out.push('\t'),
                b'r' => out.push('\r'),
                _ => {
                    // Unsupported escape — punt to a literal so we don't
                    // hang on malformed input.
                    out.push(nxt as char);
                }
            }
            i += 2;
            continue;
        }
        if c == b'"' {
            return Some((out, i + 1));
        }
        // UTF-8 multi-byte: walk through the original &str by char.
        // Fast path: ASCII byte.
        if c < 0x80 {
            out.push(c as char);
            i += 1;
        } else {
            // Decode one UTF-8 codepoint starting at `i`. We rely on the
            // source &str being valid UTF-8.
            let remainder = &blob[i..];
            let ch = remainder.chars().next()?;
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    None
}

fn parse_number(blob: &str, start: usize) -> Option<(i64, usize)> {
    let bytes = blob.as_bytes();
    let mut i = start;
    let mut neg = false;
    if i < bytes.len() && bytes[i] == b'-' {
        neg = true;
        i += 1;
    }
    let s = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == s {
        return None;
    }
    let n: i64 = blob[s..i].parse().ok()?;
    Some((if neg { -n } else { n }, i))
}

fn parse_bool(blob: &str, start: usize) -> Option<(bool, usize)> {
    if blob[start..].starts_with("true") {
        Some((true, start + 4))
    } else if blob[start..].starts_with("false") {
        Some((false, start + 5))
    } else {
        None
    }
}

fn skip_value(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    if i >= bytes.len() {
        return None;
    }
    match bytes[i] {
        b'"' => {
            i += 1;
            while i < bytes.len() {
                let c = bytes[i];
                if c == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                if c == b'"' {
                    return Some(i + 1);
                }
                i += 1;
            }
            None
        }
        b't' | b'f' | b'n' | b'-' | b'0'..=b'9' => {
            while i < bytes.len()
                && bytes[i] != b','
                && bytes[i] != b'}'
                && bytes[i] != b']'
                && !bytes[i].is_ascii_whitespace()
            {
                i += 1;
            }
            Some(i)
        }
        _ => None,
    }
}

fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(label: &str, port: u16) -> HostListEntry {
        HostListEntry {
            id: "AAAA-BBBB".to_string(),
            label: label.to_string(),
            host: "10.0.0.5".to_string(),
            port,
            username: "yi".to_string(),
            auth_is_key: true,
        }
    }

    #[test]
    fn primary_label_uses_user_label_when_set() {
        let e = sample("Personal Mac", 22);
        assert_eq!(e.primary_label(), "Personal Mac");
    }

    #[test]
    fn primary_label_falls_back_to_user_at_host() {
        let e = sample("", 22);
        assert_eq!(e.primary_label(), "yi@10.0.0.5");
    }

    #[test]
    fn subtitle_hidden_for_default_port_no_label() {
        let e = sample("", 22);
        assert_eq!(e.subtitle(), None);
    }

    #[test]
    fn subtitle_shows_when_label_present() {
        let e = sample("Personal Mac", 22);
        assert_eq!(e.subtitle(), Some("yi@10.0.0.5".to_string()));
    }

    #[test]
    fn subtitle_appends_nondefault_port() {
        let e = sample("Personal Mac", 2222);
        assert_eq!(e.subtitle(), Some("yi@10.0.0.5:2222".to_string()));
    }

    #[test]
    fn ipv6_host_is_bracketed_in_label() {
        let e = HostListEntry {
            id: "X".to_string(),
            label: String::new(),
            host: "::1".to_string(),
            port: 22,
            username: "yi".to_string(),
            auth_is_key: false,
        };
        assert_eq!(e.primary_label(), "yi@[::1]");
    }

    #[test]
    fn parse_empty_blob_returns_empty_vec() {
        assert!(parse_entries_json("").is_empty());
        assert!(parse_entries_json("  ").is_empty());
        assert!(parse_entries_json("[]").is_empty());
    }

    #[test]
    fn parse_one_entry_round_trip() {
        let json = r#"[
            {"id":"abc","label":"Mac","host":"10.0.0.5","port":22,
             "username":"yi","authIsKey":true}
        ]"#;
        let v = parse_entries_json(json);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, "abc");
        assert_eq!(v[0].label, "Mac");
        assert_eq!(v[0].host, "10.0.0.5");
        assert_eq!(v[0].port, 22);
        assert_eq!(v[0].username, "yi");
        assert!(v[0].auth_is_key);
    }

    #[test]
    fn parse_multiple_entries_preserves_order() {
        let json = r#"[
            {"id":"a","label":"","host":"h1","port":22,"username":"u1","authIsKey":false},
            {"id":"b","label":"two","host":"h2","port":2222,"username":"u2","authIsKey":true}
        ]"#;
        let v = parse_entries_json(json);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].id, "a");
        assert_eq!(v[1].id, "b");
        assert_eq!(v[1].port, 2222);
        assert!(v[1].auth_is_key);
    }

    #[test]
    fn parse_ignores_unknown_fields() {
        let json = r#"[
            {"id":"a","label":"L","host":"h","port":22,"username":"u",
             "authIsKey":false,"extraIgnored":"value","another":99}
        ]"#;
        let v = parse_entries_json(json);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].label, "L");
    }

    #[test]
    fn parse_garbage_returns_empty() {
        assert!(parse_entries_json("not json").is_empty());
        assert!(parse_entries_json("{").is_empty());
        assert!(parse_entries_json("[{").is_empty());
    }

    #[test]
    fn parse_handles_unicode_in_label() {
        let json = r#"[
            {"id":"a","label":"日本","host":"h","port":22,"username":"u","authIsKey":false}
        ]"#;
        let v = parse_entries_json(json);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].label, "日本");
    }
}
