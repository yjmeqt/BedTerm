//! Block list layout — ported from Swift `BlockListContainerView` and its
//! `+Layout` / `+Sticky` extensions.
//!
//! Computes per-frame geometry for block list rendering: block Y ranges,
//! `BtBlockLayoutEntry` arrays, and `BtBlockHeaderEntry` arrays (including
//! the sticky pinned header). Color tokens are hardcoded per palette rather
//! than resolved from an Xcode asset catalog.

use bedterm_core::blocks::Block;
use bedterm_core::cli_agent::CliAgent;
use bedterm_core::renderer::block_list_ffi::{BtBlockHeaderEntry, BtBlockLayoutEntry};

// ── Layout constants (match BlockPanelStyle / BlockHeader) ──────────────

const HEADER_HEIGHT_PT: f32 = 56.0;
const INTER_BLOCK_GAP_PT: f32 = 12.0;

// ── Color tokens ───────────────────────────────────────────────────────

pub(crate) struct PaletteColors {
    pub fg: u32,
    pub muted: u32,
    pub divider: u32,
    #[allow(dead_code)]
    pub surface_bg: u32,
}

/// Tokyo Night Storm palette.
pub(crate) const TOKYO_NIGHT: PaletteColors = PaletteColors {
    fg: 0xC0CAF5FF,         // #c0caf5
    muted: 0x565F89FF,      // #565f89
    divider: 0x292E4280,    // #292e42 alpha 0.5
    surface_bg: 0x24283BFF, // #24283b
};

// ── Agent badge tints ──────────────────────────────────────────────────

fn agent_badge_tint(agent: Option<CliAgent>) -> u32 {
    match agent {
        Some(CliAgent::Claude) => 0xD97706FF,
        Some(CliAgent::Codex) => 0x10B981FF,
        Some(CliAgent::Gemini) => 0x3B82F6FF,
        Some(CliAgent::Copilot) => 0x6366F1FF,
        Some(CliAgent::CursorCli) => 0x8B5CF6FF,
        Some(CliAgent::Auggie) => 0xF59E0BFF,
        Some(CliAgent::Goose) => 0xEC4899FF,
        Some(CliAgent::OpenCode) => 0x06B6D4FF,
        Some(CliAgent::Pi) => 0x84CC16FF,
        Some(CliAgent::Droid) => 0xEF4444FF,
        Some(CliAgent::Vibe) => 0xA855F7FF,
        Some(CliAgent::Hermes) => 0xF97316FF,
        Some(CliAgent::Amp) => 0x22D3EEFF,
        None => 0,
    }
}

fn agent_id(agent: Option<CliAgent>) -> u8 {
    match agent {
        Some(CliAgent::Claude) => 1,
        Some(CliAgent::Codex) => 2,
        _ => 3, // generic
    }
}

fn body_rows(block: &Block) -> i32 {
    if block.is_running {
        // Live block — use the block grid's used_rows if attached.
        // Fall back to zero so the block panel is header-tall until
        // commands start producing output.
        block.grid.as_ref().map(|g| g.used_rows()).unwrap_or(0) as i32
    } else {
        // Sealed block — frozen snapshot row count.
        block.end_line.saturating_sub(block.start_line)
    }
}

// ── Block ranges ───────────────────────────────────────────────────────

/// Lightweight reference to position data derived from a block.
pub(crate) struct BlockRange {
    pub block_id: u64,
    pub command: String,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    pub cli_agent: Option<CliAgent>,
    pub top_pt: f32,
    pub bot_pt: f32,
}

impl BlockRange {
    fn from_block(block: &Block, top_pt: f32, row_height_pt: f32) -> Self {
        let body_pt = body_rows(block) as f32 * row_height_pt;
        Self {
            block_id: block.id,
            command: block.command.clone(),
            exit_code: block.exit_code,
            duration_ms: block.duration_ms,
            cli_agent: block.cli_agent,
            top_pt,
            bot_pt: top_pt + HEADER_HEIGHT_PT + body_pt,
        }
    }
}

/// Compute per-block top + bottom Y in scroll-content coordinates.
pub(crate) fn compute_block_ranges(blocks: &[Block], row_height_pt: f32) -> Vec<BlockRange> {
    let mut out = Vec::with_capacity(blocks.len());
    let mut y: f32 = 0.0;
    for block in blocks {
        let range = BlockRange::from_block(block, y, row_height_pt);
        y = range.bot_pt + INTER_BLOCK_GAP_PT;
        out.push(range);
    }
    out
}

// ── Layout entries ─────────────────────────────────────────────────────

pub(crate) fn build_layout_entries(
    ranges: &[BlockRange],
    scale: f32,
    width_px: f32,
) -> Vec<BtBlockLayoutEntry> {
    ranges
        .iter()
        .map(|r| {
            let body_top_pt = r.top_pt + HEADER_HEIGHT_PT;
            let body_pt = r.bot_pt - body_top_pt;
            BtBlockLayoutEntry {
                block_id: r.block_id,
                body_y_top_px: body_top_pt * scale,
                body_height_px: body_pt * scale,
                panel_y_top_px: r.top_pt * scale,
                panel_height_px: (r.bot_pt - r.top_pt) * scale,
                panel_x_left_px: 0.0,
                panel_width_px: width_px,
                panel_bg_rgba: 0,
                panel_corner_radius_px: 0.0,
            }
        })
        .collect()
}

