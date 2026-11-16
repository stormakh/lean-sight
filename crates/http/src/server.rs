//! Blocking HTTP/1.1 server: one OS thread per connection with a small stack, a cap on live connections (more wait in
//! accept), keep-alive with an idle timeout, bounded header and body sizes, chunked request bodies, and gzip.
use crate::util::{http_date, now_unix};
use crate::{Body, Request, Response, compressible, reason};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// What the server calls for each request.
pub trait Service: Send + Sync {
    fn handle(&self, req: Request) -> Response;
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub max_conns: usize,
    pub stack_size: usize,
    /// Idle time allowed between keep-alive requests, and for each read while a request arrives.
    pub idle_timeout: Duration,
    pub write_timeout: Duration,
    pub max_header_bytes: usize,
    pub max_body_bytes: usize,
    /// gzip a response when the client accepts it, the type is compressible, and the body has at least this many bytes.
    pub gzip: bool,
    pub gzip_min_bytes: usize,
    /// gzip files up to this size (bigger ones stream as they are).
    pub gzip_max_file_bytes: u64,
    /// Called on the connection thread when the last live connection closes (for example to return free memory
    /// to the OS). Never called on a timer.
    pub on_idle: Option<fn()>,
}
impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            max_conns: 256,
            stack_size: 64 * 1024,
            idle_timeout: Duration::from_secs(30),
            write_timeout: Duration::from_secs(30),
            max_header_bytes: 64 * 1024,
            max_body_bytes: 32 * 1024 * 1024,
            gzip: true,
            gzip_min_bytes: 1400,
            gzip_max_file_bytes: 16 * 1024 * 1024,
            on_idle: None,
        }
    }
}

pub struct Server {
    listener: TcpListener,
    cfg: ServerConfig,
}

struct Slots {
    used: Mutex<usize>,
    cv: Condvar,
}
struct SlotGuard(Arc<Slots>, Option<fn()>);
impl Drop for SlotGuard {
    fn drop(&mut self) {
        let left = {
            let mut n = match self.0.used.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            *n -= 1;
            self.0.cv.notify_one();
            *n
        };
        // the accept loop holds one slot while it waits, so 1 left means no live connection
        if left <= 1 {
            if let Some(f) = self.1 {
                f();
            }
        }
    }
}

