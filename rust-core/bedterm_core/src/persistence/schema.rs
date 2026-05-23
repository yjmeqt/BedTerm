//! Schema migrations. Forward-only; each `apply` is idempotent.

use rusqlite::{Connection, Result};

pub const CURRENT_VERSION: i32 = 1;

pub fn apply(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS schema_version (
             version INTEGER NOT NULL PRIMARY KEY
         );",
    )?;
    let current: Option<i32> = conn
        .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
        .ok();
    if current.is_none() {
        conn.execute(
            "INSERT INTO schema_version(version) VALUES (?1)",
            [CURRENT_VERSION],
        )?;
    }
    apply_v1(conn)?;
    Ok(())
}

fn apply_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS snapshots (
             id              TEXT NOT NULL PRIMARY KEY,
             host_id         TEXT NOT NULL,
             kill_reason     INTEGER,
             killed_at       REAL,
             last_cwd        TEXT,
             last_command    TEXT,
             last_exit_code  INTEGER,
             created_at      REAL NOT NULL,
             owner_pid       INTEGER,
             owner_boot_time REAL
         );
         CREATE INDEX IF NOT EXISTS snapshots_by_host
             ON snapshots(host_id, killed_at DESC);

         CREATE TABLE IF NOT EXISTS blocks (
             snapshot_id      TEXT NOT NULL,
             block_seq        INTEGER NOT NULL,
             command          TEXT NOT NULL,
             stylized_command BLOB NOT NULL,
             stylized_output  BLOB NOT NULL,
             exit_code        INTEGER,
             cwd              TEXT,
             git_branch       TEXT,
             started_at       REAL NOT NULL,
             finished_at      REAL NOT NULL,
             PRIMARY KEY (snapshot_id, block_seq),
             FOREIGN KEY (snapshot_id) REFERENCES snapshots(id) ON DELETE CASCADE
         );
         CREATE INDEX IF NOT EXISTS blocks_by_snapshot
             ON blocks(snapshot_id, block_seq);",
    )?;
    Ok(())
}
