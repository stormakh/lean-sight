//! Users (table `user`). Service accounts are users with `is_service_account`.
use super::SResult;
use crate::ctx::Role;

#[derive(Debug, Clone, Default)]
pub struct User {
    pub id: i64,
    pub uid: String,
    pub version: i64,
    pub login: String,
    pub email: String,
    pub name: String,
    /// `util.EncodePassword` hex hash; "" for no password.
    pub password: String,
    pub salt: String,
    pub rands: String,
    pub company: String,
    /// The org the user works in now.
    pub org_id: i64,
    /// Grafana server admin.
    pub is_admin: bool,
    pub email_verified: bool,
    pub theme: String,
    pub help_flags1: i64,
    pub is_disabled: bool,
    pub is_service_account: bool,
    pub is_provisioned: bool,
    pub created: i64,
    pub updated: i64,
    /// 0 when never seen.
    pub last_seen_at: i64,
}

/// The user columns an identity needs, with the org a request works in, read in one query for authentication.
#[derive(Debug, Clone, Default)]
pub struct UserInOrg {
    pub id: i64,
    pub uid: String,
    pub login: String,
    pub email: String,
    pub name: String,
    /// The stored password hash (to check a basic-auth cache entry).
    pub password: String,
    pub theme: String,
    pub help_flags1: i64,
    pub is_admin: bool,
    pub is_disabled: bool,
    pub is_service_account: bool,
    pub last_seen_at: i64,
    /// `org_id` asked for when the user is a member there, else the user's current org.
    pub org_id: i64,
    /// The role in that org, `Role::None` when not a member.
    pub role: Role,
    /// "" when the org does not exist.
    pub org_name: String,
}

pub trait UserStore {
    fn user_by_id(&self, id: i64) -> SResult<User>;
    /// The user with its org, role and org name in one query: the org is `org_id` when it is > 0, not the user's
    /// current org, and the user is a member there; otherwise the user's current org.
    fn user_in_org(&self, id: i64, org_id: i64) -> SResult<UserInOrg>;
    fn user_by_uid(&self, uid: &str) -> SResult<User>;
    /// The name shown for a user in createdBy/updatedBy: `name` for a service account, else `login`.
    fn user_display_login(&self, id: i64) -> SResult<String>;
    /// Matches login or email, case-insensitive, as Grafana's login lookup does.
    fn user_by_login_or_email(&self, login_or_email: &str) -> SResult<User>;
    /// Inserts `u` (id ignored) and returns the new id. Conflict when login or email exists.
    fn user_create(&self, u: &User) -> SResult<i64>;
    /// Writes every column of `u` by id (bumps `version`).
    fn user_update(&self, u: &User) -> SResult<()>;
    fn user_set_last_seen(&self, id: i64, at: i64) -> SResult<()>;
    fn user_count(&self) -> SResult<i64>;
    /// Users matching `query` in login, email or name (empty matches all), ordered by login. Returns (page, total).
    fn user_search(&self, query: &str, limit: i64, offset: i64) -> SResult<(Vec<User>, i64)>;
    /// Deletes a user with its memberships, sessions, stars, preferences and team memberships.
    fn user_delete(&self, id: i64) -> SResult<()>;
}
