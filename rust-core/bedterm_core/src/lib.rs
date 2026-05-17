//! BedTerm terminal core — VTE parser + grid + scrollback for iOS / macOS.

pub mod ffi;
pub mod renderer;
pub mod snapshot;
pub mod term;

#[cfg(feature = "mock-tty")]
pub mod mock_tty;
