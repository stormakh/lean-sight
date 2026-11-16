//! The Prometheus data source plugin backend in Rust: a port of `pkg/promlib` from grafana-prometheus-datasource
//! v13.2.6 (`upstream-prometheus/`), the plugin Grafana 13.3.1 installs. It answers `/api/ds/query`, the health
//! check and the resource calls in process (no plugin process, no gRPC/Arrow hop); the JSON it writes is what
//! Grafana writes after that hop.
pub mod client;
pub mod converter;
pub mod exemplar;
pub mod fast;
pub mod frame;
pub mod jsonpull;
pub mod models;
pub mod querydata;
pub mod settings;

use client::{HttpReq, HttpResp, Instance, Transport};
use json::Json;

/// `healthcheck` + `getHeuristics`: (ok, message, details). Upstream `CheckHealth`.
pub fn check_health(inst: &Instance, t: Transport<'_>) -> (bool, String, Option<Json>) {
    let q = models::DataQuery {
        ref_id: "__healthcheck__".to_string(),
        from: models::SEC,
        to: 4 * models::SEC,
        json: Json::obj().with("refId", "__healthcheck__").with("expr", "1+1").with("instant", true),
        ..Default::default()
    };
    let res = querydata::execute(inst, std::slice::from_ref(&q), t, false);
    let err = res.into_iter().find(|r| r.0 == "__healthcheck__").and_then(|r| r.1.error);
    let (ok, message) = match err {
        None => (true, "Successfully queried the Prometheus API.".to_string()),
        Some(e) => (false, format!("{e} - There was an error returned querying the Prometheus API.")),
    };
    let details = heuristics(inst, t);
    (ok, message, details)
}

/// `getHeuristics`: Prometheus when buildinfo has no `features`, else Mimir with the ruler API.
pub fn heuristics(inst: &Instance, t: Transport<'_>) -> Option<Json> {
    let req = inst.resource_request("GET", "api/v1/status/buildinfo", "", &[]);
    let res = t(&req).ok()?;
    if res.status != 200 {
        return None;
    }
    let v = Json::parse_bytes(&res.body).ok()?;
    let features = match v.path("data.features") {
        Some(Json::Obj(o)) => o.len(),
        _ => 0,
    };
    let (app, ruler) = if features == 0 { ("Prometheus", false) } else { ("Mimir", true) };
    Some(Json::obj().with("application", app).with("features", Json::obj().with("rulerApiEnabled", ruler)))
}

/// Resource paths the plugin forwards (`resource/allowlist.go`).
pub fn is_allowed_resource_path(path: &str) -> bool {
    let p = path.strip_prefix('/').unwrap_or(path);
    if p.is_empty() || p.contains('%') {
        return false;
    }
    if p.split('/').any(|s| s == "." || s == "..") {
        return false;
    }
    match p {
        "api/v1/query" | "api/v1/query_range" | "api/v1/query_exemplars" | "api/v1/series" | "api/v1/labels" | "api/v1/metadata" | "api/v1/rules" | "api/v1/status/buildinfo" => true,
        "api/v1/search/metric_names" | "api/v1/search/label_names" | "api/v1/search/label_values" => true,
        _ => match p.strip_prefix("api/v1/label/").and_then(|r| r.strip_suffix("/values")) {
            Some(name) => !name.is_empty() && !name.contains('/'),
            None => false,
        },
    }
}

/// A resource response (`Resource.Execute`): status, headers to forward, body.
pub struct ResourceResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// `Resource.Execute` for a path that is not `suggestions`. `cache` is the caller's `X-Grafana-Cache` header:
/// when set, the answer carries `X-Grafana-Cache: y` and `Cache-Control: <cache>`.
pub fn call_resource(inst: &Instance, method: &str, path: &str, raw_query: &str, body: &[u8], cache: Option<&str>, t: Transport<'_>) -> Result<ResourceResponse, String> {
    if !is_allowed_resource_path(path) {
        return Ok(ResourceResponse {
            status: 403,
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: b"{\"message\":\"the requested resource path is not supported by this datasource\"}".to_vec(),
        });
    }
    let req: HttpReq = inst.resource_request(method, path, raw_query, body);
    let res: HttpResp = match t(&req) {
        Ok(r) => r,
        Err(e) => return Err(format!("error querying resource: {}", e.message)),
    };
    let mut headers = Vec::new();
    for (k, v) in res.headers {
        match k.as_str() {
            "content-encoding" | "content-length" | "transfer-encoding" | "connection" | "keep-alive" | "date" => {}
            "x-grafana-cache" | "cache-control" if cache.map(|c| !c.is_empty()).unwrap_or(false) => {}
            _ => headers.push((k, v)),
        }
    }
    if let Some(c) = cache {
        if !c.is_empty() {
            headers.push(("X-Grafana-Cache".to_string(), "y".to_string()));
            headers.push(("Cache-Control".to_string(), c.to_string()));
        }
    }
    Ok(ResourceResponse { status: res.status, headers, body: res.body })
}
