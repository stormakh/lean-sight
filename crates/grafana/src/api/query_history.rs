//! Explore query history (agent C). Upstream: pkg/services/queryhistory (api.go, database.go, models.go).
//! Viewers need `datasources:explore`, which OSS grants to Editors (and to Viewers only with viewers_can_edit).
use crate::ctx::{Access, Ctx, Role, Routes};
use crate::error::ApiError;
use crate::store::{QueryHistory, QueryHistoryQuery};
use http::Response;
use json::Json;

pub fn routes(r: &mut Routes) {
    r.post("/api/query-history", Access::SignedIn, create);
    r.get("/api/query-history", Access::SignedIn, search);
    r.delete("/api/query-history/:uid", Access::SignedIn, delete);
    r.post("/api/query-history/star/:uid", Access::SignedIn, star);
    r.delete("/api/query-history/star/:uid", Access::SignedIn, unstar);
    r.patch("/api/query-history/:uid", Access::SignedIn, patch_comment);
}

/// `permissionsMiddleware`: a Viewer without `datasources:explore` gets 401 with the handler's message.
fn check(c: &Ctx<'_>, msg: &str) -> Result<(), ApiError> {
    let viewers_can_edit = c.app.cfg.get_bool("users", "viewers_can_edit");
    if c.user.org_role == Role::Viewer && !viewers_can_edit {
        return Err(ApiError::new(401, msg));
    }
    Ok(())
}

fn dto(q: &QueryHistory) -> Json {
    let queries = Json::parse(&q.queries).unwrap_or(Json::Null);
    Json::obj()
        .with("uid", q.uid.as_str())
        .with("datasourceUid", q.datasource_uid.as_str())
        .with("createdBy", q.created_by)
        .with("createdAt", q.created_at)
        .with("comment", q.comment.as_str())
        .with("queries", queries)
        .with("starred", q.starred)
}

fn result(q: &QueryHistory) -> Response {
    Response::json(200, &Json::obj().with("result", dto(q)))
}

fn create(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    check(c, "Failed to create query history")?;
    let b = c.body_json()?;
    let ds_uid = match crate::promlib::models::get_ci(&b, "datasourceUid") {
        None | Some(Json::Null) => String::new(),
        Some(Json::Str(s)) => s.clone(),
        Some(_) => return Err(ApiError::bad_request("bad request data")),
    };
    // simplejson re-marshals the queries: map keys sorted
    let queries = match crate::promlib::models::get_ci(&b, "queries") {
        Some(v) => super::datasources::sorted_text(v),
        None => "null".to_string(),
    };
    let q = QueryHistory {
        id: 0,
        uid: crate::util::short_uid(),
        org_id: c.user.org_id,
        datasource_uid: ds_uid,
        created_by: c.user.user_id,
        created_at: crate::util::now(),
        comment: String::new(),
        queries,
        starred: false,
    };
    c.app.store.query_history_create(&q).map_err(|_| ApiError::new(500, "Failed to create query history"))?;
    Ok(result(&q))
}

fn search(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    check(c, "Failed to get query history")?;
    let now = crate::util::now_ms() * crate::promlib::models::MS;
    let get = |k: &str| c.req.query_get(k).unwrap_or_default();
    let from = crate::promlib::models::parse_time(&get("from"), now, false) / 1_000_000_000;
    let mut to = crate::promlib::models::parse_time(&get("to"), now, true) / 1_000_000_000;
    if to <= 0 {
        to = crate::util::now();
    }
    let page = get("page").parse::<i64>().unwrap_or(0).max(0);
    let limit = get("limit").parse::<i64>().unwrap_or(0).max(0);
    let q = QueryHistoryQuery {
        org_id: c.user.org_id,
        user_id: c.user.user_id,
        datasource_uids: c.req.query_all("datasourceUid"),
        search: get("searchString"),
        only_starred: get("onlyStarred") == "true",
        sort: get("sort"),
        from,
        to,
        page,
        limit,
    };
    let (rows, total) = c.app.store.query_history_search(&q).map_err(|_| ApiError::new(500, "Failed to get query history"))?;
    let mut list = Json::arr();
    for r in rows.iter() {
        list.push(dto(r));
    }
    let res = Json::obj().with("totalCount", total).with("queryHistory", list).with("page", if page <= 0 { 1 } else { page }).with("perPage", if limit <= 0 { 100 } else { limit });
    Ok(Response::json(200, &Json::obj().with("result", res)))
}

fn uid_param(c: &Ctx<'_>) -> Result<String, ApiError> {
    let uid = c.param("uid").to_string();
    if !uid.is_empty() && !crate::util::valid_uid(&uid) {
        return Err(ApiError::new(404, "Query in query history not found"));
    }
    Ok(uid)
}

fn delete(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    check(c, "Failed to delete query history")?;
    let uid = uid_param(c)?;
    let fail = || ApiError::new(500, "Failed to delete query from query history");
    let q = c.app.store.query_history_by_uid(c.user.org_id, c.user.user_id, &uid).map_err(|_| fail())?;
    c.app.store.query_history_delete(c.user.org_id, c.user.user_id, &uid).map_err(|_| fail())?;
    Ok(Response::json(200, &Json::obj().with("id", q.id).with("message", "Query deleted")))
}

fn set_star(c: &mut Ctx<'_>, on: bool, msg: &str, fail_msg: &str) -> Result<Response, ApiError> {
    check(c, msg)?;
    let uid = uid_param(c)?;
    let fail = || ApiError::new(500, fail_msg);
    let q = c.app.store.query_history_by_uid(c.user.org_id, c.user.user_id, &uid).map_err(|_| fail())?;
    if q.starred == on {
        return Err(fail());
    }
    c.app.store.query_history_star(c.user.org_id, c.user.user_id, &uid, on).map_err(|_| fail())?;
    let q = c.app.store.query_history_by_uid(c.user.org_id, c.user.user_id, &uid).map_err(|_| fail())?;
    Ok(result(&q))
}

fn star(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    set_star(c, true, "Failed to star query history", "Failed to star query in query history")
}

fn unstar(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    set_star(c, false, "Failed to unstar query history", "Failed to unstar query in query history")
}

fn patch_comment(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    check(c, "Failed to update comment of query in query history")?;
    let uid = uid_param(c)?;
    let b = c.body_json()?;
    let comment = match crate::promlib::models::get_ci(&b, "comment") {
        None | Some(Json::Null) => String::new(),
        Some(Json::Str(s)) => s.clone(),
        Some(_) => return Err(ApiError::bad_request("bad request data")),
    };
    let fail = || ApiError::new(500, "Failed to update comment of query in query history");
    c.app.store.query_history_set_comment(c.user.org_id, c.user.user_id, &uid, &comment).map_err(|_| fail())?;
    let q = c.app.store.query_history_by_uid(c.user.org_id, c.user.user_id, &uid).map_err(|_| fail())?;
    Ok(result(&q))
}
