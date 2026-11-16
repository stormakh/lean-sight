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
    /// Segment trie over the patterns; `nodes[0]` is the root.
    nodes: Vec<Node>,
}

/// A trie node: the patterns that continue with a literal, with a param, end here, or end here with a tail.
#[derive(Default)]
struct Node {
    /// (literal, child node), sorted by literal.
    lit: Vec<(String, usize)>,
    param: Option<usize>,
    /// Entries whose pattern ends at this node.
    ends: Vec<usize>,
    /// Entries whose next segment is a tail (matches any rest, also an empty one).
    tails: Vec<usize>,
}

/// Search state: the best matching entry so far and whether any entry matched the path.
struct Search<'m> {
    method: &'m str,
    best: Option<usize>,
    path_matched: bool,
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
        Router { entries: Vec::new(), nodes: vec![Node::default()] }
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
        let idx = self.entries.len();
        let mut node = 0;
        for s in segs.iter() {
            match s {
                Seg::Lit(l) => {
                    node = match self.nodes[node].lit.binary_search_by(|e| e.0.as_str().cmp(l.as_str())) {
                        Ok(k) => self.nodes[node].lit[k].1,
                        Err(k) => {
                            let child = self.nodes.len();
                            self.nodes.push(Node::default());
                            self.nodes[node].lit.insert(k, (l.clone(), child));
                            child
                        }
                    };
                }
                Seg::Param(_) => {
                    node = match self.nodes[node].param {
                        Some(c) => c,
                        None => {
                            let child = self.nodes.len();
                            self.nodes.push(Node::default());
                            self.nodes[node].param = Some(child);
                            child
                        }
                    };
                }
                Seg::Tail(_) => {}
            }
        }
        match segs.last() {
            Some(Seg::Tail(_)) => self.nodes[node].tails.push(idx),
            _ => self.nodes[node].ends.push(idx),
        }
        self.entries.push(Entry { method: method.to_ascii_uppercase(), segs, lits, value });
    }
    /// Finds the route for `method` and raw (encoded) `path`.
    pub fn find(&self, method: &str, path: &str) -> Found<'_, T> {
        // split without allocating for usual paths
        let mut buf: [&str; 24] = [""; 24];
        let t = path.strip_prefix('/').unwrap_or(path);
        let mut n = 0;
        if !t.is_empty() {
            for p in t.split('/') {
                if n == buf.len() {
                    n += 1;
                    break;
                }
                buf[n] = p;
                n += 1;
            }
        }
        if n > buf.len() {
            let parts = split(path);
            return self.find_parts(method, &parts);
        }
        self.find_parts(method, &buf[..n])
    }

    fn find_parts(&self, method: &str, parts: &[&str]) -> Found<'_, T> {
        // a trailing slash ("/api/x/") matches "/api/x"
        let mut n = parts.len();
        if n > 0 && parts[n - 1].is_empty() {
            n -= 1;
        }
        let parts = &parts[..n];
        // The trie reaches exactly the entries whose pattern matches the path; among them the winner is the one
        // with the most literal segments, then the first added.
        let mut st = Search { method, best: None, path_matched: false };
        self.walk(0, parts, 0, &mut st);
        match st.best {
            Some(b) => {
                let e = &self.entries[b];
                Found::Ok(&e.value, capture(&e.segs, parts))
            }
            None if st.path_matched => Found::MethodNotAllowed,
            None => Found::NotFound,
        }
    }

    fn walk(&self, node: usize, parts: &[&str], d: usize, st: &mut Search<'_>) {
        let nd = &self.nodes[node];
        for &i in nd.tails.iter() {
            self.consider(i, st);
        }
        if d == parts.len() {
            for &i in nd.ends.iter() {
                self.consider(i, st);
            }
            return;
        }
        let part = parts[d];
        if let Ok(k) = nd.lit.binary_search_by(|e| e.0.as_str().cmp(part)) {
            self.walk(nd.lit[k].1, parts, d + 1, st);
        }
        if let Some(c) = nd.param {
            if !part.is_empty() {
                self.walk(c, parts, d + 1, st);
            }
        }
    }

    fn consider(&self, i: usize, st: &mut Search<'_>) {
        st.path_matched = true;
        let e = &self.entries[i];
        let method = st.method;
        let method_ok = e.method == "*" || e.method == method || (method == "HEAD" && e.method == "GET");
        if !method_ok {
            return;
        }
        match st.best {
            Some(b) if self.entries[b].lits > e.lits || (self.entries[b].lits == e.lits && b < i) => {}
            _ => st.best = Some(i),
        }
    }
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