impl Server {
    pub fn bind(addr: &str, cfg: ServerConfig) -> std::io::Result<Server> {
        let listener = TcpListener::bind(addr)?;
        Ok(Server { listener, cfg })
    }
    pub fn local_port(&self) -> u16 {
        match self.listener.local_addr() {
            Ok(a) => a.port(),
            Err(_) => 0,
        }
    }
    /// Accepts connections forever.
    pub fn run(self, svc: Arc<dyn Service>) {
        let slots = Arc::new(Slots { used: Mutex::new(0), cv: Condvar::new() });
        let cfg = Arc::new(self.cfg);
        loop {
            // wait for a free slot before accepting, so extra clients wait in the kernel backlog
            {
                let mut n = match slots.used.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                while *n >= cfg.max_conns {
                    n = match slots.cv.wait(n) {
                        Ok(g) => g,
                        Err(p) => p.into_inner(),
                    };
                }
                *n += 1;
            }
            let guard = SlotGuard(slots.clone(), cfg.on_idle);
            let stream = match self.listener.accept() {
                Ok((s, _)) => s,
                Err(_) => {
                    drop(guard);
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
            };
            let svc = svc.clone();
            let cfg2 = cfg.clone();
            let spawned = std::thread::Builder::new().stack_size(cfg.stack_size).spawn(move || {
                let _g = guard;
                serve_conn(stream, &cfg2, &*svc);
            });
            if spawned.is_err() {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

enum ReadErr {
    /// Connection closed or timed out: drop it without a reply.
    Gone,
    /// Bad request: reply with this status, then close.
    Status(u16),
}

fn serve_conn(stream: TcpStream, cfg: &ServerConfig, svc: &dyn Service) {
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(cfg.idle_timeout));
    let _ = stream.set_write_timeout(Some(cfg.write_timeout));
    let remote_ip = match stream.peer_addr() {
        Ok(a) => a.ip().to_string(),
        Err(_) => String::new(),
    };
    let rd = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut r = BufReader::with_capacity(8 * 1024, rd);
    let mut w = stream;
    loop {
        let req = match read_request(&mut r, &mut w, cfg, &remote_ip) {
            Ok(req) => req,
            Err(ReadErr::Gone) => return,
            Err(ReadErr::Status(code)) => {
                let resp = Response::text(code, "text/plain; charset=utf-8", format!("{} {}", code, reason(code)));
                let _ = write_response(&mut w, resp, false, true, false, cfg);
                return;
            }
        };
        let keep = keep_alive(&req);
        let head = req.method == "HEAD";
        let gz = cfg.gzip && req.accepts_gzip();
        let resp = svc.handle(req);
        if write_response(&mut w, resp, head, !keep, gz, cfg).is_err() || !keep {
            return;
        }
    }
}

fn keep_alive(req: &Request) -> bool {
    let c = req.header("connection").unwrap_or("");
    if req.http10 { c.eq_ignore_ascii_case("keep-alive") } else { !c.eq_ignore_ascii_case("close") }
}

/// Reads one line ending in "\n" into `out`, without the line end, with at most `*budget` bytes.
fn read_line(r: &mut BufReader<TcpStream>, out: &mut Vec<u8>, budget: &mut usize) -> Result<(), ReadErr> {
    out.clear();
    loop {
        let (done, used) = {
            let buf = match r.fill_buf() {
                Ok(b) => b,
                Err(_) => return Err(ReadErr::Gone),
            };
            if buf.is_empty() {
                return Err(ReadErr::Gone);
            }
            match buf.iter().position(|c| *c == b'\n') {
                Some(i) => {
                    out.extend_from_slice(&buf[..i]);
                    (true, i + 1)
                }
                None => {
                    out.extend_from_slice(buf);
                    (false, buf.len())
                }
            }
        };
        r.consume(used);
        if used > *budget {
            return Err(ReadErr::Status(431));
        }
        *budget -= used;
        if done {
            if out.last() == Some(&b'\r') {
                out.pop();
            }
            return Ok(());
        }
    }
}

fn read_request(r: &mut BufReader<TcpStream>, w: &mut TcpStream, cfg: &ServerConfig, remote_ip: &str) -> Result<Request, ReadErr> {
    let mut budget = cfg.max_header_bytes;
    let mut line = Vec::with_capacity(256);
    // skip empty lines before a request line (RFC 7230 3.5)
    loop {
        read_line(r, &mut line, &mut budget)?;
        if !line.is_empty() {
            break;
        }
    }
    let text = match std::str::from_utf8(&line) {
        Ok(t) => t,
        Err(_) => return Err(ReadErr::Status(400)),
    };
    let mut it = text.split(' ');
    let method = it.next().unwrap_or("").to_string();
    let target = it.next().unwrap_or("");
    let version = it.next().unwrap_or("");
    if method.is_empty() || !target.starts_with('/') && target != "*" {
        return Err(ReadErr::Status(400));
    }
    let http10 = match version {
        "HTTP/1.1" => false,
        "HTTP/1.0" => true,
        _ => return Err(ReadErr::Status(505)),
    };
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.to_string(), String::new()),
    };
    let mut headers: Vec<(String, String)> = Vec::with_capacity(16);
    loop {
        read_line(r, &mut line, &mut budget)?;
        if line.is_empty() {
            break;
        }
        let t = String::from_utf8_lossy(&line);
        match t.split_once(':') {
            Some((k, v)) => headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string())),
            None => return Err(ReadErr::Status(400)),
        }
    }
    let mut req = Request { method, path, query, headers, body: Vec::new(), remote_ip: remote_ip.to_string(), params: Vec::new(), http10 };
    let chunked = match req.header("transfer-encoding") {
        Some(te) => te.to_ascii_lowercase().contains("chunked"),
        None => false,
    };
    let clen = match req.header("content-length") {
        Some(v) => match v.parse::<usize>() {
            Ok(n) => Some(n),
            Err(_) => return Err(ReadErr::Status(400)),
        },
        None => None,
    };
    if (chunked || clen.unwrap_or(0) > 0) && req.header("expect").map(|e| e.eq_ignore_ascii_case("100-continue")).unwrap_or(false) {
        if clen.unwrap_or(0) > cfg.max_body_bytes {
            return Err(ReadErr::Status(413));
        }
        if w.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").is_err() {
            return Err(ReadErr::Gone);
        }
    }
    if chunked {
        req.body = read_chunked(r, cfg.max_body_bytes)?;
    } else if let Some(n) = clen {
        if n > cfg.max_body_bytes {
            return Err(ReadErr::Status(413));
        }
        let mut body = vec![0u8; n];
        if r.read_exact(&mut body).is_err() {
            return Err(ReadErr::Gone);
        }
        req.body = body;
    }
    Ok(req)
}

