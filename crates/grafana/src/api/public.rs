//! Static files: `/public/*` from `<home>/public`, and `/public/plugins/<id>/*` from the plugin directory
//! (`<home>/data/plugins/<id>`), with Grafana's Cache-Control values (pkg/api/http_server.go mapStatic,
//! pkg/api/plugins.go getPluginAssets).
use crate::ctx::{Access, Ctx, Routes};
use crate::error::ApiError;
use http::Response;

pub fn routes(r: &mut Routes) {
    r.get("/public/plugins/:pluginId/*", Access::Public, plugin_asset);
    r.get("/public/*", Access::Public, public_file);
    r.get("/robots.txt", Access::Public, robots);
}

fn plain_404() -> Response {
    Response::text(404, "text/plain; charset=utf-8", "404 page not found\n")
}

fn public_file(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let rel = c.req.param("*").to_string();
    if rel == "views/swagger.html" {
        return Ok(plain_404());
    }
    let cfg = &c.app.cfg;
    let cache = if cfg.app_mode_development {
        "max-age=0, must-revalidate, no-cache"
    } else if rel.starts_with("build/") {
        "public, max-age=31536000"
    } else {
        "public, max-age=3600"
    };
    let root = cfg.home.join("public");
    match http::staticfile::serve(&c.req, &root, &rel, cache) {
        Some(r) => Ok(r),
        None => Ok(plain_404()),
    }
}

fn plugin_asset(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let id = c.req.param("pluginId").to_string();
    let rel = c.req.param("*").to_string();
    if id.is_empty() || id.contains('/') || id.starts_with('.') {
        return Ok(plain_404());
    }
    let cache = if c.app.cfg.app_mode_development { "max-age=0, must-revalidate, no-cache" } else { "public, max-age=3600" };
    let root = c.app.cfg.plugins_dir().join(&id);
    match http::staticfile::serve(&c.req, &root, &rel, cache) {
        Some(r) => Ok(r),
        None => Ok(plain_404()),
    }
}

fn robots(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let root = c.app.cfg.home.join("public");
    match http::staticfile::serve(&c.req, &root, "robots.txt", "public, max-age=3600") {
        Some(r) => Ok(r),
        None => Ok(plain_404()),
    }
}
