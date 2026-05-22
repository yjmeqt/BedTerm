//! SQLite-backed persistence for session snapshots and their blocks.
//!
//! Owns the schema (see `schema.rs`), the connection (`db.rs`), and
//! the read/write surface used by the FFI layer (`ops.rs`, added later).
//!
//! Capture-side wiring is in `Terminal::feed`; this module is unaware
//! of the live terminal — it only takes already-buffered byte ranges
//! plus metadata.

pub mod db;
pub mod schema;
pub mod snapshots;
pub mod types;

pub use db::Database;
pub use types::{unix_seconds_now, BlockRow, KillReason, SnapshotRow};
