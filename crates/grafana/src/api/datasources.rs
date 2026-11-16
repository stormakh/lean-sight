//! Data sources CRUD, health, resources, the data source proxy, and `/api/ds/query` dispatch (agent C).
//! Upstream: pkg/api/datasources.go, pkg/api/ds_query.go, pkg/services/query/query.go,
//! pkg/services/datasources/service, pkg/api/plugin_resource.go.
//!
//! RBAC as OSS defaults: read, query and id-read are Viewer; create, write and delete are Admin.
//! The numeric-id routes (`/api/datasources/:id...`) are not registered: Grafana 13.3.1 only adds them with the
//! `datasourceLegacyIdApi` feature flag, which is off by default.
use crate::app::App;
use crate::ctx::{Access, Ctx, Role, Routes};
use crate::error::ApiError;
use crate::promlib::client::{HttpReq, HttpResp, Instance, TransportError};
use crate::promlib::frame::{DataResponse, write_sorted};
use crate::promlib::models::{DataQuery, MS};
use crate::promlib::{self, settings};
use crate::store::{DataSource, StoreError};
use http::Response;
use json::Json;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub fn routes(r: &mut Routes) {
    let v = Access::Role(Role::Viewer);
    let a = Access::Role(Role::Admin);
    r.get("/api/datasources", v, list);
    r.post("/api/datasources", a, add);
    r.get("/api/datasources/uid/:uid", v, get_by_uid);
    r.put("/api/datasources/uid/:uid", a, update_by_uid);
    r.delete("/api/datasources/uid/:uid", a, delete_by_uid);
    r.any("/api/datasources/uid/:uid/health", v, health_by_uid);
    r.any("/api/datasources/uid/:uid/resources", v, resource_by_uid);
    r.any("/api/datasources/uid/:uid/resources/*rest", v, resource_by_uid);
    r.any("/api/datasources/proxy/uid/:uid", v, proxy_by_uid);
    r.any("/api/datasources/proxy/uid/:uid/*rest", v, proxy_by_uid);
    r.get("/api/datasources/name/:name", v, get_by_name);
    r.delete("/api/datasources/name/:name", a, delete_by_name);
    r.get("/api/datasources/id/:name", v, id_by_name);
    r.post("/api/ds/query", v, ds_query);
    // correlations are out of scope: the empty page the UI expects
    r.get("/api/datasources/correlations", v, correlations);
}

fn correlations(_c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    Ok(Response::json(200, &Json::obj().with("correlations", Json::arr()).with("totalCount", 0).with("page", 1).with("limit", 100).with("doesContinue", Json::Null)))
}

// ---------------------------------------------------------------- DTOs

/// (type name, logo) of a data source plugin type, as the plugin store gives them.
fn plugin_meta(app: &App, typ: &str) -> (String, String) {
    if typ == "prometheus" {
        return ("Prometheus".to_string(), "public/plugins/prometheus/img/prometheus_logo.svg".to_string());
    }
    // other installed or core plugins: plugin.json name and small logo
    let candidates = [app.cfg.plugins_dir().join(typ).join("plugin.json"), app.cfg.home.join("public/app/plugins/datasource").join(typ).join("plugin.json")];
    for (i, p) in candidates.iter().enumerate() {
        if typ.contains('/') || typ.contains("..") || typ.is_empty() {
            break;
        }
        if let Ok(t) = std::fs::read_to_string(p) {
            if let Ok(j) = Json::parse(&t) {
                if j.str_of("type") == "datasource" {
                    let logo = j.path("info.logos.small").and_then(|v| v.as_str()).unwrap_or("");
                    let base = if i == 0 { format!("public/plugins/{typ}/") } else { format!("public/app/plugins/datasource/{typ}/") };
                    return (j.str_of("name").to_string(), if logo.is_empty() { "public/img/icn-datasource.svg".to_string() } else { format!("{base}{logo}") });
                }
            }
        }
    }
    (typ.to_string(), "public/img/icn-datasource.svg".to_string())
}

fn json_data_value(ds: &DataSource) -> Json {
    if ds.json_data.trim().is_empty() {
        return Json::obj();
    }
    Json::parse(&ds.json_data).unwrap_or_else(|_| Json::obj())
}

/// jsonData as Go's simplejson writes it (map keys sorted, recursively).
fn sorted(v: &Json) -> Json {
    match v {
        Json::Obj(o) => {
            let mut items: Vec<(String, Json)> = o.iter().map(|(k, x)| (k.clone(), sorted(x))).collect();
            items.sort_by(|a, b| a.0.cmp(&b.0));
            Json::Obj(items)
        }
        Json::Arr(a) => Json::Arr(a.iter().map(sorted).collect()),
        _ => v.clone(),
    }
}

