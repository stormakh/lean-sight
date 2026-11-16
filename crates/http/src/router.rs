//! Path router. Patterns are `/`-separated segments:
//! - `name`: literal
//! - `:name`: one segment, captured (percent-decoded)
//! - `*` or `*name`: the rest of the path (may be empty), captured under "*" or `name`; must be last
//!
//! When several patterns match, the one with the most literal segments wins, then the one added first.
//! Method `*` matches any method. A HEAD request also matches GET routes.
use crate::util::path_decode;
use std::sync::OnceLock;

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
    /// Segment trie over the patterns, built on the first lookup (routes are added at startup).
    trie: OnceLock<Trie>,
}

/// A compact segment trie: `nodes[0]` is the root; literal text stays in the entries.
struct Trie {
    nodes: Vec<Node>,
    /// Literal edges, `(child node, entry, segment)`: the literal is `entries[entry].segs[segment]`. Each node's
    /// edges are one sorted range.
    edges: Vec<(u32, u32, u32)>,
    /// Entry indexes of the `ends` and `tails` ranges.
    idx: Vec<u32>,
}

/// A trie node: the patterns that continue with a literal, with a param, end here, or end here with a tail.
#[derive(Clone, Copy, Default)]
struct Node {
    edges: (u32, u32),
    /// Child for a param segment, 0 for none (the root is nobody's child).
    param: u32,
    /// Entries whose pattern ends at this node.
    ends: (u32, u32),
    /// Entries whose next segment is a tail (matches any rest, also an empty one).
    tails: (u32, u32),
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
        Router { entries: Vec::new(), trie: OnceLock::new() }
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
        self.trie = OnceLock::new();
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
        let trie = self.trie.get_or_init(|| build_trie(&self.entries));
        let mut st = Search { method, best: None, path_matched: false };
        self.walk(trie, 0, parts, 0, &mut st);
        match st.best {
            Some(b) => {
                let e = &self.entries[b];
                Found::Ok(&e.value, capture(&e.segs, parts))
            }
            None if st.path_matched => Found::MethodNotAllowed,
            None => Found::NotFound,
        }
    }

    fn walk(&self, t: &Trie, node: usize, parts: &[&str], d: usize, st: &mut Search<'_>) {
        let nd = t.nodes[node];
        for &i in t.idx[nd.tails.0 as usize..nd.tails.1 as usize].iter() {
            self.consider(i as usize, st);
        }
        if d == parts.len() {
            for &i in t.idx[nd.ends.0 as usize..nd.ends.1 as usize].iter() {
                self.consider(i as usize, st);
            }
            return;
        }
        let part = parts[d];
        let edges = &t.edges[nd.edges.0 as usize..nd.edges.1 as usize];
        if let Ok(k) = edges.binary_search_by(|e| lit_of(&self.entries, e.1, e.2).cmp(part)) {
            self.walk(t, edges[k].0 as usize, parts, d + 1, st);
        }
        if nd.param != 0 && !part.is_empty() {
            self.walk(t, nd.param as usize, parts, d + 1, st);
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

/// The literal text of segment `seg` of entry `entry`.
fn lit_of<T>(entries: &[Entry<T>], entry: u32, seg: u32) -> &str {
    match &entries[entry as usize].segs[seg as usize] {
        Seg::Lit(l) => l.as_str(),
        _ => "",
    }
}

fn build_trie<T>(entries: &[Entry<T>]) -> Trie {
    let mut t = Trie { nodes: Vec::new(), edges: Vec::new(), idx: Vec::new() };
    let all: Vec<u32> = (0..entries.len() as u32).collect();
    t.nodes.push(Node::default());
    build_node(entries, &mut t, 0, &all, 0);
    t.nodes.shrink_to_fit();
    t.edges.shrink_to_fit();
    t.idx.shrink_to_fit();
    t
}

/// Fills node `n` for the entries `es` (in insertion order) that reach it after `d` segments.
fn build_node<T>(entries: &[Entry<T>], t: &mut Trie, n: usize, es: &[u32], d: usize) {
    let mut lits: Vec<u32> = Vec::new();
    let mut params: Vec<u32> = Vec::new();
    let start = t.idx.len() as u32;
    for &e in es.iter() {
        let segs = &entries[e as usize].segs;
        if d == segs.len() {
            t.idx.push(e);
        }
    }
    let mid = t.idx.len() as u32;
    for &e in es.iter() {
        let segs = &entries[e as usize].segs;
        if d < segs.len() {
            match &segs[d] {
                Seg::Tail(_) => t.idx.push(e),
                Seg::Lit(_) => lits.push(e),
                Seg::Param(_) => params.push(e),
            }
        }
    }
    t.nodes[n].ends = (start, mid);
    t.nodes[n].tails = (mid, t.idx.len() as u32);
    // one edge per distinct literal, sorted; entries keep insertion order inside each group
    // insertion sort by literal (stable, so insertion order stays inside a group); it runs once, on few items,
    // and keeps the binary small
    for i in 1..lits.len() {
        let mut j = i;
        while j > 0 && lit_of(entries, lits[j - 1], d as u32) > lit_of(entries, lits[j], d as u32) {
            lits.swap(j - 1, j);
            j -= 1;
        }
    }
    let e0 = t.edges.len();
    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < lits.len() {
        let mut j = i + 1;
        while j < lits.len() && lit_of(entries, lits[j], d as u32) == lit_of(entries, lits[i], d as u32) {
            j += 1;
        }
        let child = t.nodes.len();
        t.nodes.push(Node::default());
        t.edges.push((child as u32, lits[i], d as u32));
        groups.push((i, j));
        i = j;
    }
    t.nodes[n].edges = (e0 as u32, t.edges.len() as u32);
    for (k, (i, j)) in groups.iter().enumerate() {
        let child = t.edges[e0 + k].0 as usize;
        build_node(entries, t, child, &lits[*i..*j], d + 1);
    }
    if !params.is_empty() {
        let child = t.nodes.len();
        t.nodes.push(Node::default());
        t.nodes[n].param = child as u32;
        build_node(entries, t, child, &params, d + 1);
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
