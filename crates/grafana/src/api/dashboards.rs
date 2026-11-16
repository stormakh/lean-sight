//! Dashboards and versions, including /apis/dashboard.grafana.app (agent B). Upstream: pkg/api/dashboard.go,
//! pkg/registry/apis/dashboard (sub_dto.go, mutate.go), pkg/storage/unified/apistore, pkg/services/dashboards.
//!
//! Storage model (Grafana 13 keeps dashboards as Kubernetes-style objects): `dashboard.data` is the spec without
//! `id`, `uid` and `version`, with sorted keys, as stored at `api_version` ("v0alpha1" for /api/dashboards/db
//! saves, "v1" for /apis saves). `version` is the object generation. `meta` keeps the object metadata that the
//! columns do not hold (k8s uid, resourceVersion, annotations, managedFields). Legacy reads return the stored spec;
//! v1 reads of a v0alpha1 spec run the schema migration (`dashboard_migrate`).
use crate::ctx::{Access, Ctx, Role, Routes, SignedInUser};
use crate::dashboard_migrate;
use crate::error::ApiError;
use crate::k8s;
use crate::store::{Dashboard, DashboardVersion, Folder, SaveDashboard, StoreError};
use crate::util::{local_rfc3339, rfc3339, slugify, valid_uid};
use crate::App;
use http::Response;
use json::Json;

pub const GROUP: &str = "dashboard.grafana.app";

pub fn routes(r: &mut Routes) {
    r.get("/api/dashboards/uid/:uid", Access::SignedIn, get_dashboard);
    r.delete("/api/dashboards/uid/:uid", Access::SignedIn, delete_dashboard);
    r.post("/api/dashboards/db", Access::SignedIn, post_dashboard);
    r.get("/api/dashboards/uid/:uid/versions", Access::SignedIn, get_versions);
    r.get("/api/dashboards/uid/:uid/versions/:id", Access::SignedIn, get_version);
    r.post("/api/dashboards/uid/:uid/restore", Access::SignedIn, restore_version);
    r.get("/api/dashboards/home", Access::SignedIn, get_home);
    r.get("/api/dashboards/tags", Access::SignedIn, get_tags);
    r.get("/api/dashboards/ids/:ids", Access::SignedIn, get_uids_by_ids);

    let base = "/apis/dashboard.grafana.app/:ver/namespaces/:ns/dashboards";
    r.get(base, Access::SignedIn, k8s_list);
    r.post(base, Access::SignedIn, k8s_create);
    r.get("/apis/dashboard.grafana.app/:ver/namespaces/:ns/dashboards/:name", Access::SignedIn, k8s_get);
    r.put("/apis/dashboard.grafana.app/:ver/namespaces/:ns/dashboards/:name", Access::SignedIn, k8s_update);
    r.delete("/apis/dashboard.grafana.app/:ver/namespaces/:ns/dashboards/:name", Access::SignedIn, k8s_delete);
    r.get("/apis/dashboard.grafana.app/:ver/namespaces/:ns/dashboards/:name/dto", Access::SignedIn, k8s_dto);
}

// ---------------------------------------------------------------------------------------------------------------
// shared helpers (also used by folders and search)

/// What the signed-in user may do with dashboards and folders. Grafana evaluates RBAC per resource; here the basic
/// org role decides (Admin: everything, Editor: everything but permissions, Viewer: read).
#[derive(Clone, Copy)]
pub struct Perms {
    pub can_save: bool,
    pub can_edit: bool,
    pub can_admin: bool,
    pub can_delete: bool,
    pub can_star: bool,
    pub anno_write: bool,
}

pub fn perms(app: &App, u: &SignedInUser) -> Perms {
    let editor = u.has_role(Role::Editor);
    Perms {
        can_save: editor,
        can_edit: editor || app.cfg.get_bool("users", "viewers_can_edit"),
        can_admin: u.has_role(Role::Admin),
        can_delete: editor,
        can_star: u.is_signed_in(),
        anno_write: editor,
    }
}

/// `{"dashboard":{"canAdd","canEdit","canDelete"}}`.
pub fn annotation_perms(p: &Perms) -> Json {
    Json::obj().with("dashboard", Json::obj().with("canAdd", p.anno_write).with("canEdit", p.anno_write).with("canDelete", p.anno_write))
}

/// Login of a user id for createdBy/updatedBy ("Anonymous" for 0 or unknown; a service account's name).
pub fn login_of(app: &App, user_id: i64) -> String {
    if user_id <= 0 {
        return "Anonymous".to_string();
    }
    match app.store.user_by_id(user_id) {
        Ok(u) => {
            if u.is_service_account {
                u.name
            } else {
                u.login
            }
        }
        Err(_) => "Anonymous".to_string(),
    }
}

/// `<sub>/d/<uid>/<slug>`.
pub fn dashboard_url(app: &App, uid: &str, slug: &str) -> String {
    format!("{}/d/{}/{}", app.cfg.app_sub_url, uid, slug)
}

/// `<sub>/dashboards/f/<uid>/<slug>`.
pub fn folder_url(app: &App, uid: &str, title: &str) -> String {
    format!("{}/dashboards/f/{}/{}", app.cfg.app_sub_url, uid, slugify(title))
}

/// Inserts or replaces `k` keeping the keys sorted.
pub fn set_sorted(o: &mut Json, k: &str, v: Json) {
    if let Json::Obj(list) = o {
        match list.binary_search_by(|e| e.0.as_str().cmp(k)) {
            Ok(i) => list[i].1 = v,
            Err(i) => list.insert(i, (k.to_string(), v)),
        }
    }
}

