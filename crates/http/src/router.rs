//! Path router. Patterns are `/`-separated segments:
//! - `name`: literal
//! - `:name`: one segment, captured (percent-decoded)
//! - `*` or `*name`: the rest of the path (may be empty), captured under "*" or `name`; must be last
//!
//! When several patterns match, the one with the most literal segments wins, then the one added first.
//! Method `*` matches any method. A HEAD request also matches GET routes.
use crate::util::path_decode;

#[derive(Debug, Clone)]
enum Seg {
    Lit(String),
    Param(String),
    Tail(String),
}

struct Entry<T> {
    method: String,
    segs: Vec<Seg>,
    lits: usize,
    value: T,
}

pub struct Router<T> {
    entries: Vec<Entry<T>>,
}

pub enum Found<'a, T> {
    /// The route value and the captured params.
    Ok(&'a T, Vec<(String, String)>),
    /// The path matched, but not with this method.
    MethodNotAllowed,
    NotFound,
}

impl<T> Default for Router<T> {
    fn default() -> Self {
        Router::new()
    }
}

fn split(p: &str) -> Vec<&str> {
    let t = p.strip_prefix('/').unwrap_or(p);
    if t.is_empty() {
        return Vec::new();
    }
    t.split('/').collect()
}

impl<T> Router<T> {
    pub fn new() -> Router<T> {
        Router { entries: Vec::new() }
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Adds a route. Panics on a bad pattern (a tail that is not last), at startup.
    pub fn add(&mut self, method: &str, pattern: &str, value: T) {
        let mut segs = Vec::new();
        let mut lits = 0;
        let parts = split(pattern);
        let n = parts.len();
        for (i, s) in parts.into_iter().enumerate() {
            if let Some(name) = s.strip_prefix(':') {
                segs.push(Seg::Param(name.to_string()));
            } else if let Some(name) = s.strip_prefix('*') {
                if i + 1 != n {
                    panic!("route {pattern}: '*' must be the last segment");
                }
                segs.push(Seg::Tail(if name.is_empty() { "*".to_string() } else { name.to_string() }));
            } else {
                segs.push(Seg::Lit(s.to_string()));
                lits += 1;
            }
        }
        self.entries.push(Entry { method: method.to_ascii_uppercase(), segs, lits, value });
    }
    /// Finds the route for `method` and raw (encoded) `path`.
    pub fn find(&self, method: &str, path: &str) -> Found<'_, T> {
        let parts = split(path);
        // a trailing slash ("/api/x/") matches "/api/x"
        let mut n = parts.len();
        if n > 0 && parts[n - 1].is_empty() {
            n -= 1;
        }
        let parts = &parts[..n];
        let mut best: Option<usize> = None;
        let mut path_matched = false;
        for (i, e) in self.entries.iter().enumerate() {
            if !matches(&e.segs, parts) {
                continue;
            }
            path_matched = true;
            let method_ok = e.method == "*" || e.method == method || (method == "HEAD" && e.method == "GET");
            if !method_ok {
                continue;
            }
            match best {
                Some(b) if self.entries[b].lits >= e.lits => {}
                _ => best = Some(i),
            }
        }
        match best {
            Some(b) => {
                let e = &self.entries[b];
                Found::Ok(&e.value, capture(&e.segs, parts))
            }
            None if path_matched => Found::MethodNotAllowed,
            None => Found::NotFound,
        }
    }
}

fn matches(segs: &[Seg], parts: &[&str]) -> bool {
    let mut i = 0;
    for s in segs.iter() {
        match s {
            Seg::Tail(_) => return true,
            Seg::Lit(l) => {
                if i >= parts.len() || parts[i] != l.as_str() {
                    return false;
                }
            }
            Seg::Param(_) => {
                if i >= parts.len() || parts[i].is_empty() {
                    return false;
                }
            }
        }
        i += 1;
    }
    i == parts.len()
}

fn capture(segs: &[Seg], parts: &[&str]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    for s in segs.iter() {
        match s {
            Seg::Lit(_) => {}
            Seg::Param(name) => out.push((name.clone(), path_decode(parts[i]))),
            Seg::Tail(name) => {
                let rest = if i < parts.len() { parts[i..].join("/") } else { String::new() };
                out.push((name.clone(), path_decode(&rest)));
                return out;
            }
        }
        i += 1;
    }
    out
}
