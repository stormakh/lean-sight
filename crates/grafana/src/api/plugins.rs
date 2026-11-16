//! Plugin HTTP API (agent A): the plugin list, settings, markdown, errors, and the grafana.com proxy.
//! Upstream: pkg/api/plugins.go, pkg/api/grafana_com_proxy.go, routes and access in pkg/api/api.go.
//! Everything comes from the metadata prebuilt at startup (`crate::plugins`); plugin settings are static.
use crate::ctx::{Access, Ctx, Role, Routes};
use crate::error::ApiError;
use http::Response;

pub fn routes(r: &mut Routes) {
    r.get("/api/plugins", Access::SignedIn, get_plugin_list);
    r.get("/api/plugins/errors", Access::SignedIn, get_plugin_errors);
    r.get("/api/plugins/:pluginId/settings", Access::SignedIn, get_plugin_settings);
    // authorize(plugins:write): only the Admin basic role has it.
    r.post("/api/plugins/:pluginId/settings", Access::Role(Role::Admin), update_plugin_settings);
    r.get("/api/plugins/:pluginId/markdown/:name", Access::SignedIn, get_plugin_markdown);
    r.any("/api/gnet/*", Access::SignedIn, proxy_gnet);
}

/// `GET /api/plugins` (GetPluginList) with the `type`, `enabled`, `embedded`, `core` and `accesscontrol` filters.
fn get_plugin_list(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let mut type_filter = String::new();
    let mut enabled_filter = String::new();
    let mut core_filter = String::new();
    let mut accesscontrol = false;
    for (k, v) in c.req.query_pairs() {
        // c.Query returns the first value of a key.
        match k.as_str() {
            "type" if type_filter.is_empty() => type_filter = v,
            "enabled" if enabled_filter.is_empty() => enabled_filter = v,
            "core" if core_filter.is_empty() => core_filter = v,
            "accesscontrol" => accesscontrol = matches!(v.as_str(), "1" | "t" | "T" | "TRUE" | "true" | "True"),
            _ => {}
        }
    }
    // embedded: grafana-rs loads no plugins nested in apps, so the filter never removes anything.
    let role = c.user.org_role;
    // Admins (datasources:create, plugins:write) and Grafana admins (plugins:install) see non-core plugins.
    let can_list_non_core = role == Role::Admin || c.user.is_grafana_admin;
    // accesscontrol metadata: Viewer and up have plugins.app:access, Admin also has plugins:write.
    let ac_meta = if !accesscontrol || role == Role::None {
        ""
    } else if role == Role::Admin {
        ",\"accessControl\":{\"plugins.app:access\":true,\"plugins:write\":true}"
    } else {
        ",\"accessControl\":{\"plugins.app:access\":true}"
    };
    let plugins = &c.app.plugins;
    let mut out = String::with_capacity(64 * 1024);
    out.push('[');
    for i in plugins.by_name.iter() {
        let p = &plugins.list[*i];
        if (core_filter == "0" && p.core) || (core_filter == "1" && !p.core) {
            continue;
        }
        if !p.core && !can_list_non_core {
            continue;
        }
        if !type_filter.is_empty() && type_filter != p.kind {
            continue;
        }
        if p.alpha && !plugins.enable_alpha {
            continue;
        }
        if p.builtin {
            continue;
        }
        if enabled_filter == "1" && !p.enabled {
            continue;
        }
        if out.len() > 1 {
            out.push(',');
        }
        out.push_str(&p.list_head);
        out.push_str(ac_meta);
        out.push_str(",\"angularDetected\":false}");
    }
    out.push(']');
    Ok(Response::json_text(200, out))
}

/// `GET /api/plugins/errors`: plugins skipped for a signature problem.
fn get_plugin_errors(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    Ok(Response::json_text(200, c.app.plugins.errors_json.clone()))
}