fn parse_obj(text: &str) -> Json {
    match Json::parse(text) {
        Ok(v) if v.as_obj().is_some() => v,
        _ => Json::obj(),
    }
}

fn str_tags(spec: &Json) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(Json::Arr(a)) = spec.get("tags") {
        for t in a.iter() {
            if let Some(s) = t.as_str() {
                out.push(s.to_string());
            }
        }
    }
    out
}

fn is_api_version(v: &str) -> bool {
    matches!(v, "v0alpha1" | "v1beta1" | "v1")
}

/// "v0alpha1" or "v1" (v1beta1 is stored as v1).
fn storage_version(v: &str) -> &'static str {
    if v == "v0alpha1" { "v0alpha1" } else { "v1" }
}

// ---------------------------------------------------------------------------------------------------------------
// saving

/// One write of a dashboard, from either API.
struct Write {
    org_id: i64,
    uid: String,
    /// The spec as written (without id, uid, version), keys sorted.
    spec: Json,
    folder_uid: String,
    message: String,
    api_version: String,
    /// Extra annotations from the request (k8s API), without grant-permissions and the computed ones.
    annotations: Vec<(String, String)>,
    /// Keep the folder annotation even when empty (the k8s request carried it).
    folder_anno: bool,
    manager: String,
    managed_api_version: String,
    /// FieldsV1 of this write.
    fields: Json,
    internal_id: i64,
    restored_from: i64,
}

/// Creates or updates a dashboard. `old` is the stored dashboard for an update.
fn save(c: &Ctx<'_>, w: Write, old: Option<&Dashboard>) -> Result<Dashboard, ApiError> {
    let now = crate::util::now();
    let ident = k8s::identity_of(c);
    let title = w.spec.get("title").and_then(Json::as_str).unwrap_or("").to_string();
    let mut annos: Vec<(String, Json)> = Vec::new();
    for (k, v) in w.annotations.iter() {
        annos.push((k.clone(), Json::from(v.as_str())));
    }
    let (id, version, created, created_by, k8s_uid, mut managed, created_ident) = match old {
        Some(o) => {
            let m = parse_obj(&o.meta);
            let managed = m.get("managedFields").cloned().unwrap_or_else(Json::arr);
            let cident = m.path("annotations").and_then(|a| a.get(k8s::ANNO_CREATED_BY)).and_then(Json::as_str).unwrap_or("").to_string();
            (o.id, o.version + 1, o.created, o.created_by, m.str_of("uid").to_string(), managed, cident)
        }
        None => {
            let id = if w.internal_id > 0 { w.internal_id } else { k8s::snowflake_id() };
            (id, 1, now, c.user.user_id, k8s::uuid_v4(), Json::arr(), ident.clone())
        }
    };
    annos.push((k8s::ANNO_CREATED_BY.to_string(), Json::from(created_ident.as_str())));
    annos.push((k8s::ANNO_UPDATED_BY.to_string(), Json::from(ident.as_str())));
    if old.is_some() {
        annos.push((k8s::ANNO_UPDATED_TS.to_string(), Json::from(rfc3339(now))));
    }
    if !w.folder_uid.is_empty() || w.folder_anno {
        annos.push((k8s::ANNO_FOLDER.to_string(), Json::from(w.folder_uid.as_str())));
    }
    if !w.message.is_empty() {
        annos.push((k8s::ANNO_MESSAGE.to_string(), Json::from(w.message.as_str())));
    }
    let mut annotations = Json::Obj(Vec::new());
    for (k, v) in annos {
        annotations.set(&k, v);
    }
    k8s::sort_keys(&mut annotations);
    k8s::merge_managed(&mut managed, &w.manager, &w.managed_api_version, &rfc3339(now), w.fields);
    let meta = Json::obj().with("uid", k8s_uid).with("resourceVersion", k8s::next_rv()).with("annotations", annotations).with("managedFields", managed);
    let d = Dashboard {
        id,
        uid: w.uid.clone(),
        org_id: w.org_id,
        version,
        slug: slugify(&title),
        title,
        data: w.spec.stringify(),
        folder_uid: w.folder_uid.clone(),
        created,
        updated: now,
        created_by,
        updated_by: c.user.user_id,
        gnet_id: w.spec.get("gnetId").and_then(Json::as_i64).unwrap_or(0),
        plugin_id: String::new(),
        api_version: w.api_version.clone(),
        tags: str_tags(&w.spec),
        meta: meta.stringify(),
    };
    let s = SaveDashboard { dashboard: d, is_new: old.is_none(), message: w.message.clone(), restored_from: w.restored_from };
    match c.app.store.dashboard_save(&s) {
        Ok(()) => Ok(s.dashboard),
        Err(StoreError::Conflict(_)) => Err(ApiError::new(400, "A dashboard with the same uid already exists")),
        Err(e) => Err(e.into()),
    }
}

/// Checks that a folder uid names an existing folder ("" and "general" are the root).
fn check_folder(c: &Ctx<'_>, uid: &str) -> Result<String, ApiError> {
    if uid.is_empty() || uid == "general" {
        return Ok(String::new());
    }
    match c.app.store.folder_by_uid(c.user.org_id, uid) {
        Ok(_) => Ok(uid.to_string()),
        Err(StoreError::NotFound) => Err(ApiError::new(400, "folder not found")),
        Err(e) => Err(e.into()),
    }
}

