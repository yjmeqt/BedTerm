//! Block CRUD with per-snapshot truncation enforced on insert.

use rusqlite::{params, Result};

use super::db::Database;
use super::types::BlockRow;

pub const MAX_BLOCKS_PER_SNAPSHOT: i64 = 100;

impl Database {
    pub fn insert_block(&self, row: &BlockRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO blocks (
                snapshot_id, block_seq, command,
                stylized_command, stylized_output,
                exit_code, cwd, git_branch,
                started_at, finished_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                row.snapshot_id,
                row.block_seq,
                row.command,
                row.stylized_command,
                row.stylized_output,
                row.exit_code,
                row.cwd,
                row.git_branch,
                row.started_at,
                row.finished_at,
            ],
        )?;
        // Trim: keep only the last MAX_BLOCKS_PER_SNAPSHOT for this
        // snapshot. NOT IN (SELECT ... ORDER BY block_seq DESC LIMIT ?)
        // is the same pattern Warp uses.
        self.conn.execute(
            "DELETE FROM blocks
             WHERE snapshot_id = ?1
             AND block_seq NOT IN (
                 SELECT block_seq FROM blocks
                 WHERE snapshot_id = ?1
                 ORDER BY block_seq DESC
                 LIMIT ?2
             )",
            params![row.snapshot_id, MAX_BLOCKS_PER_SNAPSHOT],
        )?;
        Ok(())
    }

    pub fn load_blocks(&self, snapshot_id: &str) -> Result<Vec<BlockRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT snapshot_id, block_seq, command,
                    stylized_command, stylized_output,
                    exit_code, cwd, git_branch,
                    started_at, finished_at
             FROM blocks WHERE snapshot_id = ?1
             ORDER BY block_seq ASC",
        )?;
        let rows = stmt.query_map(params![snapshot_id], |r| {
            Ok(BlockRow {
                snapshot_id: r.get(0)?,
                block_seq: r.get(1)?,
                command: r.get(2)?,
                stylized_command: r.get(3)?,
                stylized_output: r.get(4)?,
                exit_code: r.get(5)?,
                cwd: r.get(6)?,
                git_branch: r.get(7)?,
                started_at: r.get(8)?,
                finished_at: r.get(9)?,
            })
        })?;
        rows.collect()
    }
}