/// `GET /api/plugins/:pluginId/settings` (GetPluginSettingByID).
fn get_plugin_settings(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let plugins = &c.app.plugins;
    let id = c.param("pluginId");
    if let Some(msg) = plugins.error_for(id) {
        return Err(ApiError::with_trace(500, msg));
    }
    let p = match plugins.get(id) {
        Some(p) => p,
        None => return Err(ApiError::new(404, "Plugin not found, no installed plugin with that id")),
    };
    // App plugins need plugins.app:access, which every basic role from Viewer up has.
    if p.kind == "app" && !c.user.has_role(Role::Viewer) {
        return Err(ApiError::new(403, "Access Denied"));
    }
    Ok(Response::json_text(200, p.settings_json.clone()))
}

/// `POST /api/plugins/:pluginId/settings` (UpdatePluginSetting). Plugin settings are static in grafana-rs: a request
/// that keeps the current state answers as Grafana does; any change is rejected.
fn update_plugin_settings(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let cmd = c.body_json()?;
    if !matches!(cmd, json::Json::Obj(_)) {
        return Err(ApiError::bad_request("bad request data"));
    }
    let p = match c.app.plugins.get(c.param("pluginId")) {
        Some(p) => p,
        None => return Err(ApiError::new(404, "Plugin not installed")),
    };
    let enabled = cmd.bool_of("enabled");
    if p.auto_enabled && !enabled {
        return Err(ApiError::new(400, "Cannot disable auto-enabled plugin"));
    }
    // The state GET .../settings shows: apps enabled and pinned when autoEnabled, other plugins false.
    let current = p.kind == "app" && p.auto_enabled;
    let empty = |k: &str| match cmd.get(k) {
        None | Some(json::Json::Null) => true,
        Some(json::Json::Obj(o)) => o.is_empty(),
        _ => false,
    };
    if enabled == current && cmd.bool_of("pinned") == current && empty("jsonData") && empty("secureJsonData") {
        return Ok(Response::json(200, &json::Json::obj().with("message", "Plugin settings updated")));
    }
    Err(ApiError::new(400, "Plugin settings are read-only in grafana-rs"))
}

/// `GET /api/plugins/:pluginId/markdown/:name` (GetPluginMarkdown): NAME.md (or name.md) from the plugin directory,
/// README.md when that is missing or empty.
fn get_plugin_markdown(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let p = match c.app.plugins.get(c.param("pluginId")) {
        Some(p) => p,
        None => return Err(ApiError::new(404, "Plugin not installed")),
    };
    let name = c.param("name").to_uppercase();
    // mdFilepath: a name with an extension (even ".md") is ErrUnexpectedFileExtension.
    let base = name.rsplit('/').next().unwrap_or("");
    if base.contains('.') {
        return Err(ApiError::with_trace(500, "Could not get markdown file"));
    }
    let mut content = read_md(&p.dir, &format!("{name}.md"));
    if content.is_empty() {
        content = read_md(&p.dir, "README.md");
    }
    Ok(Response::text(200, "text/plain; charset=utf-8", content))
}

/// A file of the plugin directory, tried as given then lower case; empty when missing or not a plain relative path.
fn read_md(dir: &std::path::Path, file: &str) -> Vec<u8> {
    if file.starts_with('/') || file.split('/').any(|s| s == "..") || file.contains('\\') {
        return Vec::new();
    }
    match std::fs::read(dir.join(file)) {
        Ok(b) => b,
        Err(_) => std::fs::read(dir.join(file.to_lowercase())).unwrap_or_default(),
    }
}

/// `/api/gnet/*` (ProxyGnetRequest): grafana.com cannot be reached from grafana-rs, so every request ends as
/// Grafana's reverse proxy ends when the upstream fails: 502 with an empty body. The plugin catalog handles it
/// ("GCOM is not available") and shows the installed plugins only.
fn proxy_gnet(_c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    Ok(Response::new(502))
}
