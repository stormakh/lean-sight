//! Data source settings for the plugin: promlib `models/settings.go` (`ParsePromOptions`, lenient members) and
//! the HTTP client options of the plugin SDK that this plugin uses (basic auth, custom headers, timeout).
use json::Json;

/// `models.PromOptions`, the members the backend reads.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PromOptions {
    pub http_method: String,
    pub time_interval: String,
    pub query_timeout: String,
    pub custom_query_parameters: String,
    pub max_samples_processed_warning_threshold: f64,
    pub max_samples_processed_error_threshold: f64,
    pub query_stats_enabled: bool,
    pub prometheus_type: String,
    pub series_limit: Option<f64>,
    /// `timeout` (seconds) of the HTTP client options; 0 for the default.
    pub timeout: f64,
}

fn type_name(v: &Json) -> &'static str {
    match v {
        Json::Null => "null",
        Json::Bool(_) => "bool",
        Json::Int(_) | Json::Float(_) => "number",
        Json::Str(_) => "string",
        Json::Arr(_) => "array",
        Json::Obj(_) => "object",
    }
}

fn strict_string(o: &Json, k: &str) -> Result<String, String> {
    match super::models::get_ci(o, k) {
        None | Some(Json::Null) => Ok(String::new()),
        Some(Json::Str(s)) => Ok(s.clone()),
        Some(v) => Err(format!("error unmarshalling JSONData: json: cannot unmarshal {} into Go struct field PromOptions.{k} of type string", type_name(v))),
    }
}

/// `LenientString`: strings, or the JSON text of a number or bool.
fn lenient_string(o: &Json, k: &str) -> String {
    match super::models::get_ci(o, k) {
        Some(Json::Str(s)) => s.clone(),
        Some(v @ (Json::Int(_) | Json::Float(_) | Json::Bool(_))) => v.stringify(),
        _ => String::new(),
    }
}

/// `readFloat64`: a number or a number string.
fn lenient_f64(v: Option<&Json>) -> Option<f64> {
    match v {
        Some(Json::Int(i)) => Some(*i as f64),
        Some(Json::Float(f)) => Some(*f),
        Some(Json::Str(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// `LenientBool`: bools, `strconv.ParseBool` strings, numbers (non-zero is true).
fn lenient_bool(v: Option<&Json>) -> bool {
    match v {
        Some(Json::Bool(b)) => *b,
        Some(Json::Str(s)) => matches!(s.trim(), "1" | "t" | "T" | "TRUE" | "true" | "True"),
        Some(Json::Int(i)) => *i != 0,
        Some(Json::Float(f)) => *f != 0.0,
        _ => false,
    }
}

impl PromOptions {
    /// `ApplyDefaults`: trims and uppercases the method; empty is POST.
    pub fn apply_defaults(&mut self) {
        self.http_method = self.http_method.trim().to_ascii_uppercase();
        if self.http_method.is_empty() {
            self.http_method = "POST".to_string();
        }
    }
    /// `Validate`.
    pub fn validate(&self) -> Result<(), String> {
        let m = self.http_method.to_ascii_uppercase();
        if !m.is_empty() && m != "GET" && m != "POST" {
            return Err(format!("invalid httpMethod {}: must be GET or POST", json::quote(&self.http_method)));
        }
        Ok(())
    }
}

/// `ParsePromOptions` from the `jsonData` text.
pub fn parse_prom_options(json_data: &str) -> Result<PromOptions, String> {
    let text = if json_data.trim().is_empty() { "{}" } else { json_data };
    let o = match Json::parse(text) {
        Ok(v) => v,
        Err(e) => return Err(format!("error unmarshalling JSONData: {e:?}")),
    };
    if !matches!(o, Json::Obj(_) | Json::Null) {
        return Err(format!("error unmarshalling JSONData: json: cannot unmarshal {} into Go value of type models.PromOptions", type_name(&o)));
    }
    let g = |k: &str| super::models::get_ci(&o, k);
    let mut p = PromOptions {
        http_method: strict_string(&o, "httpMethod")?,
        time_interval: strict_string(&o, "timeInterval")?,
        query_timeout: strict_string(&o, "queryTimeout")?,
        custom_query_parameters: lenient_string(&o, "customQueryParameters"),
        max_samples_processed_warning_threshold: lenient_f64(g("maxSamplesProcessedWarningThreshold")).unwrap_or(0.0),
        max_samples_processed_error_threshold: lenient_f64(g("maxSamplesProcessedErrorThreshold")).unwrap_or(0.0),
        query_stats_enabled: lenient_bool(g("queryStatsEnabled")),
        prometheus_type: lenient_string(&o, "prometheusType"),
        series_limit: lenient_f64(g("seriesLimit")),
        timeout: lenient_f64(g("timeout")).unwrap_or(0.0),
    };
    p.apply_defaults();
    p.validate()?;
    Ok(p)
}
