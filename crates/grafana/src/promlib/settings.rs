//! Data source settings for the plugin: promlib `models/settings.go` (`ParsePromOptions`, lenient members) and
//! the HTTP client options of the plugin SDK that this plugin uses (basic auth, custom headers, timeout).
use json::Json;

/// `models.PromOptions`: every member of the upstream struct (the frontend-only ones too), with its lenient type.
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
    pub prometheus_version: String,
    pub disable_metrics_lookup: bool,
    pub cache_level: String,
    pub default_editor: String,
    pub incremental_querying: bool,
    pub incremental_query_overlap_window: String,
    pub disable_recording_rules: bool,
    pub oauth_pass_thru: bool,
    pub series_endpoint: bool,
    pub series_limit: Option<f64>,
    pub exemplar_trace_id_destinations: Vec<ExemplarTraceIdDestination>,
    pub manage_alerts: bool,
    pub allow_as_recording_rules_target: bool,
    /// `timeout` (seconds) of the HTTP client options; 0 for the default.
    pub timeout: f64,
    pub keep_cookies: Vec<String>,
    pub tls_auth: bool,
    pub server_name: String,
    pub tls_auth_with_ca_cert: bool,
    pub tls_skip_verify: bool,
    pub enable_secure_socks_proxy: bool,
}

/// `ExemplarTraceIDDestination`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExemplarTraceIdDestination {
    pub name: String,
    pub url: String,
    pub url_display_label: String,
    pub datasource_uid: String,
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

/// The warnings of `lenient.go` (`logLenient`), each as "from->to outcome value" (value cut at 64 bytes + "…").
#[derive(Default)]
struct Lenient {
    warnings: Vec<String>,
}

impl Lenient {
    fn log(&mut self, from: &str, to: &str, outcome: &str, raw: &[u8]) {
        let mut value = String::from_utf8_lossy(raw).into_owned();
        if value.len() > 64 {
            let mut cut = 64;
            while !value.is_char_boundary(cut) {
                cut -= 1;
            }
            value.truncate(cut);
            value.push('\u{2026}');
        }
        self.warnings.push(format!("{from}->{to} {outcome} {value}"));
    }

    /// `LenientBool.UnmarshalJSON`.
    fn bool(&mut self, raw: &[u8], out: &mut bool) {
        match Json::parse_bytes(raw) {
            Ok(Json::Null) => *out = false,
            Ok(Json::Bool(b)) => *out = b,
            Ok(Json::Str(s)) => match s.trim() {
                "1" | "t" | "T" | "TRUE" | "true" | "True" => {
                    *out = true;
                    self.log("string", "bool", "coerced", raw);
                }
                "0" | "f" | "F" | "FALSE" | "false" | "False" => {
                    *out = false;
                    self.log("string", "bool", "coerced", raw);
                }
                _ => self.log("string", "bool", "dropped", raw),
            },
            Ok(v @ (Json::Int(_) | Json::Float(_))) => {
                *out = v.as_f64().unwrap_or(0.0) != 0.0;
                self.log("float64", "bool", "coerced", raw);
            }
            _ => self.log("unknown", "bool", "dropped", raw),
        }
    }

    /// `LenientString.UnmarshalJSON`: a string, or the JSON text of a number or bool.
    fn string(&mut self, raw: &[u8], out: &mut String) {
        match Json::parse_bytes(raw) {
            Ok(Json::Null) => {}
            Ok(Json::Str(s)) => *out = s,
            Ok(Json::Int(_) | Json::Float(_)) => {
                *out = String::from_utf8_lossy(raw).trim().to_string();
                self.log("float64", "string", "coerced", raw);
            }
            Ok(Json::Bool(_)) => {
                *out = String::from_utf8_lossy(raw).trim().to_string();
                self.log("bool", "string", "coerced", raw);
            }
            _ => self.log("unknown", "string", "dropped", raw),
        }
    }

