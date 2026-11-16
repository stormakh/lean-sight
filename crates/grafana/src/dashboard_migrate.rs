//! Classic dashboard JSON migration to the latest schema version (42), as the v0alpha1 -> v1 conversion runs it.
//! Upstream: apps/dashboard/pkg/migration (migrate.go, frontend_defaults.go, schemaversion/v14, v40-v42).
//!
//! Ported: the frontend defaults and save-model cleanup, the built-in annotation, panel id and refId repair,
//! Angular panel auto-migration, and the schema steps V40-V42. Older dashboards (schemaVersion < 39) are migrated
//! only when they hold no panels, rows, variables or annotations (then only V14, V40 and V41 change anything);
//! otherwise `migrate` returns an error and the caller answers with the stored spec and a failed conversion
//! status, which the frontend migrates itself.
use json::Json;

pub const LATEST_VERSION: i64 = 42;

fn nil(v: Option<&Json>) -> bool {
    matches!(v, None | Some(Json::Null))
}

fn num(v: &Json) -> Option<f64> {
    match v {
        Json::Int(i) => Some(*i as f64),
        Json::Float(f) => Some(*f),
        _ => None,
    }
}

/// Go's `isEqual` of frontend_defaults.go: same type and value, numbers compared as float64.
fn is_equal(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Null, Json::Null) => true,
        (Json::Bool(x), Json::Bool(y)) => x == y,
        (Json::Str(x), Json::Str(y)) => x == y,
        (Json::Arr(x), Json::Arr(y)) => x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| is_equal(p, q)),
        (Json::Obj(x), Json::Obj(y)) => {
            if x.len() != y.len() {
                return false;
            }
            for e in x.iter() {
                match b.get(&e.0) {
                    Some(v) => {
                        if !is_equal(&e.1, v) {
                            return false;
                        }
                    }
                    None => return false,
                }
            }
            true
        }
        _ => match (num(a), num(b)) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        },
    }
}

fn default_if_nil(d: &mut Json, k: &str, v: Json) {
    if nil(d.get(k)) {
        d.set(k, v);
    }
}

fn schema_version(d: &Json) -> i64 {
    match d.get("schemaVersion") {
        Some(Json::Int(i)) => *i,
        Some(Json::Float(f)) => *f as i64,
        Some(Json::Str(s)) => s.parse::<i64>().unwrap_or(0),
        _ => 0,
    }
}

/// Migrates `dash` in place to schema version 42. On error the value may be partly changed: migrate a copy.
pub fn migrate(dash: &mut Json) -> Result<(), String> {
    if dash.as_obj().is_none() {
        return Err("dashboard is nil".to_string());
    }
    dash.remove("__elements");
    dash.remove("__inputs");
    dash.remove("__requires");

    track_markers(dash);
    apply_frontend_defaults(dash);
    if let Some(Json::Arr(panels)) = dash.get_mut("panels") {
        for p in panels.iter_mut() {
            if p.as_obj().is_some() {
                apply_panel_defaults(p);
            }
        }
    }
    if dash.get("rows").is_some() {
        return Err("migration of rows (schemaVersion < 16) is not supported".to_string());
    }
    ensure_unique_panel_ids(dash);

    let input = schema_version(dash);
    dash.set("schemaVersion", input);
    if input > LATEST_VERSION {
        return Err(format!("schema version not migrated to target version: input {input}, target {LATEST_VERSION}"));
    }
    if input < 39 {
        if !is_trivial(dash) {
            return Err(format!("migration from schema version {input} is not supported"));
        }
        if input < 14 {
            let shared = dash.get("sharedCrosshair").and_then(Json::as_bool).unwrap_or(false);
            dash.set("graphTooltip", if shared { 1 } else { 0 });
            dash.remove("sharedCrosshair");
        }
    }
    if input < 40 {
        let r = dash.get("refresh").and_then(Json::as_str).unwrap_or("").to_string();
        dash.set("refresh", r);
    }
    if input < 41 {
        if let Some(tp) = dash.get_mut("timepicker") {
            if tp.as_obj().is_some() {
                tp.remove("time_options");
            }
        }
    }
    if input < 42 {
        if let Some(Json::Arr(panels)) = dash.get_mut("panels") {
            for p in panels.iter_mut() {
                migrate_hide_from(p);
            }
        }
    }
    dash.set("schemaVersion", LATEST_VERSION);

    add_builtin_annotation(dash);
    cleanup_dashboard(dash);
    Ok(())
}

