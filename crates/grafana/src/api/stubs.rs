//! Out-of-scope routes answered with an empty or disabled result of the recorded shape (alerting, live, advisor,
//! provisioning, quotas, correlations, frontend metrics). Bodies are constant: built once and copied per request.
use crate::ctx::{Access, Ctx, Role, Routes};
use crate::error::ApiError;
use http::Response;
use json::Json;
use std::sync::OnceLock;

pub fn routes(r: &mut Routes) {
    // ---- alerting (upstream pkg/services/ngalert/api): the read routes the dashboards and Explore call ----
    // ngalert checks RBAC alert.rules:read / alert.instances:read, which every basic role from Viewer up has.
    let viewer = Access::Role(Role::Viewer);
    r.get("/api/alertmanager/grafana/api/v2/alerts", viewer, empty_array);
    r.get("/api/alertmanager/grafana/api/v2/alerts/groups", viewer, empty_array);
    r.get("/api/alertmanager/grafana/api/v2/silences", viewer, empty_array);
    r.get("/api/prometheus/grafana/api/v1/rules", viewer, prom_rules);
    r.get("/api/prometheus/grafana/api/v1/alerts", viewer, prom_alerts);
    r.get("/api/ruler/grafana/api/v1/rules", viewer, empty_object);
    r.get("/api/v1/ngalert", viewer, ngalert_status);

    // ---- live: no websocket support. Grafana's centrifuge handler answers a GET without an upgrade this way ----
    r.get("/api/live/ws", Access::SignedIn, live_ws);

    // ---- frontend metrics (pkg/api/frontend_metrics.go) ----
    r.post("/api/frontend-metrics", Access::SignedIn, frontend_metrics);

    // ---- correlations (pkg/services/correlations/api.go): none exist ----
    r.get("/api/datasources/correlations", viewer, correlations);

    // ---- advisor app (apps/advisor): no checks have run; the check types are the built-in ones ----
    let admin = Access::Role(Role::Admin);
    r.get("/apis/advisor.grafana.app/v0alpha1/namespaces/:ns/checks", admin, advisor_checks);
    r.get("/apis/advisor.grafana.app/v0alpha1/namespaces/:ns/checktypes", admin, advisor_checktypes);
    r.get("/apis/advisor.grafana.app/v0alpha1/namespaces/:ns/checktypes/:name", admin, advisor_checktype);

    // ---- provisioning (pkg/registry/apis/provisioning): settings:read is granted to Viewer ----
    r.get("/apis/provisioning.grafana.app/v0alpha1/namespaces/:ns/settings", viewer, provisioning_settings);

    // ---- quotas: Grafana itself answers 501 here without an overrides service ----
    r.get("/apis/quotas.grafana.app/v0alpha1/namespaces/:ns/usage", Access::SignedIn, quotas_usage);
}

fn json_static(status: u16, body: &'static str) -> Result<Response, ApiError> {
    Ok(Response::text(status, "application/json", body.as_bytes()))
}

fn empty_array(_c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    json_static(200, "[]")
}

fn empty_object(_c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    json_static(200, "{}")
}

