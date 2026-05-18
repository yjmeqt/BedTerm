//! BedTerm terminal core — VTE parser + grid + scrollback for iOS / macOS.

pub mod ffi;
pub mod osc133;
pub mod renderer;
pub mod snapshot;
pub mod term;

#[cfg(feature = "mock-tty")]
pub mod mock_tty;

// Re-export FFI mode constants at crate root so cbindgen emits them into the
// generated C header.
pub use ffi::{
    BT_OSC133_COMMAND_END, BT_OSC133_COMMAND_START, BT_OSC133_OUTPUT_START, BT_OSC133_PROMPT_START,
};
pub use term::{
    BT_MODE_ALT_SCREEN, BT_MODE_APP_CURSOR, BT_MODE_APP_KEYPAD, BT_MODE_BRACKETED_PASTE,
    BT_MODE_FOCUS_IN_OUT, BT_MODE_MOUSE_REPORT,
};