/// No content that the unported schema steps (V2-V39) would change.
fn is_trivial(d: &Json) -> bool {
    for k in ["services", "pulldowns", "nav", "rows"] {
        if d.get(k).is_some() {
            return false;
        }
    }
    let empty = |v: Option<&Json>| match v {
        None | Some(Json::Null) => true,
        Some(Json::Arr(a)) => a.is_empty(),
        _ => false,
    };
    if !empty(d.get("panels")) {
        return false;
    }
    if !empty(d.path("templating.list")) {
        return false;
    }
    if let Some(Json::Arr(a)) = d.path("annotations.list") {
        for x in a.iter() {
            if num(x.get("builtIn").unwrap_or(&Json::Null)) != Some(1.0) {
                return false;
            }
        }
    }
    true
}

fn track_panel_markers(p: &mut Json) {
    if p.get("transformations").is_some() {
        p.set("_originallyHadTransformations", true);
    }
    if p.path("fieldConfig.defaults.custom").is_some() && p.path("fieldConfig.defaults").and_then(Json::as_obj).is_some() {
        p.set("_originallyHadFieldConfigCustom", true);
    }
    if let Some(Json::Arr(nested)) = p.get_mut("panels") {
        for n in nested.iter_mut() {
            if n.as_obj().is_some() {
                track_panel_markers(n);
            }
        }
    }
}

fn track_markers(d: &mut Json) {
    if let Some(Json::Arr(panels)) = d.get_mut("panels") {
        for p in panels.iter_mut() {
            if p.as_obj().is_some() {
                track_panel_markers(p);
            }
        }
    }
}

fn apply_frontend_defaults(d: &mut Json) {
    default_if_nil(d, "title", Json::from("No Title"));
    default_if_nil(d, "tags", Json::arr());
    default_if_nil(d, "timezone", Json::from(""));
    default_if_nil(d, "weekStart", Json::from(""));
    default_if_nil(d, "editable", Json::Bool(true));
    default_if_nil(d, "graphTooltip", Json::Int(0));
    default_if_nil(d, "time", Json::obj().with("from", "now-6h").with("to", "now"));
    default_if_nil(d, "timepicker", Json::obj());
    default_if_nil(d, "schemaVersion", Json::Int(0));
    default_if_nil(d, "fiscalYearStartMonth", Json::Int(0));
    default_if_nil(d, "links", Json::arr());
    for k in ["templating", "annotations"] {
        let ok = match d.get_mut(k) {
            Some(t) if t.as_obj().is_some() => {
                if nil(t.get("list")) {
                    t.set("list", Json::arr());
                }
                true
            }
            _ => false,
        };
        if !ok {
            d.set(k, Json::obj().with("list", Json::arr()));
        }
    }
    sort_top_panels_by_grid(d, false);
    // initMeta: "meta" is dropped again by the cleanup, so only the key matters.
    if d.get("meta").and_then(Json::as_obj).is_none() {
        d.set("meta", Json::obj());
    }
    // removeNullValuesFromVariables
    if let Some(Json::Arr(list)) = d.get_mut("templating").and_then(|t| t.get_mut("list")) {
        for v in list.iter_mut() {
            if let Some(cur) = v.get_mut("current") {
                if cur.as_obj().is_some() {
                    let drop = match cur.get("value") {
                        Some(Json::Null) => true,
                        Some(Json::Arr(a)) => a.iter().any(|x| x.is_null()),
                        _ => false,
                    };
                    if drop {
                        cur.remove("value");
                    }
                }
            }
        }
    }
}

fn grid(p: &Json, k: &str, default_missing: Option<f64>) -> Option<f64> {
    match p.get("gridPos") {
        Some(g) if g.as_obj().is_some() => match g.get(k) {
            Some(v) => num(v),
            None => None,
        },
        _ => default_missing,
    }
}

