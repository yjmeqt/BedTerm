//! OSC 133 ("FinalTerm") sniffer with optional extension-attribute support.
//!
//! A separate VTE state machine that runs alongside alacritty's parser. It
//! only reacts to `osc_dispatch` for sequences whose first parameter is `133`;
//! every other VTE callback is a no-op. The host (Swift) drains the resulting
//! events to build Warp-style command blocks.
//!
//! ## Wire format
//!
//! Plain FinalTerm — what iTerm2, kitty, VSCode, WezTerm emit:
//!
//! ```text
//! ESC]133;A ST          prompt start
//! ESC]133;B ST          command start (prompt finished)
//! ESC]133;C ST          output start (user pressed Return)
//! ESC]133;D[;exit] ST   command end (exit code optional)
//! ```
//!
//! Plus extension parameters — every shipping integration uses these in some
//! form (kitty uses `;k=…`, iTerm2 uses `;user-host=… ;current-dir=…`):
//!
//! ```text
//! ESC]133;C;cmd=<b64> ST                command with the command text
//! ESC]133;D;0;dur=1234;cwd=<b64> ST     rich command-end payload
//! ```
//!
//! We pass extensions through verbatim as a `Vec<u8>` — `key=value;key=value`
//! joined by `;`. The host parses them into a dictionary; unknown keys are
//! ignored. This keeps the sniffer agnostic to which integration script is
//! installed (third-party scripts give empty attrs; our own enriched script
//! ships the command text and metadata directly).
//!
//! Cost is negligible — the state machine is ~30 ns/byte and the byte stream
//! is already moving past alacritty's full ANSI processor.

use alacritty_terminal::vte::{Parser, Perform};
use std::collections::VecDeque;

/// Shell-integration events emitted as the remote shell crosses prompt /
/// command boundaries. Mirrors the FinalTerm 133;A/B/C/D vocabulary used by
/// iTerm2, kitty, WezTerm, VSCode, and our own bundled bootstrap.
///
/// `attrs` carries the raw key-value tail of the OSC sequence — `key=value`
/// pairs joined by `;`, exactly as the remote shell emitted them. Empty when
/// the integration didn't ship any. The host parses them; the sniffer does
/// no semantic interpretation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Osc133Event {
    /// `ESC]133;A[;<attrs>]` — the shell is about to draw its prompt.
    PromptStart { attrs: Vec<u8> },
    /// `ESC]133;B[;<attrs>]` — the prompt finished drawing.
    CommandStart { attrs: Vec<u8> },
    /// `ESC]133;C[;<attrs>]` — output is starting. Our enriched scripts ship
    /// the command text here as `cmd=<base64>`.
    OutputStart { attrs: Vec<u8> },
    /// `ESC]133;D[;<exit>][;<attrs>]` — the command finished. `exit_code` is
    /// `None` when the shell didn't supply one. Our scripts also ship
    /// `dur=<millis>` and `cwd=<base64>` here.
    CommandEnd {
        exit_code: Option<i32>,
        attrs: Vec<u8>,
    },
}

/// Owns the inner VTE parser plus a small queue of unread events. The queue
/// is unbounded but expected to stay tiny in practice — the host drains it
/// after every `feed`.
pub struct Osc133Sniffer {
    parser: Parser,
    sink: Sink,
}

#[derive(Default)]
struct Sink {
    events: VecDeque<Osc133Event>,
}

