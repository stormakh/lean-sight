//! Request context: the signed-in identity, roles, route access rules, and the route table type.
use crate::app::App;
use crate::error::ApiError;
use http::{Request, Response};

/// Org role. Order matters: `Viewer < Editor < Admin`; `None` has no role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Role {
    #[default]
    None,
    Viewer,
    Editor,
    Admin,
}
impl Role {
    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "None" => Some(Role::None),
            "Viewer" => Some(Role::Viewer),
            "Editor" => Some(Role::Editor),
            "Admin" => Some(Role::Admin),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Role::None => "None",
            Role::Viewer => "Viewer",
            Role::Editor => "Editor",
            Role::Admin => "Admin",
        }
    }
    /// True when this role grants at least `other` (Admin includes Editor includes Viewer).
    pub fn includes(self, other: Role) -> bool {
        self >= other
    }
}

/// How the identity was authenticated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AuthMethod {
    #[default]
    None,
    Session,
    Basic,
    ApiKey,
    Anonymous,
}

/// The identity of a request (Grafana's `user.SignedInUser`).
#[derive(Debug, Clone, Default)]
pub struct SignedInUser {
    /// 0 for anonymous or nobody.
    pub user_id: i64,
    pub uid: String,
    pub org_id: i64,
    pub org_name: String,
    pub org_role: Role,
    pub login: String,
    pub name: String,
    pub email: String,
    pub theme: String,
    pub help_flags1: i64,
    pub is_grafana_admin: bool,
    pub is_anonymous: bool,
    pub is_service_account: bool,
    pub is_disabled: bool,
    pub last_seen_at: i64,
    pub auth: AuthMethod,
    /// Session row id when authenticated by the session cookie.
    pub session_id: i64,
    /// API key row id when authenticated by a token.
    pub api_key_id: i64,
}
impl SignedInUser {
    /// Nobody: not signed in, no org.
    pub fn nobody() -> SignedInUser {
        SignedInUser::default()
    }
    /// Signed in, anonymous or not.
    pub fn is_identified(&self) -> bool {
        self.auth != AuthMethod::None
    }
    /// Signed in as a real user or service account (not anonymous).
    pub fn is_signed_in(&self) -> bool {
        self.auth != AuthMethod::None && !self.is_anonymous
    }
    /// The org role includes `r`.
    pub fn has_role(&self, r: Role) -> bool {
        self.org_role.includes(r)
    }
    /// Grafana identity string: "user:1", "service-account:5", "anonymous:0".
    pub fn identity(&self) -> String {
        if self.is_anonymous {
            "anonymous:0".to_string()
        } else if self.is_service_account {
            format!("service-account:{}", self.user_id)
        } else {
            format!("user:{}", self.user_id)
        }
    }
}

/// Who may call a route. Checked by `App` before the handler runs (see upstream pkg/middleware/auth.go).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// No authentication at all (static files, health, metrics). `ctx.user` is nobody.
    Public,
    /// Authenticate if credentials are present; never reject. Login page, logout.
    Optional,
    /// `reqSignedIn`: a user, a service account, or anonymous when enabled.
    SignedIn,
    /// `reqSignedInNoAnonymous`.
    NoAnonymous,
    /// Signed in with at least this org role (`reqEditorRole` is `Role(Role::Editor)`, `reqOrgAdmin` is `Role(Role::Admin)`).
    Role(Role),
    /// `reqGrafanaAdmin`.
    GrafanaAdmin,
}

/// A request in a handler: the app, the request (owned, so a handler may take the body), and the identity.
pub struct Ctx<'a> {
    pub app: &'a App,
    pub req: Request,
    pub user: SignedInUser,
}
impl<'a> Ctx<'a> {
    /// Path param, "" when absent.
    pub fn param(&self, name: &str) -> &str {
        self.req.param(name)
    }
    /// Path param parsed as i64, or 400 `{"message": "<name> is invalid"}`.
    pub fn param_i64(&self, name: &str) -> Result<i64, ApiError> {
        match self.req.param(name).parse::<i64>() {
            Ok(v) => Ok(v),
            Err(_) => Err(ApiError::bad_request(&format!("{name} is invalid"))),
        }
    }
    /// The body parsed as JSON, or 400 "bad request data" as Grafana's binding answers.
    pub fn body_json(&self) -> Result<json::Json, ApiError> {
        match self.req.json() {
            Ok(v) => Ok(v),
            Err(_) => Err(ApiError::bad_request("bad request data")),
        }
    }
    /// 403 unless the org role includes `r`.
    pub fn require_role(&self, r: Role) -> Result<(), ApiError> {
        if self.user.has_role(r) { Ok(()) } else { Err(ApiError::forbidden()) }
    }
}

/// A route handler.
pub type Handler = fn(&mut Ctx<'_>) -> Result<Response, ApiError>;

#[derive(Clone, Copy)]
pub struct Route {
    pub access: Access,
    pub handler: Handler,
}

/// The route table. Each area registers its routes with `pub fn routes(r: &mut Routes)`.
pub struct Routes {
    pub router: http::Router<Route>,
}
impl Default for Routes {
    fn default() -> Self {
        Routes::new()
    }
}
impl Routes {
    pub fn new() -> Routes {
        Routes { router: http::Router::new() }
    }
    pub fn add(&mut self, method: &str, pattern: &str, access: Access, handler: Handler) {
        self.router.add(method, pattern, Route { access, handler });
    }
    pub fn get(&mut self, pattern: &str, access: Access, handler: Handler) {
        self.add("GET", pattern, access, handler);
    }
    pub fn post(&mut self, pattern: &str, access: Access, handler: Handler) {
        self.add("POST", pattern, access, handler);
    }
    pub fn put(&mut self, pattern: &str, access: Access, handler: Handler) {
        self.add("PUT", pattern, access, handler);
    }
    pub fn patch(&mut self, pattern: &str, access: Access, handler: Handler) {
        self.add("PATCH", pattern, access, handler);
    }
    pub fn delete(&mut self, pattern: &str, access: Access, handler: Handler) {
        self.add("DELETE", pattern, access, handler);
    }
    /// Any method.
    pub fn any(&mut self, pattern: &str, access: Access, handler: Handler) {
        self.add("*", pattern, access, handler);
    }
}
