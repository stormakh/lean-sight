//! Request authentication, as upstream pkg/services/authn does it: `Authorization: Bearer` service account tokens,
//! `Authorization: Basic` (user password, or `api_key:<token>`), the `grafana_session` cookie, then anonymous.
//! Also the session helpers the login API needs: `create_session`, `session_cookie`, `hash_token`.
use crate::app::App;
use crate::ctx::{AuthMethod, Role, SignedInUser};
use crate::error::ApiError;
use crate::store::{StoreError, User, UserAuthToken, UserInOrg};
use crate::util::now;
use http::{Cookie, Request};
use json::Json;
use std::collections::HashMap;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// Session tokens that were never seen by the client must rotate after this long (usertoken.UrgentRotateTime).
pub const URGENT_ROTATE_SECS: i64 = 60;

/// sha256 hex of token + secret key (authimpl hashToken).
pub fn hash_token(secret_key: &str, token: &str) -> String {
    let mut h = crypto::Sha256::new();
    h.update(token.as_bytes());
    h.update(secret_key.as_bytes());
    crypto::hex(&h.finish())
}

/// The error body the auth middleware writes for an errutil error (writeErrOrFallback): it includes "extra".
fn mw_err(status: u16, message_id: &str, message: &str) -> ApiError {
    let msg = if message.is_empty() { http::reason(status) } else { message };
    ApiError::raw(status, Json::obj().with("extra", Json::Null).with("message", msg).with("messageId", message_id).with("statusCode", status as i64).with("traceID", ""))
}

