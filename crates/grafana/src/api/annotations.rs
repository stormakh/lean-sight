//! Annotations (store and query). Upstream: pkg/api/annotations.go, pkg/services/annotations/annotationsimpl.
use crate::ctx::{Access, Ctx, Role, Routes};
use crate::error::ApiError;
use crate::store::{Annotation, AnnotationQuery, StoreError};
use http::Response;
use json::Json;

pub fn routes(r: &mut Routes) {
    r.get("/api/annotations", Access::SignedIn, get_annotations);
    r.post("/api/annotations", Access::SignedIn, post_annotation);
    r.post("/api/annotations/mass-delete", Access::SignedIn, mass_delete);
    r.post("/api/annotations/graphite", Access::SignedIn, post_graphite);
    r.get("/api/annotations/tags", Access::SignedIn, get_tags);
    r.get("/api/annotations/:annotationId", Access::SignedIn, get_annotation);
    r.put("/api/annotations/:annotationId", Access::SignedIn, update_annotation);
    r.patch("/api/annotations/:annotationId", Access::SignedIn, patch_annotation);
    r.delete("/api/annotations/:annotationId", Access::SignedIn, delete_annotation);
}

const DEFAULT_LIMIT: i64 = 100;

fn qi64(c: &Ctx<'_>, k: &str) -> i64 {
    c.req.query_get(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0)
}

/// Users already looked up in this request: (id, uid, login, email).
type UserCache = Vec<(i64, String, String, String)>;

fn user_info(c: &Ctx<'_>, cache: &mut UserCache, id: i64) -> Option<(String, String, String)> {
    if id <= 0 {
        return None;
    }
    if let Some(u) = cache.iter().find(|u| u.0 == id) {
        return Some((u.1.clone(), u.2.clone(), u.3.clone()));
    }
    match c.app.store.user_by_id(id) {
        Ok(u) => {
            cache.push((id, u.uid.clone(), u.login.clone(), u.email.clone()));
            Some((u.uid, u.login, u.email))
        }
        Err(_) => None,
    }
}

/// `<sub>/avatar/<sha256 of the lower-case email>` (dtos.GetGravatarUrl).
fn avatar_url(c: &Ctx<'_>, email: &str) -> String {
    if c.app.cfg.get_bool("security", "disable_gravatar") {
        return format!("{}/public/img/user_profile.png", c.app.cfg.app_sub_url);
    }
    let e = email.trim().to_lowercase();
    if e.is_empty() {
        return String::new();
    }
    format!("{}/avatar/{}", c.app.cfg.app_sub_url, crypto::hex(&crypto::sha256(e.as_bytes())))
}

/// The ItemDTO of an annotation (omitempty fields left out).
fn item_dto(c: &Ctx<'_>, cache: &mut UserCache, a: &Annotation) -> Json {
    let mut o = Json::obj().with("id", a.id);
    if a.alert_id != 0 {
        o.set("alertId", a.alert_id);
    }
    if a.dashboard_id != 0 {
        o.set("dashboardId", a.dashboard_id);
    }
    if !a.dashboard_uid.is_empty() {
        o.set("dashboardUID", a.dashboard_uid.as_str());
    }
    if a.panel_id != 0 {
        o.set("panelId", a.panel_id);
    }
    let user = user_info(c, cache, a.user_id);
    if let Some(u) = user.as_ref() {
        if !u.0.is_empty() {
            o.set("userUID", u.0.as_str());
        }
    }
    if !a.new_state.is_empty() {
        o.set("newState", a.new_state.as_str());
    }
    if !a.prev_state.is_empty() {
        o.set("prevState", a.prev_state.as_str());
    }
    for (k, v) in [("created", a.created_ms), ("updated", a.updated_ms), ("time", a.epoch_ms), ("timeEnd", a.epoch_end_ms)] {
        if v != 0 {
            o.set(k, v);
        }
    }
    if !a.text.is_empty() {
        o.set("text", a.text.as_str());
    }
    if !a.tags.is_empty() {
        o.set("tags", a.tags.clone());
    }
    if let Some(u) = user.as_ref() {
        if !u.1.is_empty() {
            o.set("login", u.1.as_str());
        }
        if !u.2.is_empty() {
            o.set("email", u.2.as_str());
            o.set("avatarUrl", avatar_url(c, &u.2));
        }
    }
    let data = match Json::parse(&a.data) {
        Ok(v) => v,
        Err(_) => Json::obj(),
    };
    o.set("data", data);
    o
}

