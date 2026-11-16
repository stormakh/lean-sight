//! Requests to Prometheus: promlib `client/client.go` (query, query_range, query_exemplars, resources),
//! `middleware/custom_query_params.go`, and Go's `url.Values.Encode` / `url.QueryEscape`.
use super::models::{Query, SEC};
use super::settings::PromOptions;

/// One HTTP request to the data source.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HttpReq {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// One HTTP response from the data source (header names in lowercase).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HttpResp {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
impl HttpResp {
    pub fn header(&self, name: &str) -> Option<&str> {
        for (k, v) in self.headers.iter() {
            if k.eq_ignore_ascii_case(name) {
                return Some(v);
            }
        }
        None
    }
    pub fn header_values(&self, name: &str) -> Vec<&str> {
        self.headers.iter().filter(|h| h.0.eq_ignore_ascii_case(name)).map(|h| h.1.as_str()).collect()
    }
}

/// A transport failure, already in the text Go's `http.Client` would give (`Post "url": dial tcp ...`).
#[derive(Debug, Clone, PartialEq)]
pub struct TransportError {
    pub message: String,
    pub timeout: bool,
}

/// Sends one request. The server passes the pooled HTTP client; tests pass canned responses.
pub type Transport<'a> = &'a dyn Fn(&HttpReq) -> Result<HttpResp, TransportError>;

/// A data source instance: URL, options, and the headers every request carries (basic auth, custom headers).
#[derive(Debug, Clone, Default)]
pub struct Instance {
    pub id: i64,
    pub uid: String,
    pub url: String,
    pub opts: PromOptions,
    pub headers: Vec<(String, String)>,
    /// Request deadline in seconds.
    pub timeout_secs: u64,
}

/// Go `url.QueryEscape`.
pub fn query_escape(s: &str, out: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &c in s.as_bytes() {
        match c {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(c as char),
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(HEX[(c >> 4) as usize] as char);
                out.push(HEX[(c & 15) as usize] as char);
            }
        }
    }
}

/// Go `url.Values.Encode`: keys sorted (stable for equal keys), `k=v` joined by `&`.
pub fn encode_values(v: &[(String, String)]) -> String {
    let mut idx: Vec<usize> = (0..v.len()).collect();
    idx.sort_by(|a, b| v[*a].0.cmp(&v[*b].0));
    let mut out = String::new();
    for (n, i) in idx.iter().enumerate() {
        if n > 0 {
            out.push('&');
        }
        query_escape(&v[*i].0, &mut out);
        out.push('=');
        query_escape(&v[*i].1, &mut out);
    }
    out
}

fn unhex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Go `url.QueryUnescape`; None on a bad escape.
pub fn query_unescape(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                let h = unhex(*b.get(i + 1)?)?;
                let l = unhex(*b.get(i + 2)?)?;
                out.push(h << 4 | l);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    Some(String::from_utf8_lossy(&out).into_owned())
}

/// Go `url.ParseQuery`: pairs in order; Err when a pair has a bad escape or a `;`.
pub fn parse_query(q: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    let mut err: Option<String> = None;
    for part in q.split('&') {
        if part.is_empty() {
            continue;
        }
        if part.contains(';') {
            err = Some("invalid semicolon separator in query".to_string());
            continue;
        }
        let (k, v) = match part.split_once('=') {
            Some((k, v)) => (k, v),
            None => (part, ""),
        };
        match (query_unescape(k), query_unescape(v)) {
            (Some(k), Some(v)) => out.push((k, v)),
            _ => {
                if err.is_none() {
                    err = Some(format!("invalid URL escape in {}", json::quote(part)));
                }
            }
        }
    }
    match err {
        Some(e) => Err(e),
        None => Ok(out),
    }
}

/// Splits `http://host:port/base?q` into (`http://host:port`, `/base`, `q`).
fn split_base(base: &str) -> (String, String, String) {
    let (no_q, q) = match base.split_once('?') {
        Some((a, b)) => (a, b),
        None => (base, ""),
    };
    let after_scheme = match no_q.find("://") {
        Some(i) => i + 3,
        None => 0,
    };
    match no_q[after_scheme..].find('/') {
        Some(i) => (no_q[..after_scheme + i].to_string(), no_q[after_scheme + i..].to_string(), q.to_string()),
        None => (no_q.to_string(), String::new(), q.to_string()),
    }
}