/// `convertModelToDtos` (dtos.DataSource).
fn dto(c: &Ctx<'_>, ds: &DataSource, access_control: Option<Json>) -> Json {
    let (_, logo) = plugin_meta(c.app, &ds.type_);
    let secure = crate::secrets::decrypt_map(&c.app.cfg.secret_key, &ds.secure_json_data);
    let mut fields = Json::obj();
    let mut keys: Vec<String> = Vec::new();
    if let Some(o) = secure.as_obj() {
        for (k, v) in o.iter() {
            if !v.as_str().unwrap_or("").is_empty() {
                keys.push(k.clone());
            }
        }
    }
    keys.sort();
    for k in keys {
        fields.set(&k, true);
    }
    let mut d = Json::obj()
        .with("id", ds.id)
        .with("uid", ds.uid.as_str())
        .with("orgId", ds.org_id)
        .with("name", ds.name.as_str())
        .with("type", ds.type_.as_str())
        .with("typeLogoUrl", logo)
        .with("access", ds.access.as_str())
        .with("url", ds.url.as_str())
        .with("user", ds.user.as_str())
        .with("database", ds.database.as_str())
        .with("basicAuth", ds.basic_auth)
        .with("basicAuthUser", ds.basic_auth_user.as_str())
        .with("withCredentials", ds.with_credentials)
        .with("isDefault", ds.is_default)
        .with("jsonData", sorted(&json_data_value(ds)))
        .with("secureJsonFields", fields)
        .with("version", ds.version)
        .with("readOnly", ds.read_only);
    if let Some(ac) = access_control {
        d.set("accessControl", ac);
    }
    d.set("apiVersion", ds.api_version.as_str());
    d
}

/// `getAccessControlMetadata` for a data source, OSS role grants (keys sorted as Go writes the map).
fn access_control(c: &Ctx<'_>) -> Json {
    let r = c.user.org_role;
    let mut m = Json::obj();
    let viewer = r.includes(Role::Viewer);
    let editor = r.includes(Role::Editor);
    let admin = r.includes(Role::Admin);
    let items: [(&str, bool); 11] = [
        ("alert.instances.external:read", viewer),
        ("alert.instances.external:write", editor),
        ("alert.notifications.external:read", viewer),
        ("alert.notifications.external:write", editor),
        ("alert.rules.external:read", viewer),
        ("alert.rules.external:write", editor),
        ("datasources.id:read", viewer),
        ("datasources:delete", admin),
        ("datasources:query", viewer),
        ("datasources:read", viewer),
        ("datasources:write", admin),
    ];
    for (k, ok) in items.iter() {
        if *ok {
            m.set(k, true);
        }
    }
    m
}

fn not_found() -> ApiError {
    ApiError::new(404, "Data source not found")
}

/// An errutil error as written with unified logging: `{"statusCode","messageId","message"}`.
fn errutil(status: u16, id: &str, msg: &str) -> ApiError {
    ApiError::raw(status, Json::obj().with("statusCode", status as i64).with("messageId", id).with("message", msg))
}

// ---------------------------------------------------------------- CRUD

fn list(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let mut all = match c.app.store.datasources(c.user.org_id) {
        Ok(v) => v,
        Err(_) => return Err(ApiError::new(500, "Failed to query datasources")),
    };
    all.sort_by_key(|d| d.name.to_lowercase());
    let mut out = Json::arr();
    for ds in all.iter() {
        let (type_name, logo) = plugin_meta(c.app, &ds.type_);
        out.push(
            Json::obj()
                .with("id", ds.id)
                .with("uid", ds.uid.as_str())
                .with("orgId", ds.org_id)
                .with("name", ds.name.as_str())
                .with("type", ds.type_.as_str())
                .with("typeName", type_name)
                .with("typeLogoUrl", logo)
                .with("access", ds.access.as_str())
                .with("url", ds.url.as_str())
                .with("user", ds.user.as_str())
                .with("database", ds.database.as_str())
                .with("basicAuth", ds.basic_auth)
                .with("isDefault", ds.is_default)
                .with("jsonData", sorted(&json_data_value(ds)))
                .with("readOnly", ds.read_only),
        );
    }
    Ok(Response::json(200, &out))
}

