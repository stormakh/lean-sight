//! The subset of Go templates that `public/views/*.html` use, with `[[` `]]` delimiters:
//! `[[.A.B]]`, `[[$.A]]`, `[[$v.A]]`, `[[if X]]`, `[[else if X]]`, `[[else]]`, `[[end]]`, `[[range $v := X]]`,
//! `and`, `or`, `not`, `eq`, `ne`, string literals, and `[[-` / `-]]` whitespace trimming.
//! Data is a `Json` value. Output: strings are HTML-escaped; objects, arrays, numbers and booleans are written as
//! JSON (what html/template writes for them inside `<script>`).
use crate::util::html_escape;
use json::Json;

#[derive(Debug, Clone)]
enum Arg {
    /// A path from the root (`.A.B` or `$.A.B`); empty = the root itself.
    Root(Vec<String>),
    /// A path from a range variable.
    Var(String, Vec<String>),
    Lit(String),
}

#[derive(Debug, Clone)]
enum Expr {
    Arg(Arg),
    And(Vec<Expr>),
    Or(Vec<Expr>),
    Not(Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Ne(Box<Expr>, Box<Expr>),
}

/// Where an output sits, as html/template's contextual escaping sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    /// HTML text or attribute: strings are HTML-escaped.
    Html,
    /// JavaScript value position: strings are written as JSON string literals.
    JsValue,
    /// Inside a quoted JavaScript string: strings are JSON-escaped without quotes.
    JsString,
}

/// Lexical position tracking while parsing (html, inside a `<script ...` tag, or in script text).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Ctx {
    Html,
    ScriptTag,
    Script,
}

struct ParseState {
    ctx: Ctx,
    after_quote: bool,
}

fn advance(st: &mut ParseState, text: &str) {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match st.ctx {
            Ctx::Html => {
                if b[i..].starts_with(b"<script") {
                    st.ctx = Ctx::ScriptTag;
                    i += 7;
                    continue;
                }
            }
            Ctx::ScriptTag => {
                if b[i] == b'>' {
                    st.ctx = Ctx::Script;
                }
            }
            Ctx::Script => {
                if b[i..].starts_with(b"</script") {
                    st.ctx = Ctx::Html;
                    i += 8;
                    continue;
                }
            }
        }
        i += 1;
    }
    st.after_quote = match b.last() {
        Some(c) => *c == b'\'' || *c == b'"',
        None => st.after_quote,
    };
}

fn mode_of(st: &ParseState) -> Mode {
    match st.ctx {
        Ctx::Script => {
            if st.after_quote {
                Mode::JsString
            } else {
                Mode::JsValue
            }
        }
        _ => Mode::Html,
    }
}

#[derive(Debug, Clone)]
enum Node {
    Text(String),
    Out(Expr, Mode),
    If(Vec<(Expr, Vec<Node>)>, Vec<Node>),
    Range(String, Expr, Vec<Node>),
}

#[derive(Debug, Clone)]
pub struct Template {
    nodes: Vec<Node>,
}

#[derive(Debug, Clone)]
enum Tok {
    Text(String),
    Action(String),
}

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let mut toks: Vec<Tok> = Vec::new();
    let mut rest = src;
    let mut trim_next = false;
    while !rest.is_empty() {
        match rest.find("[[") {
            Some(i) => {
                let mut text = rest[..i].to_string();
                let after = &rest[i + 2..];
                let (trim_prev, after) = match after.strip_prefix("- ") {
                    Some(a) => (true, a),
                    None => match after.strip_prefix('-') {
                        Some(a) if !a.starts_with(|c: char| c.is_ascii_digit()) => (true, a),
                        _ => (false, after),
                    },
                };
                if trim_next {
                    text = text.trim_start().to_string();
                }
                if trim_prev {
                    text = text.trim_end().to_string();
                }
                if !text.is_empty() {
                    toks.push(Tok::Text(text));
                }
                let end = match after.find("]]") {
                    Some(e) => e,
                    None => return Err("unclosed [[".to_string()),
                };
                let mut body = &after[..end];
                trim_next = false;
                if let Some(b) = body.strip_suffix(" -") {
                    body = b;
                    trim_next = true;
                }
                toks.push(Tok::Action(body.trim().to_string()));
                rest = &after[end + 2..];
            }
            None => {
                let mut text = rest.to_string();
                if trim_next {
                    text = text.trim_start().to_string();
                }
                if !text.is_empty() {
                    toks.push(Tok::Text(text));
                }
                rest = "";
            }
        }
    }
    Ok(toks)
}

fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    for c in s.chars() {
        if in_str {
            cur.push(c);
            if c == '"' {
                in_str = false;
            }
            continue;
        }
        if c == '"' {
            in_str = true;
            cur.push(c);
            continue;
        }
        if c == ' ' || c == '\t' || c == '\n' {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            continue;
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn parse_arg(w: &str) -> Arg {
    if w.starts_with('"') && w.ends_with('"') && w.len() >= 2 {
        return Arg::Lit(w[1..w.len() - 1].to_string());
    }
    if w == "." || w == "$" {
        return Arg::Root(Vec::new());
    }
    if let Some(p) = w.strip_prefix("$.") {
        return Arg::Root(p.split('.').map(|s| s.to_string()).collect());
    }
    if let Some(p) = w.strip_prefix('.') {
        return Arg::Root(p.split('.').map(|s| s.to_string()).collect());
    }
    if let Some(v) = w.strip_prefix('$') {
        let mut it = v.split('.');
        let name = it.next().unwrap_or("").to_string();
        return Arg::Var(name, it.map(|s| s.to_string()).collect());
    }
    Arg::Lit(w.to_string())
}

fn parse_expr(ws: &[String]) -> Result<Expr, String> {
    if ws.is_empty() {
        return Err("empty expression".to_string());
    }
    let args = |from: usize| -> Vec<Expr> { ws[from..].iter().map(|w| Expr::Arg(parse_arg(w))).collect() };
    match ws[0].as_str() {
        "and" => Ok(Expr::And(args(1))),
        "or" => Ok(Expr::Or(args(1))),
        "not" => Ok(Expr::Not(Box::new(parse_expr(&ws[1..])?))),
        "eq" | "ne" => {
            if ws.len() != 3 {
                return Err(format!("{} needs two arguments", ws[0]));
            }
            let a = Box::new(Expr::Arg(parse_arg(&ws[1])));
            let b = Box::new(Expr::Arg(parse_arg(&ws[2])));
            Ok(if ws[0] == "eq" { Expr::Eq(a, b) } else { Expr::Ne(a, b) })
        }
        _ => {
            if ws.len() != 1 {
                return Err(format!("unsupported expression {:?}", ws.join(" ")));
            }
            Ok(Expr::Arg(parse_arg(&ws[0])))
        }
    }
}

/// Parses nodes until an `end` / `else` at this level. Returns the nodes and the stopping action.
fn parse_nodes(toks: &[Tok], pos: &mut usize, st: &mut ParseState) -> Result<(Vec<Node>, String), String> {
    let mut nodes = Vec::new();
    while *pos < toks.len() {
        let t = toks[*pos].clone();
        *pos += 1;
        match t {
            Tok::Text(s) => {
                advance(st, &s);
                nodes.push(Node::Text(s));
            }
            Tok::Action(a) => {
                if a.starts_with("/*") {
                    continue;
                }
                let ws = words(&a);
                if ws.is_empty() {
                    continue;
                }
                match ws[0].as_str() {
                    "end" | "else" => return Ok((nodes, a)),
                    "if" => {
                        let mut branches: Vec<(Expr, Vec<Node>)> = Vec::new();
                        let mut cond = parse_expr(&ws[1..])?;
                        let mut els: Vec<Node> = Vec::new();
                        loop {
                            let (body, stop) = parse_nodes(toks, pos, st)?;
                            branches.push((cond.clone(), body));
                            let sw = words(&stop);
                            if sw.first().map(|s| s.as_str()) == Some("end") {
                                break;
                            }
                            if sw.len() > 2 && sw[1] == "if" {
                                cond = parse_expr(&sw[2..])?;
                                continue;
                            }
                            let (body, stop2) = parse_nodes(toks, pos, st)?;
                            if words(&stop2).first().map(|s| s.as_str()) != Some("end") {
                                return Err("expected end after else".to_string());
                            }
                            els = body;
                            break;
                        }
                        nodes.push(Node::If(branches, els));
                    }
                    "range" => {
                        // range $v := X
                        if ws.len() >= 4 && ws[2] == ":=" {
                            let var = ws[1].trim_start_matches('$').to_string();
                            let e = parse_expr(&ws[3..])?;
                            let (body, stop) = parse_nodes(toks, pos, st)?;
                            if words(&stop).first().map(|s| s.as_str()) != Some("end") {
                                return Err("range with else is not supported".to_string());
                            }
                            nodes.push(Node::Range(var, e, body));
                        } else {
                            return Err(format!("unsupported range {a:?}"));
                        }
                    }
                    _ => nodes.push(Node::Out(parse_expr(&ws)?, mode_of(st))),
                }
            }
        }
    }
    Ok((nodes, String::new()))
}

fn lookup<'a>(root: &'a Json, path: &[String]) -> Option<&'a Json> {
    let mut cur = root;
    for p in path.iter() {
        // Go templates name struct fields (`.User.Language`); the data holds JSON keys (`language`).
        cur = match cur.get(p) {
            Some(v) => v,
            None => {
                // lower the leading capitals: "Language" -> "language", "JSFiles" -> "jsFiles"
                let b = p.as_bytes();
                let mut n = 0;
                while n < b.len() && b[n].is_ascii_uppercase() {
                    n += 1;
                }
                let lower = if n == b.len() || n <= 1 { n } else { n - 1 };
                let mut k = p[..lower].to_ascii_lowercase();
                k.push_str(&p[lower..]);
                cur.get(&k)?
            }
        };
    }
    Some(cur)
}

