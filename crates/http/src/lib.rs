//! Blocking HTTP/1.1: a server (one thread per connection), a router with path params, request and response types,
//! query, form and cookie parsing, gzip, static files, and a small HTTP/1.1 client for plain http.
pub mod client;
pub mod gzip;
pub mod router;
pub mod server;
pub mod staticfile;
pub mod util;

pub use client::{Client, ClientError, ClientResponse};
pub use router::{Found, Router};
pub use server::{Server, ServerConfig, Service};
pub use util::{http_date, parse_http_date, parse_query, url_decode, url_encode};

use json::Json;

/// One HTTP request. Header names are lowercase.
#[derive(Debug, Default)]
pub struct Request {
    pub method: String,
    /// Raw path, without the query string, still percent-encoded.
    pub path: String,
    /// Raw query string without the `?`.
    pub query: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub remote_ip: String,
    /// Path params filled by the router (decoded). A wildcard tail is under its name ("*" when unnamed).
    pub params: Vec<(String, String)>,
    pub http10: bool,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        for h in self.headers.iter() {
            if h.0.eq_ignore_ascii_case(name) {
                return Some(&h.1);
            }
        }
        None
    }
    /// Path param by name, "" when absent.
    pub fn param(&self, name: &str) -> &str {
        for p in self.params.iter() {
            if p.0 == name {
                return &p.1;
            }
        }
        ""
    }
    /// All decoded query pairs, in order.
    pub fn query_pairs(&self) -> Vec<(String, String)> {
        parse_query(&self.query)
    }
    /// First value of a query key.
    pub fn query_get(&self, name: &str) -> Option<String> {
        for (k, v) in parse_query(&self.query) {
            if k == name {
                return Some(v);
            }
        }
        None
    }
    /// Every value of a query key (`?type=a&type=b`).
    pub fn query_all(&self, name: &str) -> Vec<String> {
        let mut out = Vec::new();
        for (k, v) in parse_query(&self.query) {
            if k == name {
                out.push(v);
            }
        }
        out
    }
    /// Cookie value by name.
    pub fn cookie(&self, name: &str) -> Option<String> {
        for h in self.headers.iter() {
            if h.0 != "cookie" {
                continue;
            }
            for part in h.1.split(';') {
                let part = part.trim();
                if let Some((k, v)) = part.split_once('=') {
                    if k.trim() == name {
                        let v = v.trim();
                        let v = v.strip_prefix('"').and_then(|x| x.strip_suffix('"')).unwrap_or(v);
                        return Some(url_decode(v));
                    }
                }
            }
        }
        None
    }
    /// The body as `application/x-www-form-urlencoded` pairs.
    pub fn form(&self) -> Vec<(String, String)> {
        parse_query(&String::from_utf8_lossy(&self.body))
    }
    /// The body parsed as JSON.
    pub fn json(&self) -> Result<Json, json::JsonError> {
        Json::parse_bytes(&self.body)
    }
    pub fn accepts_gzip(&self) -> bool {
        match self.header("accept-encoding") {
            Some(v) => v.split(',').any(|p| trim_fast(trim_fast(p).split(';').next().unwrap_or("")) == "gzip"),
            None => false,
        }
    }
    /// Path and query, as the client sent them.
    pub fn uri(&self) -> String {
        if self.query.is_empty() { self.path.clone() } else { format!("{}?{}", self.path, self.query) }
    }
}

#[derive(Debug)]
pub enum Body {
    Bytes(Vec<u8>),
    /// A file streamed from disk with its length.
    File(std::fs::File, u64),
}

/// Header list with room for the default headers the app and the server add, so they do not grow it again.
fn content_type_headers(ct: &str) -> Vec<(String, String)> {
    let mut h = Vec::with_capacity(8);
    h.push(("Content-Type".to_string(), ct.to_string()));
    h
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Body,
}