fn lookup(c: &Ctx<'_>, r: Result<DataSource, StoreError>, other: &str) -> Result<DataSource, ApiError> {
    let _ = c;
    match r {
        Ok(d) => Ok(d),
        Err(StoreError::NotFound) => Err(not_found()),
        Err(_) => Err(ApiError::new(500, other)),
    }
}

fn get_by_uid(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let r = c.app.store.datasource_by_uid(c.user.org_id, c.param("uid"));
    let ds = lookup(c, r, "Failed to query datasource")?;
    let ac = if c.req.query_get("accesscontrol").map(|v| v == "true").unwrap_or(false) { Some(access_control(c)) } else { None };
    Ok(Response::json(200, &dto(c, &ds, ac)))
}

fn get_by_name(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let r = c.app.store.datasource_by_name(c.user.org_id, c.param("name"));
    let ds = lookup(c, r, "Failed to query datasources")?;
    Ok(Response::json(200, &dto(c, &ds, None)))
}

fn id_by_name(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let r = c.app.store.datasource_by_name(c.user.org_id, c.param("name"));
    let ds = lookup(c, r, "Failed to query datasources")?;
    Ok(Response::json(200, &Json::obj().with("id", ds.id)))
}

/// The body of a create or update command (`AddDataSourceCommand` / `UpdateDataSourceCommand`), decoded as
/// Go's encoding/json would (case-insensitive names, type errors fail the bind).
struct Cmd {
    name: String,
    typ: String,
    access: String,
    url: String,
    user: String,
    database: String,
    basic_auth: bool,
    basic_auth_user: String,
    with_credentials: bool,
    is_default: bool,
    json_data: Option<Json>,
    secure: Vec<(String, String)>,
    uid: String,
    api_version: String,
    version: i64,
}

fn bind_cmd(c: &Ctx<'_>, name_required: bool) -> Result<Cmd, ApiError> {
    let bad = || ApiError::bad_request("bad request data");
    let b = c.body_json()?;
    if b.as_obj().is_none() {
        return Err(bad());
    }
    let g = |k: &str| crate::promlib::models::get_ci(&b, k);
    let s = |k: &str| -> Result<String, ApiError> {
        match g(k) {
            None | Some(Json::Null) => Ok(String::new()),
            Some(Json::Str(v)) => Ok(v.clone()),
            Some(_) => Err(bad()),
        }
    };
    let bo = |k: &str| -> Result<bool, ApiError> {
        match g(k) {
            None | Some(Json::Null) => Ok(false),
            Some(Json::Bool(v)) => Ok(*v),
            Some(_) => Err(bad()),
        }
    };
    let json_data = match g("jsonData") {
        None | Some(Json::Null) => None,
        Some(v @ Json::Obj(_)) => Some(v.clone()),
        Some(_) => return Err(bad()),
    };
    let mut secure = Vec::new();
    match g("secureJsonData") {
        None | Some(Json::Null) => {}
        Some(Json::Obj(o)) => {
            for (k, v) in o.iter() {
                match v {
                    Json::Str(x) => secure.push((k.clone(), x.clone())),
                    Json::Null => secure.push((k.clone(), String::new())),
                    _ => return Err(bad()),
                }
            }
        }
        Some(_) => return Err(bad()),
    }
    let version = match g("version") {
        None | Some(Json::Null) => 0,
        Some(Json::Int(i)) => *i,
        Some(_) => return Err(bad()),
    };
    let cmd = Cmd {
        name: s("name")?,
        typ: s("type")?,
        access: s("access")?,
        url: s("url")?,
        user: s("user")?,
        database: s("database")?,
        basic_auth: bo("basicAuth")?,
        basic_auth_user: s("basicAuthUser")?,
        with_credentials: bo("withCredentials")?,
        is_default: bo("isDefault")?,
        json_data,
        secure,
        uid: s("uid")?,
        api_version: s("apiVersion")?,
        version,
    };
    // binding:"Required"
    if cmd.typ.is_empty() || cmd.access.is_empty() || (name_required && cmd.name.is_empty()) {
        return Err(bad());
    }
    Ok(cmd)
}