/// Cache of recent successful basic-auth checks, so a client that sends basic auth on every request pays the
/// 10000-round PBKDF2 once per minute, not per request. Keyed by a MAC of the credentials (with a random key made at
/// startup; no password is kept); a hit is valid only while the user's stored password hash is unchanged.
/// Concurrent misses for the same credentials run one PBKDF2: the others wait for its result (when an entry expires
/// under load, 16 clients would otherwise each pay the full hash at the same moment).
pub struct BasicCache {
    map: Mutex<HashMap<[u8; 32], (i64, String, Instant)>>,
    /// Keys whose password check runs now.
    checking: Mutex<Vec<[u8; 32]>>,
    checked: Condvar,
    key: crypto::HmacSha256,
}
const BASIC_CACHE_TTL: Duration = Duration::from_secs(60);
const BASIC_CACHE_MAX: usize = 1024;
impl BasicCache {
    pub fn new() -> BasicCache {
        let mut k = [0u8; 32];
        crypto::random_bytes(&mut k);
        BasicCache { map: Mutex::new(HashMap::new()), checking: Mutex::new(Vec::new()), checked: Condvar::new(), key: crypto::HmacSha256::new(&k) }
    }
    fn cache_key(&self, login: &str, password: &str) -> [u8; 32] {
        let mut s = self.key.start();
        s.update(login.as_bytes());
        s.update(&[0]);
        s.update(password.as_bytes());
        s.finish()
    }
    fn get(&self, k: &[u8; 32]) -> Option<(i64, String)> {
        let m = match self.map.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        match m.get(k) {
            Some(e) if e.2.elapsed() < BASIC_CACHE_TTL => Some((e.0, e.1.clone())),
            _ => None,
        }
    }
    fn put(&self, k: [u8; 32], user_id: i64, pw_hash: String) {
        let mut m = match self.map.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if m.len() >= BASIC_CACHE_MAX {
            m.clear();
        }
        m.insert(k, (user_id, pw_hash, Instant::now()));
    }
    /// True when the caller must check the password and then call `end`; false after waiting for another
    /// thread's check of the same key to finish.
    fn begin(&self, k: &[u8; 32]) -> bool {
        let mut c = match self.checking.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if !c.contains(k) {
            c.push(*k);
            return true;
        }
        while c.contains(k) {
            c = match self.checked.wait(c) {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
        }
        false
    }
    fn end(&self, k: &[u8; 32]) {
        let mut c = match self.checking.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        c.retain(|x| x != k);
        self.checked.notify_all();
    }
}
impl Default for BasicCache {
    fn default() -> Self {
        BasicCache::new()
    }
}

/// Result of authenticating a request.
pub enum AuthResult {
    /// A user, service account, or anonymous identity.
    Ok(SignedInUser),
    /// No credentials and anonymous access is off.
    NoCredentials,
    /// Credentials were sent but are not valid. The error is what a protected API route answers.
    Failed(ApiError),
}

/// The org a request asks for: `orgId` query param, then the `X-Grafana-Org-Id` header. 0 when none.
fn requested_org(req: &Request) -> i64 {
    if let Some(v) = req.query_get("orgId") {
        if let Ok(n) = v.parse::<i64>() {
            return n;
        }
    }
    match req.header("x-grafana-org-id") {
        Some(v) => v.parse::<i64>().unwrap_or(0),
        None => 0,
    }
}

/// Builds the identity of `u` in its current org (or `org_id` when it is a member there).
pub fn identity_for_user(app: &App, u: &User, org_id: i64, method: AuthMethod) -> Result<SignedInUser, ApiError> {
    let st = &app.store;
    let mut org = u.org_id;
    let mut role = Role::None;
    if org_id > 0 && org_id != u.org_id {
        if let Ok(r) = st.org_user_role(org_id, u.id) {
            org = org_id;
            role = r;
        }
    }
    if role == Role::None {
        match st.org_user_role(org, u.id) {
            Ok(r) => role = r,
            Err(StoreError::NotFound) => {}
            Err(_) => return Err(ApiError::internal("")),
        }
    }
    let org_name = match st.org_by_id(org) {
        Ok(o) => o.name,
        Err(_) => String::new(),
    };
    let uo = UserInOrg {
        id: u.id,
        uid: u.uid.clone(),
        login: u.login.clone(),
        email: u.email.clone(),
        name: u.name.clone(),
        password: String::new(),
        theme: u.theme.clone(),
        help_flags1: u.help_flags1,
        is_admin: u.is_admin,
        is_disabled: u.is_disabled,
        is_service_account: u.is_service_account,
        last_seen_at: u.last_seen_at,
        org_id: org,
        role,
        org_name,
    };
    Ok(identity_of(uo, method))
}

/// The identity of a user read with `Store::user_in_org` (the same result as `identity_for_user`, in one query).
fn identity_of(u: UserInOrg, method: AuthMethod) -> SignedInUser {
    SignedInUser {
        user_id: u.id,
        uid: u.uid,
        org_id: u.org_id,
        org_name: u.org_name,
        org_role: u.role,
        login: u.login,
        name: u.name,
        email: u.email,
        theme: u.theme,
        help_flags1: u.help_flags1,
        is_grafana_admin: u.is_admin,
        is_anonymous: false,
        is_service_account: u.is_service_account,
        is_disabled: u.is_disabled,
        last_seen_at: u.last_seen_at,
        auth: method,
        session_id: 0,
        api_key_id: 0,
    }
}

/// Authenticates a request. Order as upstream: token, basic, session cookie, anonymous.
pub fn authenticate(app: &App, req: &Request) -> AuthResult {
    let org_req = requested_org(req);
    if let Some(h) = req.header("authorization") {
        let h = h.trim();
        if let Some(tok) = h.strip_prefix("Bearer ").or_else(|| h.strip_prefix("bearer ")) {
            return token_auth(app, tok.trim(), org_req);
        }
        if let Some(b64) = h.strip_prefix("Basic ").or_else(|| h.strip_prefix("basic ")) {
            if !app.cfg.basic_auth_enabled {
                return AuthResult::NoCredentials;
            }
            return basic_auth(app, b64.trim(), org_req);
        }
    }
    let cookie_name = &app.cfg.login_cookie_name;
    if !cookie_name.is_empty() {
        if let Some(raw) = req.cookie(cookie_name) {
            if !raw.is_empty() {
                return session_auth(app, &raw, org_req, req);
            }
        }
    }
    anonymous(app)
}

/// The anonymous identity, when enabled.
pub fn anonymous(app: &App) -> AuthResult {
    if !app.cfg.anonymous_enabled {
        return AuthResult::NoCredentials;
    }
    match app.store.org_by_name(&app.cfg.anonymous_org_name) {
        Ok(org) => AuthResult::Ok(SignedInUser {
            org_id: org.id,
            org_name: org.name,
            org_role: app.cfg.anonymous_org_role,
            is_anonymous: true,
            auth: AuthMethod::Anonymous,
            ..SignedInUser::default()
        }),
        Err(_) => AuthResult::NoCredentials,
    }
}

fn basic_auth(app: &App, b64: &str, org_req: i64) -> AuthResult {
    let bad = || AuthResult::Failed(mw_err(400, "basic-auth.invalid-header", "Invalid Basic Auth Header"));
    let raw = match crypto::base64_decode(b64) {
        Some(r) => r,
        None => return bad(),
    };
    let text = match String::from_utf8(raw) {
        Ok(t) => t,
        Err(_) => return bad(),
    };
    let (login, password) = match text.split_once(':') {
        Some(p) => p,
        None => return bad(),
    };
    if login == "api_key" {
        return token_auth(app, password, org_req);
    }
    let failed = || AuthResult::Failed(mw_err(401, "password-auth.failed", "Invalid username or password"));
    if password.is_empty() {
        return failed();
    }
    let ck = app.basic_cache.cache_key(login, password);
    if let Some(r) = cached_basic(app, &ck, org_req) {
        return r;
    }
    if !app.basic_cache.begin(&ck) {
        // another request checked the same credentials just now
        if let Some(r) = cached_basic(app, &ck, org_req) {
            return r;
        }
        return check_password(app, login, password, ck, org_req);
    }
    let r = check_password(app, login, password, ck, org_req);
    app.basic_cache.end(&ck);
    r
}

/// A basic-auth cache hit, when the user's password hash is still the one that was checked.
fn cached_basic(app: &App, ck: &[u8; 32], org_req: i64) -> Option<AuthResult> {
    let (uid, pw_hash) = app.basic_cache.get(ck)?;
    match app.store.user_in_org(uid, org_req) {
        Ok(uo) if uo.password == pw_hash => Some(finish_user_in_org(app, uo, AuthMethod::Basic)),
        _ => None,
    }
}

fn check_password(app: &App, login: &str, password: &str, ck: [u8; 32], org_req: i64) -> AuthResult {
    let failed = || AuthResult::Failed(mw_err(401, "password-auth.failed", "Invalid username or password"));
    let user = match app.store.user_by_login_or_email(login) {
        Ok(u) => u,
        Err(_) => return failed(),
    };
    if user.is_service_account || user.password.is_empty() {
        return failed();
    }
    let hash = crypto::encode_password(password, &user.salt);
    if !crypto::ct_eq(hash.as_bytes(), user.password.as_bytes()) {
        return failed();
    }
    app.basic_cache.put(ck, user.id, user.password.clone());
    finish_user(app, user, org_req, AuthMethod::Basic)
}

fn finish_user(app: &App, user: User, org_req: i64, method: AuthMethod) -> AuthResult {
    if user.is_disabled {
        return AuthResult::Failed(mw_err(401, "identity.disabled", ""));
    }
    match identity_for_user(app, &user, org_req, method) {
        Ok(id) => {
            touch_last_seen(app, &id);
            AuthResult::Ok(id)
        }
        Err(e) => AuthResult::Failed(e),
    }
}

fn finish_user_in_org(app: &App, uo: UserInOrg, method: AuthMethod) -> AuthResult {
    if uo.is_disabled {
        return AuthResult::Failed(mw_err(401, "identity.disabled", ""));
    }
    let id = identity_of(uo, method);
    touch_last_seen(app, &id);
    AuthResult::Ok(id)
}

/// Updates `last_seen_at` at most every 5 minutes, as Grafana's user sync does.
fn touch_last_seen(app: &App, id: &SignedInUser) {
    if id.user_id > 0 && now() - id.last_seen_at > 300 {
        let _ = app.store.user_set_last_seen(id.user_id, now());
    }
}

/// Splits a `glsa_<secret>_<checksum>` token and checks its CRC (satokengen.Decode).
fn decode_sa_token(tok: &str) -> Option<(String, String)> {
    if !tok.starts_with("gl") {
        return None;
    }
    let parts: Vec<&str> = tok.split('_').collect();
    if parts.len() != 3 {
        return None;
    }
    let key = format!("{}_{}", parts[0], parts[1]);
    let c = crypto::crc32(key.as_bytes()).to_le_bytes();
    if crypto::hex(&c) != parts[2] {
        return None;
    }
    Some((parts[1].to_string(), parts[2].to_string()))
}

fn token_auth(app: &App, tok: &str, org_req: i64) -> AuthResult {
    let invalid = || AuthResult::Failed(mw_err(401, "api-key.invalid", "Invalid API key"));
    let (secret, checksum) = match decode_sa_token(tok) {
        Some(p) => p,
        None => return invalid(),
    };
    let hashed = crypto::encode_password(&secret, &checksum);
    let key = match app.store.api_key_by_hash(&hashed) {
        Ok(k) => k,
        Err(_) => return invalid(),
    };
    let t = now();
    if key.expires > 0 && key.expires <= t {
        return AuthResult::Failed(mw_err(401, "api-key.expired", "Expired API key"));
    }
    if key.is_revoked {
        return AuthResult::Failed(mw_err(401, "api-key.revoked", "Revoked API key"));
    }
    if org_req > 0 && org_req != key.org_id {
        return AuthResult::Failed(mw_err(401, "api-key.organization-mismatch", "API key does not belong to the requested organization"));
    }
    // Grafana updates last_used_at on every request in a goroutine; once a minute is enough here.
    if t - key.last_used_at > 60 {
        let _ = app.store.api_key_set_last_used(key.id, t);
    }
    if key.service_account_id > 0 {
        let uo = match app.store.user_in_org(key.service_account_id, key.org_id) {
            Ok(u) => u,
            Err(_) => return invalid(),
        };
        if uo.is_disabled {
            return AuthResult::Failed(mw_err(401, "identity.disabled", ""));
        }
        let mut id = identity_of(uo, AuthMethod::ApiKey);
        id.api_key_id = key.id;
        return AuthResult::Ok(id);
    }
    // a legacy API key without a service account
    let org_name = app.store.org_by_id(key.org_id).map(|o| o.name).unwrap_or_default();
    AuthResult::Ok(SignedInUser {
        org_id: key.org_id,
        org_name,
        org_role: Role::parse(&key.role).unwrap_or(Role::Viewer),
        login: key.name.clone(),
        name: key.name.clone(),
        is_service_account: true,
        auth: AuthMethod::ApiKey,
        api_key_id: key.id,
        ..SignedInUser::default()
    })
}

fn session_auth(app: &App, raw: &str, org_req: i64, req: &Request) -> AuthResult {
    let cfg = &app.cfg;
    let not_found = || AuthResult::Failed(ApiError::with_trace(401, "Unauthorized"));
    let hashed = hash_token(&cfg.secret_key, raw);
    let mut tok = match app.store.token_by_hash(&hashed) {
        Ok(t) => t,
        Err(_) => return not_found(),
    };
    if tok.revoked_at > 0 {
        return AuthResult::Failed(ApiError::raw(
            401,
            Json::obj().with("message", "Token revoked").with("error", Json::obj().with("id", "ERR_TOKEN_REVOKED").with("maxConcurrentSessions", cfg.get_int("auth", "max_concurrent_sessions", 0))),
        ));
    }
    let t = now();
    if tok.created_at <= t - cfg.login_max_lifetime_secs || tok.rotated_at <= t - cfg.login_max_inactive_secs {
        return not_found();
    }
    // the client presents the current token for the first time: mark it seen
    if tok.auth_token == hashed && !tok.auth_token_seen {
        tok.auth_token_seen = true;
        tok.seen_at = t;
        let _ = app.store.token_update(&tok);
    }
    let rotate_after = if tok.auth_token_seen { cfg.token_rotation_minutes * 60 } else { URGENT_ROTATE_SECS };
    if tok.rotated_at < t - rotate_after {
        return AuthResult::Failed(mw_err(401, "session.token.rotate", ""));
    }
    let _ = req;
    let uo = match app.store.user_in_org(tok.user_id, org_req) {
        Ok(u) => u,
        Err(_) => return not_found(),
    };
    match finish_user_in_org(app, uo, AuthMethod::Session) {
        AuthResult::Ok(mut id) => {
            id.session_id = tok.id;
            AuthResult::Ok(id)
        }
        other => other,
    }
}

/// Creates a login session for a user. Returns the unhashed token (the cookie value) and the stored row.
pub fn create_session(app: &App, user_id: i64, client_ip: &str, user_agent: &str) -> Result<(String, UserAuthToken), ApiError> {
    let token = crypto::random_hex(16);
    let hashed = hash_token(&app.cfg.secret_key, &token);
    let t = now();
    let mut ua = user_agent.to_string();
    if ua.len() > 200 {
        let mut cut = 200;
        while !ua.is_char_boundary(cut) {
            cut -= 1;
        }
        ua.truncate(cut);
    }
    let mut row = UserAuthToken {
        id: 0,
        user_id,
        auth_token: hashed.clone(),
        prev_auth_token: hashed,
        user_agent: ua,
        client_ip: client_ip.to_string(),
        auth_token_seen: false,
        seen_at: 0,
        rotated_at: t,
        created_at: t,
        updated_at: t,
        revoked_at: 0,
    };
    row.id = app.store.token_create(&row)?;
    Ok((token, row))
}

/// The `grafana_session` cookie for an unhashed token, as Grafana's cookies.WriteSessionCookie writes it.
pub fn session_cookie(app: &App, token: &str) -> Cookie {
    let cfg = &app.cfg;
    let max_age = if cfg.login_max_lifetime_secs > 0 { cfg.login_max_lifetime_secs } else { 30 * 86400 };
    Cookie {
        name: cfg.login_cookie_name.clone(),
        value: http::url_encode(token),
        path: if cfg.app_sub_url.is_empty() { "/".to_string() } else { cfg.app_sub_url.clone() },
        max_age: Some(max_age),
        http_only: true,
        secure: cfg.cookie_secure,
        same_site: samesite(&cfg.cookie_samesite),
        ..Cookie::default()
    }
}

/// A cookie that deletes the session cookie.
pub fn clear_session_cookie(app: &App) -> Cookie {
    let mut c = session_cookie(app, "");
    c.max_age = Some(-1);
    c
}

fn samesite(s: &str) -> String {
    match s {
        "lax" => "Lax".to_string(),
        "strict" => "Strict".to_string(),
        "none" => "None".to_string(),
        _ => String::new(),
    }
}
