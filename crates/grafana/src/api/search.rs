//! Search: /api/search and /apis/dashboard.grafana.app/v0alpha1/.../search (agent B). Upstream: pkg/api/search.go,
//! pkg/registry/apis/dashboard/search.go, pkg/storage/unified/search/bleve.go.
//!
//! Search reads only the dashboard and folder columns (title, folder, tags), never the dashboard JSON. Text matching
//! follows the bleve query Grafana builds: an exact (case-insensitive) title match, or every query word of 3+
//! characters matching a title word or a title word prefix (edge ngrams of 3..10 characters); queries shorter than
//! 3 characters become a prefix wildcard; `*` patterns match title words or the whole title. Scores are not bleve's
//! BM25 values; they only keep its order (exact title first, then more and fuller word matches).
use crate::api::dashboards::{dashboard_url, folder_url};
use crate::ctx::{Access, Ctx, Role, Routes};
use crate::error::ApiError;
use crate::k8s;
use crate::util::slugify;
use http::Response;
use json::Json;

pub fn routes(r: &mut Routes) {
    r.get("/api/search", Access::SignedIn, legacy_search);
    r.get("/api/search/sorting", Access::SignedIn, sort_options);
    r.get("/apis/dashboard.grafana.app/:ver/namespaces/:ns/search", Access::SignedIn, k8s_search);
}

/// One searchable item: a dashboard or a folder.
struct Item {
    is_folder: bool,
    id: i64,
    uid: String,
    title: String,
    /// Parent folder uid ("" for the root).
    folder: String,
    tags: Vec<String>,
    score: f64,
}

fn items(c: &Ctx<'_>, dashboards: bool, folders: bool) -> Result<Vec<Item>, ApiError> {
    let mut out = Vec::new();
    if folders {
        for f in c.app.store.folders(c.user.org_id)? {
            out.push(Item { is_folder: true, id: f.id, uid: f.uid, title: f.title, folder: f.parent_uid, tags: Vec::new(), score: 0.0 });
        }
    }
    if dashboards {
        for d in c.app.store.dashboards(c.user.org_id)? {
            out.push(Item { is_folder: false, id: d.id, uid: d.uid, title: d.title, folder: d.folder_uid, tags: d.tags, score: 0.0 });
        }
    }
    Ok(out)
}

fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    for w in s.split(|ch: char| !ch.is_alphanumeric()) {
        if !w.is_empty() {
            out.push(w.to_lowercase());
        }
    }
    out
}