    /// `LenientFloat64.UnmarshalJSON` (`readFloat64`): a number, or a number string. None when dropped.
    fn float(&mut self, raw: &[u8]) -> Option<f64> {
        let (v, from, ok) = read_f64(raw);
        if !ok {
            self.log(from, "float64", "dropped", raw);
            return None;
        }
        if !from.is_empty() {
            self.log(from, "float64", "coerced", raw);
        }
        Some(v)
    }

    /// `LenientExemplarTraceIDDestinations.UnmarshalJSON`: a list of destinations, or nothing.
    fn exemplars(&mut self, raw: &[u8], out: &mut Vec<ExemplarTraceIdDestination>) {
        let v = Json::parse_bytes(raw).unwrap_or(Json::Bool(false));
        let list = match v {
            Json::Null => Some(Vec::new()),
            Json::Arr(items) => {
                let mut l = Vec::new();
                for it in items.iter() {
                    let mut d = ExemplarTraceIdDestination::default();
                    let ok = match it {
                        Json::Null => true,
                        Json::Obj(_) => [("name", &mut d.name), ("url", &mut d.url), ("urlDisplayLabel", &mut d.url_display_label), ("datasourceUid", &mut d.datasource_uid)]
                            .into_iter()
                            .all(|(k, dst)| match super::models::get_ci(it, k) {
                                None | Some(Json::Null) => true,
                                Some(Json::Str(s)) => {
                                    *dst = s.clone();
                                    true
                                }
                                Some(_) => false,
                            }),
                        _ => false,
                    };
                    if !ok {
                        l.clear();
                        break;
                    }
                    l.push(d);
                }
                if l.len() == items.len() { Some(l) } else { None }
            }
            _ => None,
        };
        match list {
            Some(l) => *out = l,
            None => self.log("unknown", "exemplarDestinations", "dropped", raw),
        }
    }

    /// `LenientStringSlice.UnmarshalJSON`: a list of strings, or nothing.
    fn strings(&mut self, raw: &[u8], out: &mut Vec<String>) {
        match Json::parse_bytes(raw) {
            Ok(Json::Null) => *out = Vec::new(),
            Ok(Json::Arr(items)) if items.iter().all(|x| matches!(x, Json::Str(_) | Json::Null)) => {
                *out = items.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect();
            }
            _ => self.log("unknown", "[]string", "dropped", raw),
        }
    }
}

