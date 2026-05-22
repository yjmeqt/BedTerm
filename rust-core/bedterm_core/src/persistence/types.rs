//! Plain-old-data types persisted to SQLite. Kept separate from the
//! in-memory `Block` because the persisted form is byte-oriented
//! (BLOBs of stylized output) while the live one carries a `BlockGrid`.

use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum KillReason {
    UserKilled = 0,
    RemoteLogout = 1,
    NetworkDrop = 2,
    AppRelaunch = 3,
    SwapEvicted = 4,
}

impl KillReason {
    pub fn from_i32(v: i32) -> Option<Self> {
        Some(match v {
            0 => Self::UserKilled,
            1 => Self::RemoteLogout,
            2 => Self::NetworkDrop,
            3 => Self::AppRelaunch,
            4 => Self::SwapEvicted,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SnapshotRow {
    pub id: String,
    pub host_id: String,
    pub kill_reason: Option<KillReason>,
    pub killed_at: Option<f64>,
    pub last_cwd: Option<String>,
    pub last_command: Option<String>,
    pub last_exit_code: Option<i32>,
    pub created_at: f64,
    pub owner_pid: Option<i64>,
    pub owner_boot_time: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct BlockRow {
    pub snapshot_id: String,
    pub block_seq: i64,
    pub command: String,
    pub stylized_command: Vec<u8>,
    pub stylized_output: Vec<u8>,
    pub exit_code: Option<i32>,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub started_at: f64,
    pub finished_at: f64,
}

pub fn unix_seconds_now() -> f64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
