//! Blocking HTTP/1.1 client for plain `http://` URLs, with a timeout per request and a small pool of idle
//! keep-alive connections. Handles Content-Length, chunked, and read-to-close bodies, and gzip bodies.
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub enum ClientError {
    /// The URL is not `http://host[:port]/...`.
    BadUrl(String),
    Connect(String),
    Timeout,
    Io(String),
    /// The response is not valid HTTP/1.x.
    Protocol(String),
    /// The body is bigger than the limit.
    TooLarge,
}
impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::BadUrl(u) => write!(f, "bad url: {u}"),
            ClientError::Connect(e) => write!(f, "connect: {e}"),
            ClientError::Timeout => f.write_str("timeout"),
            ClientError::Io(e) => write!(f, "io: {e}"),
            ClientError::Protocol(e) => write!(f, "protocol: {e}"),
            ClientError::TooLarge => f.write_str("response body too large"),
        }
    }
}
impl std::error::Error for ClientError {}

#[derive(Debug, Clone)]
pub struct ClientResponse {
    pub status: u16,
    /// Lowercase names.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
impl ClientResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        for h in self.headers.iter() {
            if h.0.eq_ignore_ascii_case(name) {
                return Some(&h.1);
            }
        }
        None
    }
}

/// Parsed `http://host:port/path?query`.
#[derive(Debug, Clone)]
pub struct Url {
    pub host: String,
    pub port: u16,
    /// Path and query, starting with '/'.
    pub path: String,
}
pub fn parse_url(url: &str) -> Result<Url, ClientError> {
    let rest = match url.strip_prefix("http://") {
        Some(r) => r,
        None => return Err(ClientError::BadUrl(url.to_string())),
    };
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => match rest.find('?') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        },
    };
    let hostport = match hostport.rsplit_once('@') {
        Some((_, h)) => h,
        None => hostport,
    };
    let (host, port) = if hostport.starts_with('[') {
        match hostport.find(']') {
            Some(i) => {
                let h = &hostport[1..i];
                let p = hostport[i + 1..].strip_prefix(':').unwrap_or("80");
                (h, p)
            }
            None => return Err(ClientError::BadUrl(url.to_string())),
        }
    } else {
        match hostport.rsplit_once(':') {
            Some((h, p)) => (h, p),
            None => (hostport, "80"),
        }
    };
    let port: u16 = match port.parse() {
        Ok(p) => p,
        Err(_) => return Err(ClientError::BadUrl(url.to_string())),
    };
    if host.is_empty() {
        return Err(ClientError::BadUrl(url.to_string()));
    }
    let path = if path.starts_with('?') { format!("/{path}") } else { path.to_string() };
    Ok(Url { host: host.to_string(), port, path })
}

/// An idle keep-alive connection. No read buffer is kept while idle: a reused connection gets a new one.
struct Idle {
    key: String,
    conn: TcpStream,
    since: Instant,
}

/// HTTP client with a pool of idle connections. Share one per process.
pub struct Client {
    idle: Mutex<Vec<Idle>>,
    pub max_idle: usize,
    /// An idle connection older than this is closed instead of reused.
    pub idle_ttl: Duration,
    pub max_body_bytes: usize,
}

impl Default for Client {
    fn default() -> Self {
        Client::new()
    }
}

fn io_err(e: std::io::Error) -> ClientError {
    match e.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => ClientError::Timeout,
        _ => ClientError::Io(e.to_string()),
    }
}

impl Client {
    pub fn new() -> Client {
        // Grafana's defaults: [dataproxy] max_idle_connections = 100, idle_conn_timeout_seconds = 90
        Client { idle: Mutex::new(Vec::new()), max_idle: 100, idle_ttl: Duration::from_secs(90), max_body_bytes: 256 * 1024 * 1024 }
    }

    /// Drops every idle connection (for example when idle memory matters).
    pub fn clear_idle(&self) {
        if let Ok(mut v) = self.idle.lock() {
            v.clear();
        }
    }

    fn take_idle(&self, key: &str) -> Option<TcpStream> {
        let mut v = match self.idle.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let ttl = self.idle_ttl;
        v.retain(|i| i.since.elapsed() < ttl);
        let mut found: Option<usize> = None;
        for (n, i) in v.iter().enumerate() {
            if i.key == key {
                found = Some(n);
            }
        }
        match found {
            Some(n) => Some(v.swap_remove(n).conn),
            None => None,
        }
    }
    fn put_idle(&self, key: String, conn: TcpStream) {
        let mut v = match self.idle.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if v.len() >= self.max_idle {
            v.remove(0);
        }
        v.push(Idle { key, conn, since: Instant::now() });
    }