/// The FieldsV1 of a legacy (/api/dashboards/db) write.
fn legacy_fields(spec: &Json, folder: bool, message: bool, generate_name: bool, create: bool) -> Json {
    let mut annos = Json::obj().with(".", Json::obj());
    if folder {
        annos.set("f:grafana.app/folder", Json::obj());
    }
    if create {
        annos.set("f:grafana.app/grant-permissions", Json::obj());
    }
    if message {
        annos.set("f:grafana.app/message", Json::obj());
    }
    let mut md = Json::obj().with("f:annotations", annos);
    if generate_name {
        md.set("f:generateName", Json::obj());
    }
    Json::obj().with("f:metadata", md).with("f:spec", k8s::fields_of(spec))
}

// ---------------------------------------------------------------------------------------------------------------
// legacy API

fn get_dashboard(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let d = match c.app.store.dashboard_by_uid(c.user.org_id, c.param("uid")) {
        Ok(d) => d,
        Err(StoreError::NotFound) => return Err(ApiError::new(404, "Dashboard not found")),
        Err(e) => return Err(e.into()),
    };
    let mut dash = parse_obj(&d.data);
    set_sorted(&mut dash, "id", Json::Int(d.id));
    set_sorted(&mut dash, "uid", Json::from(d.uid.as_str()));
    set_sorted(&mut dash, "version", Json::Int(d.version));
    let p = perms(c.app, &c.user);
    let updater = login_of(c.app, d.updated_by);
    let creator = if d.created_by == d.updated_by { updater.clone() } else { login_of(c.app, d.created_by) };
    let mut meta = Json::obj()
        .with("type", "db")
        .with("canSave", p.can_save)
        .with("canEdit", p.can_edit)
        .with("canAdmin", p.can_admin)
        .with("canStar", p.can_star)
        .with("canDelete", p.can_delete)
        .with("slug", d.slug.as_str())
        .with("url", dashboard_url(c.app, &d.uid, &d.slug))
        .with("expires", "0001-01-01T00:00:00Z")
        .with("created", local_rfc3339(d.created))
        .with("updated", local_rfc3339(d.updated))
        .with("updatedBy", updater)
        .with("createdBy", creator)
        .with("version", d.version)
        .with("hasAcl", false)
        .with("isFolder", false)
        .with("apiVersion", "v0alpha1");
    let (mut fid, mut fuid, mut ftitle, mut furl) = (0i64, String::new(), "General".to_string(), String::new());
    if !d.folder_uid.is_empty() {
        match c.app.store.folder_by_uid(c.user.org_id, &d.folder_uid) {
            Ok(f) => {
                furl = folder_url(c.app, &f.uid, &f.title);
                fid = f.id;
                fuid = f.uid;
                ftitle = f.title;
            }
            Err(StoreError::NotFound) => return Err(ApiError::with_trace(404, "Folder not found")),
            Err(e) => return Err(e.into()),
        }
    }
    meta.set("folderId", fid);
    meta.set("folderUid", fuid);
    meta.set("folderTitle", ftitle);
    meta.set("folderUrl", furl);
    meta.set("provisioned", false);
    meta.set("provisionedExternalId", "");
    meta.set("annotationsPermissions", annotation_perms(&p));
    let mut out = String::with_capacity(d.data.len() + 1024);
    out.push_str("{\"meta\":");
    meta.write(&mut out);
    out.push_str(",\"dashboard\":");
    dash.write(&mut out);
    out.push('}');
    Ok(Response::json_text(200, out))
}

fn delete_dashboard(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let d = match c.app.store.dashboard_by_uid(c.user.org_id, c.param("uid")) {
        Ok(d) => d,
        Err(StoreError::NotFound) => return Err(ApiError::new(404, "Dashboard not found")),
        Err(e) => return Err(e.into()),
    };
    if !perms(c.app, &c.user).can_delete {
        return Err(ApiError::forbidden());
    }
    c.app.store.dashboard_delete(c.user.org_id, &d.uid)?;
    let body = Json::obj().with("message", format!("Dashboard {} deleted", d.title)).with("title", d.title.as_str()).with("uid", d.uid.as_str());
    Ok(Response::json(200, &body))
}