/// sortPanelsByGridPos (defaults phase, `lenient` false: a panel without numeric gridPos never moves) and
/// sortPanelsByGridPosition (cleanup phase, `lenient` true: missing values count as 0).
fn sort_panels(panels: &mut [Json], lenient: bool) {
    // insertion sort: stable, and the comparator is not a total order in the strict variant (as Go's SliceStable)
    for i in 1..panels.len() {
        let mut j = i;
        while j > 0 && less(&panels[j], &panels[j - 1], lenient) {
            panels.swap(j, j - 1);
            j -= 1;
        }
    }
}

fn less(a: &Json, b: &Json, lenient: bool) -> bool {
    let d = if lenient { Some(0.0) } else { None };
    let ya = if lenient { grid(a, "y", d).or(Some(0.0)) } else { grid(a, "y", None) };
    let yb = if lenient { grid(b, "y", d).or(Some(0.0)) } else { grid(b, "y", None) };
    let (ya, yb) = match (ya, yb) {
        (Some(x), Some(y)) => (x, y),
        _ => return false,
    };
    if ya == yb {
        let xa = if lenient { grid(a, "x", d).or(Some(0.0)) } else { grid(a, "x", None) };
        let xb = if lenient { grid(b, "x", d).or(Some(0.0)) } else { grid(b, "x", None) };
        return match (xa, xb) {
            (Some(x), Some(y)) => x < y,
            _ => false,
        };
    }
    ya < yb
}

fn sort_top_panels_by_grid(d: &mut Json, lenient: bool) {
    if let Some(Json::Arr(panels)) = d.get_mut("panels") {
        sort_panels(panels, lenient);
    }
}

fn apply_panel_defaults(p: &mut Json) {
    default_if_nil(p, "gridPos", Json::obj().with("x", 0).with("y", 0).with("h", 3).with("w", 6));
    default_if_nil(p, "targets", Json::arr().push_item(Json::obj().with("refId", "A")));
    default_if_nil(p, "cachedPluginOptions", Json::obj());
    default_if_nil(p, "transparent", Json::Bool(false));
    default_if_nil(p, "options", Json::obj());
    default_if_nil(p, "links", Json::arr());
    match p.get_mut("fieldConfig") {
        None => p.set("fieldConfig", Json::obj().with("defaults", Json::obj()).with("overrides", Json::arr())),
        Some(fc) => {
            if fc.as_obj().is_some() {
                if fc.get("defaults").is_none() {
                    fc.set("defaults", Json::obj());
                }
                if fc.get("overrides").is_none() {
                    fc.set("overrides", Json::arr());
                }
            }
        }
    }
    default_if_nil(p, "title", Json::from(""));
    ensure_query_ids(p);
}

fn ensure_query_ids(p: &mut Json) {
    let targets = match p.get_mut("targets") {
        Some(Json::Arr(t)) if !t.is_empty() => t,
        _ => return,
    };
    let missing = targets.iter().any(|t| t.as_obj().is_some() && nil(t.get("refId")));
    if !missing {
        return;
    }
    let mut used: Vec<String> = Vec::new();
    for t in targets.iter() {
        if let Some(Json::Str(r)) = t.get("refId") {
            used.push(r.clone());
        }
    }
    let letters = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut i = 0;
    for t in targets.iter_mut() {
        if t.as_obj().is_some() && nil(t.get("refId")) {
            while i < letters.len() {
                let r = (letters[i] as char).to_string();
                if !used.contains(&r) {
                    t.set("refId", r.as_str());
                    used.push(r);
                    break;
                }
                i += 1;
            }
            i += 1;
        }
    }
}

/// Every panel as (top index, nested index), nested row panels right after their row (panelIterator order).
fn panel_paths(d: &Json) -> Vec<(usize, Option<usize>)> {
    let mut out = Vec::new();
    if let Some(Json::Arr(panels)) = d.get("panels") {
        for (i, p) in panels.iter().enumerate() {
            if p.as_obj().is_none() {
                continue;
            }
            out.push((i, None));
            if let Some(Json::Arr(nested)) = p.get("panels") {
                for (j, n) in nested.iter().enumerate() {
                    if n.as_obj().is_some() {
                        out.push((i, Some(j)));
                    }
                }
            }
        }
    }
    out
}