    /// Sends one request. `headers` are added as given (Host, Content-Length and Connection are set for you).
    /// The whole exchange, connect included, must finish within `timeout`.
    pub fn request(&self, method: &str, url: &str, headers: &[(&str, &str)], body: &[u8], timeout: Duration) -> Result<ClientResponse, ClientError> {
        let u = parse_url(url)?;
        let key = format!("{}:{}", u.host, u.port);
        let deadline = Instant::now() + timeout;
        // a pooled connection may have been closed by the server: retry once on a fresh one
        if let Some(conn) = self.take_idle(&key) {
            match self.exchange(conn, &u, method, headers, body, deadline) {
                Ok((resp, Some(c))) => {
                    self.put_idle(key, c);
                    return Ok(resp);
                }
                Ok((resp, None)) => return Ok(resp),
                Err(ClientError::Timeout) => return Err(ClientError::Timeout),
                Err(_) => {}
            }
        }
        let conn = connect(&u, deadline)?;
        let (resp, keep) = self.exchange(conn, &u, method, headers, body, deadline)?;
        if let Some(c) = keep {
            self.put_idle(key, c);
        }
        Ok(resp)
    }

    fn exchange(&self, conn: TcpStream, u: &Url, method: &str, headers: &[(&str, &str)], body: &[u8], deadline: Instant) -> Result<(ClientResponse, Option<TcpStream>), ClientError> {
        set_deadline(&conn, deadline)?;
        let mut r = BufReader::with_capacity(4 * 1024, conn);
        let mut h = String::with_capacity(256);
        h.push_str(method);
        h.push(' ');
        h.push_str(&u.path);
        h.push_str(" HTTP/1.1\r\nHost: ");
        h.push_str(&u.host);
        if u.port != 80 {
            h.push(':');
            h.push_str(&u.port.to_string());
        }
        h.push_str("\r\n");
        let mut has_ua = false;
        for (k, v) in headers {
            if k.eq_ignore_ascii_case("host") || k.eq_ignore_ascii_case("content-length") || k.eq_ignore_ascii_case("connection") {
                continue;
            }
            if k.eq_ignore_ascii_case("user-agent") {
                has_ua = true;
            }
            h.push_str(k);
            h.push_str(": ");
            h.push_str(v);
            h.push_str("\r\n");
        }
        if !has_ua {
            h.push_str("User-Agent: Grafana/13.3.1\r\n");
        }
        if !body.is_empty() || method == "POST" || method == "PUT" || method == "PATCH" {
            h.push_str("Content-Length: ");
            h.push_str(&body.len().to_string());
            h.push_str("\r\n");
        }
        h.push_str("\r\n");
        {
            let w = r.get_mut();
            let mut all = h.into_bytes();
            all.extend_from_slice(body);
            w.write_all(&all).map_err(io_err)?;
            w.flush().map_err(io_err)?;
        }
        // status line
        let mut line = String::new();
        loop {
            line.clear();
            let n = r.read_line(&mut line).map_err(io_err)?;
            if n == 0 {
                return Err(ClientError::Io("connection closed before response".to_string()));
            }
            let status = parse_status(&line)?;
            let mut hdrs: Vec<(String, String)> = Vec::new();
            loop {
                line.clear();
                let n = r.read_line(&mut line).map_err(io_err)?;
                if n == 0 {
                    return Err(ClientError::Protocol("eof in headers".to_string()));
                }
                let t = line.trim_end();
                if t.is_empty() {
                    break;
                }
                if let Some((k, v)) = t.split_once(':') {
                    hdrs.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
                }
            }
            if (100..200).contains(&status) {
                continue; // 100 Continue and friends
            }
            let mut resp = ClientResponse { status, headers: hdrs, body: Vec::new() };
            let close = resp.header("connection").map(|c| c.eq_ignore_ascii_case("close")).unwrap_or(false);
            let chunked = resp.header("transfer-encoding").map(|t| t.to_ascii_lowercase().contains("chunked")).unwrap_or(false);
            let clen = resp.header("content-length").and_then(|v| v.parse::<usize>().ok());
            let mut reusable = !close;
            if method == "HEAD" || status == 204 || status == 304 {
                // no body
            } else if chunked {
                resp.body = read_chunked(&mut r, self.max_body_bytes)?;
            } else if let Some(n) = clen {
                if n > self.max_body_bytes {
                    return Err(ClientError::TooLarge);
                }
                let mut b = vec![0u8; n];
                r.read_exact(&mut b).map_err(io_err)?;
                resp.body = b;
            } else {
                let mut b = Vec::new();
                let mut buf = vec![0u8; 16 * 1024];
                loop {
                    let n = r.read(&mut buf).map_err(io_err)?;
                    if n == 0 {
                        break;
                    }
                    if b.len() + n > self.max_body_bytes {
                        return Err(ClientError::TooLarge);
                    }
                    b.extend_from_slice(&buf[..n]);
                }
                resp.body = b;
                reusable = false;
            }
            if resp.header("content-encoding").map(|e| e.eq_ignore_ascii_case("gzip")).unwrap_or(false) {
                let mut out = Vec::new();
                let mut d = flate2::read::GzDecoder::new(&resp.body[..]);
                d.read_to_end(&mut out).map_err(|e| ClientError::Protocol(e.to_string()))?;
                resp.body = out;
                resp.headers.retain(|h| h.0 != "content-encoding");
            }
            // bytes after the response (none in HTTP/1.1 without pipelining) would corrupt the next exchange
            if !r.buffer().is_empty() {
                reusable = false;
            }
            return Ok((resp, if reusable { Some(r.into_inner()) } else { None }));
        }
    }
}

