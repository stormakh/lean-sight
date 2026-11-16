//! Login sessions (table `user_auth_token`) and API keys / service account tokens (table `api_key`).
use super::SResult;

#[derive(Debug, Clone, Default)]
pub struct UserAuthToken {
    pub id: i64,
    pub user_id: i64,
    /// sha256 hex of (unhashed token + secret_key), see `auth::hash_token`.
    pub auth_token: String,
    pub prev_auth_token: String,
    pub user_agent: String,
    pub client_ip: String,
    pub auth_token_seen: bool,
    pub seen_at: i64,
    pub rotated_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
    /// 0 when not revoked.
    pub revoked_at: i64,
}

#[derive(Debug, Clone, Default)]
pub struct ApiKey {
    pub id: i64,
    pub org_id: i64,
    pub name: String,
    /// `util.EncodePassword(secret, checksum)` for `glsa_` tokens.
    pub key: String,
    pub role: String,
    /// Unix seconds, 0 for never.
    pub expires: i64,
    /// The service account user id, 0 for a legacy API key.
    pub service_account_id: i64,
    pub last_used_at: i64,
    pub is_revoked: bool,
    pub created: i64,
    pub updated: i64,
}

pub trait AuthStore {
    /// Inserts a session and returns its id.
    fn token_create(&self, t: &UserAuthToken) -> SResult<i64>;
    /// The session whose `auth_token` or `prev_auth_token` equals `hashed`.
    fn token_by_hash(&self, hashed: &str) -> SResult<UserAuthToken>;
    fn token_by_id(&self, id: i64) -> SResult<UserAuthToken>;
    /// Writes every column by id.
    fn token_update(&self, t: &UserAuthToken) -> SResult<()>;
    fn token_delete(&self, id: i64) -> SResult<()>;
    /// Sessions of a user that are not revoked, newest first.
    fn tokens_of_user(&self, user_id: i64) -> SResult<Vec<UserAuthToken>>;
    fn tokens_delete_for_user(&self, user_id: i64) -> SResult<()>;

    fn api_key_by_hash(&self, hashed: &str) -> SResult<ApiKey>;
    /// Inserts a key and returns its id.
    fn api_key_create(&self, k: &ApiKey) -> SResult<i64>;
    fn api_key_set_last_used(&self, id: i64, at: i64) -> SResult<()>;
}
