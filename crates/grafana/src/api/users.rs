//! Current user, users, orgs, preferences, stars, teams (read only) (agent A).
//! Upstream: pkg/api/user.go, org.go, org_users.go, admin_users.go, user_token.go, preferences.go,
//! pkg/services/preference/prefapi (k8s handler), pkg/registry/apis/preferences (merged), pkg/services/star/api,
//! pkg/registry/apis/collections (stars), pkg/api/team.go (search, read only).
//!
//! Preferences keep the Kubernetes `PreferencesSpec` as given (JSON text in `preferences.json_data`), so a field set
//! to "" stays distinguishable from an unset one, as upstream's storage does; the columns mirror the values.
use crate::access;
use crate::api::boot::{THEMES, gravatar_url, namespace};
use crate::app::App;
use crate::ctx::{Access, Ctx, Role, Routes, SignedInUser};
use crate::error::ApiError;
use crate::store::{Preferences, StoreError, User};
use crate::util::{now, rfc3339};
use http::Response;
use json::Json;

pub fn routes(r: &mut Routes) {
    // signed in user (reqSignedInNoAnonymous)
    r.get("/api/user", Access::NoAnonymous, get_signed_in_user);
    r.put("/api/user", Access::NoAnonymous, update_signed_in_user);
    r.post("/api/user/using/:id", Access::NoAnonymous, user_set_using_org);
    r.get("/api/user/orgs", Access::NoAnonymous, get_signed_in_user_orgs);
    r.get("/api/user/teams", Access::NoAnonymous, get_signed_in_user_teams);
    r.get("/api/user/stars", Access::NoAnonymous, get_stars);
    r.post("/api/user/stars/dashboard/uid/:uid", Access::NoAnonymous, star_dashboard);
    r.delete("/api/user/stars/dashboard/uid/:uid", Access::NoAnonymous, unstar_dashboard);
    r.put("/api/user/password", Access::NoAnonymous, change_password);
    r.get("/api/user/quotas", Access::NoAnonymous, empty_list);
    r.get("/api/user/preferences", Access::NoAnonymous, get_user_prefs);
    r.put("/api/user/preferences", Access::NoAnonymous, put_user_prefs);
    r.patch("/api/user/preferences", Access::NoAnonymous, patch_user_prefs);
    r.get("/api/user/auth-tokens", Access::NoAnonymous, get_user_auth_tokens);
    r.post("/api/user/revoke-auth-token", Access::NoAnonymous, revoke_user_auth_token);
    r.put("/api/user/helpflags/:id", Access::NoAnonymous, set_help_flag);
    r.get("/api/user/helpflags/clear", Access::NoAnonymous, clear_help_flags);
    r.get("/profile/switch-org/:id", Access::NoAnonymous, switch_org_and_redirect);

    // users (server admin reads)
    r.get("/api/users", Access::SignedIn, search_users);
    r.get("/api/users/search", Access::SignedIn, search_users_paged);
    r.get("/api/users/lookup", Access::SignedIn, get_user_by_login_or_email);
    r.get("/api/users/:id", Access::SignedIn, get_user_by_id);
    r.get("/api/users/:id/teams", Access::SignedIn, get_user_teams);
    r.get("/api/users/:id/orgs", Access::SignedIn, get_user_orgs);
    r.put("/api/users/:id", Access::SignedIn, update_user);
    r.post("/api/users/:id/using/:orgId", Access::SignedIn, update_user_active_org);

    // current org
    r.get("/api/org", Access::SignedIn, get_current_org);
    r.put("/api/org", Access::SignedIn, update_current_org);
    r.put("/api/org/address", Access::SignedIn, update_current_org_address);
    r.get("/api/org/quotas", Access::SignedIn, empty_list);
    r.get("/api/org/users", Access::SignedIn, get_org_users_current);
    r.get("/api/org/users/search", Access::SignedIn, search_org_users_current);
    r.get("/api/org/users/lookup", Access::SignedIn, org_users_lookup);
    r.post("/api/org/users", Access::SignedIn, add_org_user_current);
    r.patch("/api/org/users/:userId", Access::SignedIn, update_org_user_current);
    r.delete("/api/org/users/:userId", Access::SignedIn, remove_org_user_current);
    r.get("/api/org/invites", Access::SignedIn, get_invites);
    r.get("/api/org/preferences", Access::SignedIn, get_org_prefs);
    r.put("/api/org/preferences", Access::SignedIn, put_org_prefs);
    r.patch("/api/org/preferences", Access::SignedIn, patch_org_prefs);

    // orgs (server admin)
    r.get("/api/orgs", Access::SignedIn, search_orgs);
    r.post("/api/orgs", Access::SignedIn, create_org);
    r.get("/api/orgs/name/:name", Access::SignedIn, get_org_by_name);
    r.get("/api/orgs/:orgId", Access::SignedIn, get_org_by_id);
    r.put("/api/orgs/:orgId", Access::SignedIn, update_org);
    r.put("/api/orgs/:orgId/address", Access::SignedIn, update_org_address);
    r.delete("/api/orgs/:orgId", Access::SignedIn, delete_org);
    r.get("/api/orgs/:orgId/users", Access::SignedIn, get_org_users);
    r.get("/api/orgs/:orgId/users/search", Access::SignedIn, search_org_users);
    r.post("/api/orgs/:orgId/users", Access::SignedIn, add_org_user);
    r.patch("/api/orgs/:orgId/users/:userId", Access::SignedIn, update_org_user);
    r.delete("/api/orgs/:orgId/users/:userId", Access::SignedIn, remove_org_user);
    r.get("/api/orgs/:orgId/quotas", Access::SignedIn, empty_list);

    // administering users
    r.post("/api/admin/users", Access::SignedIn, admin_create_user);
    r.put("/api/admin/users/:id/password", Access::SignedIn, admin_update_password);
    r.put("/api/admin/users/:id/permissions", Access::GrafanaAdmin, admin_update_permissions);
    r.delete("/api/admin/users/:id", Access::SignedIn, admin_delete_user);
    r.post("/api/admin/users/:id/disable", Access::SignedIn, admin_disable_user);
    r.post("/api/admin/users/:id/enable", Access::SignedIn, admin_enable_user);
    r.post("/api/admin/users/:id/logout", Access::SignedIn, admin_logout_user);
    r.get("/api/admin/users/:id/auth-tokens", Access::SignedIn, admin_get_auth_tokens);
    r.post("/api/admin/users/:id/revoke-auth-token", Access::SignedIn, admin_revoke_auth_token);
    r.get("/api/admin/users/:id/quotas", Access::SignedIn, empty_list);

    // teams (read only)
    r.get("/api/teams/search", Access::SignedIn, search_teams);
    r.get("/api/teams/:teamId", Access::SignedIn, get_team);
    r.get("/api/teams/:teamId/members", Access::SignedIn, team_members_empty);
    r.get("/api/teams/:teamId/preferences", Access::SignedIn, get_team_prefs);

    // Kubernetes-style preferences and stars
    r.get("/apis/preferences.grafana.app/v1/namespaces/:ns/preferences/merged", Access::SignedIn, get_merged_prefs);
    r.get("/apis/collections.grafana.app/v1alpha1/namespaces/:ns/stars", Access::SignedIn, list_stars);
    r.get("/apis/collections.grafana.app/v1alpha1/namespaces/:ns/stars/:name", Access::SignedIn, get_stars_object);
    r.put("/apis/collections.grafana.app/v1alpha1/namespaces/:ns/stars/:name/update/*", Access::SignedIn, update_stars);
    r.delete("/apis/collections.grafana.app/v1alpha1/namespaces/:ns/stars/:name/update/*", Access::SignedIn, update_stars);

    r.get("/avatar/:hash", Access::SignedIn, avatar);
}

// ---------------------------------------------------------------- helpers

