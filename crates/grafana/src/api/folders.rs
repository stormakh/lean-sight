//! Folders, including /apis/folder.grafana.app (agent B). Upstream: pkg/api/folder.go, pkg/registry/apis/folders,
//! pkg/services/folder. Folders live in table `folder`; `meta` keeps the Kubernetes metadata as for dashboards.
use crate::api::dashboards::{folder_url, login_of, perms};
use crate::ctx::{Access, Ctx, Role, Routes, SignedInUser};
use crate::error::ApiError;
use crate::k8s;
use crate::store::{Folder, StoreError};
use crate::util::{local_rfc3339, rfc3339, short_uid, valid_uid};
use crate::App;
use http::Response;
use json::Json;

pub const GROUP: &str = "folder.grafana.app";
const MAX_DEPTH: usize = 4;

pub fn routes(r: &mut Routes) {
    r.get("/api/folders", Access::SignedIn, get_folders);
    r.post("/api/folders", Access::SignedIn, create_folder);
    r.get("/api/folders/id/:id", Access::SignedIn, get_folder_by_id);
    r.get("/api/folders/:uid", Access::SignedIn, get_folder);
    r.put("/api/folders/:uid", Access::SignedIn, update_folder);
    r.delete("/api/folders/:uid", Access::SignedIn, delete_folder);
    r.get("/api/folders/:uid/counts", Access::SignedIn, legacy_counts);
    r.post("/api/folders/:uid/move", Access::SignedIn, move_folder);

    let base = "/apis/folder.grafana.app/v1/namespaces/:ns/folders";
    r.get(base, Access::SignedIn, k8s_list);
    r.post(base, Access::SignedIn, k8s_create);
    r.get("/apis/folder.grafana.app/v1/namespaces/:ns/folders/:name", Access::SignedIn, k8s_get);
    r.put("/apis/folder.grafana.app/v1/namespaces/:ns/folders/:name", Access::SignedIn, k8s_update);
    r.delete("/apis/folder.grafana.app/v1/namespaces/:ns/folders/:name", Access::SignedIn, k8s_delete);
    r.get("/apis/folder.grafana.app/v1/namespaces/:ns/folders/:name/parents", Access::SignedIn, k8s_parents);
    r.get("/apis/folder.grafana.app/v1/namespaces/:ns/folders/:name/access", Access::SignedIn, k8s_access);
    r.get("/apis/folder.grafana.app/v1/namespaces/:ns/folders/:name/counts", Access::SignedIn, k8s_counts);
    r.get("/apis/folder.grafana.app/v1/namespaces/:ns/folders/:name/children", Access::SignedIn, k8s_children);
}

fn folder_not_found() -> ApiError {
    ApiError::raw(404, Json::obj().with("message", "folder not found").with("status", "not-found"))
}

fn parse_obj(text: &str) -> Json {
    match Json::parse(text) {
        Ok(v) if v.as_obj().is_some() => v,
        _ => Json::obj(),
    }
}

/// The folder and its ancestors, root first.
fn ancestry(app: &App, f: &Folder) -> Vec<Folder> {
    let mut out = vec![f.clone()];
    let mut parent = f.parent_uid.clone();
    while !parent.is_empty() && out.len() < 16 {
        if out.iter().any(|x| x.uid == parent) {
            break;
        }
        match app.store.folder_by_uid(f.org_id, &parent) {
            Ok(p) => {
                parent = p.parent_uid.clone();
                out.push(p);
            }
            Err(_) => break,
        }
    }
    out.reverse();
    out
}

