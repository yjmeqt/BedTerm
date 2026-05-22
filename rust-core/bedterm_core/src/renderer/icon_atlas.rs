//! Embedded CLI-agent badge icons.
//!
//! Three PNGs ship in the crate at `assets/agent_badges/` and are
//! decoded once on first access. The header band uses these as
//! template glyphs — the renderer fills the badge circle with the
//! brand tint, then blits the template glyph on top in white so the
//! shape reads against any tint.
//!
//! Slot IDs are stable across runs; `slot_for_agent` maps the
//! Swift-side `CLIAgent` enum (encoded into `BtBlockHeaderEntry::agent_id`)
//! to a slot.

use image::ImageReader;
use std::io::Cursor;

#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum IconSlot {
    Generic = 0,
    Claude = 1,
    Codex = 2,
}

const CLAUDE_PNG: &[u8] = include_bytes!("../../assets/agent_badges/claude.png");
const CODEX_PNG: &[u8] = include_bytes!("../../assets/agent_badges/codex.png");
const GENERIC_PNG: &[u8] = include_bytes!("../../assets/agent_badges/generic.png");

/// BGRA8 premultiplied bitmap. Same format the glyph atlas expects so a
/// decoded icon can be uploaded into the existing texture without a
/// format conversion pass.
pub struct DecodedIcon {
    pub bgra: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Decode a slot's embedded PNG into a BGRA8 premultiplied bitmap.
/// Called once per (slot, atlas) — callers should cache the result.
pub fn decode(slot: IconSlot) -> DecodedIcon {
    let bytes = match slot {
        IconSlot::Generic => GENERIC_PNG,
        IconSlot::Claude => CLAUDE_PNG,
        IconSlot::Codex => CODEX_PNG,
    };
    let img = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .expect("embedded icon PNG bytes are well-formed")
        .decode()
        .expect("embedded icon PNG decodes")
        .to_rgba8();
    let (w, h) = img.dimensions();
    let mut bgra = Vec::with_capacity((w * h * 4) as usize);
    for px in img.pixels() {
        // Template render: we want a coverage mask, not the SVG's brand
        // colour, so the cell-pipeline alpha-mask path can tint by the
        // shader's `fg` (white in our case, over the brand-tinted badge
        // circle). Emit (a, a, a, a) so the texture behaves identically
        // to a monochrome glyph rasterized via swash.
        let a = px.0[3];
        bgra.push(a);
        bgra.push(a);
        bgra.push(a);
        bgra.push(a);
    }
    DecodedIcon {
        bgra,
        width: w,
        height: h,
    }
}

/// Map a `BtBlockHeaderEntry::agent_id` to an `IconSlot`. `agent_id == 0`
/// means "no badge"; anything else uses a branded icon when one exists
/// (Claude=1, Codex=2) or falls back to the generic sparkle.
pub fn slot_for_agent(agent_id: u8) -> Option<IconSlot> {
    match agent_id {
        0 => None,
        1 => Some(IconSlot::Claude),
        2 => Some(IconSlot::Codex),
        _ => Some(IconSlot::Generic),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_all_slots() {
        for slot in [IconSlot::Generic, IconSlot::Claude, IconSlot::Codex] {
            let d = decode(slot);
            assert!(d.width > 0 && d.height > 0, "slot {slot:?} decoded empty");
            assert_eq!(d.bgra.len() as u32, d.width * d.height * 4);
        }
    }

    #[test]
    fn slot_mapping_matches_swift_enum() {
        assert_eq!(slot_for_agent(0), None);
        assert_eq!(slot_for_agent(1), Some(IconSlot::Claude));
        assert_eq!(slot_for_agent(2), Some(IconSlot::Codex));
        assert_eq!(slot_for_agent(99), Some(IconSlot::Generic));
    }
}
