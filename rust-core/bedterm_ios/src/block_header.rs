//! Port of `BlockHeaderModel.swift`.
//!
//! Value-type carrier for the data the Rust renderer needs to paint one
//! block header band. Holds no UIKit handles so it's safe to build off the
//! main actor for tests.
//!
//! `CliAgent` mirrors the Swift `CLIAgent` enum (raw values 1..=13).

#![allow(dead_code)]

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum CliAgent {
    Claude = 1,
    Gemini = 2,
    Codex = 3,
    Amp = 4,
    Droid = 5,
    OpenCode = 6,
    Copilot = 7,
    Pi = 8,
    Auggie = 9,
    CursorCli = 10,
    Goose = 11,
    Hermes = 12,
    Vibe = 13,
}

impl CliAgent {
    /// Decode the FFI tag. Returns `None` for `0` (BT_CLI_AGENT_NONE) or
    /// unknown tags (forward-compat with future Rust additions).
    pub fn from_ffi_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::Claude),
            2 => Some(Self::Gemini),
            3 => Some(Self::Codex),
            4 => Some(Self::Amp),
            5 => Some(Self::Droid),
            6 => Some(Self::OpenCode),
            7 => Some(Self::Copilot),
            8 => Some(Self::Pi),
            9 => Some(Self::Auggie),
            10 => Some(Self::CursorCli),
            11 => Some(Self::Goose),
            12 => Some(Self::Hermes),
            13 => Some(Self::Vibe),
            _ => None,
        }
    }

    pub fn raw_value(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockHeaderModel {
    pub block_id: u64,
    pub command: String,
    pub subtitle: Option<String>,
    pub agent: Option<CliAgent>,
}

/// Default neutral grey for unmapped agents — preserves the "row stays
/// visually consistent" rule from the old SwiftUI view.
const DEFAULT_AGENT_TINT: u32 = 0x8080_80FF;

/// 0xRRGGBBAA brand tint for each agent's badge background. Same values
/// BlockHeader.swift used to define inline.
fn agent_tint_table(agent: CliAgent) -> u32 {
    match agent {
        CliAgent::Claude => 0xD66E_4DFF,
        CliAgent::Codex => 0x6BB6_B1FF,
        CliAgent::Gemini => 0x4D8C_F2FF,
        CliAgent::Amp => 0xFFC8_22FF,
        CliAgent::Droid => 0x9CC4_33FF,
        CliAgent::OpenCode => 0x7373_D9FF,
        CliAgent::Copilot => 0x2233_52FF,
        CliAgent::Pi => 0xF0EB_D6FF,
        CliAgent::Auggie => 0x3333_33FF,
        CliAgent::CursorCli => 0x2121_21FF,
        CliAgent::Goose => 0xA672_2EFF,
        CliAgent::Hermes => 0x8252_BCFF,
        CliAgent::Vibe => 0xF27E_2EFF,
    }
}

impl BlockHeaderModel {
    /// Tint for a given agent. `None` agent → 0, unknown future agent →
    /// neutral grey fallback.
    pub fn tint_for(agent: Option<CliAgent>) -> u32 {
        match agent {
            None => 0,
            Some(a) => {
                let raw = agent_tint_table(a);
                if raw == 0 {
                    DEFAULT_AGENT_TINT
                } else {
                    raw
                }
            }
        }
    }

    /// Maps to Rust's `IconSlot` via `bedterm_core/icon_atlas.rs` — raw
    /// value of the enum (1..=13), or 0 when no agent is identified.
    pub fn agent_id(agent: Option<CliAgent>) -> u8 {
        agent.map_or(0, CliAgent::raw_value)
    }

    /// Format a duration in seconds the same way the Swift impl did.
    pub fn format_duration(seconds: f64) -> String {
        if seconds < 1.0 {
            format!("{:.0}ms", seconds * 1000.0)
        } else if seconds < 60.0 {
            format!("{seconds:.1}s")
        } else {
            let minutes = seconds as u64 / 60;
            let secs = seconds as u64 % 60;
            format!("{minutes}m {secs}s")
        }
    }
}

/// Pack RGBA components (each 0..=255) into a `0xRRGGBBAA` u32 — mirrors
/// `UIColor.asRGBA32()` from the Swift extension.
pub fn pack_rgba32(r: u8, g: u8, b: u8, a: u8) -> u32 {
    (u32::from(r) << 24) | (u32::from(g) << 16) | (u32::from(b) << 8) | u32::from(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_ffi_round_trip() {
        for tag in 1u8..=13 {
            let a = CliAgent::from_ffi_tag(tag).unwrap();
            assert_eq!(a.raw_value(), tag);
        }
        assert!(CliAgent::from_ffi_tag(0).is_none());
        assert!(CliAgent::from_ffi_tag(99).is_none());
    }

    #[test]
    fn tint_none_is_zero() {
        assert_eq!(BlockHeaderModel::tint_for(None), 0);
    }

    #[test]
    fn tint_claude_is_brand_orange() {
        assert_eq!(
            BlockHeaderModel::tint_for(Some(CliAgent::Claude)),
            0xD66E_4DFF
        );
    }

    #[test]
    fn agent_id_none_is_zero() {
        assert_eq!(BlockHeaderModel::agent_id(None), 0);
    }

    #[test]
    fn agent_id_matches_raw_value() {
        assert_eq!(BlockHeaderModel::agent_id(Some(CliAgent::Gemini)), 2);
        assert_eq!(BlockHeaderModel::agent_id(Some(CliAgent::Vibe)), 13);
    }

    #[test]
    fn format_duration_under_one_second_renders_millis() {
        assert_eq!(BlockHeaderModel::format_duration(0.25), "250ms");
    }

    #[test]
    fn format_duration_under_one_minute_renders_seconds() {
        assert_eq!(BlockHeaderModel::format_duration(12.345), "12.3s");
    }

    #[test]
    fn format_duration_over_one_minute_renders_min_sec() {
        assert_eq!(BlockHeaderModel::format_duration(125.0), "2m 5s");
    }

    #[test]
    fn pack_rgba32_orders_channels_high_to_low() {
        assert_eq!(pack_rgba32(0xAB, 0xCD, 0xEF, 0x12), 0xABCD_EF12);
    }
}
