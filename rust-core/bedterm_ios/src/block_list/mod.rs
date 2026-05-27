//! Port of `BlockListContainerViewController` from
//! `BedTermKit/Features/Terminal/Blocks/BlockListContainerView.swift`
//! plus its four extensions.
//!
//! Child view controller that overlays the shared metal view while the
//! parent is in `.blockList` mode. The metal view does all the glyph
//! rendering — this VC provides:
//!   * a UIScrollView-style content offset (driven by a pan gesture)
//!   * per-block layout (`layout` submodule)
//!   * sticky-header pinning (`sticky` submodule)
//!   * long-press text selection inside one block (`selection` submodule)
//!
//! Block data flows through the [`source::BlockSource`] trait object;
//! W4 ships with no source wired (`refresh` lays out zero blocks). W6
//! lands the real `TerminalSession`-backed implementation; W7 mounts
//! this VC under the parent terminal VC.
//!
//! ## Host-testability
//!
//! The pure-logic submodules below compile on every target so their
//! unit tests run via `cargo test -p bedterm_ios` on macOS hosts.
//! The UIKit class definition (`define_class!` block, `Ivars`, etc.)
//! is split into [`vc`] and only compiled on `target_os = "ios"`.

// W4 wave: nothing constructs this VC yet (W7 wires it under the parent
// VC). Suppress dead-code warnings for the orchestration surface so
// `-D warnings` stays green; the unit tests still exercise the pure
// math in `layout`, `scroll`, `sticky`, `selection`.
#![allow(dead_code)]

pub mod layout;
pub mod scroll;
pub mod selection;
pub mod source;
pub mod sticky;

// The UIKit class lives in `vc.rs`, gated on iOS so the pure submodules
// above stay host-compilable for unit tests.
#[cfg(target_os = "ios")]
mod vc;

#[cfg(target_os = "ios")]
#[allow(unused_imports)]
pub use vc::BtIosBlockListViewController;

#[cfg(test)]
mod tests {
    use super::source::{BlockSource, EmptyBlockSource};

    #[test]
    fn empty_source_yields_zero_height() {
        let s = EmptyBlockSource;
        assert_eq!(s.block_count(), 0);
        assert!(s.block_at(0).is_none());
    }
}
