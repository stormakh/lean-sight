//! Helpers for the Kubernetes-style `/apis/<group>/<version>/namespaces/<ns>/...` routes (dashboards, folders):
//! namespaces, `Status` errors, object metadata, managed fields, ids. Upstream: pkg/apimachinery/utils,
//! k8s.io/apimachinery, pkg/storage/unified/apistore.
use crate::ctx::Ctx;
use crate::error::ApiError;
use http::Response;
use json::Json;
use std::sync::atomic::{AtomicI64, Ordering};

pub const ANNO_CREATED_BY: &str = "grafana.app/createdBy";
pub const ANNO_UPDATED_BY: &str = "grafana.app/updatedBy";
pub const ANNO_UPDATED_TS: &str = "grafana.app/updatedTimestamp";
pub const ANNO_FOLDER: &str = "grafana.app/folder";
pub const ANNO_MESSAGE: &str = "grafana.app/message";
pub const ANNO_GRANT: &str = "grafana.app/grant-permissions";
pub const LABEL_INTERNAL_ID: &str = "grafana.app/deprecatedInternalID";

/// Namespace of an org: "default" for org 1, "org-<id>" otherwise.
pub fn namespace_for(org_id: i64) -> String {
    if org_id == 1 { "default".to_string() } else { format!("org-{org_id}") }
}

/// Org of a namespace, or None when it is not an org namespace.
pub fn org_of_namespace(ns: &str) -> Option<i64> {
    if ns == "default" {
        return Some(1);
    }
    ns.strip_prefix("org-").and_then(|n| n.parse::<i64>().ok())
}

/// Checks that `:ns` is the signed-in org's namespace (403 Status otherwise, as the apiserver authorizer).
pub fn check_namespace(c: &Ctx<'_>) -> Result<(), ApiError> {
    let ns = c.param("ns");
    match org_of_namespace(ns) {
        Some(o) if o == c.user.org_id => Ok(()),
        _ => Err(status_error(403, "Forbidden", &format!("namespace {ns} is not the signed-in org"), None)),
    }
}

/// A Kubernetes `Status` failure. `details` is (name, group, kind).
pub fn status_error(code: u16, reason: &str, message: &str, details: Option<(&str, &str, &str)>) -> ApiError {
    let mut s = Json::obj().with("kind", "Status").with("apiVersion", "v1").with("metadata", Json::obj()).with("status", "Failure").with("message", message).with("reason", reason);
    if let Some((name, group, kind)) = details {
        s.set("details", Json::obj().with("name", name).with("group", group).with("kind", kind));
    }
    s.set("code", code as i64);
    ApiError::raw(code, s)
}

/// `<kind>.<group> "<name>" not found`.
pub fn not_found(group: &str, kind: &str, name: &str) -> ApiError {
    status_error(404, "NotFound", &format!("{kind}.{group} \"{name}\" not found"), Some((name, group, kind)))
}

/// A Kubernetes `Status` success (DELETE answers).
pub fn status_success(name: &str, group: &str, kind: &str, uid: &str) -> Response {
    let s = Json::obj()
        .with("kind", "Status")
        .with("apiVersion", "v1")
        .with("metadata", Json::obj())
        .with("status", "Success")
        .with("details", Json::obj().with("name", name).with("group", group).with("kind", kind).with("uid", uid));
    Response::json(200, &s)
}

/// An object as the apiserver writes it (indented, like Grafana's responses).
pub fn object_response(status: u16, v: &Json) -> Response {
    let mut t = v.stringify_pretty("  ");
    t.push('\n');
    Response::json_text(status, t)
}

static LAST_RV: AtomicI64 = AtomicI64::new(0);

/// A resource version: write time in microseconds, strictly increasing.
pub fn next_rv() -> i64 {
    let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_micros() as i64,
        Err(_) => 0,
    };
    let mut prev = LAST_RV.load(Ordering::Relaxed);
    loop {
        let next = if now > prev { now } else { prev + 1 };
        match LAST_RV.compare_exchange(prev, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(p) => prev = p,
        }
    }
}

static SNOW_SEQ: AtomicI64 = AtomicI64::new(0);

/// A Grafana internal id: a snowflake (bwmarrin epoch 1288834974657, random 10-bit node, 12-bit step) masked to
/// 52 bits, as `apistore` assigns `grafana.app/deprecatedInternalID`.
pub fn snowflake_id() -> i64 {
    let mut r = [0u8; 2];
    crypto::random_bytes(&mut r);
    let ms = (crate::util::now_ms() - 1288834974657).max(0);
    let node = ((r[0] as i64) << 8 | r[1] as i64) & 0x3ff;
    let seq = SNOW_SEQ.fetch_add(1, Ordering::Relaxed) & 0xfff;
    ((ms << 22) | (node << 12) | seq) & ((1i64 << 52) - 1)
}