impl Default for Osc133Sniffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Osc133Sniffer {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
            sink: Sink::default(),
        }
    }

    /// Advance the state machine over `bytes`, accumulating any new events
    /// in the internal queue.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.sink, bytes);
    }

    /// Pop the next pending event, or `None` if the queue is empty.
    pub fn pop(&mut self) -> Option<Osc133Event> {
        self.sink.events.pop_front()
    }

    /// Drop every queued event without observing them.
    pub fn clear(&mut self) {
        self.sink.events.clear();
    }

    /// Count of unread events — primarily for tests and the FFI drain.
    pub fn pending(&self) -> usize {
        self.sink.events.len()
    }

    /// Borrow the events whose queue index is at or after `start`. Used by
    /// `Terminal::feed` to apply only the events newly appended by the
    /// latest chunk to the BlockStore, without disturbing the queue state
    /// that `pop` / `drain` callers observe.
    ///
    /// Returns an empty iterator if `start >= pending()`.
    pub fn events_from(&self, start: usize) -> impl Iterator<Item = &Osc133Event> {
        self.sink.events.range(start.min(self.sink.events.len())..)
    }
}

/// Joins a slice of byte-slices back into a single `Vec<u8>` separated by
/// `;`. Used to reconstruct the attribute tail from VTE's tokenised params.
fn join_attrs(parts: &[&[u8]]) -> Vec<u8> {
    if parts.is_empty() {
        return Vec::new();
    }
    let total: usize = parts.iter().map(|p| p.len()).sum::<usize>() + parts.len() - 1;
    let mut out = Vec::with_capacity(total);
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            out.push(b';');
        }
        out.extend_from_slice(p);
    }
    out
}

