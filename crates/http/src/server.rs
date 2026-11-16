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
    let mut line: Vec<u8> = Vec::with_capacity(256);
    let mut date = DateCache { secs: -1, text: String::new() };
    loop {
        let req = match read_request(&mut r, &mut w, cfg, &remote_ip, &mut line) {
            Ok(req) => req,
            Err(ReadErr::Gone) => return,
            Err(ReadErr::Status(code)) => {
                let resp = Response::text(code, "text/plain; charset=utf-8", format!("{} {}", code, reason(code)));
                let _ = write_response(&mut w, resp, false, true, false, cfg, date.get());
                return;
            }
        };
        let keep = keep_alive(&req);
        let head = req.method == "HEAD";
        let gz = cfg.gzip && req.accepts_gzip();
        let resp = svc.handle(req);
        if write_response(&mut w, resp, head, !keep, gz, cfg, date.get()).is_err() || !keep {
            return;
        }
        // do not keep a big line buffer around between requests
        if line.capacity() > 4096 {
            line = Vec::with_capacity(256);
        }
    }
}

/// The `Date` header text, formatted once per second per connection.
struct DateCache {
    secs: i64,
    text: String,
}
impl DateCache {
    fn get(&mut self) -> &str {
        let now = now_unix();
        if now != self.secs {
            self.secs = now;
            self.text = http_date(now);
        }
        &self.text
    }
}

fn keep_alive(req: &Request) -> bool {
    let c = req.header("connection").unwrap_or("");
    if req.http10 { c.eq_ignore_ascii_case("keep-alive") } else { !c.eq_ignore_ascii_case("close") }
}

/// Index of the first b'\n' in `b`, eight bytes at a time.
fn find_nl(b: &[u8]) -> Option<usize> {
    const LO: u64 = 0x0101_0101_0101_0101;
    const HI: u64 = 0x8080_8080_8080_8080;
    let mut i = 0;
    while i + 8 <= b.len() {
        let s = &b[i..i + 8];
        let x = u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]) ^ (LO * b'\n' as u64);
        // the lowest set bit marks the first zero byte of x (higher false hits only follow a real one)
        let t = x.wrapping_sub(LO) & !x & HI;
        if t != 0 {
            return Some(i + (t.trailing_zeros() / 8) as usize);
        }
        i += 8;
    }
    while i < b.len() {
        if b[i] == b'\n' {
            return Some(i);
        }
        i += 1;
    }
    None
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
            match find_nl(buf) {
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

fn read_request(r: &mut BufReader<TcpStream>, w: &mut TcpStream, cfg: &ServerConfig, remote_ip: &str, line: &mut Vec<u8>) -> Result<Request, ReadErr> {
    let mut budget = cfg.max_header_bytes;
    // skip empty lines before a request line (RFC 7230 3.5)
    loop {
        read_line(r, line, &mut budget)?;
        if !line.is_empty() {
            break;
        }
    }
    let text = match std::str::from_utf8(line) {
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
        read_line(r, line, &mut budget)?;
        if line.is_empty() {
            break;
        }
        // from_utf8 first: it is faster than from_utf8_lossy on valid text, with the same result
        let lossy;
        let t: &str = match std::str::from_utf8(line) {
            Ok(t) => t,
            Err(_) => {
                lossy = String::from_utf8_lossy(line).into_owned();
                &lossy
            }
        };
        match t.split_once(':') {
            Some((k, v)) => headers.push((crate::trim_fast(k).to_ascii_lowercase(), crate::trim_fast(v).to_string())),
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

fn push_num(h: &mut Vec<u8>, mut n: u64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    h.extend_from_slice(&buf[i..]);
}

/// Bodies up to this size go out in the same write as the head (one copy, one syscall).
const COALESCE_BYTES: usize = 64 * 1024;

fn write_response(w: &mut TcpStream, mut resp: Response, head: bool, close: bool, gz_ok: bool, cfg: &ServerConfig, date: &str) -> std::io::Result<()> {
    // gzip
    let no_body = resp.status == 204 || resp.status == 304 || (100..200).contains(&resp.status);
    if gz_ok && !no_body && resp.header("content-encoding").is_none() && compressible(resp.header("content-type").unwrap_or("")) {
        let mut data: Option<Vec<u8>> = None;
        match &mut resp.body {
            Body::Bytes(b) => {
                if b.len() >= cfg.gzip_min_bytes {
                    data = Some(crate::gzip::gzip_vec(b));
                }
            }
            Body::File(f, len) => {
                if *len >= cfg.gzip_min_bytes as u64 && *len <= cfg.gzip_max_file_bytes {
                    let mut raw = Vec::with_capacity(*len as usize);
                    if f.read_to_end(&mut raw).is_ok() {
                        data = Some(crate::gzip::gzip_vec(&raw));
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
    let mut hlen = 64 + date.len();
    for (k, v) in resp.headers.iter() {
        hlen += k.len() + v.len() + 4;
    }
    let inline = !head && !no_body && matches!(&resp.body, Body::Bytes(b) if b.len() <= COALESCE_BYTES);
    let mut h: Vec<u8> = Vec::with_capacity(if inline { hlen + len as usize } else { hlen });
    h.extend_from_slice(b"HTTP/1.1 ");
    push_num(&mut h, resp.status as u64);
    h.push(b' ');
    h.extend_from_slice(reason(resp.status).as_bytes());
    h.extend_from_slice(b"\r\n");
    for (k, v) in resp.headers.iter() {
        if k.eq_ignore_ascii_case("content-length") {
            continue;
        }
        h.extend_from_slice(k.as_bytes());
        h.extend_from_slice(b": ");
        h.extend_from_slice(v.as_bytes());
        h.extend_from_slice(b"\r\n");
    }
    if !no_body {
        h.extend_from_slice(b"Content-Length: ");
        push_num(&mut h, len);
        h.extend_from_slice(b"\r\n");
    }
    h.extend_from_slice(b"Date: ");
    h.extend_from_slice(date.as_bytes());
    h.extend_from_slice(b"\r\n");
    if close {
        h.extend_from_slice(b"Connection: close\r\n");
    }
    h.extend_from_slice(b"\r\n");
    if head || no_body {
        return w.write_all(&h);
    }
    match resp.body {
        Body::Bytes(b) => {
            if inline {
                h.extend_from_slice(&b);
                w.write_all(&h)?;
            } else {
                w.write_all(&h)?;
                w.write_all(&b)?;
            }
        }
        Body::File(mut f, n) => {
            w.write_all(&h)?;
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
    Ok(())
}