fn get_annotations(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let mut q = AnnotationQuery {
        org_id: c.user.org_id,
        from_ms: qi64(c, "from"),
        to_ms: qi64(c, "to"),
        dashboard_uid: c.req.query_get("dashboardUID").unwrap_or_default(),
        dashboard_id: qi64(c, "dashboardId"),
        panel_id: qi64(c, "panelId"),
        user_id: qi64(c, "userId"),
        alert_id: qi64(c, "alertId"),
        type_: c.req.query_get("type").unwrap_or_default(),
        tags: c.req.query_all("tags"),
        match_any: c.req.query_get("matchAny").as_deref() == Some("true"),
        limit: qi64(c, "limit"),
    };
    if q.limit == 0 {
        q.limit = DEFAULT_LIMIT;
    }
    if q.dashboard_id != 0 && q.dashboard_uid.is_empty() {
        match c.app.store.dashboard_by_id(c.user.org_id, q.dashboard_id) {
            Ok(d) => q.dashboard_uid = d.uid,
            Err(_) => return Err(ApiError::with_trace(400, "Invalid dashboard ID in annotation request")),
        }
    }
    if !q.dashboard_uid.is_empty() {
        // the dashboard uid is the filter; the deprecated id is not stored for every row
        q.dashboard_id = 0;
    }
    let list = c.app.store.annotations_find(&q).map_err(|_| ApiError::with_trace(500, "Failed to get annotations"))?;
    let mut cache = UserCache::new();
    let mut out = Json::arr();
    for a in list.iter() {
        out.push(item_dto(c, &mut cache, a));
    }
    Ok(Response::json(200, &out))
}

fn tags_of(v: Option<&Json>) -> Option<Vec<String>> {
    match v {
        Some(Json::Arr(a)) => {
            let mut out = Vec::new();
            for t in a.iter() {
                if let Some(s) = t.as_str() {
                    let s = s.trim();
                    if !s.is_empty() && !out.iter().any(|x: &String| x == s) {
                        out.push(s.to_string());
                    }
                }
            }
            Some(out)
        }
        _ => None,
    }
}

fn data_text(v: Option<&Json>) -> Option<String> {
    match v {
        Some(Json::Null) | None => None,
        Some(d) => Some(d.stringify()),
    }
}

/// validateTimeRange: end defaults to start; swapped when reversed. Err when both are missing.
fn time_range(epoch: i64, end: i64) -> Result<(i64, i64), ()> {
    let (mut s, mut e) = (epoch, end);
    if e == 0 {
        if s == 0 {
            return Err(());
        }
        e = s;
    }
    if s == 0 {
        s = e;
    }
    if e < s {
        std::mem::swap(&mut s, &mut e);
    }
    Ok((s, e))
}

fn post_annotation(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let cmd = c.body_json()?;
    let mut dashboard_uid = cmd.str_of("dashboardUID").to_string();
    let mut dashboard_id = cmd.i64_of("dashboardId");
    if !dashboard_uid.is_empty() {
        if let Ok(d) = c.app.store.dashboard_by_uid(c.user.org_id, &dashboard_uid) {
            dashboard_id = d.id;
        }
    }
    if dashboard_id != 0 && dashboard_uid.is_empty() {
        match c.app.store.dashboard_by_id(c.user.org_id, dashboard_id) {
            Ok(d) => dashboard_uid = d.uid,
            Err(_) => return Err(ApiError::with_trace(400, "Invalid dashboard ID in annotation request")),
        }
    }
    if !c.user.has_role(Role::Editor) {
        return Err(ApiError::new(403, "Access denied to save the annotation"));
    }
    let text = cmd.str_of("text").to_string();
    if text.is_empty() {
        return Err(ApiError::with_trace(400, "Failed to save annotation"));
    }
    let now = crate::util::now_ms();
    let epoch = if cmd.i64_of("time") == 0 { now } else { cmd.i64_of("time") };
    let (s, e) = match time_range(epoch, cmd.i64_of("timeEnd")) {
        Ok(r) => r,
        Err(_) => return Err(ApiError::with_trace(400, "Failed to save annotation")),
    };
    let a = Annotation {
        id: 0,
        org_id: c.user.org_id,
        user_id: c.user.user_id,
        dashboard_id,
        dashboard_uid,
        panel_id: cmd.i64_of("panelId"),
        alert_id: 0,
        text,
        epoch_ms: s,
        epoch_end_ms: e,
        tags: tags_of(cmd.get("tags")).unwrap_or_default(),
        data: data_text(cmd.get("data")).unwrap_or_else(|| "{}".to_string()),
        prev_state: String::new(),
        new_state: String::new(),
        created_ms: now,
        updated_ms: now,
    };
    let id = c.app.store.annotation_create(&a).map_err(|_| ApiError::with_trace(500, "Failed to save annotation"))?;
    Ok(Response::json(200, &Json::obj().with("id", id).with("message", "Annotation added")))
}