fn post_dashboard(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let cmd = c.body_json()?;
    if cmd.as_obj().is_none() {
        return Err(ApiError::bad_request("bad request data"));
    }
    if cmd.bool_of("isFolder") {
        return Err(ApiError::bad_request("Use folders endpoint for saving folders."));
    }
    let mut spec = match cmd.get("dashboard") {
        Some(d) if d.as_obj().is_some() => d.clone(),
        _ => return Err(ApiError::with_trace(400, "Failed to read dashboard")),
    };
    if spec.get("elements").is_some() && spec.get("layout").is_some() {
        return Err(ApiError::bad_request(
            "Dashboard JSON looks like a v2 schema dashboard. It should be wrapped in a k8s style object OR it should include a object wrapper with an explicit 'apiVersion' and move the body into a 'spec' element",
        ));
    }
    if spec.get("spec").is_some() && (spec.get("apiVersion").is_some() || spec.get("kind").is_some() || spec.get("metadata").is_some()) {
        let av = spec.str_of("apiVersion").to_string();
        if av.is_empty() {
            return Err(ApiError::bad_request("Dashboard appears to be a k8s style resource, but is missing an explicit apiVersion."));
        }
        if !av.starts_with(GROUP) {
            return Err(ApiError::bad_request("The dashboard payload references a non dashboard apiVersion.  This should be sent to the requested api directly"));
        }
        let name = spec.path("metadata.name").and_then(Json::as_str).unwrap_or("").to_string();
        let mut inner = spec.get("spec").cloned().unwrap_or_else(Json::obj);
        if inner.get("uid").is_none() && !name.is_empty() {
            inner.set("uid", name.as_str());
        }
        spec = inner;
    }
    if !c.user.has_role(Role::Editor) {
        return Err(ApiError::forbidden());
    }
    let title = spec.get("title").and_then(Json::as_str).unwrap_or("").to_string();
    if title.is_empty() {
        return Err(ApiError::bad_request("Dashboard spec is missing required title property"));
    }
    let spec_uid = spec.get("uid").and_then(Json::as_str).unwrap_or("").to_string();
    let internal_id = match spec.get("id") {
        None | Some(Json::Null) => 0,
        Some(Json::Int(i)) => *i,
        Some(Json::Float(f)) => *f as i64,
        Some(_) => return Err(ApiError::bad_request("unsupported ID type")),
    };
    let spec_version = match spec.get("version") {
        None | Some(Json::Null) => None,
        Some(Json::Int(i)) => Some(*i),
        Some(Json::Float(f)) => Some(*f as i64),
        Some(_) => return Err(ApiError::bad_request("spec.version has unsupported type")),
    };
    spec.remove("uid");
    spec.remove("id");
    spec.remove("version");
    k8s::sort_keys(&mut spec);
    if !spec_uid.is_empty() {
        if spec_uid.len() > 40 {
            return Err(ApiError::bad_request("uid too long, max 40 characters"));
        }
        if !valid_uid(&spec_uid) {
            return Err(ApiError::bad_request("uid contains illegal characters"));
        }
    }
    let org = c.user.org_id;
    let mut name = spec_uid.clone();
    let mut old: Option<Dashboard> = None;
    let mut new_internal_id = 0;
    if internal_id > 0 {
        match c.app.store.dashboard_by_id(org, internal_id) {
            Ok(d) => {
                if name.is_empty() || name == d.uid {
                    name = d.uid.clone();
                    old = Some(d);
                } else {
                    match c.app.store.dashboard_by_uid(org, &name) {
                        Ok(_) => return Err(ApiError::bad_request("A dashboard with the same uid already exists")),
                        Err(StoreError::NotFound) => {}
                        Err(e) => return Err(e.into()),
                    }
                }
            }
            Err(StoreError::NotFound) => {
                if name.is_empty() {
                    return Err(ApiError::new(404, "Dashboard not found"));
                }
                new_internal_id = internal_id;
            }
            Err(e) => return Err(e.into()),
        }
    }
    if !name.is_empty() && old.is_none() {
        match c.app.store.dashboard_by_uid(org, &name) {
            Ok(d) => old = Some(d),
            Err(StoreError::NotFound) => {}
            Err(e) => return Err(e.into()),
        }
    }
    let generate = old.is_none() && name.is_empty();
    if generate {
        name = k8s::generate_name("a");
    }
    if let Some(o) = old.as_ref() {
        if !cmd.bool_of("overwrite") && spec_version != Some(o.version) {
            return Err(ApiError::new(409, "Dashboard already exists. Use overwrite flag to update."));
        }
    }
    let mut folder = cmd.str_of("folderUid").to_string();
    if folder.is_empty() {
        let fid = cmd.i64_of("folderId");
        if fid > 0 {
            match c.app.store.folder_by_id(org, fid) {
                Ok(f) => folder = f.uid,
                Err(StoreError::NotFound) => return Err(ApiError::new(400, "folder not found")),
                Err(e) => return Err(e.into()),
            }
        }
    }
    let folder = check_folder(c, &folder)?;
    let message = cmd.str_of("message").to_string();
    let fields = legacy_fields(&spec, !folder.is_empty(), !message.is_empty(), generate, true);
    let w = Write {
        org_id: org,
        uid: name,
        spec,
        folder_uid: folder,
        message,
        api_version: "v0alpha1".to_string(),
        annotations: Vec::new(),
        folder_anno: false,
        manager: "grafana".to_string(),
        managed_api_version: "dashboard.grafana.app/v0alpha1".to_string(),
        fields,
        internal_id: new_internal_id,
        restored_from: 0,
    };
    let d = save(c, w, old.as_ref())?;
    Ok(Response::json(200, &save_result(c.app, &d)))
}

fn save_result(app: &App, d: &Dashboard) -> Json {
    Json::obj()
        .with("folderUid", d.folder_uid.as_str())
        .with("id", d.id)
        .with("slug", d.slug.as_str())
        .with("status", "success")
        .with("uid", d.uid.as_str())
        .with("url", dashboard_url(app, &d.uid, &d.slug))
        .with("version", d.version)
}

fn version_json(app: &App, d: &Dashboard, v: &DashboardVersion, message: &str, creator: String) -> Json {
    let mut data = parse_obj(&v.data);
    set_sorted(&mut data, "uid", Json::from(d.uid.as_str()));
    set_sorted(&mut data, "version", Json::Int(v.version));
    let _ = app;
    Json::obj()
        .with("id", v.version)
        .with("dashboardId", d.id)
        .with("uid", d.uid.as_str())
        .with("parentVersion", v.parent_version)
        .with("restoredFrom", v.restored_from)
        .with("version", v.version)
        // Grafana reads the version from the k8s object: the updatedTimestamp annotation (RFC 3339 in UTC) when the
        // version has one, which every version after the first has; else creationTimestamp (metav1.Time, local).
        .with("created", if v.version > 1 { rfc3339(v.created) } else { local_rfc3339(v.created) })
        .with("message", message)
        .with("data", data)
        .with("createdBy", creator)
}

fn dashboard_or_404(c: &Ctx<'_>) -> Result<Dashboard, ApiError> {
    match c.app.store.dashboard_by_uid(c.user.org_id, c.param("uid")) {
        Ok(d) => Ok(d),
        Err(StoreError::NotFound) => Err(ApiError::with_trace(404, "Dashboard not found")),
        Err(e) => Err(e.into()),
    }
}