/// Grafana's folder DTO (`dtos.Folder`).
fn folder_dto(app: &App, u: &SignedInUser, f: &Folder, with_parents: bool) -> Json {
    let p = perms(app, u);
    let updater = login_of(app, f.updated_by);
    let creator = if f.created_by == f.updated_by { updater.clone() } else { login_of(app, f.created_by) };
    let mut o = Json::obj()
        .with("id", f.id)
        .with("uid", f.uid.as_str())
        .with("orgId", f.org_id)
        .with("title", f.title.as_str())
        .with("url", folder_url(app, &f.uid, &f.title))
        .with("hasAcl", false)
        .with("canSave", p.can_save)
        .with("canEdit", p.can_save)
        .with("canAdmin", p.can_admin)
        .with("canDelete", p.can_delete)
        .with("createdBy", creator)
        .with("created", local_rfc3339(f.created))
        .with("updatedBy", updater)
        .with("updated", local_rfc3339(f.updated));
    if f.version != 0 {
        o.set("version", f.version);
    }
    if !f.parent_uid.is_empty() {
        o.set("parentUid", f.parent_uid.as_str());
    }
    if with_parents {
        let chain = ancestry(app, f);
        if chain.len() > 1 {
            let mut parents = Json::arr();
            for a in chain.iter().take(chain.len() - 1) {
                parents.push(folder_dto(app, u, a, false));
            }
            o.set("parents", parents);
        }
    }
    o
}

fn load(c: &Ctx<'_>, uid: &str) -> Result<Folder, ApiError> {
    match c.app.store.folder_by_uid(c.user.org_id, uid) {
        Ok(f) => Ok(f),
        Err(StoreError::NotFound) => Err(folder_not_found()),
        Err(e) => Err(e.into()),
    }
}

fn get_folders(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let parent = c.req.query_get("parentUid").unwrap_or_default();
    let limit = c.req.query_get("limit").and_then(|v| v.parse::<usize>().ok()).filter(|v| *v > 0).unwrap_or(1000);
    let page = c.req.query_get("page").and_then(|v| v.parse::<usize>().ok()).filter(|v| *v > 0).unwrap_or(1);
    let all = c.app.store.folders(c.user.org_id)?;
    let mut out = Json::arr();
    for f in all.iter().filter(|f| f.parent_uid == parent).skip((page - 1) * limit).take(limit) {
        let mut h = Json::obj().with("id", f.id).with("uid", f.uid.as_str()).with("title", f.title.as_str());
        if !f.parent_uid.is_empty() {
            h.set("parentUid", f.parent_uid.as_str());
        }
        out.push(h);
    }
    Ok(Response::json(200, &out))
}

fn get_folder(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let f = load(c, c.param("uid"))?;
    Ok(Response::json(200, &folder_dto(c.app, &c.user, &f, true)))
}

fn get_folder_by_id(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let id = match c.param("id").parse::<i64>() {
        Ok(v) => v,
        Err(_) => return Err(ApiError::with_trace(400, "id is invalid")),
    };
    let f = match c.app.store.folder_by_id(c.user.org_id, id) {
        Ok(f) => f,
        Err(StoreError::NotFound) => return Err(folder_not_found()),
        Err(e) => return Err(e.into()),
    };
    Ok(Response::json(200, &folder_dto(c.app, &c.user, &f, true)))
}

/// The legacy FieldsV1 of a folder write.
fn legacy_fields(parent: bool, create: bool) -> Json {
    let mut annos = Json::obj().with(".", Json::obj());
    if parent {
        annos.set("f:grafana.app/folder", Json::obj());
    }
    if create {
        annos.set("f:grafana.app/grant-permissions", Json::obj());
    }
    Json::obj().with("f:metadata", Json::obj().with("f:annotations", annos)).with("f:spec", Json::obj().with("f:description", Json::obj()).with("f:title", Json::obj()))
}