/// `authorize(ac.EvalPermission(action))`: 403 with the access-denied body unless the basic role grants it.
fn authorize(c: &Ctx<'_>, action: &str) -> Result<(), ApiError> {
    if access::has(&c.user, action) { Ok(()) } else { Err(deny(action)) }
}
/// `authorizeInOrg(UseGlobalOrg, ...)`: the permission in the global org, which only server admins have.
fn authorize_global(c: &Ctx<'_>, action: &str) -> Result<(), ApiError> {
    if c.user.is_grafana_admin && access::has(&c.user, action) { Ok(()) } else { Err(deny(action)) }
}
/// The accesscontrol middleware's deny body (keys sorted as Go writes a map).
pub fn deny(action: &str) -> ApiError {
    let id = crypto::random_string(10, b"0123456789");
    ApiError::raw(
        403,
        Json::obj()
            .with("accessErrorId", format!("ACE{id}"))
            .with("message", format!("You'll need additional permissions to perform this action. Permissions needed: {action}"))
            .with("title", "Access denied"),
    )
}
fn success(msg: &str) -> Response {
    Response::json(200, &Json::obj().with("message", msg))
}
fn empty_list(_c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    Ok(Response::json_text(200, "[]".to_string()))
}

/// util.GetAgeString.
pub fn age_string(t: i64) -> String {
    if t <= 0 {
        return "?".to_string();
    }
    let minutes = ((now() - t) as f64) / 60.0;
    let years = (minutes / 525600.0).floor() as i64;
    let months = (minutes / 43800.0).floor() as i64;
    let days = (minutes / 1440.0).floor() as i64;
    let hours = (minutes / 60.0).floor() as i64;
    let unit = |n: i64, one: &str, many: &str| -> String { format!("{} {}", n, if n == 1 { one } else { many }) };
    if years > 0 {
        return unit(years, "year", "years");
    }
    if months > 0 {
        return unit(months, "month", "months");
    }
    if days > 0 {
        return unit(days, "day", "days");
    }
    if hours > 0 {
        return unit(hours, "hour", "hours");
    }
    if minutes as i64 > 0 {
        return unit(minutes as i64, "minute", "minutes");
    }
    "< 1 minute".to_string()
}

/// Go's zero time for a never-seen user.
fn time_or_zero(t: i64) -> String {
    if t <= 0 { "0001-01-01T00:00:00Z".to_string() } else { rfc3339(t) }
}

/// The user by `:id` (a number or a uid, as upstream's userUIDResolver allows).
fn user_param(c: &Ctx<'_>, name: &str) -> Result<User, ApiError> {
    let raw = c.param(name);
    let r = match raw.parse::<i64>() {
        Ok(id) => c.app.store.user_by_id(id),
        Err(_) => c.app.store.user_by_uid(raw),
    };
    match r {
        Ok(u) => Ok(u),
        Err(StoreError::NotFound) => Err(ApiError::new(404, "user not found")),
        Err(_) => Err(ApiError::with_trace(500, "Failed to get user")),
    }
}

fn user_profile(app: &App, u: &User, org_id: i64) -> Json {
    let mut j = Json::obj().with("id", u.id).with("uid", u.uid.as_str()).with("email", u.email.as_str()).with("name", u.name.as_str()).with("login", u.login.as_str()).with("theme", u.theme.as_str());
    if org_id != 0 {
        j.set("orgId", org_id);
    }
    j.with("isGrafanaAdmin", u.is_admin)
        .with("isDisabled", u.is_disabled)
        .with("isExternal", false)
        .with("isExternallySynced", false)
        .with("isGrafanaAdminExternallySynced", false)
        .with("authLabels", Json::arr())
        .with("updatedAt", rfc3339(u.updated))
        .with("createdAt", rfc3339(u.created))
        .with("avatarUrl", gravatar_url(&app.cfg, &u.email))
        .with("isProvisioned", u.is_provisioned)
}

fn read_body(c: &Ctx<'_>) -> Result<Json, ApiError> {
    let j = c.body_json()?;
    if !matches!(j, Json::Obj(_)) {
        return Err(ApiError::bad_request("bad request data"));
    }
    Ok(j)
}

// ---------------------------------------------------------------- signed in user

fn get_signed_in_user(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let u = match c.app.store.user_by_id(c.user.user_id) {
        Ok(u) => u,
        Err(StoreError::NotFound) => return Err(ApiError::new(404, "user not found")),
        Err(_) => return Err(ApiError::with_trace(500, "Failed to get user")),
    };
    Ok(Response::json(200, &user_profile(c.app, &u, c.user.org_id)))
}

/// handleUpdateUser for (name, email, login, theme).
fn apply_user_update(c: &Ctx<'_>, user_id: i64, body: &Json) -> Result<Response, ApiError> {
    let st = &c.app.store;
    let mut u = match st.user_by_id(user_id) {
        Ok(u) => u,
        Err(StoreError::NotFound) => return Err(ApiError::with_trace(404, "user not found")),
        Err(_) => return Err(ApiError::with_trace(500, "Failed to get user")),
    };
    let mut login = body.str_of("login").trim().to_string();
    let mut email = body.str_of("email").trim().to_string();
    if login.is_empty() && email.is_empty() {
        return Err(ApiError::errutil(400, "user.empty-username-and-email", "Need to specify either username or email"));
    }
    if login.is_empty() {
        login = email.clone();
    }
    if email.is_empty() {
        email = u.email.clone();
    }
    if email != u.email && !email.contains('@') {
        return Err(ApiError::with_trace(400, "Invalid email address"));
    }
    if login != u.login {
        if let Ok(other) = st.user_by_login_or_email(&login) {
            if other.id != u.id {
                return Err(ApiError::with_trace(409, "Update would result in user login conflict"));
            }
        }
    }
    u.login = login;
    u.email = email;
    u.name = body.str_of("name").to_string();
    if let Some(t) = body.get("theme").and_then(|v| v.as_str()) {
        u.theme = t.to_string();
    }
    u.updated = now();
    match st.user_update(&u) {
        Ok(()) => Ok(success("User updated")),
        Err(StoreError::Conflict(_)) => Err(ApiError::with_trace(409, "Login or email is already in use")),
        Err(_) => Err(ApiError::with_trace(500, "Failed to update user")),
    }
}

fn update_signed_in_user(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let body = read_body(c)?;
    apply_user_update(c, c.user.user_id, &body)
}

fn set_using_org(c: &Ctx<'_>, user_id: i64, org_id: i64) -> Result<(), ApiError> {
    let st = &c.app.store;
    if st.org_user_role(org_id, user_id).is_err() {
        return Err(ApiError::new(401, "Not a valid organization"));
    }
    let mut u = st.user_by_id(user_id).map_err(|_| ApiError::with_trace(500, "Failed to change active organization"))?;
    u.org_id = org_id;
    st.user_update(&u).map_err(|_| ApiError::with_trace(500, "Failed to change active organization"))
}

fn user_set_using_org(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let org_id = c.param_i64("id")?;
    set_using_org(c, c.user.user_id, org_id)?;
    Ok(success("Active organization changed"))
}

fn switch_org_and_redirect(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let sub = c.app.cfg.app_sub_url.clone();
    let org_id = match c.param("id").parse::<i64>() {
        Ok(v) => v,
        Err(_) => return Ok(Response::redirect(302, &format!("{sub}/"))),
    };
    let _ = set_using_org(c, c.user.user_id, org_id);
    Ok(Response::redirect(302, &format!("{sub}/")))
}

fn orgs_json(c: &Ctx<'_>, user_id: i64) -> Result<Response, ApiError> {
    let list = c.app.store.orgs_of_user(user_id).map_err(|_| ApiError::with_trace(500, "Failed to get user organizations"))?;
    let mut a = Json::arr();
    for (o, r) in list.iter() {
        a.push(Json::obj().with("orgId", o.id).with("name", o.name.as_str()).with("role", r.as_str()));
    }
    Ok(Response::json(200, &a))
}

fn get_signed_in_user_orgs(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    orgs_json(c, c.user.user_id)
}

fn team_json(t: &crate::store::Team) -> Json {
    Json::obj()
        .with("id", t.id)
        .with("uid", t.uid.as_str())
        .with("orgId", t.org_id)
        .with("name", t.name.as_str())
        .with("email", t.email.as_str())
        .with("externalUID", "")
        .with("isProvisioned", false)
        .with("avatarUrl", format!("/avatar/{}", crypto::hex(&crypto::md5(t.email.to_lowercase().as_bytes()))))
        .with("memberCount", t.member_count)
        .with("permission", 0)
        .with("accessControl", Json::Null)
}