fn post_graphite(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let cmd = c.body_json()?;
    if !c.user.has_role(Role::Editor) {
        return Err(ApiError::forbidden());
    }
    let what = cmd.str_of("what").to_string();
    if what.is_empty() {
        return Err(ApiError::with_trace(400, "Failed to save Graphite annotation"));
    }
    let tags: Vec<String> = match cmd.get("tags") {
        Some(Json::Str(s)) => s.split_whitespace().map(|t| t.to_string()).collect(),
        Some(Json::Arr(_)) => tags_of(cmd.get("tags")).unwrap_or_default(),
        None | Some(Json::Null) => Vec::new(),
        Some(_) => return Err(ApiError::with_trace(400, "Failed to save Graphite annotation")),
    };
    let data = cmd.str_of("data");
    let text = if data.is_empty() { what.clone() } else { format!("{what}\n{data}") };
    let now = crate::util::now_ms();
    let when = cmd.i64_of("when");
    let epoch = if when == 0 { now } else { when * 1000 };
    let a = Annotation { org_id: c.user.org_id, user_id: c.user.user_id, text, epoch_ms: epoch, epoch_end_ms: epoch, tags, data: "{}".to_string(), created_ms: now, updated_ms: now, ..Annotation::default() };
    let id = c.app.store.annotation_create(&a).map_err(|_| ApiError::with_trace(500, "Failed to save Graphite annotation"))?;
    Ok(Response::json(200, &Json::obj().with("id", id).with("message", "Graphite annotation added")))
}

fn annotation_id(c: &Ctx<'_>) -> Result<i64, ApiError> {
    c.param("annotationId").parse::<i64>().map_err(|_| ApiError::with_trace(400, "annotationId is invalid"))
}

fn find(c: &Ctx<'_>, id: i64) -> Result<Annotation, ApiError> {
    match c.app.store.annotation_by_id(c.user.org_id, id) {
        Ok(a) => Ok(a),
        Err(StoreError::NotFound) => Err(ApiError::new(404, "Annotation not found")),
        Err(_) => Err(ApiError::with_trace(500, "Failed to find annotation")),
    }
}

fn get_annotation(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let id = annotation_id(c)?;
    let a = find(c, id)?;
    let mut cache = UserCache::new();
    Ok(Response::json(200, &item_dto(c, &mut cache, &a)))
}

fn update_annotation(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let cmd = c.body_json()?;
    let id = annotation_id(c)?;
    if !c.user.has_role(Role::Editor) {
        return Err(ApiError::forbidden());
    }
    let mut a = find(c, id)?;
    a.text = cmd.str_of("text").to_string();
    if cmd.i64_of("time") != 0 {
        a.epoch_ms = cmd.i64_of("time");
    }
    if cmd.i64_of("timeEnd") != 0 {
        a.epoch_end_ms = cmd.i64_of("timeEnd");
    }
    if let Some(d) = data_text(cmd.get("data")) {
        a.data = d;
    }
    a.tags = tags_of(cmd.get("tags")).unwrap_or_default();
    write(c, a, "Failed to update annotation")?;
    Ok(Response::json(200, &Json::obj().with("message", "Annotation updated")))
}