/// Builds `meta` for a folder write. `old` is the stored folder for an update.
fn folder_meta(c: &Ctx<'_>, old: Option<&Folder>, parent: &str, manager: &str, fields: Json, extra: &[(String, String)]) -> String {
    let now = crate::util::now();
    let ident = k8s::identity_of(c);
    let (k8s_uid, mut managed, created_ident) = match old {
        Some(o) => {
            let m = parse_obj(&o.meta);
            let managed = m.get("managedFields").cloned().unwrap_or_else(Json::arr);
            let cident = m.path("annotations").and_then(|a| a.get(k8s::ANNO_CREATED_BY)).and_then(Json::as_str).unwrap_or("").to_string();
            (m.str_of("uid").to_string(), managed, cident)
        }
        None => (k8s::uuid_v4(), Json::arr(), ident.clone()),
    };
    let mut annos = Json::obj().with(k8s::ANNO_CREATED_BY, created_ident).with(k8s::ANNO_UPDATED_BY, ident.as_str());
    if !parent.is_empty() {
        annos.set(k8s::ANNO_FOLDER, parent);
    }
    if old.is_some() {
        annos.set(k8s::ANNO_UPDATED_TS, rfc3339(now));
    }
    for (k, v) in extra {
        annos.set(k, v.as_str());
    }
    k8s::sort_keys(&mut annos);
    k8s::merge_managed(&mut managed, manager, "folder.grafana.app/v1", &rfc3339(now), fields);
    Json::obj().with("uid", k8s_uid).with("resourceVersion", k8s::next_rv()).with("annotations", annos).with("managedFields", managed).stringify()
}

fn check_parent(c: &Ctx<'_>, parent: &str, self_uid: &str) -> Result<String, ApiError> {
    if parent.is_empty() || parent == "general" {
        return Ok(String::new());
    }
    if parent == self_uid {
        return Err(ApiError::bad_request("folder cannot be parent of itself"));
    }
    let p = match c.app.store.folder_by_uid(c.user.org_id, parent) {
        Ok(p) => p,
        Err(StoreError::NotFound) => return Err(folder_not_found()),
        Err(e) => return Err(e.into()),
    };
    let chain = ancestry(c.app, &p);
    if !self_uid.is_empty() && chain.iter().any(|f| f.uid == self_uid) {
        return Err(ApiError::bad_request("folder cannot be parent of itself"));
    }
    if chain.len() >= MAX_DEPTH {
        return Err(ApiError::bad_request("folder max depth exceeded, max depth is 4"));
    }
    Ok(p.uid)
}

/// Creates a folder. Returns the stored row.
#[allow(clippy::too_many_arguments)]
fn create(c: &Ctx<'_>, uid: &str, title: &str, description: &str, parent: &str, manager: &str, fields: Json, extra: &[(String, String)]) -> Result<Folder, ApiError> {
    let now = crate::util::now();
    let mut f = Folder {
        id: k8s::snowflake_id(),
        uid: uid.to_string(),
        org_id: c.user.org_id,
        title: title.to_string(),
        description: description.to_string(),
        parent_uid: parent.to_string(),
        version: 1,
        created: now,
        updated: now,
        created_by: c.user.user_id,
        updated_by: c.user.user_id,
        meta: String::new(),
    };
    f.meta = folder_meta(c, None, parent, manager, fields, extra);
    match c.app.store.folder_create(&f) {
        Ok(id) => {
            f.id = id;
            Ok(f)
        }
        Err(StoreError::Conflict(_)) => Err(ApiError::with_trace(409, "a folder with the same UID already exists")),
        Err(e) => Err(e.into()),
    }
}

fn create_folder(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let cmd = c.body_json()?;
    if !c.user.has_role(Role::Editor) {
        return Err(ApiError::forbidden());
    }
    let title = cmd.str_of("title").trim().to_string();
    if title.is_empty() {
        return Err(ApiError::bad_request("folder title cannot be empty"));
    }
    let mut uid = cmd.str_of("uid").to_string();
    if uid.is_empty() {
        uid = short_uid();
    } else if !valid_uid(&uid) || uid == "general" {
        return Err(ApiError::bad_request("invalid uid for folder provided"));
    }
    let parent = check_parent(c, cmd.str_of("parentUid"), &uid)?;
    let f = create(c, &uid, &title, cmd.str_of("description"), &parent, "grafana", legacy_fields(!parent.is_empty(), true), &[])?;
    Ok(Response::json(200, &folder_dto(c.app, &c.user, &f, true)))
}