/// Glob match with `*` (any run) and `?` (one character).
fn glob(p: &[char], s: &[char]) -> bool {
    let (mut pi, mut si) = (0, 0);
    let (mut star, mut mark) = (usize::MAX, 0);
    while si < s.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = pi;
            pi += 1;
            mark = si;
        } else if star != usize::MAX {
            pi = star + 1;
            mark += 1;
            si = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// A prepared text query.
enum Text {
    All,
    Wildcard(Vec<char>),
    Match { phrase: String, terms: Vec<String> },
}

fn text_query(q: &str) -> Text {
    let mut q = q.to_string();
    if q == "*" {
        return Text::All;
    }
    if !q.is_empty() && q.len() < 3 && !q.contains('*') {
        q.push('*');
    }
    if q.len() <= 1 {
        return Text::All;
    }
    if q.contains('*') {
        return Text::Wildcard(q.to_lowercase().chars().collect());
    }
    let mut terms = Vec::new();
    for w in q.split_whitespace() {
        if w.len() >= 3 {
            for t in words(w) {
                terms.push(t);
            }
        }
    }
    Text::Match { phrase: q.trim_end_matches(' ').to_lowercase(), terms }
}

/// Some(score) when the title matches.
fn score(t: &Text, title: &str) -> Option<f64> {
    match t {
        Text::All => Some(0.0),
        Text::Wildcard(p) => {
            let lt = title.to_lowercase();
            let whole: Vec<char> = lt.chars().collect();
            if glob(p, &whole) {
                return Some(0.0);
            }
            for w in words(title) {
                let wc: Vec<char> = w.chars().collect();
                if glob(p, &wc) {
                    return Some(0.0);
                }
            }
            None
        }
        Text::Match { phrase, terms } => {
            let lt = title.to_lowercase();
            let tw = words(title);
            let mut s = 0.0;
            if lt == *phrase {
                s += 10.0;
            }
            if !terms.is_empty() {
                let mut all_words = true;
                let mut all_prefix = true;
                let mut part = 0.0;
                for t in terms.iter() {
                    let exact = tw.iter().any(|w| w == t);
                    let pre: String = t.chars().take(10).collect();
                    let prefix = tw.iter().find(|w| w.starts_with(pre.as_str()));
                    all_words &= exact;
                    match prefix {
                        Some(w) => part += pre.chars().count() as f64 / w.chars().count().max(1) as f64,
                        None => all_prefix = false,
                    }
                }
                if all_words {
                    s += 2.0 * terms.len() as f64 / tw.len().max(1) as f64;
                }
                if all_prefix {
                    s += part / tw.len().max(1) as f64;
                }
            }
            if s > 0.0 { Some(s / 10.0) } else { None }
        }
    }
}

fn by_title(a: &Item, b: &Item) -> std::cmp::Ordering {
    a.title.to_lowercase().cmp(&b.title.to_lowercase()).then_with(|| a.uid.cmp(&b.uid))
}

fn qint(c: &Ctx<'_>, k: &str) -> Option<i64> {
    c.req.query_get(k).and_then(|v| v.parse::<i64>().ok())
}

fn k8s_search(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    k8s::check_namespace(c)?;
    let types = c.req.query_all("type");
    let has_dash = types.is_empty() || types.iter().any(|t| t == "dashboard");
    let has_folder = types.is_empty() || types.iter().any(|t| t == "folder");
    let (want_dash, want_folder) = if !has_dash && !has_folder { (true, true) } else { (has_dash, has_folder) };
    let limit = qint(c, "limit").unwrap_or(50);
    let limit = if limit <= 0 { 50 } else { limit } as usize;
    let mut offset = qint(c, "offset").unwrap_or(0).max(0) as usize;
    if c.req.query_get("offset").is_none() {
        if let Some(p) = qint(c, "page") {
            if p > 1 {
                offset = (p as usize - 1) * limit;
            }
        }
    }
    let permission = c.req.query_get("permission").unwrap_or_default().to_lowercase();
    if permission == "create" && (has_dash || !has_folder) {
        return Err(k8s::status_error(400, "BadRequest", "permission=create is only supported with type=folder", None));
    }
    let empty = || Response::json_text(200, "{\"totalHits\":0,\"hits\":[]}\n".to_string());
    if (permission == "edit" || permission == "create" || permission == "admin") && !c.user.has_role(if permission == "admin" { Role::Admin } else { Role::Editor }) {
        return Ok(empty());
    }
    let tags = c.req.query_all("tag");
    let names = c.req.query_all("name");
    let folder = c.req.query_get("folder").map(|f| if f == "general" { String::new() } else { f });
    let q = c.req.query_get("query").unwrap_or_default();
    let text = text_query(&q);
    let scored = matches!(text, Text::Match { .. });
    let mut hits: Vec<Item> = Vec::new();
    for mut it in items(c, want_dash, want_folder)? {
        if !tags.is_empty() && !tags.iter().all(|t| it.tags.contains(t)) {
            continue;
        }
        if !names.is_empty() && !names.contains(&it.uid) {
            continue;
        }
        if let Some(f) = folder.as_ref() {
            if it.folder != *f {
                continue;
            }
        }
        match score(&text, &it.title) {
            Some(s) => it.score = s,
            None => continue,
        }
        hits.push(it);
    }
    let mut sort_desc = None;
    for s in c.req.query_all("sort") {
        let (f, desc) = match s.strip_prefix('-') {
            Some(f) => (f.to_string(), true),
            None => (s.clone(), false),
        };
        if f == "title" || f == "name_sort" || f == "alpha" {
            sort_desc = Some(desc);
        }
    }
    match sort_desc {
        Some(desc) => hits.sort_by(|a, b| if desc { by_title(b, a) } else { by_title(a, b) }),
        None if scored => hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal).then_with(|| by_title(a, b))),
        None => hits.sort_by(by_title),
    }
    let total = hits.len();
    if total == 0 {
        return Ok(empty());
    }
    let max_score = if scored { hits.iter().map(|h| h.score).fold(0.0, f64::max) } else { 1.0 };
    let mut out = String::with_capacity(64 + hits.len() * 96);
    out.push_str("{\"totalHits\":");
    json::write_i64(total as i64, &mut out);
    out.push_str(",\"hits\":[");
    for (i, h) in hits.iter().skip(offset).take(limit).enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(if h.is_folder { "{\"resource\":\"folders\",\"name\":" } else { "{\"resource\":\"dashboards\",\"name\":" });
        json::write_str(&h.uid, &mut out);
        out.push_str(",\"title\":");
        json::write_str(&h.title, &mut out);
        if !h.tags.is_empty() {
            out.push_str(",\"tags\":[");
            for (j, t) in h.tags.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                json::write_str(t, &mut out);
            }
            out.push(']');
        }
        if !h.folder.is_empty() {
            out.push_str(",\"folder\":");
            json::write_str(&h.folder, &mut out);
        }
        if scored && h.score != 0.0 {
            out.push_str(",\"score\":");
            json::write_f64(h.score, &mut out);
        }
        out.push('}');
    }
    out.push_str("],\"queryCost\":");
    json::write_f64((total * 100) as f64, &mut out);
    out.push_str(",\"maxScore\":");
    json::write_f64(max_score, &mut out);
    out.push_str("}\n");
    Ok(Response::json_text(200, out))
}