fn teams_json(c: &Ctx<'_>, org_id: i64, user_id: i64) -> Result<Response, ApiError> {
    let list = c.app.store.teams_of_user(org_id, user_id).map_err(|_| ApiError::with_trace(500, "Failed to get user teams"))?;
    let mut a = Json::arr();
    for t in list.iter() {
        a.push(team_json(t));
    }
    Ok(Response::json(200, &a))
}

fn get_signed_in_user_teams(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    teams_json(c, c.user.org_id, c.user.user_id)
}

fn change_password(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let body = read_body(c)?;
    let st = &c.app.store;
    let mut u = st.user_by_id(c.user.user_id).map_err(|_| ApiError::with_trace(500, "Failed to change user password"))?;
    let old = body.str_of("oldPassword");
    let new = body.str_of("newPassword");
    if crypto::encode_password(old, &u.salt) != u.password {
        return Err(ApiError::errutil(400, "user.password-missmatch", "Invalid old password"));
    }
    validate_password(c.app, new)?;
    set_password(&mut u, new);
    st.user_update(&u).map_err(|_| ApiError::with_trace(500, "Failed to change user password"))?;
    Ok(success("User password changed"))
}

/// user.ValidatePassword.
fn validate_password(app: &App, pw: &str) -> Result<(), ApiError> {
    if !app.cfg.get_bool("auth.basic", "password_policy") {
        if pw.len() < 4 {
            return Err(ApiError::errutil(400, "password.password-policy-too-short", "New password is too short"));
        }
        return Ok(());
    }
    let ok = pw.chars().count() >= 12
        && pw.chars().any(|ch| ch.is_uppercase())
        && pw.chars().any(|ch| ch.is_lowercase())
        && pw.chars().any(|ch| ch.is_ascii_digit())
        && pw.chars().any(|ch| !ch.is_alphanumeric());
    if !ok {
        return Err(ApiError::errutil(400, "password.password-policy-infringe", "New password doesn't comply with the password policy"));
    }
    Ok(())
}