/// Go `path.Join(base, endpoint)` then `url.String()`: a cleaned path with a leading slash.
fn join_path(base: &str, endpoint: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for seg in base.split('/').chain(endpoint.split('/')) {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    let mut out = String::new();
    for p in parts {
        out.push('/');
        out.push_str(p);
    }
    if out.is_empty() {
        out.push('/');
    }
    out
}

impl Instance {
    /// `createUrl`: base URL + endpoint, with `qs` set over the base query (re-encoded, sorted).
    pub fn create_url(&self, endpoint: &str, qs: &[(String, String)]) -> String {
        let (origin, path, q) = split_base(&self.url);
        let mut u = origin;
        u.push_str(&join_path(&path, endpoint));
        if !qs.is_empty() {
            let mut vals = parse_query(&q).unwrap_or_default();
            for (k, v) in qs {
                vals.retain(|p| &p.0 != k);
                vals.push((k.clone(), v.clone()));
            }
            u.push('?');
            u.push_str(&encode_values(&vals));
        } else if !q.is_empty() {
            u.push('?');
            u.push_str(&q);
        }
        u
    }

    /// The custom query parameters middleware: adds `customQueryParameters`, the max-samples thresholds and
    /// `stats=all` (query endpoints, when enabled) to the URL query. A no-op when none is configured.
    pub fn apply_middleware(&self, url: &mut String) {
        let o = &self.opts;
        let warn = o.max_samples_processed_warning_threshold;
        let errv = o.max_samples_processed_error_threshold;
        if o.custom_query_parameters.is_empty() && warn == 0.0 && errv == 0.0 && !o.query_stats_enabled {
            return;
        }
        let mut values: Vec<(String, String)> = Vec::new();
        if !o.custom_query_parameters.is_empty() {
            match parse_query(&o.custom_query_parameters) {
                Ok(v) => values = v,
                Err(_) => return,
            }
        }
        let set = |values: &mut Vec<(String, String)>, k: &str, v: String| {
            values.retain(|p| p.0 != k);
            values.push((k.to_string(), v));
        };
        if warn > 0.0 {
            set(&mut values, "max_samples_processed_warning_threshold", format!("{}", warn));
        }
        if errv > 0.0 {
            set(&mut values, "max_samples_processed_error_threshold", format!("{}", errv));
        }
        let (no_q, q) = match url.split_once('?') {
            Some((a, b)) => (a.to_string(), b.to_string()),
            None => (url.clone(), String::new()),
        };
        let mut cur = match parse_query(&q) {
            Ok(v) => v,
            Err(_) => parse_query(&q.split('&').filter(|p| !p.contains(';')).collect::<Vec<&str>>().join("&")).unwrap_or_default(),
        };
        // values are added grouped by key, as Go ranges over a map (order of keys does not matter: Encode sorts)
        for (k, v) in values {
            cur.push((k, v));
        }
        if o.query_stats_enabled && (no_q.ends_with("/api/v1/query") || no_q.ends_with("/api/v1/query_range")) {
            cur.retain(|p| p.0 != "stats");
            cur.push(("stats".to_string(), "all".to_string()));
        }
        let mut u = no_q;
        let enc = encode_values(&cur);
        if !enc.is_empty() || url.contains('?') {
            u.push('?');
            u.push_str(&enc);
        }
        *url = u;
    }

    fn base_request(&self, method: &str, url: String, body: Vec<u8>) -> HttpReq {
        let mut headers = self.headers.clone();
        if method == "POST" {
            headers.push(("Content-Type".to_string(), "application/x-www-form-urlencoded".to_string()));
        }
        let mut url = url;
        self.apply_middleware(&mut url);
        HttpReq { method: method.to_string(), url, headers, body }
    }

    /// `createQueryRequest`: POST sends the form in the body; GET puts it in the URL.
    pub fn query_request(&self, endpoint: &str, qv: Vec<(String, String)>) -> HttpReq {
        if self.opts.http_method == "POST" {
            let url = self.create_url(endpoint, &[]);
            return self.base_request("POST", url, encode_values(&qv).into_bytes());
        }
        let url = self.create_url(endpoint, &qv);
        self.base_request("GET", url, Vec::new())
    }

    /// `QueryRange`.
    pub fn range_request(&self, q: &Query) -> HttpReq {
        let (start, end, step) = q.time_range();
        let mut qv = vec![("query".to_string(), q.expr.clone()), ("start".to_string(), format_time(start)), ("end".to_string(), format_time(end)), ("step".to_string(), format_seconds(step))];
        if !self.opts.query_timeout.is_empty() {
            qv.push(("timeout".to_string(), self.opts.query_timeout.clone()));
        }
        self.query_request("api/v1/query_range", qv)
    }

    /// `QueryInstant`.
    pub fn instant_request(&self, q: &Query) -> HttpReq {
        let mut qv = vec![("query".to_string(), q.expr.clone()), ("time".to_string(), format_time(q.end))];
        if !self.opts.query_timeout.is_empty() {
            qv.push(("timeout".to_string(), self.opts.query_timeout.clone()));
        }
        self.query_request("api/v1/query", qv)
    }

    /// `QueryExemplars`.
    pub fn exemplar_request(&self, q: &Query) -> HttpReq {
        let (start, end, _) = q.time_range();
        let qv = vec![("query".to_string(), q.expr.clone()), ("start".to_string(), format_time(start)), ("end".to_string(), format_time(end))];
        self.query_request("api/v1/query_exemplars", qv)
    }

    /// The plugin's HTTP client: the `force-http-get` middleware (installed when the data source is GET-configured)
    /// answers a POST with 405 without sending it; anything else goes to the transport.
    pub fn round_trip(&self, r: &HttpReq, t: Transport<'_>) -> Result<HttpResp, TransportError> {
        if self.opts.http_method == "GET" && r.method == "POST" {
            return Ok(HttpResp { status: 405, headers: Vec::new(), body: Vec::new() });
        }
        t(r)
    }

    /// `QueryRange`: builds the request and sends it.
    pub fn query_range(&self, q: &Query, t: Transport<'_>) -> Result<HttpResp, TransportError> {
        self.round_trip(&self.range_request(q), t)
    }

    /// `QueryInstant`: builds the request and sends it.
    pub fn query_instant(&self, q: &Query, t: Transport<'_>) -> Result<HttpResp, TransportError> {
        self.round_trip(&self.instant_request(q), t)
    }

    /// `QueryExemplars`: builds the request and sends it.
    pub fn query_exemplars(&self, q: &Query, t: Transport<'_>) -> Result<HttpResp, TransportError> {
        self.round_trip(&self.exemplar_request(q), t)
    }

    /// `QueryResource(CallResourceRequest{Method, Path, URL, Body})`: the resource path is `path`; only the query
    /// string of `url` is used (Go `url.Parse(req.URL).RawQuery`).
    pub fn query_resource(&self, method: &str, path: &str, url: &str, body: &[u8], t: Transport<'_>) -> Result<HttpResp, TransportError> {
        self.round_trip(&self.resource_request(method, path, raw_query_of(url), body), t)
    }

    /// `QueryResource`: `path` is the resource path, `raw_query` the caller's query string. GET calls go as GET
    /// (a form body is merged into the URL); other calls use the configured method with the body as is.
    pub fn resource_request(&self, method: &str, path: &str, raw_query: &str, body: &[u8]) -> HttpReq {
        let (origin, base_path, _) = split_base(&self.url);
        let mut url = origin;
        url.push_str(&join_path(&base_path, path));
        let m = if method == "GET" { "GET" } else { self.opts.http_method.as_str() };
        let mut q = raw_query.to_string();
        let mut out_body = Vec::new();
        if m == "GET" {
            if !body.is_empty() {
                if let Ok(bp) = parse_query(&String::from_utf8_lossy(body)) {
                    let mut cur = parse_query(&q).unwrap_or_default();
                    cur.extend(bp);
                    q = encode_values(&cur);
                }
            }
        } else {
            out_body = body.to_vec();
        }
        if !q.is_empty() {
            url.push('?');
            url.push_str(&q);
        }
        self.base_request(if m == "GET" { "GET" } else { "POST" }, url, out_body)
    }
}

/// Go `url.Parse(u).RawQuery`: the text after the first `?`, before a `#`.
pub fn raw_query_of(url: &str) -> &str {
    let u = url.split('#').next().unwrap_or("");
    u.split_once('?').map(|p| p.1).unwrap_or("")
}

/// The data source settings the plugin instance is built from (`backend.DataSourceInstanceSettings`).
#[derive(Debug, Clone, Default)]
pub struct DsSettings<'a> {
    pub id: i64,
    pub uid: &'a str,
    pub url: &'a str,
    pub json_data: &'a str,
    pub basic_auth: bool,
    pub basic_auth_user: &'a str,
    /// The decrypted secureJsonData (an object; `Json::Null` for none).
    pub secure: Option<&'a json::Json>,
    /// `[dataproxy] timeout`: the request deadline when jsonData has no `timeout`.
    pub default_timeout_secs: u64,
}

