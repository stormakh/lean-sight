//! Folders (table `folder`). Nested folders use `parent_uid` ("" for the root).
use super::SResult;

#[derive(Debug, Clone, Default)]
pub struct Folder {
    pub id: i64,
    pub uid: String,
    pub org_id: i64,
    pub title: String,
    pub description: String,
    pub parent_uid: String,
    pub version: i64,
    pub created: i64,
    pub updated: i64,
    pub created_by: i64,
    pub updated_by: i64,
    /// Kubernetes metadata as JSON text (as `Dashboard::meta`).
    pub meta: String,
}

pub trait FolderStore {
    fn folder_by_uid(&self, org_id: i64, uid: &str) -> SResult<Folder>;
    fn folder_by_id(&self, org_id: i64, id: i64) -> SResult<Folder>;
    /// Every folder of the org, by title.
    fn folders(&self, org_id: i64) -> SResult<Vec<Folder>>;
    /// Inserts (with `f.id` when it is not 0) and returns the id. Conflict when the uid or id exists, or the title
    /// exists under the same parent.
    fn folder_create(&self, f: &Folder) -> SResult<i64>;
    /// Writes title, description, parent_uid, version, updated, updated_by, meta by (org_id, uid).
    fn folder_update(&self, f: &Folder) -> SResult<()>;
    /// Deletes the folder, its descendant folders, and the dashboards in them.
    fn folder_delete(&self, org_id: i64, uid: &str) -> SResult<()>;
}
