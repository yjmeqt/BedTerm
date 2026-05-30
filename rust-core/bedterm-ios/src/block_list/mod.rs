//! Port of `BlockListContainerViewController` from
//! `BedTermKit/Features/Terminal/Blocks/BlockListContainerView.swift`.
//!
//! Pure-logic submodules (layout, scroll, selection, source, sticky) are
//! re-exported from `bedterm_app`. The UIKit `BtIosBlockListViewController`
//! class lives in [`vc`] (iOS-gated).

#![allow(dead_code)]
#![cfg_attr(not(target_os = "ios"), allow(unused_imports))]

// Re-export pure submodules from bedterm-app
pub use bedterm_app::block_list::layout;
pub use bedterm_app::block_list::scroll;
pub use bedterm_app::block_list::source;
pub use bedterm_app::block_list::sticky;
// selection is local — re-exports pure types from bedterm-app plus the
// iOS-only copy_to_pasteboard function.
pub mod selection;

// The UIKit class lives in `vc.rs`, gated on iOS.
#[cfg(target_os = "ios")]
mod vc;

#[cfg(target_os = "ios")]
#[allow(unused_imports)]
pub use vc::BtIosBlockListViewController;
