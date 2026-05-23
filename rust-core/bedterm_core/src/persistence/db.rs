//! Thin wrapper around a single rusqlite `Connection`. All other
//! persistence operations live in sibling files and take `&Database`.

use rusqlite::{Connection, Result};
use std::path::Path;

use super::schema;

pub struct Database {
    pub(crate) conn: Connection,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        schema::apply(&conn)?;
        let db = Self { conn };
        db.sweep_orphans()?;
        Ok(db)
    }

    /// Mark every snapshot with a NULL `killed_at` as `AppRelaunch` (3).
    /// The `killed_at` is set to the latest `blocks.finished_at` for that
    /// snapshot, or the current time if there are no blocks.
    ///
    /// NOTE: The literal `3` matches `KillReason::AppRelaunch as i32`.
    /// If the enum order ever changes, this number must change with it.
    pub fn sweep_orphans(&self) -> Result<()> {
        self.conn.execute(
            "UPDATE snapshots
             SET kill_reason = 3,
                 killed_at = COALESCE(
                     (SELECT MAX(finished_at) FROM blocks WHERE blocks.snapshot_id = snapshots.id),
                     unixepoch()
                 )
             WHERE killed_at IS NULL",
            [],
        )?;
        Ok(())
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        schema::apply(&conn)?;
        Ok(Self { conn })
    }

    pub fn schema_version(&self) -> Result<i32> {
        self.conn
            .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
    }
}
