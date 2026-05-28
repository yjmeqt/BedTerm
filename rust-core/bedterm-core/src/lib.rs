//! BedTerm terminal core — VTE parser + grid + scrollback for iOS / macOS.

pub mod block_grid;
pub mod blocks;
pub mod blocks_ffi;
pub mod cli_agent;
pub mod dcs;
pub mod device_presets;
pub mod ffi;
pub mod renderer;
pub mod snapshot;
pub mod term;

// Re-export FFI mode constants at crate root so cbindgen emits them into the
// generated C header.
pub use blocks_ffi::{
    BT_BLOCK_END_LINE_RUNNING, BT_CLI_AGENT_AMP, BT_CLI_AGENT_AUGGIE, BT_CLI_AGENT_CLAUDE,
    BT_CLI_AGENT_CODEX, BT_CLI_AGENT_COPILOT, BT_CLI_AGENT_CURSOR_CLI, BT_CLI_AGENT_DROID,
    BT_CLI_AGENT_GEMINI, BT_CLI_AGENT_GOOSE, BT_CLI_AGENT_HERMES, BT_CLI_AGENT_NONE,
    BT_CLI_AGENT_OPENCODE, BT_CLI_AGENT_PI, BT_CLI_AGENT_VIBE,
};
pub use term::{
    BT_MODE_ALT_SCREEN, BT_MODE_APP_CURSOR, BT_MODE_APP_KEYPAD, BT_MODE_BRACKETED_PASTE,
    BT_MODE_FOCUS_IN_OUT, BT_MODE_MOUSE_REPORT,
};
