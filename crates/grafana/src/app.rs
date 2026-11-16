//! The application: settings, store, routes, and request dispatch with authentication.
use crate::auth::{self, AuthResult, BasicCache};
use crate::ctx::{Access, Ctx, Routes, SignedInUser};
use crate::error::ApiError;
use crate::settings::Settings;
use crate::store::Store;
use crate::template::Template;
use http::{Found, Request, Response};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub struct App {
    pub cfg: Settings,
    pub store: Arc<dyn Store>,
    pub routes: Routes,
    /// HTTP client for data sources (Prometheus), with a pool of idle connections.
    pub client: http::Client,
    pub basic_cache: BasicCache,
    /// `public/views/index.html`, parsed once.
    pub index_template: Option<Template>,
    /// Boot data `assets` from the build manifest.
    pub assets: json::Json,
    pub started: Instant,
    pub started_unix: i64,
    /// Requests served, by status class (index 1..5 for 1xx..5xx).
    pub requests: [AtomicU64; 6],
}

impl App {
    /// Builds the app and registers every route (`api::register`).
    pub fn new(cfg: Settings, store: Arc<dyn Store>) -> App {
        let index_template = match std::fs::read_to_string(cfg.home.join("public/views/index.html")) {
            Ok(t) => match Template::parse(&t) {
                Ok(tpl) => Some(tpl),
                Err(e) => {
                    eprintln!("grafana-rs: cannot parse index.html: {e}");
                    None
                }
            },
            Err(e) => {
                eprintln!("grafana-rs: cannot read index.html: {e}");
                None
            }
        };
        let assets = crate::api::boot::load_assets(&cfg.home);
        let mut routes = Routes::new();
        crate::api::register(&mut routes);
        App {
            cfg,
            store,
            routes,
            client: http::Client::new(),
            basic_cache: BasicCache::new(),
            index_template,
            assets,
            started: Instant::now(),
            started_unix: crate::util::now(),
            requests: [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)],
        }
    }

    /// True for paths that Grafana treats as API calls (JSON errors instead of pages).
    pub fn is_api_path(path: &str) -> bool {
        path.starts_with("/api")
    }

    fn dispatch(&self, mut req: Request) -> Response {
        let (route, params) = match self.routes.router.find(&req.method, &req.path) {
            Found::Ok(r, p) => (*r, p),
            Found::MethodNotAllowed | Found::NotFound => return self.not_found(req),
        };
        req.params = params;
        let user = match route.access {
            Access::Public => SignedInUser::nobody(),
            _ => match auth::authenticate(self, &req) {
                AuthResult::Ok(u) => u,
                AuthResult::NoCredentials => {
                    if route.access != Access::Optional {
                        return self.not_authorized(&req, ApiError::unauthorized());
                    }
                    SignedInUser::nobody()
                }
                AuthResult::Failed(e) => {
                    if route.access != Access::Optional {
                        return self.not_authorized(&req, e);
                    }
                    SignedInUser::nobody()
                }
            },
        };
        match route.access {
            Access::Public | Access::Optional | Access::SignedIn => {}
            Access::NoAnonymous => {
                if user.is_anonymous {
                    return self.not_authorized(&req, ApiError::unauthorized());
                }
            }
            Access::Role(r) => {
                if !user.has_role(r) {
                    return self.forbidden(&req);
                }
            }
            Access::GrafanaAdmin => {
                if !user.is_grafana_admin {
                    return self.forbidden(&req);
                }
            }
        }
        let mut ctx = Ctx { app: self, req, user };
        let resp = match (route.handler)(&mut ctx) {
            Ok(r) => r,
            Err(e) => e.to_response(),
        };
        resp
    }

    /// No route: JSON 404 for /api, else the index page with status 404 (the frontend router decides).
    pub fn not_found(&self, req: Request) -> Response {
        if App::is_api_path(&req.path) {
            return ApiError::not_found().to_response();
        }
        let user = match auth::authenticate(self, &req) {
            AuthResult::Ok(u) => u,
            _ => SignedInUser::nobody(),
        };
        let mut ctx = Ctx { app: self, req, user };
        match crate::api::boot::index(&mut ctx) {
            Ok(mut r) => {
                r.status = 404;
                r
            }
            Err(e) => e.to_response(),
        }
    }

    /// 401 for API paths; a redirect to the login page for pages (middleware notAuthorized).
    pub fn not_authorized(&self, req: &Request, err: ApiError) -> Response {
        if App::is_api_path(&req.path) {
            return err.to_response();
        }
        let uri = req.uri();
        let sub = &self.cfg.app_sub_url;
        let mut loc = format!("{sub}/login");
        if uri != "/" {
            loc.push_str("?redirectTo=");
            loc.push_str(&http::url_encode(&uri));
        }
        Response::redirect(302, &loc)
    }

    /// 403 JSON for API paths; a redirect home for pages (middleware accessForbidden).
    pub fn forbidden(&self, req: &Request) -> Response {
        if App::is_api_path(&req.path) {
            return ApiError::forbidden().to_response();
        }
        Response::redirect(302, &format!("{}/", self.cfg.app_sub_url))
    }
}

/// Grafana's AddDefaultResponseHeaders (static files are served before it and skip it).
pub fn add_default_headers(path: &str, resp: &mut Response) {
    if path.starts_with("/public/") {
        return;
    }
    let cachable = path.starts_with("/avatar/") || path.starts_with("/api/datasources/proxy/") || (path.contains("/resources/") && resp.header("cache-control").is_some());
    if !cachable {
        resp.set_header("Cache-Control", "no-store");
    }
    resp.set_header("X-Content-Type-Options", "nosniff");
    resp.set_header("X-Frame-Options", "deny");
    resp.set_header("X-Xss-Protection", "1; mode=block");
}

impl http::Service for App {
    fn handle(&self, req: Request) -> Response {
        let static_path = req.path.starts_with("/public/");
        let path = if static_path { String::new() } else { req.path.clone() };
        let mut resp = self.dispatch(req);
        if !static_path {
            add_default_headers(&path, &mut resp);
        }
        let class = (resp.status / 100) as usize;
        if class < 6 {
            self.requests[class].fetch_add(1, Ordering::Relaxed);
        }
        resp
    }
}
