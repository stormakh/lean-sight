//! Login page and API, logout, login ping, session rotation (agent A).
//! Upstream: pkg/api/login.go (LoginView, LoginPost, LoginAPIPing, Logout), pkg/api/user_token.go
//! (RotateUserAuthToken), pkg/services/authn/authn.go (HandleLoginResponse, WriteSessionCookie), the password client.
//! Different from Grafana: no login attempt rate limiting; no OAuth/SAML/LDAP.
use crate::auth::{self as authn, URGENT_ROTATE_SECS};
use crate::ctx::{Access, Ctx, Routes};
use crate::error::ApiError;
use crate::util::now;
use http::{Cookie, Response};
use json::Json;

pub fn routes(r: &mut Routes) {
    r.get("/login", Access::Optional, login_view);
    r.post("/login", Access::Public, login_post);
    r.get("/logout", Access::Optional, logout);
    r.get("/api/login/ping", Access::Optional, login_ping);
    r.post("/api/user/auth-tokens/rotate", Access::Public, rotate_token);
    r.get("/user/auth-tokens/rotate", Access::Public, rotate_token_redirect);
}

/// `grafana_session_expiry`: the time the client should rotate (token.NextRotation, 5 s leeway).
fn expiry_cookie(c: &Ctx<'_>, rotated_at: i64) -> Cookie {
    let cfg = &c.app.cfg;
    let mut ck = authn::session_cookie(c.app, "");
    ck.name = "grafana_session_expiry".to_string();
    ck.value = (rotated_at + cfg.token_rotation_minutes * 60 - 5).to_string();
    ck.http_only = false;
    ck
}

fn write_session(c: &Ctx<'_>, resp: &mut Response, token: &str, rotated_at: i64) {
    resp.set_cookie(&authn::session_cookie(c.app, token));
    resp.set_cookie(&expiry_cookie(c, rotated_at));
}

fn delete_session(c: &Ctx<'_>, resp: &mut Response) {
    let mut a = authn::clear_session_cookie(c.app);
    a.max_age = Some(-1);
    resp.set_cookie(&a);
    let mut b = expiry_cookie(c, 0);
    b.value = String::new();
    b.max_age = Some(-1);
    resp.set_cookie(&b);
}

/// LoginView: signed-in users go home; everyone else gets the index page (the frontend shows the login form).
fn login_view(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    if c.user.is_signed_in() {
        return Ok(Response::redirect(302, &format!("{}/", c.app.cfg.app_sub_url)));
    }
    crate::api::boot::index(c)
}

/// LoginPost with the password client: `{"user","password"}`.
fn login_post(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let bad = || ApiError::raw(400, Json::obj().with("statusCode", 400).with("messageId", "form-auth.invalid").with("message", "bad login data"));
    let form = match c.req.json() {
        Ok(j) => j,
        Err(_) => return Err(bad()),
    };
    let login = form.str_of("user").to_string();
    let password = form.str_of("password").to_string();
    if login.is_empty() || password.is_empty() {
        return Err(bad());
    }
    let failed = || ApiError::raw(401, Json::obj().with("statusCode", 401).with("messageId", "password-auth.failed").with("message", "Invalid username or password"));
    if c.app.cfg.disable_login_form {
        return Err(failed());
    }
    let st = &c.app.store;
    let user = match st.user_by_login_or_email(&login) {
        Ok(u) => u,
        Err(_) => return Err(failed()),
    };
    if user.is_service_account || user.password.is_empty() || !crypto::ct_eq(crypto::encode_password(&password, &user.salt).as_bytes(), user.password.as_bytes()) {
        return Err(failed());
    }
    if user.is_disabled {
        return Err(ApiError::raw(401, Json::obj().with("statusCode", 401).with("messageId", "user.disabled").with("message", "User is disabled")));
    }
    let ua = c.req.header("user-agent").unwrap_or("").to_string();
    let ip = c.req.remote_ip.clone();
    let (token, row) = authn::create_session(c.app, user.id, &ip, &ua)?;
    let _ = st.user_set_last_seen(user.id, now());
    let body = Json::obj().with("message", "Logged in").with("redirectUrl", format!("{}/", c.app.cfg.app_sub_url));
    let mut resp = Response::json(200, &body);
    write_session(c, &mut resp, &token, row.rotated_at);
    Ok(resp)
}

