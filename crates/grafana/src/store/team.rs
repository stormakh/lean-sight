//! Teams (tables `team`, `team_member`), read only in grafana-rs.
use super::SResult;

#[derive(Debug, Clone, Default)]
pub struct Team {
    pub id: i64,
    pub uid: String,
    pub org_id: i64,
    pub name: String,
    pub email: String,
    pub member_count: i64,
    pub created: i64,
    pub updated: i64,
}

pub trait TeamStore {
    /// Teams whose name contains `query` (empty matches all), by name. Returns (page, total).
    fn team_search(&self, org_id: i64, query: &str, limit: i64, offset: i64) -> SResult<(Vec<Team>, i64)>;
    fn teams_of_user(&self, org_id: i64, user_id: i64) -> SResult<Vec<Team>>;
}
