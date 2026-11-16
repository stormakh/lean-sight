//! Preferences (table `preferences`), per org, user or team. `json_data` keeps the extra fields as JSON text
//! (language, regionalFormat, queryHistory, navbar, cookies, ...).
use super::SResult;

#[derive(Debug, Clone, Default)]
pub struct Preferences {
    pub id: i64,
    pub org_id: i64,
    /// 0 for org or team preferences.
    pub user_id: i64,
    /// 0 for org or user preferences.
    pub team_id: i64,
    pub version: i64,
    pub home_dashboard_id: i64,
    pub home_dashboard_uid: String,
    pub timezone: String,
    pub theme: String,
    pub week_start: String,
    pub json_data: String,
    pub created: i64,
    pub updated: i64,
}

pub trait PrefStore {
    /// The row for exactly (org_id, user_id, team_id); NotFound when there is none.
    fn prefs_get(&self, org_id: i64, user_id: i64, team_id: i64) -> SResult<Preferences>;
    /// Inserts or replaces the row for (org_id, user_id, team_id).
    fn prefs_save(&self, p: &Preferences) -> SResult<()>;
}