/// A random RFC 4122 version 4 UUID (Kubernetes `metadata.uid`).
pub fn uuid_v4() -> String {
    let mut b = [0u8; 16];
    crypto::random_bytes(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h = crypto::hex(&b);
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// `generateName` suffix: 5 characters of the apiserver's alphabet (k8s.io/apimachinery/pkg/util/rand.String).
pub fn generate_name(prefix: &str) -> String {
    const A: &[u8] = b"bcdfghjklmnpqrstvwxz2456789";
    let mut r = [0u8; 5];
    crypto::random_bytes(&mut r);
    let mut s = String::with_capacity(prefix.len() + 5);
    s.push_str(prefix);
    for x in r.iter() {
        s.push(A[(*x as usize) % A.len()] as char);
    }
    s
}

/// Sorts object keys at every depth (Go marshals `map[string]any` with sorted keys).
pub fn sort_keys(v: &mut Json) {
    match v {
        Json::Obj(o) => {
            json::sort::sort_by(o, |a, b| a.0.cmp(&b.0));
            for e in o.iter_mut() {
                sort_keys(&mut e.1);
            }
        }
        Json::Arr(a) => {
            for x in a.iter_mut() {
                sort_keys(x);
            }
        }
        _ => {}
    }
}

/// The FieldsV1 set of a JSON value: `{"f:<key>": {}}` per key, recursing into non-empty objects with `".": {}`.
pub fn fields_of(v: &Json) -> Json {
    let mut out = Json::obj();
    if let Some(o) = v.as_obj() {
        let mut keys: Vec<&(String, Json)> = o.iter().collect();
        json::sort::sort_by(&mut keys, |a, b| a.0.cmp(&b.0));
        for e in keys {
            let child = match &e.1 {
                Json::Obj(c) if !c.is_empty() => {
                    let mut f = Json::obj().with(".", Json::obj());
                    if let Json::Obj(inner) = fields_of(&e.1) {
                        for x in inner {
                            f.set(&x.0, x.1);
                        }
                    }
                    f
                }
                _ => Json::obj(),
            };
            out.set(&format!("f:{}", e.0), child);
        }
    }
    out
}

/// Union of two FieldsV1 sets (sorted keys).
pub fn fields_union(a: &Json, b: &Json) -> Json {
    let mut out = a.clone();
    if let (Some(_), Some(bo)) = (a.as_obj(), b.as_obj()) {
        for e in bo.iter() {
            let merged = match out.get(&e.0) {
                Some(prev) => fields_union(prev, &e.1),
                None => e.1.clone(),
            };
            out.set(&e.0, merged);
        }
        sort_keys(&mut out);
    }
    out
}

/// Adds or merges a managed-fields entry (same manager, operation and apiVersion: fields are merged, time replaced).
pub fn merge_managed(list: &mut Json, manager: &str, api_version: &str, time: &str, fields: Json) {
    if let Some(a) = list.as_arr_mut() {
        for e in a.iter_mut() {
            if e.str_of("manager") == manager && e.str_of("operation") == "Update" && e.str_of("apiVersion") == api_version {
                let prev = e.get("fieldsV1").cloned().unwrap_or_else(Json::obj);
                e.set("time", time);
                e.set("fieldsV1", fields_union(&prev, &fields));
                return;
            }
        }
        a.push(
            Json::obj()
                .with("manager", manager)
                .with("operation", "Update")
                .with("apiVersion", api_version)
                .with("time", time)
                .with("fieldsType", "FieldsV1")
                .with("fieldsV1", fields),
        );
    }
}

/// The field manager of a request: the user agent up to the first '/', as the apiserver names it.
pub fn manager_of(c: &Ctx<'_>) -> String {
    let ua = c.req.header("user-agent").unwrap_or("");
    let m = ua.split('/').next().unwrap_or("").trim();
    if m.is_empty() { "unknown".to_string() } else { m.to_string() }
}

/// Identity string of the signed-in user for createdBy/updatedBy annotations ("user:<uid>", "anonymous:0").
pub fn identity_of(c: &Ctx<'_>) -> String {
    if c.user.is_anonymous || c.user.user_id == 0 {
        "anonymous:0".to_string()
    } else if c.user.is_service_account {
        format!("service-account:{}", c.user.uid)
    } else {
        format!("user:{}", c.user.uid)
    }
}

/// A query parameter as Kubernetes reads `labelSelector`/`fieldSelector` terms: `key=value` pairs split on ','.
pub fn selector_value<'a>(sel: &'a str, key: &str) -> Option<&'a str> {
    for part in sel.split(',') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix(key) {
            if let Some(v) = rest.strip_prefix("==") {
                return Some(v);
            }
            if let Some(v) = rest.strip_prefix('=') {
                return Some(v);
            }
        }
    }
    None
}
