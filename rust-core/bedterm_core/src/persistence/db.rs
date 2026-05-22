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
        Ok(Self { conn })
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
