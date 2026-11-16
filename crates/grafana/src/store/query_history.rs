//! Explore query history (tables `query_history`, `query_history_star`).
use super::SResult;

#[derive(Debug, Clone, Default)]
pub struct QueryHistory {
    pub id: i64,
    pub uid: String,
    pub org_id: i64,
    pub datasource_uid: String,
    pub created_by: i64,
    pub created_at: i64,
    pub comment: String,
    /// JSON text (array of queries).
    pub queries: String,
    /// Filled by reads for the asking user.
    pub starred: bool,
}

#[derive(Debug, Clone, Default)]
pub struct QueryHistoryQuery {
    pub org_id: i64,
    pub user_id: i64,
    pub datasource_uids: Vec<String>,
    pub search: String,
    pub only_starred: bool,
    /// "time-desc" (default) or "time-asc".
    pub sort: String,
    /// Unix seconds, 0 for unbounded.
    pub from: i64,
    pub to: i64,
    pub page: i64,
    pub limit: i64,
}

pub trait QueryHistoryStore {
    /// Inserts and returns the id.
    fn query_history_create(&self, q: &QueryHistory) -> SResult<i64>;
    /// (page, total count).
    fn query_history_search(&self, q: &QueryHistoryQuery) -> SResult<(Vec<QueryHistory>, i64)>;
    fn query_history_by_uid(&self, org_id: i64, user_id: i64, uid: &str) -> SResult<QueryHistory>;
    fn query_history_delete(&self, org_id: i64, user_id: i64, uid: &str) -> SResult<()>;
    fn query_history_set_comment(&self, org_id: i64, user_id: i64, uid: &str, comment: &str) -> SResult<()>;
    fn query_history_star(&self, org_id: i64, user_id: i64, uid: &str, starred: bool) -> SResult<()>;
}