impl Perform for Sink {
    // Every non-OSC callback is a no-op: alacritty's parser handles the
    // semantic work, and we only need to observe `osc_dispatch`.

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        // FinalTerm OSC layout: `133 ; <kind> [; <args>...]`
        if params.first().copied() != Some(b"133".as_ref()) {
            return;
        }
        let Some(kind) = params.get(1) else { return };
        let event = match *kind {
            b"A" => Osc133Event::PromptStart {
                attrs: join_attrs(&params[2..]),
            },
            b"B" => Osc133Event::CommandStart {
                attrs: join_attrs(&params[2..]),
            },
            b"C" => Osc133Event::OutputStart {
                attrs: join_attrs(&params[2..]),
            },
            b"D" => {
                // 133;D's positional-2 slot is conventionally the exit code.
                // Some scripts skip it and jump straight to key=value pairs
                // (e.g. `;err=14`). Try parsing as int — on success it's the
                // exit code and attrs start at 3; on failure the slot is
                // already an attr and attrs start at 2.
                let (exit_code, attrs_start) = params
                    .get(2)
                    .and_then(|s| std::str::from_utf8(s).ok())
                    .and_then(|s| s.parse::<i32>().ok())
                    .map(|n| (Some(n), 3usize))
                    .unwrap_or((None, 2));
                let attrs = if attrs_start < params.len() {
                    join_attrs(&params[attrs_start..])
                } else {
                    Vec::new()
                };
                Osc133Event::CommandEnd { exit_code, attrs }
            }
            // Other 133 sub-kinds (e.g. `L` for "new line") aren't part of
            // the block-boundary lifecycle we care about; ignore quietly.
            _ => return,
        };
        self.events.push_back(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ESC]133;<k>[;args] ST` — `ST` is either `BEL` (0x07) or `ESC\\`.
    fn osc(suffix: &str) -> Vec<u8> {
        let mut v = vec![0x1B, b']'];
        v.extend_from_slice(suffix.as_bytes());
        v.push(0x07);
        v
    }

    #[test]
    fn parses_prompt_start_without_attrs() {
        let mut s = Osc133Sniffer::new();
        s.feed(&osc("133;A"));
        assert_eq!(s.pop(), Some(Osc133Event::PromptStart { attrs: vec![] }));
    }

    #[test]
    fn parses_full_lifecycle() {
        let mut s = Osc133Sniffer::new();
        s.feed(&osc("133;A"));
        s.feed(&osc("133;B"));
        s.feed(&osc("133;C"));
        s.feed(&osc("133;D;0"));
        assert_eq!(s.pop(), Some(Osc133Event::PromptStart { attrs: vec![] }));
        assert_eq!(s.pop(), Some(Osc133Event::CommandStart { attrs: vec![] }));
        assert_eq!(s.pop(), Some(Osc133Event::OutputStart { attrs: vec![] }));
        assert_eq!(
            s.pop(),
            Some(Osc133Event::CommandEnd {
                exit_code: Some(0),
                attrs: vec![]
            })
        );
    }

    #[test]
    fn parses_attrs_on_prompt_start() {
        // iTerm2-style extension parameters after the kind.
        let mut s = Osc133Sniffer::new();
        s.feed(&osc("133;A;user-host=alice@host;current-dir=/Users/alice"));
        assert_eq!(
            s.pop(),
            Some(Osc133Event::PromptStart {
                attrs: b"user-host=alice@host;current-dir=/Users/alice".to_vec()
            })
        );
    }

    #[test]
    fn parses_command_with_cmd_attr() {
        // The shape our own enriched script will emit at output-start.
        let mut s = Osc133Sniffer::new();
        s.feed(&osc("133;C;cmd=bHM="));
        assert_eq!(
            s.pop(),
            Some(Osc133Event::OutputStart {
                attrs: b"cmd=bHM=".to_vec()
            })
        );
    }

    #[test]
    fn parses_command_end_with_exit_and_attrs() {
        // `133;D;<exit>;<attrs>` — exit at positional-2, attrs at 3.
        let mut s = Osc133Sniffer::new();
        s.feed(&osc("133;D;0;dur=1234;cwd=L1VzZXJzL2FsaWNl"));
        assert_eq!(
            s.pop(),
            Some(Osc133Event::CommandEnd {
                exit_code: Some(0),
                attrs: b"dur=1234;cwd=L1VzZXJzL2FsaWNl".to_vec()
            })
        );
    }

    #[test]
    fn parses_command_end_without_exit_with_attrs() {
        // `133;D;<key>=<value>` with no positional exit code; attrs start
        // at 2, not 3. The integer-parse failure on `err=14` is the signal.
        let mut s = Osc133Sniffer::new();
        s.feed(&osc("133;D;err=14"));
        assert_eq!(
            s.pop(),
            Some(Osc133Event::CommandEnd {
                exit_code: None,
                attrs: b"err=14".to_vec()
            })
        );
    }

    #[test]
    fn parses_nonzero_exit() {
        let mut s = Osc133Sniffer::new();
        s.feed(&osc("133;D;127"));
        assert_eq!(
            s.pop(),
            Some(Osc133Event::CommandEnd {
                exit_code: Some(127),
                attrs: vec![]
            })
        );
    }

    #[test]
    fn parses_missing_exit() {
        let mut s = Osc133Sniffer::new();
        s.feed(&osc("133;D"));
        assert_eq!(
            s.pop(),
            Some(Osc133Event::CommandEnd {
                exit_code: None,
                attrs: vec![]
            })
        );
    }

    #[test]
    fn ignores_other_oscs() {
        let mut s = Osc133Sniffer::new();
        s.feed(&osc("0;hello"));
        s.feed(&osc("4;1;#ff0000"));
        s.feed(&osc("8;;https://example.com"));
        assert_eq!(s.pending(), 0);
    }

    #[test]
    fn st_terminator_alternative() {
        let mut s = Osc133Sniffer::new();
        s.feed(&[0x1B, b']', b'1', b'3', b'3', b';', b'A', 0x1B, 0x5C]);
        assert_eq!(s.pop(), Some(Osc133Event::PromptStart { attrs: vec![] }));
    }

    #[test]
    fn ignores_unknown_subkind() {
        let mut s = Osc133Sniffer::new();
        s.feed(&osc("133;Q"));
        s.feed(&osc("133;L"));
        assert_eq!(s.pending(), 0);
    }

    #[test]
    fn handles_split_input() {
        let mut s = Osc133Sniffer::new();
        s.feed(&[0x1B, b']', b'1']);
        s.feed(&[b'3', b'3', b';', b'A', 0x07]);
        assert_eq!(s.pop(), Some(Osc133Event::PromptStart { attrs: vec![] }));
    }
}