fn get_versions(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let d = dashboard_or_404(c)?;
    let limit = c.req.query_get("limit").and_then(|v| v.parse::<i64>().ok()).filter(|v| *v > 0).unwrap_or(1000);
    let start = c.req.query_get("start").and_then(|v| v.parse::<i64>().ok()).filter(|v| *v > 0).unwrap_or(0);
    let list = c.app.store.dashboard_versions(d.id, limit, start)?;
    let mut logins: Vec<(i64, String)> = Vec::new();
    let mut out = Json::arr();
    for v in list.iter() {
        let mut msg = v.message.clone();
        if v.restored_from == v.version {
            msg = "Initial save (created by migration)".to_string();
        }
        if v.restored_from > 0 {
            msg = format!("Restored from version {}", v.restored_from);
        }
        if v.parent_version == 0 {
            msg = "Initial save".to_string();
        }
        let creator = match logins.iter().find(|x| x.0 == v.created_by) {
            Some(x) => x.1.clone(),
            None => {
                let l = login_of(c.app, v.created_by);
                logins.push((v.created_by, l.clone()));
                l
            }
        };
        out.push(version_json(c.app, &d, v, &msg, creator));
    }
    Ok(Response::json(200, &Json::obj().with("continueToken", "").with("versions", out)))
}

fn get_version(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let d = dashboard_or_404(c)?;
    let version = match c.param("id").parse::<i64>() {
        Ok(v) => v,
        Err(_) => return Err(ApiError::internal("")),
    };
    let v = match c.app.store.dashboard_version(d.id, version) {
        Ok(v) => v,
        Err(StoreError::NotFound) => return Err(ApiError::with_trace(404, &format!("Dashboard version {} not found for dashboardId {}", version, d.id))),
        Err(e) => return Err(e.into()),
    };
    let creator = login_of(c.app, v.created_by);
    let msg = v.message.clone();
    Ok(Response::json(200, &version_json(c.app, &d, &v, &msg, creator)))
}

fn restore_version(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let body = c.body_json()?;
    let d = dashboard_or_404(c)?;
    if !perms(c.app, &c.user).can_save {
        return Err(ApiError::forbidden());
    }
    let version = body.i64_of("version");
    let v = match c.app.store.dashboard_version(d.id, version) {
        Ok(v) => v,
        Err(StoreError::NotFound) => return Err(ApiError::with_trace(404, "Dashboard version not found")),
        Err(e) => return Err(e.into()),
    };
    let spec = parse_obj(&v.data);
    let message = format!("Restored from version {version}");
    let fields = legacy_fields(&spec, !d.folder_uid.is_empty(), true, false, false);
    let w = Write {
        org_id: d.org_id,
        uid: d.uid.clone(),
        spec,
        folder_uid: d.folder_uid.clone(),
        message,
        api_version: v.api_version.clone(),
        annotations: Vec::new(),
        folder_anno: false,
        manager: "grafana".to_string(),
        managed_api_version: "dashboard.grafana.app/v0alpha1".to_string(),
        fields,
        internal_id: 0,
        restored_from: version,
    };
    let nd = save(c, w, Some(&d))?;
    Ok(Response::json(200, &save_result(c.app, &nd)))
}

fn get_home(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let org = c.user.org_id;
    let mut home_uid = String::new();
    if c.user.user_id > 0 {
        if let Ok(p) = c.app.store.prefs_get(org, c.user.user_id, 0) {
            home_uid = p.home_dashboard_uid;
        }
    }
    if home_uid.is_empty() {
        if let Ok(p) = c.app.store.prefs_get(org, 0, 0) {
            home_uid = p.home_dashboard_uid;
        }
    }
    let home_page = c.app.cfg.get("users", "home_page");
    if home_uid.is_empty() && !home_page.is_empty() {
        return Ok(Response::json(200, &Json::obj().with("redirectUri", home_page)));
    }
    if !home_uid.is_empty() {
        if let Ok(d) = c.app.store.dashboard_by_uid(org, &home_uid) {
            return Ok(Response::json(200, &Json::obj().with("redirectUri", dashboard_url(c.app, &d.uid, &d.slug))));
        }
    }
    let path = c.app.cfg.get("dashboards", "default_home_dashboard_path");
    if path.is_empty() {
        return Err(ApiError::new(404, "No custom home dashboard configured"));
    }
    let doc = match std::fs::read(path).ok().and_then(|b| Json::parse_bytes(&b).ok()) {
        Some(d) => d,
        None => return Err(ApiError::with_trace(500, "Failed to load home dashboard")),
    };
    let meta = Json::obj()
        .with("canSave", false)
        .with("canEdit", c.user.has_role(Role::Editor))
        .with("canAdmin", false)
        .with("canStar", false)
        .with("canDelete", false)
        .with("slug", "")
        .with("url", "")
        .with("expires", "0001-01-01T00:00:00Z")
        .with("created", "0001-01-01T00:00:00Z")
        .with("updated", "0001-01-01T00:00:00Z")
        .with("updatedBy", "")
        .with("createdBy", "")
        .with("version", 0)
        .with("hasAcl", false)
        .with("isFolder", false)
        .with("folderId", 0)
        .with("folderUid", "")
        .with("folderTitle", "General")
        .with("folderUrl", "")
        .with("provisioned", false)
        .with("provisionedExternalId", "")
        .with("annotationsPermissions", Json::Null);
    Ok(Response::json(200, &Json::obj().with("meta", meta).with("dashboard", doc)))
}