fn legacy_search(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let limit = qint(c, "limit").unwrap_or(0);
    if limit > 5000 {
        return Err(ApiError::new(422, "Limit is above maximum allowed (5000), use page parameter to access hits beyond limit"));
    }
    let limit = if limit <= 0 { 1000 } else { limit } as usize;
    let page = qint(c, "page").unwrap_or(1).max(1) as usize;
    if c.req.query_get("deleted").as_deref() == Some("true") {
        if !c.user.has_role(Role::Admin) {
            return Err(ApiError::new(401, "Unauthorized"));
        }
        return Ok(Response::json(200, &Json::arr()));
    }
    let dash_ids: Vec<i64> = c.req.query_all("dashboardIds").iter().filter_map(|s| s.parse().ok()).collect();
    let mut dash_uids = c.req.query_all("dashboardUIDs");
    if dash_uids.is_empty() {
        dash_uids = c.req.query_all("dashboardUID");
    }
    let folder_ids: Vec<i64> = c.req.query_all("folderIds").iter().filter_map(|s| s.parse().ok()).collect();
    let folder_uids = c.req.query_all("folderUIDs");
    if (!dash_ids.is_empty() && !dash_uids.is_empty()) || (!folder_ids.is_empty() && !folder_uids.is_empty()) {
        return Err(ApiError::new(400, "search supports UIDs or IDs, not both"));
    }
    if c.req.query_get("permission").as_deref() == Some("Edit") && !c.user.has_role(Role::Editor) {
        return Ok(Response::json(200, &Json::arr()));
    }
    let ty = c.req.query_get("type").unwrap_or_default();
    let (want_dash, want_folder) = match ty.as_str() {
        "dash-db" => (true, false),
        "dash-folder" => (false, true),
        _ => (true, true),
    };
    let tags = c.req.query_all("tag");
    let starred_only = c.req.query_get("starred").as_deref() == Some("true");
    let stars: Vec<String> = if c.user.is_signed_in() { c.app.store.stars_of_user(c.user.org_id, c.user.user_id).unwrap_or_default() } else { Vec::new() };
    let folders = c.app.store.folders(c.user.org_id)?;
    // folder filter: uids ("general" or "" is the root); ids (0 is the root)
    let mut folder_filter: Option<Vec<String>> = None;
    if !folder_uids.is_empty() {
        folder_filter = Some(folder_uids.iter().map(|u| if u == "general" { String::new() } else { u.clone() }).collect());
    } else if !folder_ids.is_empty() {
        let mut v = Vec::new();
        for id in folder_ids.iter() {
            if *id == 0 {
                v.push(String::new());
            } else if let Some(f) = folders.iter().find(|f| f.id == *id) {
                v.push(f.uid.clone());
            }
        }
        folder_filter = Some(v);
    }
    let text = text_query(&c.req.query_get("query").unwrap_or_default());
    let mut hits: Vec<Item> = Vec::new();
    for it in items(c, want_dash, want_folder)? {
        if !tags.is_empty() && !tags.iter().all(|t| it.tags.contains(t)) {
            continue;
        }
        if starred_only && !stars.contains(&it.uid) {
            continue;
        }
        if !dash_uids.is_empty() && (it.is_folder || !dash_uids.contains(&it.uid)) {
            continue;
        }
        if !dash_ids.is_empty() && (it.is_folder || !dash_ids.contains(&it.id)) {
            continue;
        }
        if let Some(ff) = folder_filter.as_ref() {
            if !ff.contains(&it.folder) {
                continue;
            }
        }
        if score(&text, &it.title).is_none() {
            continue;
        }
        hits.push(it);
    }
    let desc = c.req.query_get("sort").as_deref() == Some("alpha-desc");
    hits.sort_by(|a, b| {
        if a.is_folder != b.is_folder {
            return if a.is_folder { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater };
        }
        let o = a.title.to_lowercase().cmp(&b.title.to_lowercase()).then_with(|| a.uid.cmp(&b.uid));
        if desc { o.reverse() } else { o }
    });
    let mut out = Json::arr();
    for h in hits.iter().skip((page - 1) * limit).take(limit) {
        let slug = slugify(&h.title);
        let url = if h.is_folder { folder_url(c.app, &h.uid, &h.title) } else { dashboard_url(c.app, &h.uid, &slug) };
        let mut tags = Json::arr();
        for t in h.tags.iter() {
            tags.push(t.as_str());
        }
        let mut o = Json::obj()
            .with("id", h.id)
            .with("uid", h.uid.as_str())
            .with("orgId", c.user.org_id)
            .with("title", h.title.as_str())
            .with("uri", format!("db/{slug}"))
            .with("url", url)
            .with("slug", "")
            .with("type", if h.is_folder { "dash-folder" } else { "dash-db" })
            .with("tags", tags)
            .with("isStarred", stars.contains(&h.uid));
        if !h.folder.is_empty() {
            if let Some(f) = folders.iter().find(|f| f.uid == h.folder) {
                o.set("folderId", f.id);
                o.set("folderUid", f.uid.as_str());
                o.set("folderTitle", f.title.as_str());
                o.set("folderUrl", folder_url(c.app, &f.uid, &f.title));
            } else {
                o.set("folderUid", h.folder.as_str());
            }
        }
        o.set("sortMeta", 0);
        o.set("isDeleted", false);
        out.push(o);
    }
    Ok(Response::json(200, &out))
}

fn sort_options(_c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let opt = |name: &str, display: &str, desc: &str, meta: &str| Json::obj().with("name", name).with("displayName", display).with("description", desc).with("meta", meta);
    let list = Json::arr()
        .push_item(opt("alpha-asc", "Alphabetically (A–Z)", "Sort results in an alphabetically ascending order", ""))
        .push_item(opt("alpha-desc", "Alphabetically (Z–A)", "Sort results in an alphabetically descending order", ""));
    Ok(Response::json(200, &Json::obj().with("sortOptions", list)))
}