/// `validation.ValidateURL`: required for some types; otherwise it must parse as a URL.
fn validate_url(typ: &str, url: &str) -> Result<(), ApiError> {
    let required = matches!(typ, "graphite" | "influxdb" | "influxdb_08" | "elasticsearch" | "prometheus" | "grafana-amazonprometheus-datasource" | "grafana-azureprometheus-datasource" | "alertmanager" | "jaeger" | "loki" | "opentsdb" | "tempo" | "zipkin" | "mysql" | "grafana-postgresql-datasource" | "postgres" | "mssql");
    let bad = || ApiError::bad_request("Validation error, invalid URL");
    if required && url.trim().is_empty() {
        return Err(bad());
    }
    // url.Parse rejects control characters, spaces in the host and bad percent escapes
    if url.bytes().any(|b| b < 0x20 || b == 0x7f) {
        return Err(bad());
    }
    let b = url.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let ok = b.get(i + 1).map(|c| c.is_ascii_hexdigit()).unwrap_or(false) && b.get(i + 2).map(|c| c.is_ascii_hexdigit()).unwrap_or(false);
            if !ok {
                return Err(bad());
            }
        }
        i += 1;
    }
    Ok(())
}

fn add(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let mut cmd = bind_cmd(c, false)?;
    if !cmd.url.is_empty() {
        validate_url(&cmd.typ, &cmd.url)?;
    }
    if let Some(j) = &cmd.json_data {
        if crate::promlib::models::get_ci(j, "teamHttpHeaders").is_some() {
            return Err(ApiError::new(403, "Cannot create datasource with team HTTP headers, need to use updateDatasourceLBACRules API"));
        }
    }
    let org = c.user.org_id;
    let existing = c.app.store.datasources(org).map_err(|_| ApiError::new(500, "Failed to add datasource"))?;
    if existing.is_empty() {
        cmd.is_default = true;
    }
    if cmd.name.is_empty() {
        let mut name = cmd.typ.clone();
        let mut n = 0;
        while existing.iter().any(|d| d.name.to_lowercase() == name.to_lowercase()) {
            n += 1;
            name = format!("{}-{n}", cmd.typ);
        }
        cmd.name = name;
    }
    if existing.iter().any(|d| d.name == cmd.name) {
        return Err(ApiError::new(409, "data source with the same name already exists"));
    }
    if cmd.uid.is_empty() {
        cmd.uid = crate::util::short_uid();
    } else if !crate::util::valid_uid(&cmd.uid) {
        return Err(errutil(400, "datasource.uidInvalid", "Invalid datasource UID."));
    }
    if existing.iter().any(|d| d.uid == cmd.uid) {
        return Err(ApiError::new(409, "data source with the same uid already exists"));
    }
    let mut secure = Json::obj();
    for (k, v) in cmd.secure.iter() {
        secure.set(k, v.as_str());
    }
    let now = crate::util::now();
    let mut ds = DataSource {
        id: 0,
        uid: cmd.uid.clone(),
        org_id: org,
        version: 1,
        type_: cmd.typ.clone(),
        name: cmd.name.clone(),
        access: cmd.access.clone(),
        url: cmd.url.clone(),
        user: cmd.user.clone(),
        database: cmd.database.clone(),
        basic_auth: cmd.basic_auth,
        basic_auth_user: cmd.basic_auth_user.clone(),
        with_credentials: cmd.with_credentials,
        is_default: cmd.is_default,
        json_data: cmd.json_data.as_ref().map(|j| j.stringify()).unwrap_or_else(|| "{}".to_string()),
        secure_json_data: crate::secrets::encrypt_map(&c.app.cfg.secret_key, &secure),
        read_only: false,
        api_version: cmd.api_version.clone(),
        created: now,
        updated: now,
    };
    ds.id = match c.app.store.datasource_create(&ds) {
        Ok(id) => id,
        Err(StoreError::Conflict(_)) => return Err(ApiError::new(409, "data source with the same name already exists")),
        Err(_) => return Err(ApiError::new(500, "Failed to add datasource")),
    };
    let d = dto(c, &ds, None);
    Ok(Response::json(200, &Json::obj().with("datasource", d).with("id", ds.id).with("message", "Datasource added").with("name", ds.name.as_str())))
}