fn prom_rules(_c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    json_static(200, r#"{"status":"success","data":{"groups":[]}}"#)
}

fn prom_alerts(_c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    json_static(200, r#"{"status":"success","data":{"alerts":[]}}"#)
}

fn ngalert_status(_c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    json_static(200, r#"{"alertmanagersChoice":"internal","numExternalAlertmanagers":0}"#)
}

/// Grafana answers a plain GET with 400 "Bad Request" (gorilla websocket upgrader). We also answer an upgrade
/// request this way: there is no websocket support. Boot data must send `liveEnabled: false` (Grafana's value for
/// `[live] max_connections = 0`) so the UI does not dial; if it dials anyway, centrifuge retries with backoff.
fn live_ws(_c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    Ok(Response::text(400, "text/plain; charset=utf-8", &b"Bad Request\n"[..]).with_header("Sec-Websocket-Version", "13"))
}

/// The metrics are not recorded here; the body is only checked to be `{"events": [...]}` JSON as Grafana binds it.
fn frontend_metrics(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    match c.req.json() {
        Ok(Json::Obj(_)) => Ok(Response::new(200)),
        _ => Err(ApiError::bad_request("bad request data")),
    }
}

/// Empty page; `page` and `limit` echo the query as upstream (limit 1..1000, default 100; page default 1).
fn correlations(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let mut limit = c.req.query_get("limit").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    if limit <= 0 {
        limit = 100;
    } else if limit > 1000 {
        limit = 1000;
    }
    let mut page = c.req.query_get("page").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    if page <= 0 {
        page = 1;
    }
    let mut s = String::with_capacity(96);
    s.push_str(r#"{"correlations":[],"totalCount":0,"page":"#);
    json::write_i64(page, &mut s);
    s.push_str(r#","limit":"#);
    json::write_i64(limit, &mut s);
    s.push_str(r#","doesContinue":null}"#);
    Ok(Response::json_text(200, s))
}

fn advisor_checks(_c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    json_static(
        200,
        "{\n  \"kind\": \"CheckList\",\n  \"apiVersion\": \"advisor.grafana.app/v0alpha1\",\n  \"metadata\": {\n    \"resourceVersion\": \"1\"\n  },\n  \"items\": []\n}",
    )
}

/// The check types of Grafana 13.3.1, captured from a real instance (`captured/advisor-checktypes.json`). Grafana
/// registers them at startup, so `pub fn init` (called by `App::new`) gives each a new uid, the startup time, and a
/// resourceVersion of the startup time in microseconds (with the capture's offsets between the objects).
pub fn init() {
    checktypes();
}

fn checktypes() -> &'static (String, Vec<(String, String)>) {
    static CT: OnceLock<(String, Vec<(String, String)>)> = OnceLock::new();
    CT.get_or_init(|| {
        let mut list = Json::parse(include_str!("captured/advisor-checktypes.json")).unwrap_or(Json::Null);
        let now_ms = crate::util::now_ms();
        let start_us = now_ms * 1000;
        let ts = crate::util::rfc3339(now_ms / 1000);
        let rv_of = |m: &Json| m.str_of("resourceVersion").parse::<i64>().unwrap_or(0);
        let mut min_rv = i64::MAX;
        if let Some(items) = list.get("items").and_then(|v| v.as_arr()) {
            for it in items.iter() {
                if let Some(m) = it.get("metadata") {
                    min_rv = min_rv.min(rv_of(m));
                }
            }
        }
        let rv = |old: i64| (start_us + old - min_rv).to_string();
        if let Some(m) = list.get_mut("metadata") {
            let v = rv(rv_of(m));
            m.set("resourceVersion", v);
        }
        let mut one = Vec::new();
        if let Some(Json::Arr(items)) = list.get_mut("items") {
            for it in items.iter_mut() {
                if let Some(m) = it.get_mut("metadata") {
                    let v = rv(rv_of(m));
                    m.set("uid", crate::k8s::uuid_v4());
                    m.set("resourceVersion", v);
                    m.set("creationTimestamp", ts.as_str());
                    if let Some(Json::Arr(mf)) = m.get_mut("managedFields") {
                        for f in mf.iter_mut() {
                            f.set("time", ts.as_str());
                        }
                    }
                }
                let name = it.get("metadata").map(|m| m.str_of("name")).unwrap_or("");
                one.push((name.to_string(), it.stringify_pretty("  ")));
            }
        }
        (list.stringify_pretty("  "), one)
    })
}

fn advisor_checktypes(_c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    Ok(Response::json_text(200, checktypes().0.clone()))
}

fn advisor_checktype(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let name = c.param("name");
    for (n, body) in checktypes().1.iter() {
        if n == name {
            return Ok(Response::json_text(200, body.clone()));
        }
    }
    let status = Json::obj()
        .with("kind", "Status")
        .with("apiVersion", "v1")
        .with("metadata", Json::obj())
        .with("status", "Failure")
        .with("message", format!("checktypes.advisor.grafana.app \"{name}\" not found"))
        .with("reason", "NotFound")
        .with("details", Json::obj().with("name", name).with("group", "advisor.grafana.app").with("kind", "checktypes"))
        .with("code", 404);
    Ok(Response::json_text(404, status.stringify_pretty("  ")))
}

fn provisioning_settings(_c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    json_static(
        200,
        concat!(
            r#"{"allowedTargets":["folder","folderless"],"allowImageRendering":true,"maxRepositories":10,"#,
            r#""availableRepositoryTypes":["git","github","local"],"availableConnectionTypes":["github"],"#,
            r#""availableResources":[{"group":"folder.grafana.app","kind":"Folder"},{"group":"dashboard.grafana.app","kind":"Dashboard"},"#,
            r#"{"group":"dashboard.grafana.app","kind":"LibraryPanel","disabled":true},{"group":"playlist.grafana.app","kind":"Playlist","disabled":true}],"#,
            r#""items":[]}"#,
            "\n"
        ),
    )
}

fn quotas_usage(_c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    json_static(501, "{\"metadata\":{},\"status\":\"Failure\",\"message\":\"overrides service not configured on resource server\",\"code\":501}\n")
}