fn update_folder(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let cmd = c.body_json()?;
    let old = load(c, c.param("uid"))?;
    if !c.user.has_role(Role::Editor) {
        return Err(ApiError::forbidden());
    }
    if !cmd.bool_of("overwrite") {
        if let Some(v) = cmd.get("version").and_then(Json::as_i64) {
            if v != old.version {
                return Err(ApiError::raw(412, Json::obj().with("message", "the folder has been changed by someone else").with("status", "version-mismatch")));
            }
        }
    }
    let mut f = old.clone();
    if let Some(t) = cmd.get("title").and_then(Json::as_str) {
        if t.trim().is_empty() {
            return Err(ApiError::bad_request("folder title cannot be empty"));
        }
        f.title = t.trim().to_string();
    }
    if let Some(d) = cmd.get("description").and_then(Json::as_str) {
        f.description = d.to_string();
    }
    f.version = old.version + 1;
    f.updated = crate::util::now();
    f.updated_by = c.user.user_id;
    f.meta = folder_meta(c, Some(&old), &f.parent_uid, "grafana", legacy_fields(!f.parent_uid.is_empty(), false), &[]);
    c.app.store.folder_update(&f)?;
    Ok(Response::json(200, &folder_dto(c.app, &c.user, &f, true)))
}

fn move_folder(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let cmd = c.body_json()?;
    let old = load(c, c.param("uid"))?;
    if !c.user.has_role(Role::Editor) {
        return Err(ApiError::forbidden());
    }
    let parent = check_parent(c, cmd.str_of("parentUid"), &old.uid)?;
    let mut f = old.clone();
    f.parent_uid = parent;
    f.version = old.version + 1;
    f.updated = crate::util::now();
    f.updated_by = c.user.user_id;
    f.meta = folder_meta(c, Some(&old), &f.parent_uid, "grafana", legacy_fields(!f.parent_uid.is_empty(), false), &[]);
    c.app.store.folder_update(&f)?;
    Ok(Response::json(200, &folder_dto(c.app, &c.user, &f, true)))
}

fn delete_folder(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let f = load(c, c.param("uid"))?;
    if !c.user.has_role(Role::Editor) {
        return Err(ApiError::forbidden());
    }
    c.app.store.folder_delete(c.user.org_id, &f.uid)?;
    Ok(Response::json(200, &Json::obj().with("message", "Folder deleted")))
}

/// (folders, dashboards) below a folder, recursively.
fn descendant_counts(app: &App, org_id: i64, uid: &str) -> Result<(i64, i64), ApiError> {
    let folders = app.store.folders(org_id)?;
    let dashboards = app.store.dashboards(org_id)?;
    let mut set: Vec<String> = vec![uid.to_string()];
    let mut i = 0;
    while i < set.len() {
        let cur = set[i].clone();
        for f in folders.iter() {
            if f.parent_uid == cur && !set.contains(&f.uid) {
                set.push(f.uid.clone());
            }
        }
        i += 1;
    }
    let nd = dashboards.iter().filter(|d| set.contains(&d.folder_uid)).count() as i64;
    Ok((set.len() as i64 - 1, nd))
}

fn legacy_counts(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let f = load(c, c.param("uid"))?;
    let (nf, nd) = descendant_counts(c.app, c.user.org_id, &f.uid)?;
    let o = Json::obj().with("alertrule", 0).with("dashboard", nd).with("folder", nf).with("librarypanel", 0);
    Ok(Response::json(200, &o))
}

// ---------------------------------------------------------------------------------------------------------------
// Kubernetes-style API