fn update_by_uid(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let cmd = bind_cmd(c, true)?;
    validate_url(&cmd.typ, &cmd.url)?;
    let url_uid = c.param("uid").to_string();
    if !cmd.uid.is_empty() && cmd.uid != url_uid {
        return Err(ApiError::bad_request("UID in the payload must match the UID in the URL"));
    }
    let r = c.app.store.datasource_by_uid(c.user.org_id, &url_uid);
    let old = lookup(c, r, "Failed to update datasource")?;
    if old.read_only {
        return Err(ApiError::new(403, "Cannot update read-only data source"));
    }
    if cmd.name != old.name {
        if let Ok(_) = c.app.store.datasource_by_name(c.user.org_id, &cmd.name) {
            return Err(ApiError::new(409, "Failed to update datasource: data source with the same name already exists"));
        }
    }
    // optimistic locking: a version older than the stored one is rejected
    if cmd.version != 0 && old.version > cmd.version {
        return Err(ApiError::new(409, "Datasource has already been updated by someone else. Please reload and try again"));
    }
    // fillWithSecureJSONData: stored secrets stay unless the command sets them
    let mut secure = crate::secrets::decrypt_map(&c.app.cfg.secret_key, &old.secure_json_data);
    for (k, v) in cmd.secure.iter() {
        secure.set(k, v.as_str());
    }
    let mut ds = old.clone();
    ds.name = cmd.name.clone();
    ds.type_ = cmd.typ.clone();
    ds.access = cmd.access.clone();
    ds.url = cmd.url.clone();
    ds.user = cmd.user.clone();
    ds.database = cmd.database.clone();
    ds.basic_auth = cmd.basic_auth;
    ds.basic_auth_user = cmd.basic_auth_user.clone();
    ds.with_credentials = cmd.with_credentials;
    ds.is_default = cmd.is_default;
    ds.json_data = cmd.json_data.as_ref().map(|j| j.stringify()).unwrap_or_else(|| "{}".to_string());
    ds.secure_json_data = crate::secrets::encrypt_map(&c.app.cfg.secret_key, &secure);
    ds.api_version = cmd.api_version.clone();
    ds.version = cmd.version + 1;
    ds.updated = crate::util::now();
    match c.app.store.datasource_update(&ds) {
        Ok(()) => {}
        Err(StoreError::NotFound) => return Err(not_found()),
        Err(StoreError::Conflict(_)) => return Err(ApiError::new(409, "Failed to update datasource: data source with the same name already exists")),
        Err(_) => return Err(ApiError::new(500, "Failed to update datasource")),
    }
    let r = c.app.store.datasource_by_id(c.user.org_id, ds.id);
    let fresh = lookup(c, r, "Failed to query datasource")?;
    let d = dto(c, &fresh, None);
    Ok(Response::json(200, &Json::obj().with("datasource", d).with("id", ds.id).with("message", "Datasource updated").with("name", cmd.name.as_str())))
}

fn delete_by_uid(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let uid = c.param("uid").to_string();
    if uid.is_empty() {
        return Err(ApiError::bad_request("Missing datasource uid"));
    }
    let ds = match c.app.store.datasource_by_uid(c.user.org_id, &uid) {
        Ok(d) => d,
        Err(StoreError::NotFound) => return Err(not_found()),
        Err(_) => return Err(ApiError::bad_request("Failed to delete datasource")),
    };
    if ds.read_only {
        return Err(ApiError::new(403, "Cannot delete read-only data source"));
    }
    c.app.store.datasource_delete(c.user.org_id, ds.id).map_err(|_| ApiError::new(500, "Failed to delete datasource"))?;
    Ok(Response::json(200, &Json::obj().with("id", ds.id).with("message", "Data source deleted")))
}

fn delete_by_name(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let name = c.param("name").to_string();
    if name.is_empty() {
        return Err(ApiError::bad_request("Missing valid datasource name"));
    }
    let r = c.app.store.datasource_by_name(c.user.org_id, &name);
    let ds = lookup(c, r, "Failed to delete datasource")?;
    if ds.read_only {
        return Err(ApiError::new(403, "Cannot delete read-only data source"));
    }
    c.app.store.datasource_delete(c.user.org_id, ds.id).map_err(|_| ApiError::new(500, "Failed to delete datasource"))?;
    Ok(Response::json(200, &Json::obj().with("id", ds.id).with("message", "Data source deleted")))
}

// ---------------------------------------------------------------- plugin instances and transport

struct CacheEntry {
    org_id: i64,
    uid: String,
    version: i64,
    updated: i64,
    inst: Arc<Instance>,
}

/// Parsed settings per data source version (decrypting secrets once, not per request).
static INSTANCES: Mutex<Vec<CacheEntry>> = Mutex::new(Vec::new());