// ── Header descriptors ─────────────────────────────────────────────────

pub(crate) fn subtitle(block_range: &BlockRange) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(code) = block_range.exit_code {
        parts.push(format!("exit {}", code));
    }
    if let Some(ms) = block_range.duration_ms {
        if ms >= 1000 {
            parts.push(format!("{:.1}s", ms as f64 / 1000.0));
        } else {
            parts.push(format!("{}ms", ms));
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

/// Build per-block header descriptors (natural in-flow headers, not sticky).
pub(crate) fn build_header_descriptors(
    ranges: &[BlockRange],
    scale: f32,
    width_px: f32,
    colors: &PaletteColors,
) -> (Vec<BtBlockHeaderEntry>, Vec<(Vec<u8>, Option<Vec<u8>>)>) {
    let header_h_px = HEADER_HEIGHT_PT * scale;
    let mut entries = Vec::with_capacity(ranges.len());
    let mut storage: Vec<(Vec<u8>, Option<Vec<u8>>)> = Vec::with_capacity(ranges.len());

    for (idx, range) in ranges.iter().enumerate() {
        let cmd_bytes = range.command.as_bytes().to_vec();
        let sub = subtitle(range).map(|s| s.into_bytes());
        let y_top_px = range.top_pt * scale;
        let divider = if idx == 0 { 0 } else { colors.divider };

        entries.push(BtBlockHeaderEntry {
            block_id: range.block_id,
            header_y_top_px: y_top_px,
            header_height_px: header_h_px,
            panel_x_left_px: 0.0,
            panel_width_px: width_px,
            command_utf8: std::ptr::null(),
            command_len: cmd_bytes.len() as u32,
            subtitle_utf8: std::ptr::null(),
            subtitle_len: sub.as_ref().map(|s| s.len() as u32).unwrap_or(0),
            agent_id: agent_id(range.cli_agent),
            _pad: [0; 3],
            badge_tint_rgba: agent_badge_tint(range.cli_agent),
            command_fg_rgba: colors.fg,
            subtitle_fg_rgba: colors.muted,
            divider_rgba: divider,
            is_sticky: 0,
            _pad2: [0; 3],
        });
        storage.push((cmd_bytes, sub));
    }
    (entries, storage)
}

// ── Sticky headers ─────────────────────────────────────────────────────

#[allow(dead_code)]
pub(crate) struct StickyDescriptor {
    pub entry: BtBlockHeaderEntry,
    pub block_id: u64,
    pub storage: (Vec<u8>, Option<Vec<u8>>),
}

#[allow(dead_code)]
pub(crate) fn sticky_active_index(ranges: &[BlockRange], scroll_y_pt: f32) -> Option<usize> {
    ranges
        .iter()
        .position(|r| r.top_pt <= scroll_y_pt && r.bot_pt > scroll_y_pt)
}

#[allow(dead_code)]
pub(crate) fn build_sticky_descriptor(
    _ranges: &[BlockRange],
    _scroll_y_pt: f32,
    _scale: f32,
    _width_px: f32,
    _colors: &PaletteColors,
) -> Option<StickyDescriptor> {
    // For offline CLI rendering we don't pin a sticky header — the
    // viewport shows the entire block list at rest. Sticky pinning is
    // only meaningful with live scroll interaction.
    None
}

/// Flatten header UTF-8 storage into a contiguous blob and patch
/// `command_utf8` / `subtitle_utf8` pointers.
pub(crate) fn patch_header_pointers(
    headers: &mut [BtBlockHeaderEntry],
    storage: &[(Vec<u8>, Option<Vec<u8>>)],
) -> Vec<u8> {
    let mut blob: Vec<u8> = Vec::new();
    let mut cmd_ranges: Vec<std::ops::Range<usize>> = Vec::with_capacity(storage.len());
    let mut sub_ranges: Vec<Option<std::ops::Range<usize>>> = Vec::with_capacity(storage.len());

    for (cmd, sub) in storage {
        let c_start = blob.len();
        blob.extend_from_slice(cmd);
        cmd_ranges.push(c_start..blob.len());

        if let Some(s) = sub {
            let s_start = blob.len();
            blob.extend_from_slice(s);
            sub_ranges.push(Some(s_start..blob.len()));
        } else {
            sub_ranges.push(None);
        }
    }

    let base = blob.as_ptr();
    for (idx, header) in headers.iter_mut().enumerate() {
        if idx < cmd_ranges.len() {
            header.command_utf8 = unsafe { base.add(cmd_ranges[idx].start) };
            if let Some(ref sub_range) = sub_ranges[idx] {
                header.subtitle_utf8 = unsafe { base.add(sub_range.start) };
            }
        }
    }
    blob
}