fn get_tags(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let tags = c.app.store.dashboard_tags(c.user.org_id).map_err(|_| ApiError::with_trace(500, "Failed to get tags from database"))?;
    let mut out = Json::arr();
    for (t, n) in tags {
        out.push(Json::obj().with("term", t).with("count", n));
    }
    let mut text = out.stringify();
    text.push('\n');
    Ok(Response::text(200, "application/json; charset=UTF-8", text))
}

fn get_uids_by_ids(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let mut out = Json::arr();
    for s in c.param("ids").split(',') {
        if let Ok(id) = s.parse::<i64>() {
            if let Ok(d) = c.app.store.dashboard_by_id(c.user.org_id, id) {
                out.push(d.uid.as_str());
            }
        }
    }
    Ok(Response::json(200, &out))
}

// ---------------------------------------------------------------------------------------------------------------
// Kubernetes-style API

fn check_version(c: &Ctx<'_>) -> Result<String, ApiError> {
    k8s::check_namespace(c)?;
    let v = c.param("ver");
    if !is_api_version(v) {
        return Err(ApiError::not_found());
    }
    Ok(v.to_string())
}

fn forbidden_status(verb: &str, name: &str) -> ApiError {
    let msg = if name.is_empty() { format!("dashboards.dashboard.grafana.app is forbidden: User cannot {verb} resource") } else { format!("dashboards.dashboard.grafana.app \"{name}\" is forbidden: User cannot {verb} resource") };
    k8s::status_error(403, "Forbidden", &msg, Some((name, GROUP, "dashboards")))
}

/// The spec and status of a stored dashboard (or version) as API version `ver` returns them.
fn spec_for(data: &str, stored: &str, ver: &str) -> (Json, Json) {
    let spec = parse_obj(data);
    let want = storage_version(ver);
    if stored == want || stored.is_empty() {
        return (spec, Json::obj());
    }
    if stored == "v0alpha1" {
        let mut m = spec.clone();
        match dashboard_migrate::migrate(&mut m) {
            Ok(()) => {
                k8s::sort_keys(&mut m);
                return (m, Json::obj().with("conversion", Json::obj().with("failed", false).with("storedVersion", "v0alpha1")));
            }
            Err(e) => {
                return (spec, Json::obj().with("conversion", Json::obj().with("failed", true).with("storedVersion", "v0alpha1").with("error", e)));
            }
        }
    }
    (spec, Json::obj().with("conversion", Json::obj().with("failed", false).with("storedVersion", stored)))
}

/// The Dashboard object. `meta_text`/`generation`/`data`/`stored` come from the row or a version row.
#[allow(clippy::too_many_arguments)]
fn object(org_id: i64, uid: &str, id: i64, created: i64, generation: i64, meta_text: &str, data: &str, stored: &str, ver: &str, kind: &str) -> Json {
    let m = parse_obj(meta_text);
    let mut md = Json::obj().with("name", uid).with("namespace", k8s::namespace_for(org_id)).with("uid", m.str_of("uid"));
    md.set("resourceVersion", m.get("resourceVersion").and_then(Json::as_i64).unwrap_or(0).to_string());
    md.set("generation", generation);
    md.set("creationTimestamp", rfc3339(created));
    if id != 0 {
        md.set("labels", Json::obj().with(k8s::LABEL_INTERNAL_ID, id.to_string()));
    }
    if let Some(a) = m.get("annotations") {
        if a.as_obj().map(|o| !o.is_empty()).unwrap_or(false) {
            md.set("annotations", a.clone());
        }
    }
    if let Some(mf) = m.get("managedFields") {
        if mf.as_arr().map(|a| !a.is_empty()).unwrap_or(false) {
            md.set("managedFields", mf.clone());
        }
    }
    let (spec, status) = spec_for(data, stored, ver);
    Json::obj().with("kind", kind).with("apiVersion", format!("{GROUP}/{ver}")).with("metadata", md).with("spec", spec).with("status", status)
}

fn dashboard_object(d: &Dashboard, ver: &str, kind: &str) -> Json {
    object(d.org_id, &d.uid, d.id, d.created, d.version, &d.meta, &d.data, &d.api_version, ver, kind)
}

fn load(c: &Ctx<'_>, name: &str) -> Result<Dashboard, ApiError> {
    match c.app.store.dashboard_by_uid(c.user.org_id, name) {
        Ok(d) => Ok(d),
        Err(StoreError::NotFound) => Err(k8s::not_found(GROUP, "dashboards", name)),
        Err(e) => Err(e.into()),
    }
}

fn k8s_get(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let ver = check_version(c)?;
    let d = load(c, c.param("name"))?;
    Ok(k8s::object_response(200, &dashboard_object(&d, &ver, "Dashboard")))
}

fn k8s_dto(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let ver = check_version(c)?;
    let d = load(c, c.param("name"))?;
    if c.req.query_get("includeAccess").as_deref() == Some("false") {
        return Ok(k8s::object_response(200, &dashboard_object(&d, &ver, "Dashboard")));
    }
    let mut o = dashboard_object(&d, &ver, "DashboardWithAccessInfo");
    let p = perms(c.app, &c.user);
    let access = Json::obj()
        .with("slug", d.slug.as_str())
        .with("url", dashboard_url(c.app, &d.uid, &d.slug))
        .with("isPublic", false)
        .with("canSave", p.can_save)
        .with("canEdit", p.can_edit)
        .with("canAdmin", p.can_admin)
        .with("canStar", p.can_star)
        .with("canDelete", p.can_delete)
        .with("annotationsPermissions", annotation_perms(&p));
    o.set("access", access);
    Ok(k8s::object_response(200, &o))
}