impl Response {
    pub fn new(status: u16) -> Response {
        Response { status, headers: Vec::new(), body: Body::Bytes(Vec::new()) }
    }
    /// `application/json` from a value.
    pub fn json(status: u16, v: &Json) -> Response {
        let mut s = String::with_capacity(256);
        v.write(&mut s);
        Response::json_text(status, s)
    }
    /// `application/json` from JSON text you already wrote.
    pub fn json_text(status: u16, text: String) -> Response {
        Response { status, headers: content_type_headers("application/json"), body: Body::Bytes(text.into_bytes()) }
    }
    pub fn text(status: u16, content_type: &str, body: impl Into<Vec<u8>>) -> Response {
        Response { status, headers: content_type_headers(content_type), body: Body::Bytes(body.into()) }
    }
    pub fn html(status: u16, body: String) -> Response {
        Response::text(status, "text/html; charset=UTF-8", body)
    }
    /// 302 (or another 3xx) to `location`, with the small HTML body Go's `http.Redirect` writes for GET.
    pub fn redirect(status: u16, location: &str) -> Response {
        let mut body = String::with_capacity(location.len() + 32);
        body.push_str("<a href=\"");
        for c in location.chars() {
            match c {
                '&' => body.push_str("&amp;"),
                '<' => body.push_str("&lt;"),
                '>' => body.push_str("&gt;"),
                '"' => body.push_str("&#34;"),
                '\'' => body.push_str("&#39;"),
                c => body.push(c),
            }
        }
        body.push_str("\">");
        body.push_str(reason(status));
        body.push_str("</a>.\n\n");
        Response::text(status, "text/html; charset=utf-8", body).with_header("Location", location)
    }
    /// Builder: adds a header (does not replace).
    pub fn with_header(mut self, k: &str, v: &str) -> Response {
        self.headers.push((k.to_string(), v.to_string()));
        self
    }
    /// Sets a header, replacing any with the same name.
    pub fn set_header(&mut self, k: &str, v: &str) {
        for h in self.headers.iter_mut() {
            if h.0.eq_ignore_ascii_case(k) {
                h.1 = v.to_string();
                return;
            }
        }
        self.headers.push((k.to_string(), v.to_string()));
    }
    pub fn header(&self, k: &str) -> Option<&str> {
        for h in self.headers.iter() {
            if h.0.eq_ignore_ascii_case(k) {
                return Some(&h.1);
            }
        }
        None
    }
    /// Adds a `Set-Cookie` header.
    pub fn set_cookie(&mut self, c: &Cookie) {
        self.headers.push(("Set-Cookie".to_string(), c.to_header()));
    }
    pub fn body_bytes(&self) -> &[u8] {
        match &self.body {
            Body::Bytes(b) => b,
            Body::File(_, _) => &[],
        }
    }
}

/// A `Set-Cookie` value, written as Go's `http.Cookie.String()` writes it.
#[derive(Debug, Clone, Default)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub path: String,
    pub domain: String,
    /// Some(n): `Max-Age=n` (n <= 0 writes `Max-Age=0`, deleting the cookie).
    pub max_age: Option<i64>,
    pub http_only: bool,
    pub secure: bool,
    /// "Lax", "Strict", "None" or "" for none.
    pub same_site: String,
}
impl Cookie {
    pub fn to_header(&self) -> String {
        let mut s = String::with_capacity(64 + self.value.len());
        s.push_str(&self.name);
        s.push('=');
        s.push_str(&self.value);
        if !self.path.is_empty() {
            s.push_str("; Path=");
            s.push_str(&self.path);
        }
        if !self.domain.is_empty() {
            s.push_str("; Domain=");
            s.push_str(&self.domain);
        }
        if let Some(m) = self.max_age {
            s.push_str("; Max-Age=");
            s.push_str(&(if m > 0 { m } else { 0 }).to_string());
        }
        if self.http_only {
            s.push_str("; HttpOnly");
        }
        if self.secure {
            s.push_str("; Secure");
        }
        if !self.same_site.is_empty() {
            s.push_str("; SameSite=");
            s.push_str(&self.same_site);
        }
        s
    }
}

/// Standard reason phrase.
pub fn reason(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        101 => "Switching Protocols",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        412 => "Precondition Failed",
        413 => "Request Entity Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        499 => "Client Closed Request",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Unknown",
    }
}

/// `s.trim()`, skipping the work when `s` starts and ends with a printable ASCII byte (never whitespace).
pub(crate) fn trim_fast(s: &str) -> &str {
    let b = s.as_bytes();
    match (b.first(), b.last()) {
        (Some(f), Some(l)) if (0x21..0x80).contains(f) && (0x21..0x80).contains(l) => s,
        _ => s.trim(),
    }
}

/// True for content types worth compressing.
pub fn compressible(content_type: &str) -> bool {
    let ct = content_type.split(';').next().unwrap_or("").trim();
    ct.starts_with("text/") || ct == "application/json" || ct == "application/javascript" || ct == "image/svg+xml" || ct.ends_with("+json") || ct == "application/xml"
}