fn set_deadline(s: &TcpStream, deadline: Instant) -> Result<(), ClientError> {
    let now = Instant::now();
    if now >= deadline {
        return Err(ClientError::Timeout);
    }
    let left = deadline - now;
    s.set_read_timeout(Some(left)).map_err(io_err)?;
    s.set_write_timeout(Some(left)).map_err(io_err)?;
    Ok(())
}

fn connect(u: &Url, deadline: Instant) -> Result<TcpStream, ClientError> {
    let addrs = match (u.host.as_str(), u.port).to_socket_addrs() {
        Ok(a) => a,
        Err(e) => return Err(ClientError::Connect(e.to_string())),
    };
    let mut last = String::from("no address");
    for a in addrs {
        let now = Instant::now();
        if now >= deadline {
            return Err(ClientError::Timeout);
        }
        match TcpStream::connect_timeout(&a, deadline - now) {
            Ok(s) => {
                let _ = s.set_nodelay(true);
                return Ok(s);
            }
            Err(e) => last = e.to_string(),
        }
    }
    Err(ClientError::Connect(last))
}

fn parse_status(line: &str) -> Result<u16, ClientError> {
    let mut it = line.split(' ');
    let v = it.next().unwrap_or("");
    if !v.starts_with("HTTP/1.") {
        return Err(ClientError::Protocol(format!("bad status line {:?}", line.trim_end())));
    }
    match it.next().unwrap_or("").trim().parse::<u16>() {
        Ok(s) => Ok(s),
        Err(_) => Err(ClientError::Protocol(format!("bad status line {:?}", line.trim_end()))),
    }
}

fn read_chunked(r: &mut BufReader<TcpStream>, max: usize) -> Result<Vec<u8>, ClientError> {
    let mut body = Vec::new();
    let mut line = String::new();
    loop {
        line.clear();
        r.read_line(&mut line).map_err(io_err)?;
        let hexpart = line.trim().split(';').next().unwrap_or("").trim().to_string();
        let n = match usize::from_str_radix(&hexpart, 16) {
            Ok(n) => n,
            Err(_) => return Err(ClientError::Protocol("bad chunk size".to_string())),
        };
        if n == 0 {
            loop {
                line.clear();
                let k = r.read_line(&mut line).map_err(io_err)?;
                if k == 0 || line.trim().is_empty() {
                    return Ok(body);
                }
            }
        }
        if body.len() + n > max {
            return Err(ClientError::TooLarge);
        }
        let start = body.len();
        body.resize(start + n, 0);
        r.read_exact(&mut body[start..]).map_err(io_err)?;
        let mut crlf = [0u8; 2];
        r.read_exact(&mut crlf).map_err(io_err)?;
    }
}