fn list_meta(rv: i64, cont: &str) -> Json {
    let mut m = Json::obj().with("resourceVersion", rv.to_string());
    if !cont.is_empty() {
        m.set("continue", cont);
    }
    m
}

fn k8s_list(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let ver = check_version(c)?;
    let labels = c.req.query_get("labelSelector").unwrap_or_default();
    let fields = c.req.query_get("fieldSelector").unwrap_or_default();
    let limit = c.req.query_get("limit").and_then(|v| v.parse::<i64>().ok()).filter(|v| *v > 0).unwrap_or(500);
    let cont = c.req.query_get("continue").unwrap_or_default();
    let as_table = c.req.header("accept").map(|a| a.contains("as=Table")).unwrap_or(false);
    let rv = k8s::next_rv();
    if k8s::selector_value(&labels, "grafana.app/get-trash").is_some() {
        if as_table {
            let t = Json::obj().with("kind", "Table").with("apiVersion", "meta.k8s.io/v1").with("metadata", list_meta(rv, "")).with("columnDefinitions", Json::arr()).with("rows", Json::arr());
            return Ok(k8s::object_response(200, &t));
        }
        let l = Json::obj().with("kind", "DashboardList").with("apiVersion", format!("{GROUP}/{ver}")).with("metadata", list_meta(rv, "")).with("items", Json::arr());
        return Ok(k8s::object_response(200, &l));
    }
    let mut items = Json::arr();
    let mut next = String::new();
    if k8s::selector_value(&labels, "grafana.app/get-history").is_some() {
        let name = k8s::selector_value(&fields, "metadata.name").unwrap_or("");
        let d = load(c, name)?;
        let offset = cont.parse::<i64>().unwrap_or(0).max(0);
        let list = c.app.store.dashboard_versions(d.id, limit + 1, offset)?;
        for (i, v) in list.iter().enumerate() {
            if i as i64 == limit {
                next = (offset + limit).to_string();
                break;
            }
            items.push(object(d.org_id, &d.uid, d.id, d.created, v.version, &v.meta, &v.data, &v.api_version, &ver, "Dashboard"));
        }
    } else {
        let list = c.app.store.dashboards_full(c.user.org_id, &cont, limit + 1)?;
        let name = k8s::selector_value(&fields, "metadata.name");
        for (i, d) in list.iter().enumerate() {
            if i as i64 == limit {
                if let Some(prev) = list.get(i - 1) {
                    next = prev.uid.clone();
                }
                break;
            }
            if let Some(n) = name {
                if n != d.uid {
                    continue;
                }
            }
            items.push(dashboard_object(d, &ver, "Dashboard"));
        }
    }
    let l = Json::obj().with("kind", "DashboardList").with("apiVersion", format!("{GROUP}/{ver}")).with("metadata", list_meta(rv, &next)).with("items", items);
    Ok(k8s::object_response(200, &l))
}

/// The pieces of a k8s write body.
struct K8sBody {
    name: String,
    generate_name: String,
    annotations: Json,
    labels: Json,
    spec: Json,
}

fn k8s_body(c: &Ctx<'_>) -> Result<K8sBody, ApiError> {
    let b = match c.req.json() {
        Ok(v) if v.as_obj().is_some() => v,
        _ => return Err(k8s::status_error(400, "BadRequest", "the body of the request was in an unknown format", None)),
    };
    let md = b.get("metadata").cloned().unwrap_or_else(Json::obj);
    let spec = match b.get("spec") {
        Some(s) if s.as_obj().is_some() => s.clone(),
        _ => Json::obj(),
    };
    Ok(K8sBody {
        name: md.str_of("name").to_string(),
        generate_name: md.str_of("generateName").to_string(),
        annotations: md.get("annotations").cloned().filter(|a| a.as_obj().is_some()).unwrap_or_else(Json::obj),
        labels: md.get("labels").cloned().filter(|a| a.as_obj().is_some()).unwrap_or_else(Json::obj),
        spec,
    })
}

/// FieldsV1 of a k8s write: the request's annotations, labels, generateName and spec.
fn k8s_fields(b: &K8sBody) -> Json {
    let mut md = Json::obj();
    if b.annotations.as_obj().map(|o| !o.is_empty()).unwrap_or(false) {
        let mut a = Json::obj().with(".", Json::obj());
        if let Json::Obj(f) = k8s::fields_of(&b.annotations) {
            for e in f {
                a.set(&e.0, e.1);
            }
        }
        md.set("f:annotations", a);
    }
    if !b.generate_name.is_empty() {
        md.set("f:generateName", Json::obj());
    }
    if b.labels.as_obj().map(|o| !o.is_empty()).unwrap_or(false) {
        let mut a = Json::obj().with(".", Json::obj());
        if let Json::Obj(f) = k8s::fields_of(&b.labels) {
            for e in f {
                a.set(&e.0, e.1);
            }
        }
        md.set("f:labels", a);
    }
    let mut out = Json::obj();
    if md.as_obj().map(|o| !o.is_empty()).unwrap_or(false) {
        out.set("f:metadata", md);
    }
    out.set("f:spec", k8s::fields_of(&b.spec));
    out
}

/// Extra annotations to keep (everything but the computed ones and grant-permissions).
fn extra_annotations(a: &Json) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(o) = a.as_obj() {
        for e in o.iter() {
            match e.0.as_str() {
                k8s::ANNO_GRANT | k8s::ANNO_CREATED_BY | k8s::ANNO_UPDATED_BY | k8s::ANNO_UPDATED_TS | k8s::ANNO_FOLDER | k8s::ANNO_MESSAGE => {}
                _ => {
                    if let Some(s) = e.1.as_str() {
                        out.push((e.0.clone(), s.to_string()));
                    }
                }
            }
        }
    }
    out
}