/// `readFloat64`: (value, the JSON type it came from, "" for a number; readable).
fn read_f64(raw: &[u8]) -> (f64, &'static str, bool) {
    match Json::parse_bytes(raw) {
        Ok(Json::Null) => (0.0, "", true),
        Ok(v @ (Json::Int(_) | Json::Float(_))) => match v.as_f64() {
            Some(f) if f.is_finite() => (f, "", true),
            _ => (0.0, "unknown", false),
        },
        Ok(Json::Str(s)) => match super::jsonpull::parse_go_float(s.trim()) {
            Some(f) => (f, "string", true),
            None => (0.0, "string", false),
        },
        _ => (0.0, "unknown", false),
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
    parse_prom_options_logged(json_data).0
}

/// `ParsePromOptions`, with the warnings Grafana logs for each lenient conversion ("from->to outcome value").
pub fn parse_prom_options_logged(json_data: &str) -> (Result<PromOptions, String>, Vec<String>) {
    let mut lx = Lenient::default();
    let r = parse(json_data, &mut lx);
    (r, lx.warnings)
}

fn parse(json_data: &str, lx: &mut Lenient) -> Result<PromOptions, String> {
    let text = if json_data.trim().is_empty() { "{}" } else { json_data };
    let o = match Json::parse(text) {
        Ok(v) => v,
        Err(e) => return Err(format!("error unmarshalling JSONData: {e:?}")),
    };
    if !matches!(o, Json::Obj(_) | Json::Null) {
        return Err(format!("error unmarshalling JSONData: json: cannot unmarshal {} into Go value of type models.PromOptions", type_name(&o)));
    }
    let mut p = PromOptions::default();
    let mut first_err: Option<String> = None;
    // encoding/json: every member in order goes to the field of the same name (case-insensitive); the last wins
    let mut it = super::jsonpull::Iter::new(text.as_bytes());
    while let Some(key) = it.read_object()? {
        let raw = it.skip_and_return_bytes()?;
        let k = key.to_ascii_lowercase();
        match k.as_str() {
            "httpmethod" | "timeinterval" | "querytimeout" => {
                let name = match k.as_str() {
                    "httpmethod" => "httpMethod",
                    "timeinterval" => "timeInterval",
                    _ => "queryTimeout",
                };
                let dst = match k.as_str() {
                    "httpmethod" => &mut p.http_method,
                    "timeinterval" => &mut p.time_interval,
                    _ => &mut p.query_timeout,
                };
                match Json::parse_bytes(raw) {
                    Ok(Json::Null) => {}
                    Ok(Json::Str(s)) => *dst = s,
                    Ok(v) => {
                        if first_err.is_none() {
                            first_err = Some(format!("error unmarshalling JSONData: json: cannot unmarshal {} into Go struct field PromOptions.{name} of type string", type_name(&v)));
                        }
                    }
                    Err(_) => {}
                }
            }
            "customqueryparameters" => lx.string(raw, &mut p.custom_query_parameters),
            "maxsamplesprocessedwarningthreshold" => p.max_samples_processed_warning_threshold = lx.float(raw).unwrap_or(0.0),
            "maxsamplesprocessederrorthreshold" => p.max_samples_processed_error_threshold = lx.float(raw).unwrap_or(0.0),
            "querystatsenabled" => lx.bool(raw, &mut p.query_stats_enabled),
            "prometheustype" => lx.string(raw, &mut p.prometheus_type),
            "prometheusversion" => lx.string(raw, &mut p.prometheus_version),
            "disablemetricslookup" => lx.bool(raw, &mut p.disable_metrics_lookup),
            "cachelevel" => lx.string(raw, &mut p.cache_level),
            "defaulteditor" => lx.string(raw, &mut p.default_editor),
            "incrementalquerying" => lx.bool(raw, &mut p.incremental_querying),
            "incrementalqueryoverlapwindow" => lx.string(raw, &mut p.incremental_query_overlap_window),
            "disablerecordingrules" => lx.bool(raw, &mut p.disable_recording_rules),
            "oauthpassthru" => lx.bool(raw, &mut p.oauth_pass_thru),
            "seriesendpoint" => lx.bool(raw, &mut p.series_endpoint),
            // a pointer: null leaves it unset; a dropped value is cleared (clearDroppedPointers)
            "serieslimit" => {
                p.series_limit = if matches!(Json::parse_bytes(raw), Ok(Json::Null)) { None } else { lx.float(raw) };
            }
            "exemplartraceiddestinations" => lx.exemplars(raw, &mut p.exemplar_trace_id_destinations),
            "managealerts" => lx.bool(raw, &mut p.manage_alerts),
            "allowasrecordingrulestarget" => lx.bool(raw, &mut p.allow_as_recording_rules_target),
            "timeout" => p.timeout = lx.float(raw).unwrap_or(0.0),
            "keepcookies" => lx.strings(raw, &mut p.keep_cookies),
            "tlsauth" => lx.bool(raw, &mut p.tls_auth),
            "servername" => lx.string(raw, &mut p.server_name),
            "tlsauthwithcacert" => lx.bool(raw, &mut p.tls_auth_with_ca_cert),
            "tlsskipverify" => lx.bool(raw, &mut p.tls_skip_verify),
            "enablesecuresocksproxy" => lx.bool(raw, &mut p.enable_secure_socks_proxy),
            _ => {}
        }
    }
    if let Some(e) = first_err {
        return Err(e);
    }
    p.apply_defaults();
    p.validate()?;
    Ok(p)
}