/// The plugin instance for a data source (`newInstanceSettings`): options, basic auth, custom headers, timeout.
pub fn instance(app: &App, ds: &DataSource) -> Result<Arc<Instance>, String> {
    if let Ok(v) = INSTANCES.lock() {
        for e in v.iter() {
            if e.org_id == ds.org_id && e.uid == ds.uid && e.version == ds.version && e.updated == ds.updated {
                return Ok(e.inst.clone());
            }
        }
    }
    let opts = settings::parse_prom_options(&ds.json_data).map_err(|e| format!("error reading settings: {e}"))?;
    let jd = json_data_value(ds);
    let secure = crate::secrets::decrypt_map(&app.cfg.secret_key, &ds.secure_json_data);
    let mut headers: Vec<(String, String)> = Vec::new();
    if ds.basic_auth {
        let pw = secure.str_of("basicAuthPassword");
        let cred = format!("{}:{}", ds.basic_auth_user, pw);
        headers.push(("Authorization".to_string(), format!("Basic {}", crypto::base64(cred.as_bytes()))));
    }
    for i in 1..100 {
        let name = jd.str_of(&format!("httpHeaderName{i}")).to_string();
        if name.is_empty() {
            break;
        }
        let value = secure.str_of(&format!("httpHeaderValue{i}")).to_string();
        headers.push((name, value));
    }
    let default_timeout = app.cfg.get_int("dataproxy", "timeout", 30).max(1) as u64;
    let timeout_secs = if opts.timeout > 0.0 { opts.timeout as u64 } else { default_timeout };
    let inst = Arc::new(Instance { id: ds.id, uid: ds.uid.clone(), url: ds.url.clone(), opts, headers, timeout_secs });
    if let Ok(mut v) = INSTANCES.lock() {
        v.retain(|e| !(e.org_id == ds.org_id && e.uid == ds.uid));
        if v.len() >= 64 {
            v.remove(0);
        }
        v.push(CacheEntry { org_id: ds.org_id, uid: ds.uid.clone(), version: ds.version, updated: ds.updated, inst: inst.clone() });
    }
    Ok(inst)
}

fn title_method(m: &str) -> String {
    let lower = m.to_ascii_lowercase();
    match lower.get(..1) {
        Some(f) => f.to_ascii_uppercase() + &lower[1..],
        None => String::new(),
    }
}

fn host_port(url: &str) -> String {
    let rest = url.split_once("://").map(|p| p.1).unwrap_or(url);
    let hp = rest.split(['/', '?']).next().unwrap_or("");
    if hp.contains(':') { hp.to_string() } else { format!("{hp}:80") }
}

/// Sends a plugin request with the shared pooled client; errors read as Go's `*url.Error` text.
pub fn send(app: &App, r: &HttpReq, timeout_secs: u64) -> Result<HttpResp, TransportError> {
    let hdrs: Vec<(&str, &str)> = r.headers.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    match app.client.request(&r.method, &r.url, &hdrs, &r.body, Duration::from_secs(timeout_secs)) {
        Ok(res) => Ok(HttpResp { status: res.status, headers: res.headers, body: res.body }),
        Err(e) => {
            let prefix = format!("{} {}", title_method(&r.method), json::quote(&r.url));
            let (msg, timeout) = match e {
                http::ClientError::Timeout => (format!("{prefix}: context deadline exceeded (Client.Timeout exceeded while awaiting headers)"), true),
                http::ClientError::Connect(m) => {
                    let what = if m.contains("refused") { "connect: connection refused".to_string() } else { m };
                    (format!("{prefix}: dial tcp {}: {what}", host_port(&r.url)), false)
                }
                http::ClientError::BadUrl(_) => (format!("{prefix}: unsupported protocol scheme"), false),
                other => (format!("{prefix}: {other}"), false),
            };
            Err(TransportError { message: msg, timeout })
        }
    }
}

// ---------------------------------------------------------------- health, resources, proxy

fn ds_for_plugin_call(c: &Ctx<'_>, meta_err: &str) -> Result<DataSource, ApiError> {
    let uid = c.param("uid");
    if !crate::util::valid_uid(uid) {
        return Err(ApiError::bad_request("UID is invalid"));
    }
    match c.app.store.datasource_by_uid(c.user.org_id, uid) {
        Ok(d) => Ok(d),
        Err(_) => Err(ApiError::new(500, meta_err)),
    }
}

fn health_by_uid(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let ds = ds_for_plugin_call(c, "Unable to load datasource metadata")?;
    if ds.type_ != "prometheus" {
        return Err(ApiError::new(404, "Plugin not found"));
    }
    let inst = instance(c.app, &ds);
    let (ok, message, details) = match inst {
        Ok(inst) => {
            let app = c.app;
            let to = inst.timeout_secs;
            let t = |r: &HttpReq| send(app, r, to);
            promlib::check_health(&inst, &t)
        }
        Err(e) => (false, format!("{e} - error getting datasource info"), None),
    };
    let mut payload = Json::obj();
    if let Some(d) = details {
        payload.set("details", d);
    }
    payload.set("message", message);
    payload.set("status", if ok { "OK" } else { "ERROR" });
    Ok(Response::json(if ok { 200 } else { 400 }, &payload))
}