fn panel_at(d: &mut Json, at: (usize, Option<usize>)) -> Option<&mut Json> {
    let top = match d.get_mut("panels") {
        Some(Json::Arr(p)) => p.get_mut(at.0)?,
        _ => return None,
    };
    match at.1 {
        None => Some(top),
        Some(j) => match top.get_mut("panels") {
            Some(Json::Arr(n)) => n.get_mut(j),
            _ => None,
        },
    }
}

fn ensure_unique_panel_ids(d: &mut Json) {
    let paths = panel_paths(d);
    if paths.is_empty() {
        return;
    }
    let mut max = 0f64;
    for at in paths.iter() {
        if let Some(p) = panel_at(d, *at) {
            if let Some(id) = p.get("id").and_then(num) {
                if id > 0.0 && id.trunc() > max {
                    max = id.trunc();
                }
            }
        }
    }
    let mut next = max as i64 + 1;
    let mut seen: Vec<f64> = Vec::new();
    for at in paths.iter() {
        let p = match panel_at(d, *at) {
            Some(p) => p,
            None => continue,
        };
        match p.get("id").and_then(num) {
            Some(id) if id > 0.0 => {
                if seen.contains(&id) {
                    p.set("id", next);
                    next += 1;
                } else {
                    seen.push(id);
                }
            }
            _ => {
                p.set("id", next);
                next += 1;
            }
        }
    }
}