/// Strips id/uid/version from a written spec, runs the v1 migration on write, sorts keys.
fn prepare_spec(mut spec: Json, ver: &str) -> (Json, i64) {
    let id = match spec.get("id") {
        Some(Json::Int(i)) => *i,
        Some(Json::Float(f)) => *f as i64,
        _ => 0,
    };
    spec.remove("uid");
    spec.remove("version");
    spec.remove("id");
    if storage_version(ver) == "v1" {
        let mut m = spec.clone();
        if dashboard_migrate::migrate(&mut m).is_ok() {
            spec = m;
        }
    }
    k8s::sort_keys(&mut spec);
    (spec, id)
}

fn k8s_create(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let ver = check_version(c)?;
    let b = k8s_body(c)?;
    if !perms(c.app, &c.user).can_save {
        return Err(forbidden_status("create", ""));
    }
    let name = if !b.name.is_empty() {
        b.name.clone()
    } else if !b.generate_name.is_empty() {
        k8s::generate_name(&b.generate_name)
    } else {
        return Err(k8s::status_error(400, "BadRequest", "name or generateName is required", None));
    };
    if name.len() > 40 || !valid_uid(&name) {
        return Err(k8s::status_error(422, "Invalid", &format!("Dashboard.dashboard.grafana.app \"{name}\" is invalid: metadata.name: Invalid value: \"{name}\""), None));
    }
    match c.app.store.dashboard_by_uid(c.user.org_id, &name) {
        Ok(_) => return Err(k8s::status_error(409, "AlreadyExists", &format!("dashboards.dashboard.grafana.app \"{name}\" already exists"), Some((&name, GROUP, "dashboards")))),
        Err(StoreError::NotFound) => {}
        Err(e) => return Err(e.into()),
    }
    let folder_raw = b.annotations.get(k8s::ANNO_FOLDER).and_then(Json::as_str).map(|s| s.to_string());
    let folder = check_folder(c, folder_raw.as_deref().unwrap_or(""))?;
    let message = b.annotations.get(k8s::ANNO_MESSAGE).and_then(Json::as_str).unwrap_or("").to_string();
    let fields = k8s_fields(&b);
    let (spec, id) = prepare_spec(b.spec.clone(), &ver);
    let label_id = b.labels.get(k8s::LABEL_INTERNAL_ID).and_then(Json::as_str).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
    let mut internal_id = if id > 0 { id } else { label_id };
    if internal_id > 0 && c.app.store.dashboard_by_id(c.user.org_id, internal_id).is_ok() {
        internal_id = 0;
    }
    let w = Write {
        org_id: c.user.org_id,
        uid: name,
        spec,
        folder_uid: folder,
        message,
        api_version: storage_version(&ver).to_string(),
        annotations: extra_annotations(&b.annotations),
        folder_anno: folder_raw.is_some(),
        manager: k8s::manager_of(c),
        managed_api_version: format!("{GROUP}/{ver}"),
        fields,
        internal_id,
        restored_from: 0,
    };
    let d = save(c, w, None)?;
    Ok(k8s::object_response(201, &dashboard_object(&d, &ver, "Dashboard")))
}

fn k8s_update(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let ver = check_version(c)?;
    let name = c.param("name").to_string();
    let b = k8s_body(c)?;
    let old = load(c, &name)?;
    if !perms(c.app, &c.user).can_save {
        return Err(forbidden_status("update", &name));
    }
    if !b.name.is_empty() && b.name != name {
        return Err(k8s::status_error(400, "BadRequest", "the name of the object does not match the name on the URL", None));
    }
    let folder_raw = b.annotations.get(k8s::ANNO_FOLDER).and_then(Json::as_str).map(|s| s.to_string());
    let folder = check_folder(c, folder_raw.as_deref().unwrap_or(""))?;
    let message = b.annotations.get(k8s::ANNO_MESSAGE).and_then(Json::as_str).unwrap_or("").to_string();
    let fields = k8s_fields(&b);
    let (spec, _) = prepare_spec(b.spec.clone(), &ver);
    let w = Write {
        org_id: c.user.org_id,
        uid: name,
        spec,
        folder_uid: folder,
        message,
        api_version: storage_version(&ver).to_string(),
        annotations: extra_annotations(&b.annotations),
        folder_anno: folder_raw.is_some(),
        manager: k8s::manager_of(c),
        managed_api_version: format!("{GROUP}/{ver}"),
        fields,
        internal_id: 0,
        restored_from: 0,
    };
    let d = save(c, w, Some(&old))?;
    Ok(k8s::object_response(200, &dashboard_object(&d, &ver, "Dashboard")))
}

fn k8s_delete(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    check_version(c)?;
    let name = c.param("name").to_string();
    let d = load(c, &name)?;
    if !perms(c.app, &c.user).can_delete {
        return Err(forbidden_status("delete", &name));
    }
    c.app.store.dashboard_delete(c.user.org_id, &d.uid)?;
    let m = parse_obj(&d.meta);
    Ok(k8s::status_success(&name, GROUP, "dashboards", m.str_of("uid")))
}

/// Folder data the dashboard APIs show.
pub fn folder_of(app: &App, org_id: i64, uid: &str) -> Option<Folder> {
    if uid.is_empty() {
        return None;
    }
    app.store.folder_by_uid(org_id, uid).ok()
}
