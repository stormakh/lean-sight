//! Organizations and memberships (tables `org`, `org_user`).
use super::SResult;
use crate::ctx::Role;

#[derive(Debug, Clone, Default)]
pub struct Org {
    pub id: i64,
    pub name: String,
    pub created: i64,
    pub updated: i64,
}

/// A user's membership in an org, joined with the user row (for org user lists).
#[derive(Debug, Clone)]
pub struct OrgMember {
    pub org_id: i64,
    pub user_id: i64,
    pub role: Role,
    pub login: String,
    pub email: String,
    pub name: String,
    pub last_seen_at: i64,
    pub created: i64,
}

pub trait OrgStore {
    fn org_by_id(&self, id: i64) -> SResult<Org>;
    fn org_by_name(&self, name: &str) -> SResult<Org>;
    /// Creates an org and returns its id. Conflict when the name exists.
    fn org_create(&self, name: &str) -> SResult<i64>;
    /// The user's role in the org, NotFound when not a member.
    fn org_user_role(&self, org_id: i64, user_id: i64) -> SResult<Role>;
    /// Adds (or updates the role of) a member.
    fn org_user_set(&self, org_id: i64, user_id: i64, role: Role) -> SResult<()>;
    /// Orgs of a user with the role in each, by org name.
    fn orgs_of_user(&self, user_id: i64) -> SResult<Vec<(Org, Role)>>;
    /// Members of an org, by login.
    fn org_members(&self, org_id: i64) -> SResult<Vec<OrgMember>>;
    /// Renames an org. Conflict when the name exists, NotFound when the org does not.
    fn org_update_name(&self, id: i64, name: &str) -> SResult<()>;
    /// Deletes an org with its memberships and its org-scoped rows (preferences, stars, data sources,
    /// dashboards, folders, annotations, api keys); users whose current org it was move to another membership.
    fn org_delete(&self, id: i64) -> SResult<()>;
    /// Orgs whose name contains `query` (empty matches all), by name. Returns (page, total).
    fn org_search(&self, query: &str, limit: i64, offset: i64) -> SResult<(Vec<Org>, i64)>;
    /// Removes a membership; when it was the user's current org, the user moves to another membership (or org 0).
    fn org_user_remove(&self, org_id: i64, user_id: i64) -> SResult<()>;
}