fn read_chunked(r: &mut BufReader<TcpStream>, max: usize) -> Result<Vec<u8>, ReadErr> {
    let mut body = Vec::new();
    let mut line = Vec::with_capacity(32);
    loop {
        let mut budget = 4096;
        read_line(r, &mut line, &mut budget)?;
        let t = String::from_utf8_lossy(&line);
        let hexpart = t.split(';').next().unwrap_or("").trim();
        let n = match usize::from_str_radix(hexpart, 16) {
            Ok(n) => n,
            Err(_) => return Err(ReadErr::Status(400)),
        };
        if n == 0 {
            // trailers until an empty line
            loop {
                let mut b2 = 16 * 1024;
                read_line(r, &mut line, &mut b2)?;
                if line.is_empty() {
                    return Ok(body);
                }
            }
        }
        if body.len() + n > max {
            return Err(ReadErr::Status(413));
        }
        let start = body.len();
        body.resize(start + n, 0);
        if r.read_exact(&mut body[start..]).is_err() {
            return Err(ReadErr::Gone);
        }
        let mut crlf = [0u8; 2];
        if r.read_exact(&mut crlf).is_err() {
            return Err(ReadErr::Gone);
        }
    }
}

fn gzip_bytes(b: &[u8]) -> Option<Vec<u8>> {
    let mut e = flate2::write::GzEncoder::new(Vec::with_capacity(b.len() / 4 + 64), flate2::Compression::fast());
    if e.write_all(b).is_err() {
        return None;
    }
    e.finish().ok()
}

fn write_response(w: &mut TcpStream, mut resp: Response, head: bool, close: bool, gz_ok: bool, cfg: &ServerConfig) -> std::io::Result<()> {
    // gzip
    let no_body = resp.status == 204 || resp.status == 304 || (100..200).contains(&resp.status);
    if gz_ok && !no_body && resp.header("content-encoding").is_none() && compressible(resp.header("content-type").unwrap_or("")) {
        let mut data: Option<Vec<u8>> = None;
        match &mut resp.body {
            Body::Bytes(b) => {
                if b.len() >= cfg.gzip_min_bytes {
                    data = gzip_bytes(b);
                }
            }
            Body::File(f, len) => {
                if *len >= cfg.gzip_min_bytes as u64 && *len <= cfg.gzip_max_file_bytes {
                    let mut raw = Vec::with_capacity(*len as usize);
                    if f.read_to_end(&mut raw).is_ok() {
                        data = gzip_bytes(&raw);
                    }
                }
            }
        }
        if let Some(z) = data {
            resp.body = Body::Bytes(z);
            resp.headers.push(("Content-Encoding".to_string(), "gzip".to_string()));
            resp.headers.push(("Vary".to_string(), "Accept-Encoding".to_string()));
        }
    }
    let len: u64 = match &resp.body {
        Body::Bytes(b) => b.len() as u64,
        Body::File(_, n) => *n,
    };
    let mut h = String::with_capacity(256);
    h.push_str("HTTP/1.1 ");
    h.push_str(&resp.status.to_string());
    h.push(' ');
    h.push_str(reason(resp.status));
    h.push_str("\r\n");
    for (k, v) in resp.headers.iter() {
        if k.eq_ignore_ascii_case("content-length") {
            continue;
        }
        h.push_str(k);
        h.push_str(": ");
        h.push_str(v);
        h.push_str("\r\n");
    }
    if !no_body {
        h.push_str("Content-Length: ");
        h.push_str(&len.to_string());
        h.push_str("\r\n");
    }
    h.push_str("Date: ");
    h.push_str(&http_date(now_unix()));
    h.push_str("\r\n");
    if close {
        h.push_str("Connection: close\r\n");
    }
    h.push_str("\r\n");
    if head || no_body {
        w.write_all(h.as_bytes())?;
        return w.flush();
    }
    match resp.body {
        Body::Bytes(b) => {
            if b.len() <= 16 * 1024 {
                let mut all = h.into_bytes();
                all.extend_from_slice(&b);
                w.write_all(&all)?;
            } else {
                w.write_all(h.as_bytes())?;
                w.write_all(&b)?;
            }
        }
        Body::File(mut f, n) => {
            w.write_all(h.as_bytes())?;
            let mut left = n;
            let mut buf = vec![0u8; 32 * 1024];
            while left > 0 {
                let want = if left < buf.len() as u64 { left as usize } else { buf.len() };
                let got = f.read(&mut buf[..want])?;
                if got == 0 {
                    break;
                }
                w.write_all(&buf[..got])?;
                left -= got as u64;
            }
        }
    }
    w.flush()
}