fn folder_object(f: &Folder) -> Json {
    let m = parse_obj(&f.meta);
    let mut md = Json::obj().with("name", f.uid.as_str()).with("namespace", k8s::namespace_for(f.org_id)).with("uid", m.str_of("uid"));
    md.set("resourceVersion", m.get("resourceVersion").and_then(Json::as_i64).unwrap_or(0).to_string());
    md.set("generation", f.version);
    md.set("creationTimestamp", rfc3339(f.created));
    md.set("labels", Json::obj().with(k8s::LABEL_INTERNAL_ID, f.id.to_string()));
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
    Json::obj()
        .with("kind", "Folder")
        .with("apiVersion", "folder.grafana.app/v1")
        .with("metadata", md)
        .with("spec", Json::obj().with("title", f.title.as_str()).with("description", f.description.as_str()))
}

fn k8s_load(c: &Ctx<'_>, name: &str) -> Result<Folder, ApiError> {
    match c.app.store.folder_by_uid(c.user.org_id, name) {
        Ok(f) => Ok(f),
        Err(StoreError::NotFound) => Err(k8s::not_found(GROUP, "folders", name)),
        Err(e) => Err(e.into()),
    }
}

fn forbidden_status(verb: &str, name: &str) -> ApiError {
    let msg = if name.is_empty() { format!("folders.folder.grafana.app is forbidden: User cannot {verb} resource") } else { format!("folders.folder.grafana.app \"{name}\" is forbidden: User cannot {verb} resource") };
    k8s::status_error(403, "Forbidden", &msg, Some((name, GROUP, "folders")))
}

fn k8s_list(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    k8s::check_namespace(c)?;
    let limit = c.req.query_get("limit").and_then(|v| v.parse::<usize>().ok()).filter(|v| *v > 0).unwrap_or(usize::MAX);
    let cont = c.req.query_get("continue").unwrap_or_default();
    let labels = c.req.query_get("labelSelector").unwrap_or_default();
    let fields = c.req.query_get("fieldSelector").unwrap_or_default();
    let parent = k8s::selector_value(&labels, "grafana.app/folder").or(k8s::selector_value(&fields, "metadata.parent"));
    let mut all = c.app.store.folders(c.user.org_id)?;
    all.sort_by(|a, b| a.uid.cmp(&b.uid));
    let mut items = Json::arr();
    let mut next = String::new();
    let mut n = 0;
    for f in all.iter().filter(|f| f.uid.as_str() > cont.as_str()) {
        if let Some(p) = parent {
            let p = if p == "general" { "" } else { p };
            if f.parent_uid != p {
                continue;
            }
        }
        if n == limit {
            next = all.iter().filter(|x| x.uid < f.uid).map(|x| x.uid.clone()).last().unwrap_or_default();
            break;
        }
        items.push(folder_object(f));
        n += 1;
    }
    let mut md = Json::obj().with("resourceVersion", k8s::next_rv().to_string());
    if !next.is_empty() {
        md.set("continue", next);
    }
    let l = Json::obj().with("kind", "FolderList").with("apiVersion", "folder.grafana.app/v1").with("metadata", md).with("items", items);
    Ok(k8s::object_response(200, &l))
}

fn k8s_get(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    k8s::check_namespace(c)?;
    let f = k8s_load(c, c.param("name"))?;
    Ok(k8s::object_response(200, &folder_object(&f)))
}

struct Body {
    name: String,
    generate_name: String,
    annotations: Json,
    title: String,
    description: Option<String>,
    spec: Json,
}

fn body(c: &Ctx<'_>) -> Result<Body, ApiError> {
    let b = match c.req.json() {
        Ok(v) if v.as_obj().is_some() => v,
        _ => return Err(k8s::status_error(400, "BadRequest", "the body of the request was in an unknown format", None)),
    };
    let md = b.get("metadata").cloned().unwrap_or_else(Json::obj);
    let spec = b.get("spec").cloned().filter(|s| s.as_obj().is_some()).unwrap_or_else(Json::obj);
    Ok(Body {
        name: md.str_of("name").to_string(),
        generate_name: md.str_of("generateName").to_string(),
        annotations: md.get("annotations").cloned().filter(|a| a.as_obj().is_some()).unwrap_or_else(Json::obj),
        title: spec.str_of("title").trim().to_string(),
        description: spec.get("description").and_then(Json::as_str).map(|s| s.to_string()),
        spec,
    })
}