fn resource_by_uid(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let ds = ds_for_plugin_call(c, "Unable to load datasource meta data")?;
    if ds.type_ != "prometheus" {
        return Err(ApiError::new(500, "Unable to find datasource plugin"));
    }
    let inst = instance(c.app, &ds).map_err(|_| ApiError::new(500, "Failed to get plugin settings"))?;
    let path = c.param("rest").to_string();
    if path.eq_ignore_ascii_case("suggestions") {
        return Err(ApiError::new(500, "Failed to call resource"));
    }
    let app = c.app;
    let to = inst.timeout_secs;
    let t = |r: &HttpReq| send(app, r, to);
    let body = std::mem::take(&mut c.req.body);
    match promlib::call_resource(&inst, &c.req.method, &path, &c.req.query, &body, &t) {
        Ok(r) => {
            let mut resp = Response::new(r.status);
            resp.body = http::Body::Bytes(r.body);
            for (k, v) in r.headers.iter() {
                resp.set_header(k, v);
            }
            Ok(resp)
        }
        Err(_) => Err(ApiError::new(500, "Failed to call resource")),
    }
}

/// The data source proxy (`ProxyDataSourceRequestWithUID`), as a plain forward to the data source URL with its
/// authentication headers.
fn proxy_by_uid(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let uid = c.param("uid").to_string();
    let ds = match c.app.store.datasource_by_uid(c.user.org_id, &uid) {
        Ok(d) => d,
        Err(StoreError::NotFound) => return Err(ApiError::new(404, "Unable to find datasource")),
        Err(_) => return Err(ApiError::new(500, "Unable to load datasource meta data")),
    };
    if ds.access != "proxy" {
        return Err(ApiError::bad_request("Unable to proxy request to datasource, access mode is not proxy"));
    }
    let inst = instance(c.app, &ds).map_err(|_| ApiError::new(500, "Unable to load datasource meta data"))?;
    let rest = c.param("rest").to_string();
    if rest.split('/').any(|s| s == "..") {
        return Err(ApiError::bad_request("Invalid proxy path"));
    }
    let mut url = ds.url.trim_end_matches('/').to_string();
    url.push('/');
    url.push_str(&rest);
    if !c.req.query.is_empty() {
        url.push('?');
        url.push_str(&c.req.query);
    }
    let mut headers = inst.headers.clone();
    for name in ["content-type", "accept"] {
        if let Some(v) = c.req.header(name) {
            headers.push((name.to_string(), v.to_string()));
        }
    }
    let req = HttpReq { method: c.req.method.clone(), url, headers, body: std::mem::take(&mut c.req.body) };
    match send(c.app, &req, inst.timeout_secs) {
        Ok(r) => {
            let mut resp = Response::new(r.status);
            for (k, v) in r.headers.iter() {
                match k.as_str() {
                    "content-type" | "content-disposition" | "cache-control" | "etag" | "last-modified" => resp.set_header(k, v),
                    _ => {}
                }
            }
            resp.body = http::Body::Bytes(r.body);
            Ok(resp)
        }
        Err(_) => Err(ApiError::new(502, "Bad Gateway")),
    }
}

// ---------------------------------------------------------------- /api/ds/query

fn to_i64(v: Option<&Json>, default: i64) -> i64 {
    match v {
        Some(Json::Int(i)) => *i,
        Some(Json::Float(f)) if f.fract() == 0.0 => *f as i64,
        _ => default,
    }
}