impl Instance {
    /// `newInstanceSettings`: `ParsePromOptions`, then the SDK HTTP client options (`CreateTransportOptions`): basic
    /// auth, the custom headers `httpHeaderName<N>` / `httpHeaderValue<N>`, the timeout.
    pub fn new(s: &DsSettings<'_>) -> Result<Instance, String> {
        let (opts, warnings) = super::settings::parse_prom_options_logged(s.json_data);
        for w in warnings.iter() {
            // log.DefaultLogger.Warn("datasource jsonData value does not match its declared type", ...)
            eprintln!("grafana-rs: datasource {} jsonData value does not match its declared type: {w}", s.uid);
        }
        let opts = opts.map_err(|e| format!("error reading settings: {e}"))?;
        let jd = if s.json_data.trim().is_empty() { json::Json::obj() } else { json::Json::parse(s.json_data).unwrap_or_else(|_| json::Json::obj()) };
        let none = json::Json::Null;
        let secure = s.secure.unwrap_or(&none);
        let mut headers: Vec<(String, String)> = Vec::new();
        if s.basic_auth {
            let cred = format!("{}:{}", s.basic_auth_user, secure.str_of("basicAuthPassword"));
            headers.push(("Authorization".to_string(), format!("Basic {}", crypto::base64(cred.as_bytes()))));
        }
        for i in 1..100 {
            let name = jd.str_of(&format!("httpHeaderName{i}")).to_string();
            if name.is_empty() {
                break;
            }
            headers.push((name, secure.str_of(&format!("httpHeaderValue{i}")).to_string()));
        }
        let timeout_secs = if opts.timeout > 0.0 { opts.timeout as u64 } else { s.default_timeout_secs.max(1) };
        Ok(Instance { id: s.id, uid: s.uid.to_string(), url: s.url.to_string(), opts, headers, timeout_secs })
    }
}