fn truthy(v: &Json) -> bool {
    match v {
        Json::Null => false,
        Json::Bool(b) => *b,
        Json::Int(n) => *n != 0,
        Json::Float(f) => *f != 0.0,
        Json::Str(s) => !s.is_empty(),
        Json::Arr(a) => !a.is_empty(),
        Json::Obj(o) => !o.is_empty(),
    }
}

struct Scope<'a> {
    root: &'a Json,
    vars: Vec<(String, &'a Json)>,
}

fn eval<'a>(e: &Expr, sc: &Scope<'a>) -> Json {
    match e {
        Expr::Arg(a) => match a {
            Arg::Lit(s) => Json::Str(s.clone()),
            Arg::Root(p) => lookup(sc.root, p).cloned().unwrap_or(Json::Null),
            Arg::Var(name, p) => {
                for (n, v) in sc.vars.iter().rev() {
                    if n == name {
                        return lookup(v, p).cloned().unwrap_or(Json::Null);
                    }
                }
                Json::Null
            }
        },
        Expr::And(xs) => {
            let mut last = Json::Bool(true);
            for x in xs.iter() {
                last = eval(x, sc);
                if !truthy(&last) {
                    return last;
                }
            }
            last
        }
        Expr::Or(xs) => {
            let mut last = Json::Bool(false);
            for x in xs.iter() {
                last = eval(x, sc);
                if truthy(&last) {
                    return last;
                }
            }
            last
        }
        Expr::Not(x) => Json::Bool(!truthy(&eval(x, sc))),
        Expr::Eq(a, b) => Json::Bool(eval(a, sc) == eval(b, sc)),
        Expr::Ne(a, b) => Json::Bool(eval(a, sc) != eval(b, sc)),
    }
}

fn render_nodes<'a>(nodes: &[Node], sc: &mut Scope<'a>, out: &mut String) {
    for n in nodes.iter() {
        match n {
            Node::Text(t) => out.push_str(t),
            Node::Out(e, mode) => match (eval(e, sc), mode) {
                (Json::Str(s), Mode::Html) => html_escape(&s, out),
                (Json::Str(s), Mode::JsValue) => json::write_str(&s, out),
                (Json::Str(s), Mode::JsString) => {
                    let q = json::quote(&s);
                    out.push_str(&q[1..q.len() - 1]);
                }
                (Json::Null, Mode::JsValue) => out.push_str("null"),
                (Json::Null, _) => {}
                (v, _) => v.write(out),
            },
            Node::If(branches, els) => {
                let mut done = false;
                for (c, body) in branches.iter() {
                    if truthy(&eval(c, sc)) {
                        render_nodes(body, sc, out);
                        done = true;
                        break;
                    }
                }
                if !done {
                    render_nodes(els, sc, out);
                }
            }
            Node::Range(var, e, body) => {
                let list = eval(e, sc);
                if let Json::Arr(items) = list {
                    for it in items.iter() {
                        // the item lives only for this iteration: render with a scope that owns a reference to it
                        let mut inner = Scope { root: sc.root, vars: Vec::new() };
                        for v in sc.vars.iter() {
                            inner.vars.push((v.0.clone(), v.1));
                        }
                        inner.vars.push((var.clone(), it));
                        render_nodes(body, &mut inner, out);
                    }
                }
            }
        }
    }
}

impl Template {
    pub fn parse(src: &str) -> Result<Template, String> {
        let toks = lex(src)?;
        let mut pos = 0;
        let mut st = ParseState { ctx: Ctx::Html, after_quote: false };
        let (nodes, stop) = parse_nodes(&toks, &mut pos, &mut st)?;
        if !stop.is_empty() {
            return Err(format!("unexpected {stop:?}"));
        }
        Ok(Template { nodes })
    }
    pub fn render(&self, data: &Json) -> String {
        let mut out = String::with_capacity(16 * 1024);
        let mut sc = Scope { root: data, vars: Vec::new() };
        render_nodes(&self.nodes, &mut sc, &mut out);
        out
    }
}
