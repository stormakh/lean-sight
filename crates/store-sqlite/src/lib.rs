//! `grafana::Store` on SQLite. One connection behind a mutex (SQLite writes one at a time anyway); every Store method
//! is one statement or one transaction. One file per Store area, as in `grafana/src/store/`.
mod annotation;
mod auth;
mod dashboard;
mod datasource;
mod folder;
mod kv;
mod org;
mod prefs;
mod query_history;
mod star;
mod team;
mod user;

use grafana::store::{MetaStore, SResult, StoreError};
use rusqlite::{Connection, ErrorCode};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// One connection: a pool of read-only connections was measured (2 and 4 readers): about 0.23 MB PSS each and no
/// CPU gain that stood out from the noise, because each request now takes the lock only 1-4 times.
pub struct SqliteStore {
    conn: Mutex<Connection>,
}

const SCHEMA: &str = include_str!("schema.sql");
const SCHEMA_VERSION: i64 = 2;

pub(crate) fn map_err(e: rusqlite::Error) -> StoreError {
    match e {
        rusqlite::Error::QueryReturnedNoRows => StoreError::NotFound,
        rusqlite::Error::SqliteFailure(f, msg) if f.code == ErrorCode::ConstraintViolation => StoreError::Conflict(msg.unwrap_or_else(|| "constraint failed".to_string())),
        other => StoreError::Other(other.to_string()),
    }
}

/// Tags as stored in a text column: a JSON array.
pub(crate) fn tags_to_text(tags: &[String]) -> String {
    let mut a = json::Json::arr();
    for t in tags {
        a.push(t.as_str());
    }
    a.stringify()
}
pub(crate) fn tags_from_text(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(json::Json::Arr(a)) = json::Json::parse(s) {
        for v in a {
            if let json::Json::Str(t) = v {
                out.push(t);
            }
        }
    }
    out
}

impl SqliteStore {
    /// Opens (creating when needed) the database file and applies the schema.
    pub fn open(path: &Path) -> SResult<SqliteStore> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let conn = Connection::open(path).map_err(map_err)?;
        // Exclusive locking: this process is the only user of the file, so SQLite takes the file lock once and keeps
        // the WAL index in heap memory. In the default mode each statement took and dropped shared-memory read
        // locks (2 fcntl calls per statement). Another process cannot open the database while the server runs.
        conn.execute_batch(
            "PRAGMA locking_mode=EXCLUSIVE; PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=OFF; PRAGMA busy_timeout=5000; PRAGMA cache_size=-1024; PRAGMA temp_store=MEMORY;",
        )
        .map_err(map_err)?;
        // Query planner stability guarantee: plans never depend on bound values. Without it SQLite marks statements
        // whose plan looked at a parameter (ORDER BY with an equality, LIKE) as expired on every new binding, so each
        // cached statement was parsed and planned again on every use.
        conn.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_ENABLE_QPSG, true).map_err(map_err)?;
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(map_err)?;
        if v < SCHEMA_VERSION {
            conn.execute_batch(SCHEMA).map_err(map_err)?;
            conn.execute_batch(&format!("PRAGMA user_version={SCHEMA_VERSION}")).map_err(map_err)?;
        }
        conn.set_prepared_statement_cache_capacity(64);
        Ok(SqliteStore { conn: Mutex::new(conn) })
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Connection> {
        match self.conn.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    /// Runs `f` on the connection.
    pub(crate) fn db<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> SResult<T> {
        let c = self.lock();
        f(&c).map_err(map_err)
    }

    /// Runs `f` in a transaction; commits when it returns Ok, rolls back otherwise.
    pub(crate) fn tx<T>(&self, f: impl FnOnce(&Connection) -> Result<T, StoreError>) -> SResult<T> {
        let mut c = self.lock();
        let t = c.transaction().map_err(map_err)?;
        let r = f(&t)?;
        t.commit().map_err(map_err)?;
        Ok(r)
    }

    /// Frees SQLite's page cache memory (for idle periods).
    pub fn release_memory(&self) {
        let c = self.lock();
        let _ = c.execute_batch("PRAGMA shrink_memory");
    }
}

impl MetaStore for SqliteStore {
    fn ping(&self) -> bool {
        self.db(|c| c.prepare_cached("SELECT 1")?.query_row([], |r| r.get::<_, i64>(0))).is_ok()
    }
}