/// `POST /api/ds/query` (QueryMetricsV2): groups the queries by data source, runs each group with the plugin, and
/// writes `{"results":{refId: DataResponse}}` (status 400 when any response has an error).
fn ds_query(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let body = c.body_json()?;
    if body.as_obj().is_none() {
        return Err(ApiError::bad_request("bad request data"));
    }
    let str_field = |k: &str| -> Result<String, ApiError> {
        match crate::promlib::models::get_ci(&body, k) {
            None | Some(Json::Null) => Ok(String::new()),
            Some(Json::Str(s)) => Ok(s.clone()),
            Some(_) => Err(ApiError::bad_request("bad request data")),
        }
    };
    let from = str_field("from")?;
    let to = str_field("to")?;
    let queries: Vec<Json> = match crate::promlib::models::get_ci(&body, "queries") {
        None | Some(Json::Null) => Vec::new(),
        Some(Json::Arr(a)) => a.clone(),
        Some(_) => return Err(ApiError::bad_request("bad request data")),
    };
    if queries.is_empty() {
        return Err(errutil(400, "query.noQueries", "No queries found"));
    }
    let now = crate::util::now_ms() * MS;
    let from_ns = crate::promlib::models::parse_time(&from, now, false);
    let to_ns = crate::promlib::models::parse_time(&to, now, true);

    // group by data source, in first-seen order
    let mut groups: Vec<(DataSource, Vec<DataQuery>)> = Vec::new();
    let mut ref_ids: Vec<String> = Vec::new();
    for q in queries.into_iter() {
        let dsv = q.get("datasource");
        let mut uid = dsv.and_then(|d| d.get("uid")).and_then(|u| u.as_str()).unwrap_or("").to_string();
        if uid.is_empty() {
            uid = dsv.and_then(|d| d.as_str()).unwrap_or("").to_string();
        }
        let ds = match groups.iter().position(|g| g.0.uid == uid && !uid.is_empty()) {
            Some(i) => groups[i].0.clone(),
            None => {
                if uid == "__expr__" || uid == "-100" || uid == "grafana" || uid == "-- Grafana --" {
                    return Err(ApiError::new(500, "Query data error"));
                }
                let found = if !uid.is_empty() {
                    c.app.store.datasource_by_uid(c.user.org_id, &uid)
                } else {
                    let id = to_i64(q.get("datasourceId"), 0);
                    if id <= 0 {
                        return Err(errutil(400, "query.invalidDatasourceId", "Query does not contain a valid data source identifier"));
                    }
                    c.app.store.datasource_by_id(c.user.org_id, id)
                };
                match found {
                    Ok(d) => d,
                    Err(StoreError::NotFound) => return Err(not_found()),
                    Err(_) => return Err(ApiError::new(500, "Query data error")),
                }
            }
        };
        let ref_id = match q.get("refId") {
            Some(Json::Str(s)) => s.clone(),
            _ => "A".to_string(),
        };
        if ref_ids.contains(&ref_id) {
            return Err(errutil(400, "query.duplicateRefId", "Multiple queries using the same RefId is not allowed "));
        }
        ref_ids.push(ref_id.clone());
        let dq = DataQuery {
            ref_id,
            query_type: q.get("queryType").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            max_data_points: to_i64(q.get("maxDataPoints"), 100),
            interval: to_i64(q.get("intervalMs"), 1000) * MS,
            from: from_ns,
            to: to_ns,
            json: q,
        };
        match groups.iter().position(|g| g.0.id == ds.id) {
            Some(i) => groups[i].1.push(dq),
            None => groups.push((ds, vec![dq])),
        }
    }

    let single = groups.len() == 1;
    let mut results: Vec<(String, DataResponse)> = Vec::new();
    for (ds, qs) in groups.iter() {
        if ds.type_ != "prometheus" {
            if single {
                return Err(errutil(404, "plugin.notRegistered", "Plugin not registered"));
            }
            for q in qs {
                results.push((q.ref_id.clone(), DataResponse::err("plugin not registered".to_string(), "")));
            }
            continue;
        }
        let inst = match instance(c.app, ds) {
            Ok(i) => i,
            Err(e) => {
                if single {
                    return Err(ApiError::new(500, "Query data error"));
                }
                for q in qs {
                    results.push((q.ref_id.clone(), DataResponse::err(e.clone(), "")));
                }
                continue;
            }
        };
        let app = c.app;
        let to = inst.timeout_secs;
        let t = |r: &HttpReq| send(app, r, to);
        results.extend(promlib::querydata::execute(&inst, qs, &t, false));
    }

    // after the plugin hop: frames always present, each frame carries its refId
    let mut status = 200;
    for (ref_id, r) in results.iter_mut() {
        if r.error.is_some() {
            status = 400;
            if r.error_source.is_empty() {
                r.error_source = crate::promlib::frame::SOURCE_PLUGIN.to_string();
            }
        }
        let frames = r.frames_mut();
        for f in frames.iter_mut() {
            if f.ref_id.is_empty() {
                f.ref_id = ref_id.clone();
            }
        }
    }
    results.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = String::with_capacity(4096);
    out.push_str("{\"results\":{");
    for (i, (ref_id, r)) in results.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        json::write_str(ref_id, &mut out);
        out.push(':');
        r.write_json(&mut out);
    }
    out.push_str("}}\n");
    Ok(Response::json_text(status, out))
}

/// Writes any JSON value with Go's sorted map keys (used by query history).
pub fn sorted_text(v: &Json) -> String {
    let mut s = String::new();
    write_sorted(v, &mut s);
    s
}