fn patch_annotation(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let cmd = c.body_json()?;
    let id = annotation_id(c)?;
    if !c.user.has_role(Role::Editor) {
        return Err(ApiError::forbidden());
    }
    let mut a = find(c, id)?;
    if let Some(t) = tags_of(cmd.get("tags")) {
        a.tags = t;
    }
    if !cmd.str_of("text").is_empty() {
        a.text = cmd.str_of("text").to_string();
    }
    if cmd.i64_of("time") > 0 {
        a.epoch_ms = cmd.i64_of("time");
    }
    if cmd.i64_of("timeEnd") > 0 {
        a.epoch_end_ms = cmd.i64_of("timeEnd");
    }
    if let Some(d) = data_text(cmd.get("data")) {
        a.data = d;
    }
    write(c, a, "Failed to patch annotation")?;
    Ok(Response::json(200, &Json::obj().with("message", "Annotation patched")))
}

fn write(c: &Ctx<'_>, mut a: Annotation, fail: &str) -> Result<(), ApiError> {
    let (s, e) = time_range(a.epoch_ms, a.epoch_end_ms).map_err(|_| ApiError::with_trace(500, fail))?;
    a.epoch_ms = s;
    a.epoch_end_ms = e;
    a.updated_ms = crate::util::now_ms();
    c.app.store.annotation_update(&a).map_err(|_| ApiError::with_trace(500, fail))
}

fn delete_annotation(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let id = annotation_id(c)?;
    if !c.user.has_role(Role::Editor) {
        return Err(ApiError::forbidden());
    }
    match c.app.store.annotation_delete(c.user.org_id, id) {
        Ok(()) | Err(StoreError::NotFound) => Ok(Response::json(200, &Json::obj().with("message", "Annotation deleted"))),
        Err(_) => Err(ApiError::with_trace(500, "Failed to delete annotation")),
    }
}

fn mass_delete(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let cmd = c.body_json()?;
    let mut dashboard_uid = cmd.str_of("dashboardUID").to_string();
    let mut dashboard_id = cmd.i64_of("dashboardId");
    let panel_id = cmd.i64_of("panelId");
    let annotation = cmd.i64_of("annotationId");
    if !dashboard_uid.is_empty() {
        if let Ok(d) = c.app.store.dashboard_by_uid(c.user.org_id, &dashboard_uid) {
            dashboard_id = d.id;
        }
    }
    if dashboard_id != 0 && dashboard_uid.is_empty() {
        match c.app.store.dashboard_by_id(c.user.org_id, dashboard_id) {
            Ok(d) => dashboard_uid = d.uid,
            Err(_) => return Err(ApiError::with_trace(400, "Invalid dashboard ID in annotation request")),
        }
    }
    if (dashboard_id != 0 && panel_id == 0) || (panel_id != 0 && dashboard_id == 0) {
        return Err(ApiError::with_trace(400, "bad request data"));
    }
    if !c.user.has_role(Role::Editor) {
        return Err(ApiError::new(403, "Access denied to mass delete annotations"));
    }
    let r = if annotation != 0 {
        find(c, annotation)?;
        c.app.store.annotation_delete(c.user.org_id, annotation).map(|_| 0)
    } else {
        c.app.store.annotations_delete(c.user.org_id, &dashboard_uid, panel_id)
    };
    match r {
        Ok(_) | Err(StoreError::NotFound) => Ok(Response::json(200, &Json::obj().with("message", "Annotations deleted"))),
        Err(_) => Err(ApiError::with_trace(500, "Failed to delete annotations")),
    }
}

fn get_tags(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let prefix = c.req.query_get("tag").unwrap_or_default();
    let mut limit = qi64(c, "limit");
    if limit == 0 {
        limit = DEFAULT_LIMIT;
    }
    let q = AnnotationQuery { org_id: c.user.org_id, limit: i64::MAX, ..AnnotationQuery::default() };
    let all = c.app.store.annotations_find(&q).map_err(|_| ApiError::with_trace(500, "Failed to find annotation tags"))?;
    let mut counts: Vec<(String, i64)> = Vec::new();
    for a in all.iter() {
        for t in a.tags.iter() {
            if !t.starts_with(prefix.as_str()) {
                continue;
            }
            match counts.iter_mut().find(|x| x.0 == *t) {
                Some(x) => x.1 += 1,
                None => counts.push((t.clone(), 1)),
            }
        }
    }
    counts.sort_by(|a, b| a.0.cmp(&b.0));
    let mut tags = Json::arr();
    for (t, n) in counts.into_iter().take(limit.max(0) as usize) {
        tags.push(Json::obj().with("tag", t).with("count", n));
    }
    Ok(Response::json(200, &Json::obj().with("result", Json::obj().with("tags", tags))))
}
