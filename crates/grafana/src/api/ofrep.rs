//! OpenFeature remote evaluation (OFREP) with Grafana's static provider (upstream pkg/services/ofrep/static.go,
//! pkg/services/featuremgmt/static_evaluator.go). The flags are Grafana 13.3.1's standard flags with their default
//! values (`captured/ofrep-flags.json`, from the recording), then the `[feature_toggles]` settings (`enable = a,b` and
//! `<flag> = <value>`), as upstream `buildStaticFlagsMapFromCfg`. `dashboardNewLayouts` is always false: grafana-rs
//! serves only the v1 dashboard API. Both answers are built once.
use crate::ctx::{Access, Ctx, Routes};
use crate::error::ApiError;
use crate::settings::Settings;
use http::Response;
use json::Json;
use std::sync::OnceLock;

pub fn routes(r: &mut Routes) {
    // No auth middleware upstream: the static provider answers everyone the same.
    r.post("/ofrep/v1/evaluate/flags", Access::Public, all_flags);
    r.post("/ofrep/v1/evaluate/flags/:flagKey", Access::Public, one_flag);
    // Deprecated path that upstream keeps for older clients.
    r.post("/apis/features.grafana.app/v0alpha1/namespaces/:namespace/ofrep/v1/evaluate/flags", Access::Public, all_flags);
    r.post("/apis/features.grafana.app/v0alpha1/namespaces/:namespace/ofrep/v1/evaluate/flags/:flagKey", Access::Public, one_flag);
}

const REASON: &str = "static provider evaluation result";

struct Flags {
    all: String,
    /// (key, single-flag body)
    one: Vec<(String, String)>,
    /// Keys of the flags whose value is `true` (boot data `featureToggles`).
    enabled: Vec<String>,
}

/// The flags that evaluate to `true`, in flag order (boot data `settings.featureToggles`).
pub fn enabled_flags(cfg: &Settings) -> &'static [String] {
    &flags(cfg).enabled
}

fn flags(cfg: &Settings) -> &'static Flags {
    static F: OnceLock<Flags> = OnceLock::new();
    F.get_or_init(|| build(cfg))
}

/// Grafana's `setting.ParseFlag`: int, then float, then JSON object, then bool, else the string.
fn parse_flag(v: &str) -> Json {
    if let Ok(i) = v.parse::<i64>() {
        return Json::Int(i);
    }
    if let Ok(f) = v.parse::<f64>() {
        if f.is_finite() {
            return Json::Float(f);
        }
    }
    if v.starts_with('{') {
        if let Ok(j) = Json::parse(v) {
            if let Json::Obj(_) = j {
                return j;
            }
        }
    }
    match v {
        "1" | "t" | "T" | "TRUE" | "true" | "True" => Json::Bool(true),
        "0" | "f" | "F" | "FALSE" | "false" | "False" => Json::Bool(false),
        _ => Json::Str(v.to_string()),
    }
}

/// Grafana's `util.SplitString`: a JSON list, or names separated by commas or spaces.
fn split_list(s: &str) -> Vec<String> {
    let t = s.trim();
    let mut out = Vec::new();
    if t.starts_with('[') {
        if let Ok(Json::Arr(a)) = Json::parse(t) {
            for v in a.iter() {
                if let Some(x) = v.as_str() {
                    out.push(x.to_string());
                }
            }
        }
        return out;
    }
    for part in t.split(|c: char| c == ',' || c.is_whitespace()) {
        let p = part.trim_matches('"');
        if !p.is_empty() {
            out.push(p.to_string());
        }
    }
    out
}

fn set_flag(list: &mut Vec<(String, Json)>, key: &str, value: Json) {
    for f in list.iter_mut() {
        if f.0 == key {
            f.1 = value;
            return;
        }
    }
    list.push((key.to_string(), value));
}

fn build(cfg: &Settings) -> Flags {
    let data = Json::parse(include_str!("captured/ofrep-flags.json")).unwrap_or(Json::Null);
    let mut list: Vec<(String, Json)> = Vec::new();
    if let Some(fs) = data.get("flags").and_then(|v| v.as_arr()) {
        for f in fs.iter() {
            list.push((f.str_of("key").to_string(), f.get("value").cloned().unwrap_or(Json::Bool(false))));
        }
    }
    for name in split_list(cfg.get("feature_toggles", "enable")) {
        set_flag(&mut list, &name, Json::Bool(true));
    }
    for (k, v) in cfg.section("feature_toggles") {
        if k != "enable" {
            set_flag(&mut list, &k, parse_flag(&v));
        }
    }
    set_flag(&mut list, "dashboardNewLayouts", Json::Bool(false));

    let mut all = Json::arr();
    let mut one = Vec::with_capacity(list.len());
    let mut enabled = Vec::new();
    for (k, v) in list {
        if v == Json::Bool(true) {
            enabled.push(k.clone());
        }
        let f = Json::obj().with("key", k.as_str()).with("value", v).with("reason", REASON).with("variant", "default");
        let mut s = f.stringify();
        s.push('\n');
        one.push((k, s));
        all.push(f);
    }
    let mut all_s = Json::obj().with("flags", all).stringify();
    all_s.push('\n');
    Flags { all: all_s, one, enabled }
}

fn all_flags(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    Ok(Response::json_text(200, flags(&c.app.cfg).all.clone()))
}

/// An unknown flag: upstream `http.Error(w, "failed to evaluate flag", 500)`.
fn one_flag(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let key = c.param("flagKey");
    for (k, body) in flags(&c.app.cfg).one.iter() {
        if k == key {
            return Ok(Response::json_text(200, body.clone()));
        }
    }
    Ok(Response::text(500, "text/plain; charset=utf-8", &b"failed to evaluate flag\n"[..]))
}