fn migrate_hide_from(p: &mut Json) {
    if let Some(Json::Arr(overrides)) = p.get_mut("fieldConfig").and_then(|f| f.get_mut("overrides")) {
        for o in overrides.iter_mut() {
            if let Some(Json::Arr(props)) = o.get_mut("properties") {
                for prop in props.iter_mut() {
                    if prop.get("id").and_then(Json::as_str) == Some("custom.hideFrom") {
                        if let Some(v) = prop.get_mut("value") {
                            if v.as_obj().is_some() && v.get("viz").and_then(Json::as_bool) == Some(true) {
                                v.set("tooltip", true);
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(Json::Arr(nested)) = p.get_mut("panels") {
        for n in nested.iter_mut() {
            if n.as_obj().is_some() {
                migrate_hide_from(n);
            }
        }
    }
}

fn add_builtin_annotation(d: &mut Json) {
    let list = match d.get_mut("annotations").and_then(|a| a.get_mut("list")) {
        Some(Json::Arr(l)) => l,
        _ => return,
    };
    for a in list.iter() {
        if a.get("builtIn").and_then(num) == Some(1.0) {
            return;
        }
    }
    let b = Json::obj()
        .with("builtIn", 1)
        .with("datasource", Json::obj().with("type", "grafana").with("uid", "-- Grafana --"))
        .with("enable", true)
        .with("hide", true)
        .with("iconColor", "rgba(0, 211, 255, 1)")
        .with("name", "Annotations & Alerts")
        .with("type", "dashboard");
    list.insert(0, b);
}

fn remove_nulls(v: &mut Json) {
    match v {
        Json::Obj(o) => {
            o.retain(|e| !e.1.is_null());
            for e in o.iter_mut() {
                remove_nulls(&mut e.1);
            }
        }
        Json::Arr(a) => {
            for x in a.iter_mut() {
                remove_nulls(x);
            }
        }
        _ => {}
    }
}

fn remove_if_default(d: &mut Json, k: &str, def: &Json) {
    let eq = match d.get(k) {
        Some(v) => is_equal(v, def),
        None => false,
    };
    if eq {
        d.remove(k);
    }
}

fn cleanup_dashboard(d: &mut Json) {
    for k in [
        "events",
        "meta",
        "originalTime",
        "originalTemplating",
        "originalLibraryPanels",
        "panelInEdit",
        "panelInView",
        "getVariablesFromState",
        "formatDate",
        "appEventsSubscription",
        "panelsAffectedByVariableChange",
        "lastRefresh",
        "timeRangeUpdatedDuringEditOrView",
        "originalDashboard",
        "variables",
    ] {
        d.remove(k);
    }
    remove_if_default(d, "gnetId", &Json::Null);
    if let Some(t) = d.get_mut("templating") {
        if t.as_obj().is_some() {
            remove_nulls(t);
            if let Some(Json::Arr(list)) = t.get_mut("list") {
                for v in list.iter_mut() {
                    cleanup_variable(v);
                }
            }
        }
    }
    let panels = match d.remove("panels") {
        Some(Json::Arr(p)) => p,
        _ => Vec::new(),
    };
    let mut kept: Vec<Json> = Vec::with_capacity(panels.len());
    for p in panels {
        if p.as_obj().is_none() {
            continue;
        }
        if truthy(p.get("repeatPanelId")) || truthy(p.get("repeatedByRow")) {
            continue;
        }
        kept.push(p);
    }
    for p in kept.iter_mut() {
        cleanup_panel(p, false);
        if let Some(Json::Arr(nested)) = p.get_mut("panels") {
            for n in nested.iter_mut() {
                if n.as_obj().is_some() {
                    cleanup_panel(n, true);
                }
            }
        }
    }
    sort_panels(&mut kept, true);
    d.set("panels", Json::Arr(kept));
    remove_if_default(d, "style", &Json::from("dark"));
    remove_if_default(d, "hideControls", &Json::Bool(false));
    remove_if_default(d, "id", &Json::Null);
    for k in ["version", "iteration", "nav", "pulldowns"] {
        d.remove(k);
    }
}

fn truthy(v: Option<&Json>) -> bool {
    match v {
        None | Some(Json::Null) => false,
        Some(Json::Bool(b)) => *b,
        Some(Json::Str(s)) => !s.is_empty(),
        Some(Json::Int(i)) => *i != 0,
        Some(Json::Float(f)) => *f != 0.0,
        _ => true,
    }
}

fn cleanup_variable(v: &mut Json) {
    if v.as_obj().is_none() {
        return;
    }
    remove_if_default(v, "datasource", &Json::Null);
    v.remove("index");
    let ty = v.get("type").and_then(Json::as_str).unwrap_or("").to_string();
    match ty.as_str() {
        "query" => {
            let refresh = v.get("refresh").and_then(num).map(|f| f as i64).unwrap_or(1);
            if refresh != 0 && v.get("options").is_none() {
                v.set("options", Json::arr());
            }
        }
        "constant" => {
            v.remove("options");
        }
        "datasource" => v.set("options", Json::arr()),
        _ => {}
    }
}

fn angular_target(ty: &str, p: &Json) -> Option<&'static str> {
    match ty {
        "graph" | "graphite" => {
            let mode = p.path("xaxis.mode").and_then(Json::as_str).unwrap_or("");
            Some(match mode {
                "series" => {
                    if p.get("legend").and_then(Json::as_obj).is_some() && p.path("legend.values").and_then(Json::as_bool) == Some(true) {
                        "bargauge"
                    } else {
                        "barchart"
                    }
                }
                "histogram" => "histogram",
                _ => "timeseries",
            })
        }
        "table-old" => Some("table"),
        "singlestat" | "grafana-singlestat-panel" => Some("stat"),
        "grafana-piechart-panel" => Some("piechart"),
        "grafana-worldmap-panel" => Some("geomap"),
        "natel-discrete-panel" => Some("state-timeline"),
        _ => None,
    }
}

fn panel_defaults() -> [(&'static str, Json); 8] {
    [
        ("gridPos", Json::obj().with("x", 0).with("y", 0).with("h", 3).with("w", 6)),
        ("targets", Json::arr().push_item(Json::obj().with("refId", "A"))),
        ("cachedPluginOptions", Json::obj()),
        ("transparent", Json::Bool(false)),
        ("options", Json::obj()),
        ("links", Json::arr()),
        ("fieldConfig", Json::obj().with("defaults", Json::obj()).with("overrides", Json::arr())),
        ("title", Json::from("")),
    ]
}

fn cleanup_panel(p: &mut Json, nested: bool) {
    // applyPanelAutoMigration
    if let Some(ty) = p.get("type").and_then(Json::as_str).map(|s| s.to_string()) {
        if let Some(n) = angular_target(&ty, p) {
            p.set("autoMigrateFrom", ty.as_str());
            p.set("type", n);
        }
    }
    // library panels keep only id, title, gridPos, libraryPanel
    if !nil(p.get("libraryPanel")) {
        let mut out = Json::obj();
        for k in ["id", "title", "gridPos", "libraryPanel"] {
            if let Some(v) = p.get(k) {
                if !v.is_null() {
                    out.set(k, v.clone());
                }
            }
        }
        *p = out;
        return;
    }
    if p.get("type").and_then(Json::as_str) == Some("row") {
        remove_if_default(p, "repeat", &Json::from(""));
    }
    let original: Vec<String> = p.as_obj().map(|o| o.iter().map(|e| e.0.clone()).collect()).unwrap_or_default();
    for k in [
        "events",
        "isViewing",
        "isEditing",
        "isInView",
        "hasRefreshed",
        "cachedPluginOptions",
        "plugin",
        "queryRunner",
        "replaceVariables",
        "configRev",
        "hasSavedPanelEditChange",
        "getDisplayTitle",
        "dataSupport",
        "key",
        "isNew",
        "refreshWhenInView",
        "scopedVars",
    ] {
        p.remove(k);
    }
    for (k, def) in panel_defaults().iter() {
        remove_if_default(p, k, def);
    }
    let empty_tr = matches!(p.get("transformations"), Some(Json::Arr(t)) if t.is_empty());
    if empty_tr && (p.get("_originallyHadTransformations").and_then(Json::as_bool) != Some(true) || !nested) {
        p.remove("transformations");
    }
    remove_nulls(p);
    filter_default_values(p, &original);
    p.remove("_originallyHadTransformations");
    p.remove("_originallyHadFieldConfigCustom");
}

fn filter_default_values(p: &mut Json, original: &[String]) {
    let ty = p.get("type").and_then(Json::as_str).unwrap_or("").to_string();
    let had_custom = p.get("_originallyHadFieldConfigCustom").and_then(Json::as_bool) == Some(true);
    if ty == "table" {
        let auto_old = p.get("autoMigrateFrom").and_then(Json::as_str) == Some("table-old");
        for k in ["pageSize", "scroll", "fontSize", "showHeader", "sort"] {
            if p.get(k).is_some() && (!auto_old || !original.iter().any(|o| o == k)) {
                p.remove(k);
            }
        }
    }
    // Go compares the gridPos default with int values against float64 panel values: never equal, so it is skipped.
    for (k, def) in panel_defaults().iter() {
        if *k == "gridPos" {
            continue;
        }
        let eq = match p.get(k) {
            Some(v) => is_equal(v, def),
            None => false,
        };
        if !eq {
            continue;
        }
        if *k == "fieldConfig" {
            if had_custom {
                if let Some(defs) = p.get_mut("fieldConfig").and_then(|f| f.get_mut("defaults")) {
                    if defs.as_obj().is_some() {
                        if defs.get("custom").is_none() {
                            defs.set("custom", Json::obj());
                        }
                        continue;
                    }
                }
            }
            p.remove(k);
        } else if !original.iter().any(|o| o == k) {
            p.remove(k);
        }
    }
    remove_if_default(p, "targets", &Json::arr());
    if had_custom && p.get("fieldConfig").is_none() {
        p.set("fieldConfig", Json::obj().with("defaults", Json::obj().with("custom", Json::obj())).with("overrides", Json::arr()));
    }
    let auto_from = p.get("autoMigrateFrom").and_then(Json::as_str).unwrap_or("").to_string();
    if let Some(defs) = p.get_mut("fieldConfig").and_then(|f| f.get_mut("defaults")) {
        if defs.as_obj().is_some() {
            let empty_custom = matches!(defs.get("custom"), Some(Json::Obj(c)) if c.is_empty());
            if empty_custom {
                let mut singlestat = auto_from == "singlestat" || auto_from == "grafana-singlestat-panel";
                if !singlestat {
                    let fixed = defs.path("color.fixedColor").and_then(Json::as_str).is_some() && defs.path("color.mode").and_then(Json::as_str) == Some("fixed");
                    singlestat = fixed && defs.get("mappings").and_then(Json::as_arr).is_some();
                }
                if singlestat {
                    defs.remove("custom");
                }
            }
            if had_custom && defs.get("custom").is_none() {
                defs.set("custom", Json::obj());
            }
        }
    }
}
