//! Dashboards (tables `dashboard`, `dashboard_version`, `dashboard_tag`). `data` is the classic dashboard JSON text.
use super::SResult;

#[derive(Debug, Clone, Default)]
pub struct Dashboard {
    pub id: i64,
    pub uid: String,
    pub org_id: i64,
    pub version: i64,
    pub slug: String,
    pub title: String,
    pub data: String,
    /// "" for the root (General).
    pub folder_uid: String,
    pub created: i64,
    pub updated: i64,
    pub created_by: i64,
    pub updated_by: i64,
    pub gnet_id: i64,
    pub plugin_id: String,
    /// Stored API version of `data`: "v0alpha1" (legacy API saves) or "v1".
    pub api_version: String,
    pub tags: Vec<String>,
    /// Kubernetes metadata that is not derived from the columns, as JSON text:
    /// `{"uid","resourceVersion","annotations":{...},"managedFields":[...]}` (see api::dashboards).
    pub meta: String,
}

/// A dashboard without its JSON, for lists and search.
#[derive(Debug, Clone, Default)]
pub struct DashboardMeta {
    pub id: i64,
    pub uid: String,
    pub org_id: i64,
    pub version: i64,
    pub slug: String,
    pub title: String,
    pub folder_uid: String,
    pub created: i64,
    pub updated: i64,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DashboardVersion {
    pub id: i64,
    pub dashboard_id: i64,
    pub dashboard_uid: String,
    pub parent_version: i64,
    pub restored_from: i64,
    pub version: i64,
    pub created: i64,
    pub created_by: i64,
    pub message: String,
    pub data: String,
    pub api_version: String,
    /// The Kubernetes metadata of this generation (as `Dashboard::meta`).
    pub meta: String,
}

/// One save: insert when `is_new` (with the given id), else update by (org_id, id). The caller sets every field
/// (version, times, users, meta) and checks versions and permissions; the store writes the dashboard row, a
/// `dashboard_version` row and the tags in one transaction.
#[derive(Debug, Clone, Default)]
pub struct SaveDashboard {
    pub dashboard: Dashboard,
    pub is_new: bool,
    pub message: String,
    pub restored_from: i64,
}

pub trait DashboardStore {
    fn dashboard_by_uid(&self, org_id: i64, uid: &str) -> SResult<Dashboard>;
    /// `dashboard_by_uid` plus the shown login (`user_display_login`) of `created_by` and `updated_by`, None when
    /// that user does not exist; one query for the dashboard view.
    fn dashboard_by_uid_with_logins(&self, org_id: i64, uid: &str) -> SResult<(Dashboard, Option<String>, Option<String>)>;
    fn dashboard_by_id(&self, org_id: i64, id: i64) -> SResult<Dashboard>;
    /// Saves (Conflict when a new dashboard's uid or id exists).
    fn dashboard_save(&self, s: &SaveDashboard) -> SResult<()>;
    /// Deletes the dashboard, its versions, tags and stars.
    fn dashboard_delete(&self, org_id: i64, uid: &str) -> SResult<()>;
    /// Every dashboard of the org without JSON, by title.
    fn dashboards(&self, org_id: i64) -> SResult<Vec<DashboardMeta>>;
    /// Every dashboard of the org with JSON, by uid, from `after_uid` (exclusive), at most `limit`.
    fn dashboards_full(&self, org_id: i64, after_uid: &str, limit: i64) -> SResult<Vec<Dashboard>>;
    /// Versions, newest first.
    fn dashboard_versions(&self, dashboard_id: i64, limit: i64, offset: i64) -> SResult<Vec<DashboardVersion>>;
    fn dashboard_version(&self, dashboard_id: i64, version: i64) -> SResult<DashboardVersion>;
    /// (tag, count) over the org, by tag.
    fn dashboard_tags(&self, org_id: i64) -> SResult<Vec<(String, i64)>>;
}
