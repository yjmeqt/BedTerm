//! Snapshot CRUD. Block rows live in `blocks.rs`.

use rusqlite::{params, Result};

use super::db::Database;
use super::types::{unix_seconds_now, KillReason, SnapshotRow};

impl Database {
    pub fn insert_snapshot(&self, row: &SnapshotRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO snapshots (
                id, host_id, kill_reason, killed_at,
                last_cwd, last_command, last_exit_code,
                created_at, owner_pid, owner_boot_time
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                row.id,
                row.host_id,
                row.kill_reason.map(|k| k as i32),
                row.killed_at,
                row.last_cwd,
                row.last_command,
                row.last_exit_code,
                row.created_at,
                row.owner_pid,
                row.owner_boot_time,
            ],
        )?;
        Ok(())
    }

    pub fn record_kill(
        &self,
        snapshot_id: &str,
        reason: KillReason,
        last_cwd: Option<&str>,
        last_command: Option<&str>,
        last_exit_code: Option<i32>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE snapshots
             SET kill_reason = ?2, killed_at = ?3,
                 last_cwd = COALESCE(?4, last_cwd),
                 last_command = COALESCE(?5, last_command),
                 last_exit_code = COALESCE(?6, last_exit_code)
             WHERE id = ?1",
            params![
                snapshot_id,
                reason as i32,
                unix_seconds_now(),
                last_cwd,
                last_command,
                last_exit_code,
            ],
        )?;
        Ok(())
    }

    pub fn list_snapshots(&self, host_id: &str) -> Result<Vec<SnapshotRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, host_id, kill_reason, killed_at,
                    last_cwd, last_command, last_exit_code,
                    created_at, owner_pid, owner_boot_time
             FROM snapshots WHERE host_id = ?1
             ORDER BY COALESCE(killed_at, created_at) DESC",
        )?;
        let rows = stmt.query_map(params![host_id], |r| {
            Ok(SnapshotRow {
                id: r.get(0)?,
                host_id: r.get(1)?,
                kill_reason: r.get::<_, Option<i32>>(2)?.and_then(KillReason::from_i32),
                killed_at: r.get(3)?,
                last_cwd: r.get(4)?,
                last_command: r.get(5)?,
                last_exit_code: r.get(6)?,
                created_at: r.get(7)?,
                owner_pid: r.get(8)?,
                owner_boot_time: r.get(9)?,
            })
        })?;
        rows.collect()
    }

    pub fn discard_snapshot(&self, id: &str) -> Result<()> {
        // Foreign key with ON DELETE CASCADE handles blocks.
        self.conn
            .execute("DELETE FROM snapshots WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn discard_for_host(&self, host_id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM snapshots WHERE host_id = ?1", params![host_id])?;
        Ok(())
    }
}