/// The headers Grafana's TracingHeaderMiddleware copies from the incoming request into the plugin request, which
/// the plugin's HTTP client forwards to the data source (`ForwardHTTPHeaders: true` in `CreateTransportOptions`).
/// Grafana's alerting headers (FromAlert, X-Rule-*) are not here: grafana-rs has no alerting.
pub const FORWARDED_HEADERS: [&str; 10] = [
    "X-Query-Group-Id",
    "X-Panel-Id",
    "X-Dashboard-Uid",
    "X-Datasource-Uid",
    "X-Grafana-From-Expr",
    "X-Grafana-Org-Id",
    "X-Panel-Plugin-Id",
    "X-Dashboard-Title",
    "X-Panel-Title",
    "X-Grafana-Caller-Id",
];

/// The forwarded headers present (non-empty) in `incoming`, with Grafana's name and its gRPC-safe value: bytes
/// outside printable ASCII are percent-encoded (`sanitizeHTTPHeaderValueForGRPC` for UTF-8 values).
pub fn forwarded_headers(incoming: &[(String, String)]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for name in FORWARDED_HEADERS.iter() {
        let v = match incoming.iter().find(|h| h.0.eq_ignore_ascii_case(name)) {
            Some(h) if !h.1.is_empty() => &h.1,
            _ => continue,
        };
        let mut val = String::with_capacity(v.len());
        for b in v.bytes() {
            if (0x20..=0x7e).contains(&b) {
                val.push(b as char);
            } else {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                val.push('%');
                val.push(HEX[(b >> 4) as usize] as char);
                val.push(HEX[(b & 15) as usize] as char);
            }
        }
        out.push((name.to_string(), val));
    }
    out
}

/// `formatTime`: Unix seconds with the fraction, as `strconv.FormatFloat(f, 'f', -1, 64)`.
pub fn format_time(t: i64) -> String {
    let secs = t.div_euclid(SEC);
    let nsec = t.rem_euclid(SEC);
    let v = secs as f64 + nsec as f64 / 1e9;
    format!("{}", v)
}

/// A duration in seconds as `strconv.FormatFloat(d.Seconds(), 'f', -1, 64)`.
pub fn format_seconds(d: i64) -> String {
    let sec = d / SEC;
    let nsec = d % SEC;
    let v = sec as f64 + nsec as f64 / 1e9;
    format!("{}", v)
}
