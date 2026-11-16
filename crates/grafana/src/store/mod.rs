//! Storage interface. `Store` is the sum of one trait per area, so each area agent adds methods to its own file
//! (here and in `store-sqlite/src/<area>.rs`) without touching the others.
//!
//! Rules:
//! - Every method is atomic: it runs in one database transaction. When an API call needs several writes to happen
//!   together (dashboard row + version row + tags), add ONE method that does them all, do not chain calls.
//! - Times are unix seconds (`i64`) unless the field name ends in `_ms`.
//! - JSON columns (dashboard data, data source json_data, ...) are kept as JSON text: parse only when needed.
//! - Every lookup is scoped by `org_id` when the table has one.
pub mod annotation;
pub mod auth;
pub mod dashboard;
pub mod datasource;
pub mod folder;
pub mod kv;
pub mod org;
pub mod prefs;
pub mod query_history;
pub mod star;
pub mod team;
pub mod user;

pub use annotation::*;
pub use auth::*;
pub use dashboard::*;
pub use datasource::*;
pub use folder::*;
pub use kv::*;
pub use org::*;
pub use prefs::*;
pub use query_history::*;
pub use star::*;
pub use team::*;
pub use user::*;

#[derive(Debug, Clone, PartialEq)]
pub enum StoreError {
    NotFound,
    /// A unique constraint failed (same name, uid, login, ...). The text says which.
    Conflict(String),
    Other(String),
}
impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::NotFound => f.write_str("not found"),
            StoreError::Conflict(m) => write!(f, "conflict: {m}"),
            StoreError::Other(m) => write!(f, "store error: {m}"),
        }
    }
}
impl std::error::Error for StoreError {}

pub type SResult<T> = Result<T, StoreError>;

/// Health and housekeeping.
pub trait MetaStore {
    /// True when the database answers (`/api/health` "database": "ok").
    fn ping(&self) -> bool;
}

/// Everything the app needs from storage.
pub trait Store:
    MetaStore + OrgStore + UserStore + AuthStore + PrefStore + StarStore + FolderStore + DashboardStore + DataSourceStore + AnnotationStore + QueryHistoryStore + KvStore + TeamStore + Send + Sync
{
}
impl<T> Store for T where
    T: MetaStore + OrgStore + UserStore + AuthStore + PrefStore + StarStore + FolderStore + DashboardStore + DataSourceStore + AnnotationStore + QueryHistoryStore + KvStore + TeamStore + Send + Sync
{
}