fn body_fields(b: &Body) -> Json {
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
    let mut out = Json::obj();
    if md.as_obj().map(|o| !o.is_empty()).unwrap_or(false) {
        out.set("f:metadata", md);
    }
    out.set("f:spec", k8s::fields_of(&b.spec));
    out
}

fn extra_annotations(a: &Json) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(o) = a.as_obj() {
        for e in o.iter() {
            match e.0.as_str() {
                k8s::ANNO_GRANT | k8s::ANNO_CREATED_BY | k8s::ANNO_UPDATED_BY | k8s::ANNO_UPDATED_TS | k8s::ANNO_FOLDER => {}
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

fn k8s_create(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    k8s::check_namespace(c)?;
    let b = body(c)?;
    if !c.user.has_role(Role::Editor) {
        return Err(forbidden_status("create", ""));
    }
    let name = if !b.name.is_empty() {
        b.name.clone()
    } else if !b.generate_name.is_empty() {
        k8s::generate_name(&b.generate_name)
    } else {
        short_uid()
    };
    if !valid_uid(&name) || name == "general" {
        return Err(k8s::status_error(400, "BadRequest", "invalid uid for folder provided", None));
    }
    if b.title.is_empty() {
        return Err(k8s::status_error(400, "BadRequest", "folder title cannot be empty", None));
    }
    let parent_req = b.annotations.get(k8s::ANNO_FOLDER).and_then(Json::as_str).unwrap_or("").to_string();
    let parent = check_parent(c, &parent_req, &name)?;
    let fields = body_fields(&b);
    let f = match create(c, &name, &b.title, b.description.as_deref().unwrap_or(""), &parent, &k8s::manager_of(c), fields, &extra_annotations(&b.annotations)) {
        Ok(f) => f,
        Err(e) if e.status == 409 => return Err(k8s::status_error(409, "AlreadyExists", &format!("folders.folder.grafana.app \"{name}\" already exists"), Some((&name, GROUP, "folders")))),
        Err(e) => return Err(e),
    };
    Ok(k8s::object_response(201, &folder_object(&f)))
}

fn k8s_update(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    k8s::check_namespace(c)?;
    let name = c.param("name").to_string();
    let b = body(c)?;
    let old = k8s_load(c, &name)?;
    if !c.user.has_role(Role::Editor) {
        return Err(forbidden_status("update", &name));
    }
    if b.title.is_empty() {
        return Err(k8s::status_error(400, "BadRequest", "folder title cannot be empty", None));
    }
    let parent_req = b.annotations.get(k8s::ANNO_FOLDER).and_then(Json::as_str).unwrap_or("").to_string();
    let parent = check_parent(c, &parent_req, &name)?;
    let mut f = old.clone();
    f.title = b.title.clone();
    f.description = b.description.clone().unwrap_or_default();
    f.parent_uid = parent;
    f.version = old.version + 1;
    f.updated = crate::util::now();
    f.updated_by = c.user.user_id;
    f.meta = folder_meta(c, Some(&old), &f.parent_uid, &k8s::manager_of(c), body_fields(&b), &extra_annotations(&b.annotations));
    c.app.store.folder_update(&f)?;
    Ok(k8s::object_response(200, &folder_object(&f)))
}

fn k8s_delete(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    k8s::check_namespace(c)?;
    let name = c.param("name").to_string();
    let f = k8s_load(c, &name)?;
    if !c.user.has_role(Role::Editor) {
        return Err(forbidden_status("delete", &name));
    }
    c.app.store.folder_delete(c.user.org_id, &f.uid)?;
    let m = parse_obj(&f.meta);
    Ok(k8s::status_success(&name, GROUP, "folders", m.str_of("uid")))
}

fn k8s_parents(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    k8s::check_namespace(c)?;
    let name = c.param("name").to_string();
    let mut items = Json::arr();
    if name != "general" && name != "sharedwithme" {
        let f = k8s_load(c, &name)?;
        for a in ancestry(c.app, &f) {
            let mut it = Json::obj().with("name", a.uid.as_str()).with("title", a.title.as_str());
            if !a.description.is_empty() {
                it.set("description", a.description.as_str());
            }
            if !a.parent_uid.is_empty() {
                it.set("parent", a.parent_uid.as_str());
            }
            items.push(it);
        }
    }
    let l = Json::obj().with("kind", "FolderInfoList").with("apiVersion", "folder.grafana.app/v1").with("metadata", Json::obj()).with("items", items);
    Ok(k8s::object_response(200, &l))
}

/// The actions of `accessControl` in FolderAccessInfo, sorted.
const ACTIONS: [&str; 35] = [
    "alert.rules:create",
    "alert.rules:delete",
    "alert.rules:read",
    "alert.rules:write",
    "alert.silences:create",
    "alert.silences:read",
    "alert.silences:write",
    "annotations:create",
    "annotations:delete",
    "annotations:read",
    "annotations:write",
    "dashboards.permissions:read",
    "dashboards.permissions:write",
    "dashboards:create",
    "dashboards:delete",
    "dashboards:read",
    "dashboards:write",
    "folders.permissions:read",
    "folders.permissions:write",
    "folders:create",
    "folders:delete",
    "folders:read",
    "folders:write",
    "library.panels:create",
    "library.panels:delete",
    "library.panels:read",
    "library.panels:write",
    "notebooks:create",
    "notebooks:delete",
    "notebooks:read",
    "notebooks:write",
    "variables:create",
    "variables:delete",
    "variables:read",
    "variables:write",
];

fn k8s_access(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    k8s::check_namespace(c)?;
    let name = c.param("name").to_string();
    if name != "general" {
        k8s_load(c, &name)?;
    }
    let p = perms(c.app, &c.user);
    let mut ac = Json::obj();
    for a in ACTIONS.iter() {
        let ok = if a.contains(".permissions:") {
            p.can_admin
        } else if a.ends_with(":read") {
            c.user.has_role(Role::Viewer)
        } else {
            p.can_save
        };
        if ok {
            ac.set(a, true);
        }
    }
    let o = Json::obj()
        .with("kind", "FolderAccessInfo")
        .with("apiVersion", "folder.grafana.app/v1")
        .with("canSave", p.can_save)
        .with("canEdit", p.can_save)
        .with("canAdmin", p.can_admin)
        .with("canDelete", p.can_delete)
        .with("accessControl", ac);
    Ok(k8s::object_response(200, &o))
}

fn k8s_counts(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    k8s::check_namespace(c)?;
    let f = k8s_load(c, c.param("name"))?;
    let (nf, nd) = descendant_counts(c.app, c.user.org_id, &f.uid)?;
    let counts = Json::arr()
        .push_item(Json::obj().with("group", "dashboard.grafana.app").with("resource", "dashboards").with("count", nd))
        .push_item(Json::obj().with("group", "folder.grafana.app").with("resource", "folders").with("count", nf));
    let o = Json::obj().with("kind", "DescendantCounts").with("apiVersion", "folder.grafana.app/v1").with("counts", counts);
    Ok(k8s::object_response(200, &o))
}

fn k8s_children(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    k8s::check_namespace(c)?;
    let name = c.param("name").to_string();
    let parent = if name == "general" { String::new() } else { k8s_load(c, &name)?.uid };
    let mut items = Json::arr();
    for f in c.app.store.folders(c.user.org_id)?.iter().filter(|f| f.parent_uid == parent) {
        items.push(folder_object(f));
    }
    let l = Json::obj().with("kind", "FolderList").with("apiVersion", "folder.grafana.app/v1").with("metadata", Json::obj()).with("items", items);
    Ok(k8s::object_response(200, &l))
}