/// Logout: revokes the session and goes to the login page.
fn logout(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    if c.user.session_id > 0 {
        let _ = c.app.store.token_delete(c.user.session_id);
    }
    let mut resp = Response::redirect(302, &format!("{}/login", c.app.cfg.app_sub_url));
    delete_session(c, &mut resp);
    Ok(resp)
}

fn login_ping(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    if c.user.is_signed_in() || c.user.is_anonymous {
        return Ok(Response::json(200, &Json::obj().with("message", "Logged in")));
    }
    Err(ApiError::unauthorized())
}

enum Rotated {
    Ok(Response),
    Invalid,
    NotFound,
}

/// RotateToken: a new token for the presented one (skipped when rotated less than 5 s ago).
fn do_rotate(c: &Ctx<'_>) -> Rotated {
    let cfg = &c.app.cfg;
    let raw = c.req.cookie(&cfg.login_cookie_name).unwrap_or_default();
    if raw.is_empty() {
        return Rotated::Invalid;
    }
    let raw = http::url_decode(&raw);
    let st = &c.app.store;
    let hashed = authn::hash_token(&cfg.secret_key, &raw);
    let tok = match st.token_by_hash(&hashed) {
        Ok(t) => t,
        Err(_) => return Rotated::NotFound,
    };
    let t = now();
    if tok.revoked_at > 0 || tok.created_at <= t - cfg.login_max_lifetime_secs || tok.rotated_at <= t - cfg.login_max_inactive_secs {
        return Rotated::NotFound;
    }
    let _ = URGENT_ROTATE_SECS;
    let mut resp = Response::json(200, &Json::obj());
    if tok.rotated_at + 5 > t {
        return Rotated::Ok(resp);
    }
    let new_token = crypto::random_hex(16);
    let ua: String = c.req.header("user-agent").unwrap_or("").chars().take(200).collect();
    // only while the token we read is still current: of concurrent rotations exactly one wins; the others answer
    // as a skipped rotation (200, no new cookie), and the client keeps the cookie the winner sets
    match st.token_rotate(tok.id, &tok.auth_token, &authn::hash_token(&cfg.secret_key, &new_token), &ua, &c.req.remote_ip, t) {
        Ok(true) => write_session(c, &mut resp, &new_token, t),
        Ok(false) => {}
        Err(_) => return Rotated::NotFound,
    }
    Rotated::Ok(resp)
}

/// `POST /api/user/auth-tokens/rotate`.
fn rotate_token(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    match do_rotate(c) {
        Rotated::Ok(r) => Ok(r),
        Rotated::Invalid => {
            let mut r = ApiError::unauthorized().to_response();
            delete_session(c, &mut r);
            Ok(r)
        }
        Rotated::NotFound => Err(ApiError::unauthorized()),
    }
}

/// `GET /user/auth-tokens/rotate?redirectTo=...`: rotates, then redirects (to the login page on failure).
fn rotate_token_redirect(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let sub = c.app.cfg.app_sub_url.clone();
    let mut to = c.req.query_get("redirectTo").unwrap_or_default();
    if to.is_empty() || !to.starts_with('/') || to.starts_with("//") || to.contains("://") {
        to = format!("{sub}/");
    }
    match do_rotate(c) {
        Rotated::Ok(r) => {
            let mut red = Response::redirect(302, &to);
            for (k, v) in r.headers.iter() {
                if k.eq_ignore_ascii_case("set-cookie") {
                    red.headers.push((k.clone(), v.clone()));
                }
            }
            Ok(red)
        }
        _ => {
            let mut red = Response::redirect(302, &format!("{sub}/login"));
            delete_session(c, &mut red);
            Ok(red)
        }
    }
}
