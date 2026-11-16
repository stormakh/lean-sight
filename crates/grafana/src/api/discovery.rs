//! Kubernetes API discovery: `GET /apis` (APIGroupList), `/apis/<group>` (APIGroup) and `/apis/<group>/<version>`
//! (APIResourceList). The groups, versions and resources are those Grafana 13.3.1 serves, captured from a real
//! instance into `captured/discovery.json`. The bodies are built once, pretty-printed like the apiserver's answers.
use crate::ctx::{Access, Ctx, Routes};
use crate::error::ApiError;
use http::Response;
use json::Json;
use std::sync::OnceLock;

pub fn routes(r: &mut Routes) {
    r.get("/apis", Access::SignedIn, group_list);
    r.get("/apis/:group", Access::SignedIn, group);
    r.get("/apis/:group/:version", Access::SignedIn, resource_list);
}

struct Discovery {
    list: String,
    /// (group name, APIGroup body)
    groups: Vec<(String, String)>,
    /// ("group/version", APIResourceList body)
    resources: Vec<(String, String)>,
    version_not_found: String,
}

fn discovery(port: u16) -> &'static Discovery {
    static D: OnceLock<Discovery> = OnceLock::new();
    D.get_or_init(|| {
        let data = Json::parse(include_str!("captured/discovery.json")).unwrap_or(Json::Null);
        let mut list_groups = Json::arr();
        let mut groups = Vec::new();
        let addr = format!("0.0.0.0:{port}");
        if let Some(gs) = data.get("groups").and_then(|v| v.as_arr()) {
            for g in gs.iter() {
                let name = g.str_of("name");
                let versions = g.get("versions").cloned().unwrap_or(Json::arr());
                let preferred = g.get("preferredVersion").cloned().unwrap_or(Json::obj());
                let one = Json::obj()
                    .with("kind", "APIGroup")
                    .with("apiVersion", "v1")
                    .with("name", name)
                    .with("versions", versions.clone())
                    .with("preferredVersion", preferred.clone());
                groups.push((name.to_string(), one.stringify_pretty("  ")));
                let cidr = Json::obj().with("clientCIDR", "0.0.0.0/0").with("serverAddress", addr.as_str());
                list_groups.push(
                    Json::obj()
                        .with("name", name)
                        .with("versions", versions)
                        .with("preferredVersion", preferred)
                        .with("serverAddressByClientCIDRs", Json::arr().push_item(cidr)),
                );
            }
        }
        let list = Json::obj().with("kind", "APIGroupList").with("groups", list_groups).stringify_pretty("  ");
        let mut resources = Vec::new();
        if let Some(rs) = data.get("resources").and_then(|v| v.as_obj()) {
            for (gv, res) in rs.iter() {
                let body = Json::obj()
                    .with("kind", "APIResourceList")
                    .with("apiVersion", "v1")
                    .with("groupVersion", gv.as_str())
                    .with("resources", res.clone());
                resources.push((gv.clone(), body.stringify_pretty("  ")));
            }
        }
        let version_not_found = Json::obj()
            .with("kind", "Status")
            .with("apiVersion", "v1")
            .with("metadata", Json::obj())
            .with("status", "Failure")
            .with("message", "the server could not find the requested resource")
            .with("reason", "NotFound")
            .with("details", Json::obj())
            .with("code", 404)
            .stringify_pretty("  ");
        Discovery { list, groups, resources, version_not_found }
    })
}

/// An unknown group: the apiserver's mux answers with Go's plain 404.
fn page_not_found() -> Response {
    Response::text(404, "text/plain; charset=utf-8", &b"404 page not found\n"[..])
}

fn group_list(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    Ok(Response::json_text(200, discovery(c.app.cfg.http_port).list.clone()))
}

fn group(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let d = discovery(c.app.cfg.http_port);
    let name = c.param("group");
    for (g, body) in d.groups.iter() {
        if g == name {
            return Ok(Response::json_text(200, body.clone()));
        }
    }
    Ok(page_not_found())
}

fn resource_list(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let d = discovery(c.app.cfg.http_port);
    let group = c.param("group");
    let version = c.param("version");
    for (gv, body) in d.resources.iter() {
        if gv.len() == group.len() + 1 + version.len() && gv.starts_with(group) && gv.ends_with(version) && gv.as_bytes()[group.len()] == b'/' {
            return Ok(Response::json_text(200, body.clone()));
        }
    }
    if d.groups.iter().any(|g| g.0 == group) {
        return Ok(Response::json_text(404, d.version_not_found.clone()));
    }
    Ok(page_not_found())
}