fn set_password(u: &mut User, pw: &str) {
    u.salt = crypto::random_string(10, b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789");
    u.password = crypto::encode_password(pw, &u.salt);
    u.updated = now();
}

fn tokens_json(c: &Ctx<'_>, user_id: i64, current_session: i64) -> Result<Response, ApiError> {
    let list = c.app.store.tokens_of_user(user_id).map_err(|_| ApiError::with_trace(500, "Failed to get user auth tokens"))?;
    let mut a = Json::arr();
    for t in list.iter() {
        a.push(
            Json::obj()
                .with("id", t.id)
                .with("isActive", t.id == current_session)
                .with("clientIp", t.client_ip.as_str())
                .with("browser", "Other")
                .with("browserVersion", "")
                .with("os", "Other")
                .with("osVersion", "")
                .with("device", "Other")
                .with("createdAt", rfc3339(t.created_at))
                .with("seenAt", rfc3339(if t.seen_at > 0 { t.seen_at } else { t.created_at }))
                .with("authModule", ""),
        );
    }
    Ok(Response::json(200, &a))
}

fn get_user_auth_tokens(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    tokens_json(c, c.user.user_id, c.user.session_id)
}

fn revoke_token(c: &Ctx<'_>, user_id: i64, body: &Json) -> Result<Response, ApiError> {
    let id = body.i64_of("authTokenId");
    let st = &c.app.store;
    let mut t = match st.token_by_id(id) {
        Ok(t) if t.user_id == user_id => t,
        _ => return Err(ApiError::with_trace(404, "User auth token not found")),
    };
    if t.id == c.user.session_id && user_id == c.user.user_id {
        return Err(ApiError::new(400, "Cannot revoke active user auth token"));
    }
    t.revoked_at = now();
    st.token_update(&t).map_err(|_| ApiError::with_trace(500, "Failed to revoke user auth token"))?;
    Ok(success("User auth token revoked"))
}

fn revoke_user_auth_token(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let body = read_body(c)?;
    revoke_token(c, c.user.user_id, &body)
}

fn set_help_flag(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let flag = c.param_i64("id")?;
    let st = &c.app.store;
    let mut u = st.user_by_id(c.user.user_id).map_err(|_| ApiError::with_trace(500, "Unable to update help flag"))?;
    u.help_flags1 |= flag;
    st.user_update(&u).map_err(|_| ApiError::with_trace(500, "Unable to update help flag"))?;
    Ok(Response::json(200, &Json::obj().with("helpFlags1", u.help_flags1).with("message", "Help flag set")))
}

fn clear_help_flags(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let st = &c.app.store;
    let mut u = st.user_by_id(c.user.user_id).map_err(|_| ApiError::with_trace(500, "Unable to update help flag"))?;
    u.help_flags1 = 0;
    st.user_update(&u).map_err(|_| ApiError::with_trace(500, "Unable to update help flag"))?;
    Ok(Response::json(200, &Json::obj().with("helpFlags1", 0).with("message", "Help flag set")))
}

// ---------------------------------------------------------------- stars

fn get_stars(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let list = c.app.store.stars_of_user(c.user.org_id, c.user.user_id)?;
    let mut a = Json::arr();
    for s in list.iter() {
        a.push(Json::Str(s.clone()));
    }
    Ok(Response::json(200, &a))
}

fn star_dashboard(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let uid = c.param("uid").to_string();
    if uid.is_empty() {
        return Err(ApiError::new(400, "Invalid dashboard UID"));
    }
    c.app.store.star_add(c.user.org_id, c.user.user_id, &uid).map_err(|_| ApiError::with_trace(500, "Failed to star dashboard"))?;
    Ok(success("Dashboard starred!"))
}

fn unstar_dashboard(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let uid = c.param("uid").to_string();
    if uid.is_empty() {
        return Err(ApiError::new(400, "Invalid dashboard UID"));
    }
    c.app.store.star_remove(c.user.org_id, c.user.user_id, &uid).map_err(|_| ApiError::with_trace(500, "Failed to unstar dashboard"))?;
    Ok(success("Dashboard unstarred"))
}

/// A Kubernetes `Stars` object for the user (legacy asStarsResource).
fn stars_object(c: &Ctx<'_>, names: &[String]) -> Json {
    let mut sorted: Vec<String> = names.to_vec();
    sorted.sort();
    let mut n = Json::arr();
    for s in sorted.iter() {
        n.push(Json::Str(s.clone()));
    }
    let t = now();
    Json::obj()
        .with("kind", "Stars")
        .with("apiVersion", "collections.grafana.app/v1alpha1")
        .with(
            "metadata",
            Json::obj()
                .with("name", format!("user-{}", c.user.uid))
                .with("namespace", namespace(c.user.org_id))
                .with("resourceVersion", (t * 1000).to_string())
                .with("creationTimestamp", rfc3339(t))
                .with("annotations", Json::obj().with("grafana.app/createdBy", format!("user:{}", c.user.uid))),
        )
        .with("spec", Json::obj().with("resource", Json::arr().push_item(Json::obj().with("group", "dashboard.grafana.app").with("kind", "Dashboard").with("names", n))))
}

fn user_stars(c: &Ctx<'_>) -> Vec<String> {
    if c.user.user_id == 0 || c.user.is_anonymous {
        return Vec::new();
    }
    c.app.store.stars_of_user(c.user.org_id, c.user.user_id).unwrap_or_default()
}

fn list_stars(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let names = user_stars(c);
    let mut items = Json::arr();
    if !names.is_empty() {
        items.push(stars_object(c, &names));
    }
    let list = Json::obj().with("kind", "StarsList").with("apiVersion", "collections.grafana.app/v1alpha1").with("metadata", Json::obj()).with("items", items);
    Ok(Response::json(200, &list))
}

fn k8s_status(code: u16, reason: &str, message: &str) -> ApiError {
    ApiError::raw(
        code,
        Json::obj()
            .with("kind", "Status")
            .with("apiVersion", "v1")
            .with("metadata", Json::obj())
            .with("status", "Failure")
            .with("message", message)
            .with("reason", reason)
            .with("code", code as i64),
    )
}

fn get_stars_object(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let name = c.param("name").to_string();
    if name != format!("user-{}", c.user.uid) || c.user.uid.is_empty() {
        return Err(k8s_status(404, "NotFound", &format!("stars.collections.grafana.app \"{name}\" not found")));
    }
    let names = user_stars(c);
    if names.is_empty() {
        return Err(k8s_status(404, "NotFound", &format!("stars.collections.grafana.app \"{name}\" not found")));
    }
    Ok(Response::json(200, &stars_object(c, &names)))
}

/// `PUT|DELETE .../stars/user-<uid>/update/<group>/<kind>/<name>` (starsREST.Connect).
fn update_stars(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let name = c.param("name").to_string();
    if c.user.uid.is_empty() || name != format!("user-{}", c.user.uid) {
        return Err(k8s_status(403, "Forbidden", "only works with user stars"));
    }
    let rest = c.param("*").to_string();
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.len() != 3 || parts[0] != "dashboard.grafana.app" || parts[1] != "Dashboard" || parts[2].is_empty() {
        return Err(k8s_status(400, "BadRequest", "expected /update/{group}/{kind}/{name}"));
    }
    let st = &c.app.store;
    if c.req.method == "DELETE" {
        st.star_remove(c.user.org_id, c.user.user_id, parts[2])?;
    } else {
        st.star_add(c.user.org_id, c.user.user_id, parts[2])?;
    }
    let names = user_stars(c);
    if names.is_empty() {
        return Ok(Response::json(200, &Json::obj().with("kind", "Status").with("apiVersion", "v1").with("metadata", Json::obj()).with("status", "Success").with("code", 200)));
    }
    Ok(Response::json(200, &stars_object(c, &names)))
}

// ---------------------------------------------------------------- preferences

/// Merged preferences (user > teams > org > configured defaults), as preferences_merged.go resolves them.
#[derive(Debug, Clone, Default)]
pub struct Prefs {
    pub home_url: Option<String>,
    pub home_dashboard_uid: String,
    pub timezone: String,
    pub week_start: String,
    pub theme: String,
    pub language: String,
    pub query_history: Option<Json>,
    pub navbar: Option<Json>,
    /// Item names that contributed ("user-<uid>", "team-<uid>", "namespace").
    pub sources: Vec<String>,
    /// Latest update of a contributing row (unix seconds), 0 for none.
    pub latest: i64,
}

const SPEC_KEYS: [&str; 8] = ["homeURL", "homeDashboardUID", "timezone", "weekStart", "theme", "language", "queryHistory", "navbar"];

/// The stored spec of a preferences row (from json_data, else from the columns).
fn spec_of(p: &Preferences) -> Json {
    if !p.json_data.is_empty() {
        if let Ok(j) = Json::parse(&p.json_data) {
            if let Json::Obj(_) = j {
                if j.get("timezone").is_some() || j.get("theme").is_some() || j.get("weekStart").is_some() || j.get("language").is_some() || j.get("homeDashboardUID").is_some() || j.get("navbar").is_some() || j.get("queryHistory").is_some() {
                    return canonical(&j);
                }
                // legacy json_data (language, queryHistory, navbar) with the column values
                let mut s = Json::obj();
                if !p.home_dashboard_uid.is_empty() {
                    s.set("homeDashboardUID", p.home_dashboard_uid.as_str());
                }
                if !p.timezone.is_empty() {
                    s.set("timezone", p.timezone.as_str());
                }
                if !p.week_start.is_empty() {
                    s.set("weekStart", p.week_start.as_str());
                }
                if !p.theme.is_empty() {
                    s.set("theme", p.theme.as_str());
                }
                for k in ["language", "queryHistory", "navbar"] {
                    if let Some(v) = j.get(k) {
                        s.set(k, v.clone());
                    }
                }
                return canonical(&s);
            }
        }
    }
    let mut s = Json::obj();
    if !p.home_dashboard_uid.is_empty() {
        s.set("homeDashboardUID", p.home_dashboard_uid.as_str());
    }
    if !p.timezone.is_empty() {
        s.set("timezone", p.timezone.as_str());
    }
    if !p.week_start.is_empty() {
        s.set("weekStart", p.week_start.as_str());
    }
    if !p.theme.is_empty() {
        s.set("theme", p.theme.as_str());
    }
    s
}

/// The spec keys in the struct order of PreferencesSpec.
fn canonical(j: &Json) -> Json {
    let mut s = Json::obj();
    for k in SPEC_KEYS.iter() {
        if let Some(v) = j.get(k) {
            if !v.is_null() {
                s.set(k, v.clone());
            }
        }
    }
    s
}

fn default_prefs(app: &App) -> (String, String, String, String, String) {
    let cfg = &app.cfg;
    (
        cfg.get("users", "home_page").to_string(),
        crate::api::boot::cfg_or(cfg, "date_formats", "default_timezone", "browser"),
        crate::api::boot::cfg_or(cfg, "date_formats", "default_week_start", "browser"),
        crate::api::boot::cfg_or(cfg, "users", "default_theme", "dark"),
        crate::api::boot::cfg_or(cfg, "users", "default_language", "en-US"),
    )
}

fn merge_str(dst: &mut String, spec: &Json, key: &str) {
    if dst.is_empty() {
        let v = spec.str_of(key);
        if !v.is_empty() {
            *dst = v.to_string();
        }
    }
}

/// GetPreferencesWithDefaults / the merged route for the identity.
pub fn merged_prefs(app: &App, u: &SignedInUser) -> Prefs {
    let mut p = Prefs::default();
    if u.is_identified() && u.org_id > 0 {
        let st = &app.store;
        let mut items: Vec<(String, Preferences)> = Vec::new();
        if u.user_id > 0 && !u.is_anonymous {
            if let Ok(row) = st.prefs_get(u.org_id, u.user_id, 0) {
                items.push((format!("user-{}", u.uid), row));
            }
            if let Ok(teams) = st.teams_of_user(u.org_id, u.user_id) {
                for t in teams.iter() {
                    if let Ok(row) = st.prefs_get(u.org_id, 0, t.id) {
                        items.push((format!("team-{}", t.uid), row));
                    }
                }
            }
        }
        if let Ok(row) = st.prefs_get(u.org_id, 0, 0) {
            items.push(("namespace".to_string(), row));
        }
        for (name, row) in items.iter() {
            let spec = spec_of(row);
            merge_str(&mut p.home_dashboard_uid, &spec, "homeDashboardUID");
            merge_str(&mut p.timezone, &spec, "timezone");
            merge_str(&mut p.week_start, &spec, "weekStart");
            merge_str(&mut p.theme, &spec, "theme");
            merge_str(&mut p.language, &spec, "language");
            if p.query_history.is_none() {
                p.query_history = spec.get("queryHistory").cloned();
            }
            if p.navbar.is_none() {
                p.navbar = spec.get("navbar").cloned();
            }
            p.sources.push(name.clone());
            if row.updated > p.latest {
                p.latest = row.updated;
            }
        }
    }
    let (home, tz, ws, theme, lang) = default_prefs(app);
    if p.home_dashboard_uid.is_empty() {
        p.home_url = Some(home);
    }
    if p.timezone.is_empty() {
        p.timezone = tz;
    }
    if p.week_start.is_empty() {
        p.week_start = ws;
    }
    if p.theme.is_empty() {
        p.theme = theme;
    }
    if p.language.is_empty() {
        p.language = lang;
    }
    p
}

fn get_merged_prefs(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let p = merged_prefs(c.app, &c.user);
    let mut spec = Json::obj();
    if let Some(h) = &p.home_url {
        spec.set("homeURL", h.as_str());
    }
    if !p.home_dashboard_uid.is_empty() {
        spec.set("homeDashboardUID", p.home_dashboard_uid.as_str());
    }
    spec.set("timezone", p.timezone.as_str());
    spec.set("weekStart", p.week_start.as_str());
    spec.set("theme", p.theme.as_str());
    spec.set("language", p.language.as_str());
    if let Some(q) = &p.query_history {
        spec.set("queryHistory", q.clone());
    }
    if let Some(n) = &p.navbar {
        spec.set("navbar", n.clone());
    }
    let mut meta = Json::obj();
    if p.latest > 0 {
        meta.set("resourceVersion", (p.latest * 1000).to_string());
        meta.set("creationTimestamp", rfc3339(p.latest));
    }
    meta.set("annotations", Json::obj().with("preferences.grafana.app/source", p.sources.join(",")));
    let j = Json::obj().with("kind", "Preferences").with("apiVersion", "preferences.grafana.app/v1").with("metadata", meta).with("spec", spec);
    let mut text = j.stringify();
    text.push('\n');
    Ok(Response::json_text(200, text))
}

fn get_prefs_for(c: &Ctx<'_>, user_id: i64, team_id: i64) -> Result<Response, ApiError> {
    let spec = match c.app.store.prefs_get(c.user.org_id, user_id, team_id) {
        Ok(row) => spec_of(&row),
        Err(StoreError::NotFound) => Json::obj(),
        Err(_) => return Err(ApiError::with_trace(500, "Failed to handle preferences request")),
    };
    Ok(Response::json(200, &spec))
}

fn get_user_prefs(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    get_prefs_for(c, c.user.user_id, 0)
}

fn valid_timezone(tz: &str) -> bool {
    if tz.is_empty() || tz == "utc" || tz == "browser" || tz == "UTC" || tz == "Local" {
        return true;
    }
    if tz.contains("..") || tz.starts_with('/') {
        return false;
    }
    std::path::Path::new("/usr/share/zoneinfo").join(tz).is_file()
}

/// Validates and saves a spec for (user_id, team_id) of the current org.
fn save_spec(c: &Ctx<'_>, user_id: i64, team_id: i64, owner_name: &str, spec: Json) -> Result<Response, ApiError> {
    let mut errs: Vec<String> = Vec::new();
    if let Some(tz) = spec.get("timezone").and_then(|v| v.as_str()) {
        if !valid_timezone(tz) {
            errs.push(format!("spec.timezone: Invalid value: {}: must be a valid IANA timezone (e.g., America/New_York), 'utc', 'browser', or empty", json::quote(tz)));
        }
    }
    if let Some(t) = spec.get("theme").and_then(|v| v.as_str()) {
        if !t.is_empty() && !THEMES.iter().any(|x| x.0 == t) {
            errs.push(format!("spec.theme: Invalid value: {}: must match a configured theme", json::quote(t)));
        }
    }
    if !errs.is_empty() {
        let msg = if errs.len() == 1 {
            format!("Preferences.preferences.grafana.app \"{owner_name}\" is invalid: {}", errs[0])
        } else {
            format!("Preferences.preferences.grafana.app \"{owner_name}\" is invalid: [{}]", errs.join(", "))
        };
        return Err(ApiError::with_trace(422, &msg));
    }
    let st = &c.app.store;
    let t = now();
    let created = match st.prefs_get(c.user.org_id, user_id, team_id) {
        Ok(r) => r.created,
        Err(_) => t,
    };
    let row = Preferences {
        id: 0,
        org_id: c.user.org_id,
        user_id,
        team_id,
        version: 0,
        home_dashboard_id: 0,
        home_dashboard_uid: spec.str_of("homeDashboardUID").to_string(),
        timezone: spec.str_of("timezone").to_string(),
        theme: spec.str_of("theme").to_string(),
        week_start: spec.str_of("weekStart").to_string(),
        json_data: canonical(&spec).stringify(),
        created,
        updated: t,
    };
    st.prefs_save(&row).map_err(|_| ApiError::with_trace(500, "Failed to handle preferences request"))?;
    Ok(success("Preferences updated"))
}

/// The home dashboard uid of an update: `homeDashboardUID`, else the uid of `homeDashboardId`.
fn home_uid(c: &Ctx<'_>, body: &Json) -> Result<Option<String>, ApiError> {
    if let Some(v) = body.get("homeDashboardUID") {
        if let Some(s) = v.as_str() {
            return Ok(Some(s.to_string()));
        }
    }
    let id = body.i64_of("homeDashboardId");
    if id == 0 {
        return Ok(None);
    }
    match c.app.store.dashboard_by_id(c.user.org_id, id) {
        Ok(d) => Ok(Some(d.uid)),
        Err(_) => Err(ApiError::with_trace(404, "Dashboard not found")),
    }
}

fn check_update_body(body: &Json) -> Result<(), ApiError> {
    // UpdatePrefsCmd binding: theme must be one of "", light, dark, system (and the extra themes).
    let theme = body.str_of("theme");
    if !theme.is_empty() && !THEMES.iter().any(|x| x.0 == theme) {
        return Err(ApiError::bad_request("bad request data"));
    }
    Ok(())
}

/// updateCmdToSpec.
fn update_spec(c: &Ctx<'_>, body: &Json) -> Result<Json, ApiError> {
    check_update_body(body)?;
    let mut s = Json::obj();
    if let Some(h) = home_uid(c, body)? {
        s.set("homeDashboardUID", h);
    }
    s.set("timezone", body.str_of("timezone"));
    s.set("weekStart", body.str_of("weekStart"));
    s.set("theme", body.str_of("theme"));
    s.set("language", body.str_of("language"));
    if let Some(q) = body.get("queryHistory") {
        if let Json::Obj(_) = q {
            s.set("queryHistory", Json::obj().with("homeTab", q.str_of("homeTab")));
        }
    }
    if let Some(n) = body.get("navbar") {
        if let Json::Obj(_) = n {
            s.set("navbar", Json::obj().with("bookmarkUrls", n.get("bookmarkUrls").cloned().unwrap_or(Json::Null)));
        }
    }
    Ok(s)
}

/// patchCmdToSpec applied on the stored spec.
fn patch_spec(c: &Ctx<'_>, user_id: i64, team_id: i64, body: &Json) -> Result<Json, ApiError> {
    check_update_body(body)?;
    let mut s = match c.app.store.prefs_get(c.user.org_id, user_id, team_id) {
        Ok(r) => spec_of(&r),
        Err(_) => Json::obj(),
    };
    if let Some(h) = home_uid(c, body)? {
        s.set("homeDashboardUID", h);
    }
    for k in ["timezone", "weekStart", "theme", "language"] {
        if let Some(v) = body.get(k).and_then(|v| v.as_str()) {
            s.set(k, v);
        }
    }
    if let Some(q) = body.get("queryHistory") {
        if let Json::Obj(_) = q {
            s.set("queryHistory", Json::obj().with("homeTab", q.str_of("homeTab")));
        }
    }
    if let Some(n) = body.get("navbar") {
        if let Json::Obj(_) = n {
            s.set("navbar", Json::obj().with("bookmarkUrls", n.get("bookmarkUrls").cloned().unwrap_or(Json::Null)));
        }
    }
    Ok(canonical(&s))
}

fn put_user_prefs(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let body = read_body(c)?;
    let spec = update_spec(c, &body)?;
    let name = format!("user-{}", c.user.uid);
    save_spec(c, c.user.user_id, 0, &name, spec)
}

fn patch_user_prefs(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let body = read_body(c)?;
    let spec = patch_spec(c, c.user.user_id, 0, &body)?;
    let name = format!("user-{}", c.user.uid);
    save_spec(c, c.user.user_id, 0, &name, spec)
}

fn get_org_prefs(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize(c, "orgs.preferences:read")?;
    get_prefs_for(c, 0, 0)
}

fn put_org_prefs(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize(c, "orgs.preferences:write")?;
    let body = read_body(c)?;
    let spec = update_spec(c, &body)?;
    save_spec(c, 0, 0, "namespace", spec)
}

fn patch_org_prefs(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize(c, "orgs.preferences:write")?;
    let body = read_body(c)?;
    let spec = patch_spec(c, 0, 0, &body)?;
    save_spec(c, 0, 0, "namespace", spec)
}

fn get_team_prefs(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize(c, "teams:read")?;
    let id = c.param_i64("teamId")?;
    get_prefs_for(c, 0, id)
}

// ---------------------------------------------------------------- users

fn search_hit(app: &App, u: &User) -> Json {
    Json::obj()
        .with("id", u.id)
        .with("uid", u.uid.as_str())
        .with("name", u.name.as_str())
        .with("login", u.login.as_str())
        .with("email", u.email.as_str())
        .with("role", "")
        .with("avatarUrl", gravatar_url(&app.cfg, &u.email))
        .with("isAdmin", u.is_admin)
        .with("isDisabled", u.is_disabled)
        .with("isProvisioned", u.is_provisioned)
        .with("lastSeenAt", time_or_zero(u.last_seen_at))
        .with("lastSeenAtAge", age_string(u.last_seen_at))
        .with("authLabels", Json::arr())
        .with("created", rfc3339(u.created))
}

fn page_params(c: &Ctx<'_>, default_per_page: i64) -> (String, i64, i64) {
    let q = c.req.query_get("query").unwrap_or_default();
    let mut per_page = c.req.query_get("perpage").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    if per_page <= 0 {
        per_page = default_per_page;
    }
    let mut page = c.req.query_get("page").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    if page <= 0 {
        page = 1;
    }
    (q, page, per_page)
}

fn search_users(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize_global(c, "users:read")?;
    let (q, page, per_page) = page_params(c, 1000);
    let (list, _) = c.app.store.user_search(&q, per_page, (page - 1) * per_page).map_err(|_| ApiError::with_trace(500, "Failed to fetch users"))?;
    let mut a = Json::arr();
    for u in list.iter() {
        a.push(search_hit(c.app, u));
    }
    Ok(Response::json(200, &a))
}

fn search_users_paged(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize_global(c, "users:read")?;
    let (q, page, per_page) = page_params(c, 1000);
    let (list, total) = c.app.store.user_search(&q, per_page, (page - 1) * per_page).map_err(|_| ApiError::with_trace(500, "Failed to fetch users"))?;
    let mut a = Json::arr();
    for u in list.iter() {
        a.push(search_hit(c.app, u));
    }
    Ok(Response::json(200, &Json::obj().with("totalCount", total).with("users", a).with("page", page).with("perPage", per_page)))
}

fn get_user_by_id(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize_global(c, "users:read")?;
    let u = user_param(c, "id")?;
    Ok(Response::json(200, &user_profile(c.app, &u, u.org_id)))
}

fn get_user_by_login_or_email(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize_global(c, "users:read")?;
    let q = c.req.query_get("loginOrEmail").unwrap_or_default();
    let u = match c.app.store.user_by_login_or_email(&q) {
        Ok(u) => u,
        Err(StoreError::NotFound) => return Err(ApiError::new(404, "user not found")),
        Err(_) => return Err(ApiError::with_trace(500, "Failed to get user")),
    };
    Ok(Response::json(200, &user_profile(c.app, &u, u.org_id)))
}

fn get_user_teams(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize_global(c, "users:read")?;
    let u = user_param(c, "id")?;
    teams_json(c, c.user.org_id, u.id)
}

fn get_user_orgs(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize_global(c, "users:read")?;
    let u = user_param(c, "id")?;
    orgs_json(c, u.id)
}

fn update_user(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize_global(c, "users:write")?;
    let body = read_body(c)?;
    let u = user_param(c, "id")?;
    apply_user_update(c, u.id, &body)
}

fn update_user_active_org(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize_global(c, "users:write")?;
    let u = user_param(c, "id")?;
    let org_id = c.param_i64("orgId").map_err(|_| ApiError::bad_request("orgId is invalid"))?;
    set_using_org(c, u.id, org_id)?;
    Ok(success("Active organization changed"))
}

// ---------------------------------------------------------------- orgs

fn org_details(o: &crate::store::Org) -> Json {
    Json::obj().with("id", o.id).with("name", o.name.as_str()).with(
        "address",
        Json::obj().with("address1", "").with("address2", "").with("city", "").with("zipCode", "").with("state", "").with("country", ""),
    )
}

fn get_org_json(c: &Ctx<'_>, org_id: i64) -> Result<Response, ApiError> {
    match c.app.store.org_by_id(org_id) {
        Ok(o) => Ok(Response::json(200, &org_details(&o))),
        Err(StoreError::NotFound) => Err(ApiError::with_trace(404, "Organization not found")),
        Err(_) => Err(ApiError::with_trace(500, "Failed to get organization")),
    }
}

fn get_current_org(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize(c, "orgs:read")?;
    get_org_json(c, c.user.org_id)
}

fn rename_org(c: &Ctx<'_>, org_id: i64, body: &Json) -> Result<Response, ApiError> {
    let name = body.str_of("name");
    if name.is_empty() {
        return Err(ApiError::bad_request("bad request data"));
    }
    match c.app.store.org_update_name(org_id, name) {
        Ok(()) => Ok(success("Organization updated")),
        Err(StoreError::Conflict(_)) => Err(ApiError::with_trace(409, "Organization name taken")),
        Err(StoreError::NotFound) => Err(ApiError::with_trace(404, "Organization not found")),
        Err(_) => Err(ApiError::with_trace(500, "Failed to update organization")),
    }
}

fn update_current_org(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize(c, "orgs:write")?;
    let body = read_body(c)?;
    rename_org(c, c.user.org_id, &body)
}

fn update_current_org_address(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize(c, "orgs:write")?;
    let _ = read_body(c)?;
    // grafana-rs keeps no org address; accept and ignore, as the address is informational only.
    Ok(success("Address updated"))
}

fn org_param(c: &Ctx<'_>) -> Result<i64, ApiError> {
    c.param("orgId").parse::<i64>().map_err(|_| ApiError::bad_request("orgId is invalid"))
}

/// authorizeInOrg(UseOrgFromContextParams): server admins, or the user's own org with the permission.
fn authorize_org(c: &Ctx<'_>, org_id: i64, action: &str) -> Result<(), ApiError> {
    if c.user.is_grafana_admin && access::actions(&SignedInUser { org_role: Role::Admin, is_grafana_admin: true, ..SignedInUser::default() }).contains(&action) {
        return Ok(());
    }
    if org_id == c.user.org_id && access::has(&c.user, action) {
        return Ok(());
    }
    Err(deny(action))
}

fn search_orgs(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize_global(c, "orgs:read")?;
    let q = c.req.query_get("query").unwrap_or_else(|| c.req.query_get("name").unwrap_or_default());
    let mut per_page = c.req.query_get("perpage").and_then(|v| v.parse::<i64>().ok()).unwrap_or(1000);
    if per_page <= 0 {
        per_page = 1000;
    }
    let page = c.req.query_get("page").and_then(|v| v.parse::<i64>().ok()).unwrap_or(1).max(1);
    let (list, _) = c.app.store.org_search(&q, per_page, (page - 1) * per_page).map_err(|_| ApiError::with_trace(500, "Failed to search orgs"))?;
    let mut a = Json::arr();
    for o in list.iter() {
        a.push(Json::obj().with("id", o.id).with("name", o.name.as_str()));
    }
    Ok(Response::json(200, &a))
}

fn create_org(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let allowed = c.user.is_grafana_admin || (c.app.cfg.get_bool("users", "allow_org_create") && c.user.is_signed_in());
    if !allowed {
        return Err(deny("orgs:create"));
    }
    let body = read_body(c)?;
    let name = body.str_of("name").to_string();
    if name.is_empty() {
        return Err(ApiError::bad_request("bad request data"));
    }
    let st = &c.app.store;
    if st.org_by_name(&name).is_ok() {
        return Err(ApiError::with_trace(409, "Organization name taken"));
    }
    let id = match st.org_create(&name) {
        Ok(id) => id,
        Err(StoreError::Conflict(_)) => return Err(ApiError::with_trace(409, "Organization name taken")),
        Err(_) => return Err(ApiError::with_trace(500, "Failed to create organization")),
    };
    if c.user.user_id > 0 && !c.user.is_service_account {
        let _ = st.org_user_set(id, c.user.user_id, Role::Admin);
    }
    Ok(Response::json(200, &Json::obj().with("message", "Organization created").with("orgId", id)))
}

fn get_org_by_id(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let id = org_param(c)?;
    authorize_org(c, id, "orgs:read")?;
    get_org_json(c, id)
}

fn get_org_by_name(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize_global(c, "orgs:read")?;
    match c.app.store.org_by_name(c.param("name")) {
        Ok(o) => Ok(Response::json(200, &org_details(&o))),
        Err(StoreError::NotFound) => Err(ApiError::with_trace(404, "Organization not found")),
        Err(_) => Err(ApiError::with_trace(500, "Failed to get organization")),
    }
}

fn update_org(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let id = org_param(c)?;
    authorize_org(c, id, "orgs:write")?;
    let body = read_body(c)?;
    rename_org(c, id, &body)
}

fn update_org_address(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let id = org_param(c)?;
    authorize_org(c, id, "orgs:write")?;
    Ok(success("Address updated"))
}

fn delete_org(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let id = org_param(c)?;
    authorize_org(c, id, "orgs:delete")?;
    if id == c.user.org_id {
        return Err(ApiError::new(400, "Can not delete org for current user"));
    }
    match c.app.store.org_delete(id) {
        Ok(()) => Ok(success("Organization deleted")),
        Err(StoreError::NotFound) => Err(ApiError::with_trace(404, "Failed to delete organization. ID not found")),
        Err(_) => Err(ApiError::with_trace(500, "Failed to update organization")),
    }
}

// ---------------------------------------------------------------- org users

fn org_user_json(app: &App, m: &crate::store::OrgMember, uid: &str, disabled: bool, provisioned: bool) -> Json {
    Json::obj()
        .with("orgId", m.org_id)
        .with("userId", m.user_id)
        .with("uid", uid)
        .with("email", m.email.as_str())
        .with("name", m.name.as_str())
        .with("avatarUrl", gravatar_url(&app.cfg, &m.email))
        .with("login", m.login.as_str())
        .with("role", m.role.as_str())
        .with("lastSeenAt", time_or_zero(m.last_seen_at))
        .with("created", rfc3339(m.created))
        .with("lastSeenAtAge", age_string(m.last_seen_at))
        .with("isDisabled", disabled)
        .with("authLabels", Json::Null)
        .with("isExternallySynced", false)
        .with("isProvisioned", provisioned)
}

/// Members of an org (optionally filtered by `query` on login, email or name), service accounts excluded.
fn org_users(c: &Ctx<'_>, org_id: i64, query: &str, limit: i64) -> Result<Vec<Json>, ApiError> {
    let st = &c.app.store;
    let list = st.org_members(org_id).map_err(|_| ApiError::with_trace(500, "Failed to get users for current organization"))?;
    let q = query.to_lowercase();
    let mut out = Vec::new();
    for m in list.iter() {
        let u = match st.user_by_id(m.user_id) {
            Ok(u) => u,
            Err(_) => continue,
        };
        if u.is_service_account {
            continue;
        }
        if !q.is_empty() && !(m.login.to_lowercase().contains(&q) || m.email.to_lowercase().contains(&q) || m.name.to_lowercase().contains(&q)) {
            continue;
        }
        out.push(org_user_json(c.app, m, &u.uid, u.is_disabled, u.is_provisioned));
        if limit > 0 && out.len() as i64 >= limit {
            break;
        }
    }
    Ok(out)
}

fn get_org_users_current(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize(c, "org.users:read")?;
    let q = c.req.query_get("query").unwrap_or_default();
    let limit = c.req.query_get("limit").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    let list = org_users(c, c.user.org_id, &q, limit)?;
    Ok(Response::json(200, &Json::Arr(list)))
}

fn search_org_users_for(c: &Ctx<'_>, org_id: i64) -> Result<Response, ApiError> {
    let (q, page, per_page) = page_params(c, 1000);
    let all = org_users(c, org_id, &q, 0)?;
    let total = all.len() as i64;
    let start = ((page - 1) * per_page).min(total) as usize;
    let end = (start as i64 + per_page).min(total) as usize;
    let mut a = Json::arr();
    for j in all[start..end].iter() {
        a.push(j.clone());
    }
    Ok(Response::json(200, &Json::obj().with("totalCount", total).with("orgUsers", a).with("page", page).with("perPage", per_page)))
}

fn search_org_users_current(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize(c, "org.users:read")?;
    search_org_users_for(c, c.user.org_id)
}

fn org_users_lookup(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    if !access::has_any(&c.user, &["org.users:read", "teams.permissions:write", "folders.permissions:write", "dashboards.permissions:write"]) {
        return Err(deny("org.users:read"));
    }
    let q = c.req.query_get("query").unwrap_or_default();
    let limit = c.req.query_get("limit").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    let list = org_users(c, c.user.org_id, &q, limit)?;
    let mut a = Json::arr();
    for j in list.iter() {
        a.push(Json::obj().with("uid", j.str_of("uid")).with("userId", j.i64_of("userId")).with("login", j.str_of("login")).with("avatarUrl", j.str_of("avatarUrl")));
    }
    Ok(Response::json(200, &a))
}

fn add_org_user_to(c: &Ctx<'_>, org_id: i64, body: &Json) -> Result<Response, ApiError> {
    let role = match Role::parse(body.str_of("role")) {
        Some(r) => r,
        None => return Err(ApiError::bad_request("bad request data")),
    };
    let st = &c.app.store;
    let u = match st.user_by_login_or_email(body.str_of("loginOrEmail")) {
        Ok(u) => u,
        Err(_) => return Err(ApiError::with_trace(404, "User not found")),
    };
    if u.is_service_account {
        return Err(ApiError::with_trace(404, "User not found"));
    }
    if st.org_user_role(org_id, u.id).is_ok() {
        return Err(ApiError::with_trace(409, "User is already member of this organization"));
    }
    st.org_user_set(org_id, u.id, role).map_err(|_| ApiError::with_trace(500, "Could not add user to organization"))?;
    Ok(Response::json(200, &Json::obj().with("message", "User added to organization").with("userId", u.id)))
}

fn add_org_user_current(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize(c, "org.users:add")?;
    let body = read_body(c)?;
    add_org_user_to(c, c.user.org_id, &body)
}

fn add_org_user(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let id = org_param(c)?;
    authorize_org(c, id, "org.users:add")?;
    let body = read_body(c)?;
    add_org_user_to(c, id, &body)
}

fn update_org_user_in(c: &Ctx<'_>, org_id: i64, user_id: i64, body: &Json) -> Result<Response, ApiError> {
    let role = match Role::parse(body.str_of("role")) {
        Some(r) => r,
        None => return Err(ApiError::bad_request("bad request data")),
    };
    let st = &c.app.store;
    let cur = match st.org_user_role(org_id, user_id) {
        Ok(r) => r,
        Err(_) => return Err(ApiError::with_trace(404, "User not found")),
    };
    if cur == Role::Admin && role != Role::Admin && last_admin(c, org_id) {
        return Err(ApiError::with_trace(400, "Cannot change role so that there is no organization admin left"));
    }
    st.org_user_set(org_id, user_id, role).map_err(|_| ApiError::with_trace(500, "Failed update org user"))?;
    Ok(success("Organization user updated"))
}

fn last_admin(c: &Ctx<'_>, org_id: i64) -> bool {
    match c.app.store.org_members(org_id) {
        Ok(list) => list.iter().filter(|m| m.role == Role::Admin).count() <= 1,
        Err(_) => false,
    }
}

fn user_id_param(c: &Ctx<'_>) -> Result<i64, ApiError> {
    let raw = c.param("userId");
    match raw.parse::<i64>() {
        Ok(v) => Ok(v),
        Err(_) => match c.app.store.user_by_uid(raw) {
            Ok(u) => Ok(u.id),
            Err(_) => Err(ApiError::bad_request("userId is invalid")),
        },
    }
}

fn update_org_user_current(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize(c, "org.users:write")?;
    let body = read_body(c)?;
    let uid = user_id_param(c)?;
    update_org_user_in(c, c.user.org_id, uid, &body)
}

fn update_org_user(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let id = org_param(c)?;
    authorize_org(c, id, "org.users:write")?;
    let body = read_body(c)?;
    let uid = user_id_param(c)?;
    update_org_user_in(c, id, uid, &body)
}

fn remove_org_user_in(c: &Ctx<'_>, org_id: i64, user_id: i64) -> Result<Response, ApiError> {
    let st = &c.app.store;
    let cur = match st.org_user_role(org_id, user_id) {
        Ok(r) => r,
        Err(_) => return Err(ApiError::with_trace(404, "User not found")),
    };
    if cur == Role::Admin && last_admin(c, org_id) {
        return Err(ApiError::with_trace(400, "Cannot remove last organization admin"));
    }
    st.org_user_remove(org_id, user_id).map_err(|_| ApiError::with_trace(500, "Failed to remove user from organization"))?;
    Ok(success("User removed from organization"))
}

fn remove_org_user_current(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize(c, "org.users:remove")?;
    let uid = user_id_param(c)?;
    remove_org_user_in(c, c.user.org_id, uid)
}

fn remove_org_user(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let id = org_param(c)?;
    authorize_org(c, id, "org.users:remove")?;
    let uid = user_id_param(c)?;
    remove_org_user_in(c, id, uid)
}

fn get_org_users(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let id = org_param(c)?;
    authorize_org(c, id, "org.users:read")?;
    let list = org_users(c, id, "", 0)?;
    Ok(Response::json(200, &Json::Arr(list)))
}

fn search_org_users(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let id = org_param(c)?;
    authorize_org(c, id, "org.users:read")?;
    search_org_users_for(c, id)
}

fn get_invites(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize(c, "org.users:add")?;
    Ok(Response::json_text(200, "[]".to_string()))
}

// ---------------------------------------------------------------- admin users

fn admin_create_user(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize_global(c, "users:create")?;
    let body = read_body(c)?;
    let st = &c.app.store;
    let mut login = body.str_of("login").to_string();
    let email = body.str_of("email").to_string();
    if login.is_empty() {
        login = email.clone();
    }
    let password = body.str_of("password").to_string();
    if login.is_empty() || password.is_empty() {
        return Err(ApiError::bad_request("bad request data"));
    }
    validate_password(c.app, &password)?;
    let mut org_id = body.i64_of("orgId");
    if org_id == 0 {
        org_id = match st.org_by_name(c.app.cfg.get("users", "auto_assign_org_id")) {
            Ok(o) => o.id,
            Err(_) => 1,
        };
    } else if st.org_by_id(org_id).is_err() {
        return Err(ApiError::with_trace(400, "organization not found"));
    }
    let t = now();
    let mut u = User {
        uid: crate::util::short_uid(),
        login: login.clone(),
        email: if email.is_empty() { login.clone() } else { email },
        name: body.str_of("name").to_string(),
        org_id,
        created: t,
        updated: t,
        rands: crypto::random_string(10, b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"),
        ..User::default()
    };
    set_password(&mut u, &password);
    let id = match st.user_create(&u) {
        Ok(id) => id,
        Err(StoreError::Conflict(_)) => return Err(ApiError::errutil(412, "user.sync.user-already-exists", "User with email '' or username '' already exists")),
        Err(_) => return Err(ApiError::with_trace(500, "failed to create user")),
    };
    let role = Role::parse(c.app.cfg.get("users", "auto_assign_org_role")).unwrap_or(Role::Viewer);
    let _ = st.org_user_set(org_id, id, role);
    Ok(Response::json(200, &Json::obj().with("id", id).with("uid", u.uid.as_str()).with("message", "User created")))
}

fn admin_update_password(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize_global(c, "users.password:write")?;
    let body = read_body(c)?;
    let mut u = user_param(c, "id")?;
    let pw = body.str_of("password");
    validate_password(c.app, pw)?;
    set_password(&mut u, pw);
    c.app.store.user_update(&u).map_err(|_| ApiError::with_trace(500, "Failed to update user password"))?;
    let _ = c.app.store.tokens_delete_for_user(u.id);
    Ok(success("User password updated"))
}

fn admin_update_permissions(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let body = read_body(c)?;
    let mut u = user_param(c, "id")?;
    let admin = body.bool_of("isGrafanaAdmin");
    if !admin && u.id == c.user.user_id {
        return Err(ApiError::with_trace(400, "Cannot remove own admin permission"));
    }
    u.is_admin = admin;
    u.updated = now();
    c.app.store.user_update(&u).map_err(|_| ApiError::with_trace(500, "Failed to update user permissions"))?;
    Ok(success("User permissions updated"))
}

fn admin_delete_user(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize_global(c, "users:delete")?;
    let u = user_param(c, "id")?;
    match c.app.store.user_delete(u.id) {
        Ok(()) => Ok(success("User deleted")),
        Err(StoreError::NotFound) => Err(ApiError::with_trace(404, "user not found")),
        Err(_) => Err(ApiError::with_trace(500, "Failed to delete user")),
    }
}

fn set_disabled(c: &Ctx<'_>, disabled: bool) -> Result<Response, ApiError> {
    let mut u = user_param(c, "id")?;
    u.is_disabled = disabled;
    u.updated = now();
    c.app.store.user_update(&u).map_err(|_| ApiError::with_trace(500, "Failed to update user"))?;
    if disabled {
        let _ = c.app.store.tokens_delete_for_user(u.id);
        Ok(success("User disabled"))
    } else {
        Ok(success("User enabled"))
    }
}

fn admin_disable_user(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize_global(c, "users:disable")?;
    set_disabled(c, true)
}

fn admin_enable_user(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize_global(c, "users:enable")?;
    set_disabled(c, false)
}

fn admin_logout_user(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize_global(c, "users:logout")?;
    let u = user_param(c, "id")?;
    if u.id == c.user.user_id {
        return Err(ApiError::new(400, "You cannot logout yourself"));
    }
    c.app.store.tokens_delete_for_user(u.id).map_err(|_| ApiError::with_trace(500, "Failed to logout user"))?;
    Ok(success("User logged out"))
}

fn admin_get_auth_tokens(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize_global(c, "users.authtoken:read")?;
    let u = user_param(c, "id")?;
    tokens_json(c, u.id, c.user.session_id)
}

fn admin_revoke_auth_token(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize_global(c, "users.authtoken:write")?;
    let body = read_body(c)?;
    let u = user_param(c, "id")?;
    revoke_token(c, u.id, &body)
}

// ---------------------------------------------------------------- teams (read only)

fn search_teams(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize(c, "teams:read")?;
    let mut per_page = c.req.query_get("perpage").and_then(|v| v.parse::<i64>().ok()).unwrap_or(1000);
    if per_page <= 0 {
        per_page = 1000;
    }
    let page = c.req.query_get("page").and_then(|v| v.parse::<i64>().ok()).unwrap_or(1).max(1);
    let q = c.req.query_get("query").unwrap_or_else(|| c.req.query_get("name").unwrap_or_default());
    let (list, total) = c.app.store.team_search(c.user.org_id, &q, per_page, (page - 1) * per_page).map_err(|_| ApiError::with_trace(500, "Failed to search Teams"))?;
    let mut a = Json::arr();
    for t in list.iter() {
        a.push(team_json(t));
    }
    Ok(Response::json(200, &Json::obj().with("totalCount", total).with("teams", a).with("page", page).with("perPage", per_page)))
}

fn get_team(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize(c, "teams:read")?;
    let id = c.param_i64("teamId")?;
    match c.app.store.team_by_id(c.user.org_id, id) {
        Ok(t) => Ok(Response::json(200, &team_json(&t))),
        Err(StoreError::NotFound) => Err(ApiError::with_trace(404, "Team not found")),
        Err(_) => Err(ApiError::with_trace(500, "Failed to get Team")),
    }
}

fn team_members_empty(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    authorize(c, "teams:read")?;
    Ok(Response::json_text(200, "[]".to_string()))
}

// ---------------------------------------------------------------- avatar

/// `/avatar/:hash`: Grafana proxies gravatar.com; grafana-rs has no TLS client, so it always serves the
/// fallback image Grafana uses when gravatar cannot be reached.
fn avatar(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let root = c.app.cfg.home.join("public");
    match http::staticfile::serve(&c.req, &root, "img/user_profile.png", "max-age=86400") {
        Some(mut r) => {
            r.set_header("Cache-Control", "max-age=86400");
            Ok(r)
        }
        None => Err(ApiError::not_found()),
    }
}
