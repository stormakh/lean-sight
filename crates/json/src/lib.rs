//! JSON value with insertion-ordered object keys, a parser, and a compact writer.
//!
//! Numbers: an integer literal that fits in i64 parses to `Json::Int` and writes back with the same digits, so int64
//! ids keep every digit. Every other number is `Json::Float` and writes as Go's `encoding/json` writes a float64
//! (shortest digits that read back the same; exponent form below 1e-6 and from 1e21).
//! Strings write as Go's `json.Marshal` does: `<`, `>`, `&`, U+2028 and U+2029 are escaped.
pub mod sort;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Default)]
pub enum Json {
    #[default]
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct JsonError {
    pub msg: &'static str,
    pub offset: usize,
}
impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at offset {}", self.msg, self.offset)
    }
}
impl std::error::Error for JsonError {}

/// Max nesting depth the parser accepts. Request threads have small stacks.
pub const MAX_DEPTH: usize = 128;

impl Json {
    // ---- builders ----
    /// An empty object. Chain `.with(k, v)` to add keys in order.
    pub fn obj() -> Json {
        Json::Obj(Vec::new())
    }
    pub fn arr() -> Json {
        Json::Arr(Vec::new())
    }
    /// Builder: sets `k` (replacing an existing key in place) and returns self.
    pub fn with(mut self, k: &str, v: impl Into<Json>) -> Json {
        self.set(k, v);
        self
    }
    /// Builder: pushes `v` to an array and returns self.
    pub fn push_item(mut self, v: impl Into<Json>) -> Json {
        self.push(v);
        self
    }
    /// Sets a key on an object: replaces the value in place if the key exists, else appends. No-op on non-objects.
    pub fn set(&mut self, k: &str, v: impl Into<Json>) {
        if let Json::Obj(o) = self {
            let v = v.into();
            for e in o.iter_mut() {
                if e.0 == k {
                    e.1 = v;
                    return;
                }
            }
            o.push((k.to_string(), v));
        }
    }
    /// Appends to an array. No-op on non-arrays.
    pub fn push(&mut self, v: impl Into<Json>) {
        if let Json::Arr(a) = self {
            a.push(v.into());
        }
    }
    /// Removes a key from an object and returns its value.
    pub fn remove(&mut self, k: &str) -> Option<Json> {
        if let Json::Obj(o) = self {
            let mut i = 0;
            while i < o.len() {
                if o[i].0 == k {
                    return Some(o.remove(i).1);
                }
                i += 1;
            }
        }
        None
    }

    // ---- access ----
    pub fn get(&self, k: &str) -> Option<&Json> {
        if let Json::Obj(o) = self {
            for e in o.iter() {
                if e.0 == k {
                    return Some(&e.1);
                }
            }
        }
        None
    }
    pub fn get_mut(&mut self, k: &str) -> Option<&mut Json> {
        if let Json::Obj(o) = self {
            for e in o.iter_mut() {
                if e.0 == k {
                    return Some(&mut e.1);
                }
            }
        }
        None
    }
    /// Array element.
    pub fn at(&self, i: usize) -> Option<&Json> {
        match self {
            Json::Arr(a) => a.get(i),
            _ => None,
        }
    }
    /// Dotted path: `"spec.panels.0.title"`. A numeric segment indexes an array.
    pub fn path(&self, p: &str) -> Option<&Json> {
        let mut cur = self;
        for seg in p.split('.') {
            let next = match cur {
                Json::Obj(_) => cur.get(seg),
                Json::Arr(a) => match seg.parse::<usize>() {
                    Ok(i) => a.get(i),
                    Err(_) => None,
                },
                _ => None,
            };
            match next {
                Some(n) => cur = n,
                None => return None,
            }
        }
        Some(cur)
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    /// Integer value. A float with an integral value converts; a string does not.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Json::Int(n) => Some(*n),
            Json::Float(f) if f.fract() == 0.0 && f.abs() < 9.2e18 => Some(*f as i64),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Int(n) => Some(*n as f64),
            Json::Float(f) => Some(*f),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_arr(&self) -> Option<&Vec<Json>> {
        match self {
            Json::Arr(a) => Some(a),
            _ => None,
        }
    }
    pub fn as_arr_mut(&mut self) -> Option<&mut Vec<Json>> {
        match self {
            Json::Arr(a) => Some(a),
            _ => None,
        }
    }
    pub fn as_obj(&self) -> Option<&Vec<(String, Json)>> {
        match self {
            Json::Obj(o) => Some(o),
            _ => None,
        }
    }
    pub fn as_obj_mut(&mut self) -> Option<&mut Vec<(String, Json)>> {
        match self {
            Json::Obj(o) => Some(o),
            _ => None,
        }
    }
    /// `get(k)` as a string, or "".
    pub fn str_of(&self, k: &str) -> &str {
        match self.get(k) {
            Some(Json::Str(s)) => s,
            _ => "",
        }
    }
    /// `get(k)` as i64, or 0.
    pub fn i64_of(&self, k: &str) -> i64 {
        self.get(k).and_then(Json::as_i64).unwrap_or(0)
    }
    /// `get(k)` as bool, or false.
    pub fn bool_of(&self, k: &str) -> bool {
        self.get(k).and_then(Json::as_bool).unwrap_or(false)
    }

    // ---- text ----
    pub fn parse(t: &str) -> Result<Json, JsonError> {
        parse_text(t.as_bytes(), t)
    }
    pub fn parse_bytes(b: &[u8]) -> Result<Json, JsonError> {
        // Valid UTF-8 (the usual case) is checked once here, so strings slice the text without checking again.
        // Invalid input keeps the old behavior: each string is decoded lossily.
        match std::str::from_utf8(b) {
            Ok(t) => parse_text(b, t),
            Err(_) => parse_text(b, ""),
        }
    }
}

/// Parses `b`. `t` is the same text as a str when it is valid UTF-8, or "" when it is not.
fn parse_text(b: &[u8], t: &str) -> Result<Json, JsonError> {
    let mut p = 0;
    let v = parse_value(b, t, &mut p, 0)?;
    skip_ws(b, &mut p);
    if p != b.len() {
        return Err(JsonError { msg: "trailing data after JSON value", offset: p });
    }
    Ok(v)
}

impl Json {
    /// Compact JSON text.
    pub fn stringify(&self) -> String {
        let mut s = String::with_capacity(128);
        self.write(&mut s);
        s
    }
    /// Appends compact JSON text to `out`.
    pub fn write(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Int(n) => write_i64(*n, out),
            Json::Float(f) => write_f64(*f, out),
            Json::Str(v) => write_str(v, out),
            Json::Arr(a) => {
                out.push('[');
                let mut first = true;
                for v in a.iter() {
                    if !first {
                        out.push(',');
                    }
                    first = false;
                    v.write(out);
                }
                out.push(']');
            }
            Json::Obj(o) => {
                out.push('{');
                let mut first = true;
                for e in o.iter() {
                    if !first {
                        out.push(',');
                    }
                    first = false;
                    write_str(&e.0, out);
                    out.push(':');
                    e.1.write(out);
                }
                out.push('}');
            }
        }
    }
    /// Indented JSON text, as Go's `json.MarshalIndent(v, "", indent)`.
    pub fn stringify_pretty(&self, indent: &str) -> String {
        let mut s = String::with_capacity(128);
        self.write_pretty(&mut s, indent, 0);
        s
    }
    fn write_pretty(&self, out: &mut String, indent: &str, depth: usize) {
        match self {
            Json::Arr(a) if !a.is_empty() => {
                out.push('[');
                let mut first = true;
                for v in a.iter() {
                    if !first {
                        out.push(',');
                    }
                    first = false;
                    newline(out, indent, depth + 1);
                    v.write_pretty(out, indent, depth + 1);
                }
                newline(out, indent, depth);
                out.push(']');
            }
            Json::Obj(o) if !o.is_empty() => {
                out.push('{');
                let mut first = true;
                for e in o.iter() {
                    if !first {
                        out.push(',');
                    }
                    first = false;
                    newline(out, indent, depth + 1);
                    write_str(&e.0, out);
                    out.push_str(": ");
                    e.1.write_pretty(out, indent, depth + 1);
                }
                newline(out, indent, depth);
                out.push('}');
            }
            _ => self.write(out),
        }
    }
}

fn newline(out: &mut String, indent: &str, depth: usize) {
    out.push('\n');
    for _ in 0..depth {
        out.push_str(indent);
    }
}

impl fmt::Display for Json {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.stringify())
    }
}

// ---- From conversions for builders ----
impl From<&str> for Json {
    fn from(v: &str) -> Json {
        Json::Str(v.to_string())
    }
}
impl From<String> for Json {
    fn from(v: String) -> Json {
        Json::Str(v)
    }
}
impl From<&String> for Json {
    fn from(v: &String) -> Json {
        Json::Str(v.clone())
    }
}
impl From<bool> for Json {
    fn from(v: bool) -> Json {
        Json::Bool(v)
    }
}
impl From<i64> for Json {
    fn from(v: i64) -> Json {
        Json::Int(v)
    }
}
impl From<i32> for Json {
    fn from(v: i32) -> Json {
        Json::Int(v as i64)
    }
}
impl From<u32> for Json {
    fn from(v: u32) -> Json {
        Json::Int(v as i64)
    }
}
impl From<usize> for Json {
    fn from(v: usize) -> Json {
        Json::Int(v as i64)
    }
}
impl From<f64> for Json {
    fn from(v: f64) -> Json {
        Json::Float(v)
    }
}
impl From<Vec<Json>> for Json {
    fn from(v: Vec<Json>) -> Json {
        Json::Arr(v)
    }
}
impl From<Vec<String>> for Json {
    fn from(v: Vec<String>) -> Json {
        let mut a = Vec::with_capacity(v.len());
        for s in v {
            a.push(Json::Str(s));
        }
        Json::Arr(a)
    }
}
impl<T: Into<Json>> From<Option<T>> for Json {
    fn from(v: Option<T>) -> Json {
        match v {
            Some(x) => x.into(),
            None => Json::Null,
        }
    }
}

// ---- writer helpers (public so handlers can stream JSON text without building a tree) ----

/// Appends `n` in decimal.
pub fn write_i64(n: i64, out: &mut String) {
    if n == 0 {
        out.push('0');
        return;
    }
    let mut buf = [0u8; 21];
    let mut i = buf.len();
    let neg = n < 0;
    let mut u = n.unsigned_abs();
    while u > 0 {
        i -= 1;
        buf[i] = b'0' + (u % 10) as u8;
        u /= 10;
    }
    if neg {
        i -= 1;
        buf[i] = b'-';
    }
    out.push_str(std::str::from_utf8(&buf[i..]).unwrap_or(""));
}

/// Appends a float64 as Go's encoding/json writes it. NaN and infinities write `null` (Go fails there).
pub fn write_f64(f: f64, out: &mut String) {
    if !f.is_finite() {
        out.push_str("null");
        return;
    }
    let a = f.abs();
    if a != 0.0 && (a < 1e-6 || a >= 1e21) {
        // Go: 'e' format, then "e-07" -> "e-7". Rust `{:e}` gives "1e-7" / "1.5e21".
        let t = format!("{:e}", f);
        match t.find('e') {
            Some(i) => {
                out.push_str(&t[..i]);
                out.push('e');
                let exp = &t[i + 1..];
                if exp.starts_with('-') {
                    out.push_str(exp);
                } else {
                    out.push('+');
                    out.push_str(exp);
                }
            }
            None => out.push_str(&t),
        }
    } else {
        // Rust Display prints the shortest round-trip digits without an exponent; integral values have no ".0".
        use std::fmt::Write;
        let _ = write!(out, "{}", f);
    }
}

/// 1 for bytes that `write_str` must look at: '"', '\\', control bytes, '<', '>', '&', and 0xE2 (the first byte of
/// U+2028 and U+2029).
const fn esc_table() -> [u8; 256] {
    let mut t = [0u8; 256];
    let mut i = 0;
    while i < 0x20 {
        t[i] = 1;
        i += 1;
    }
    t[b'"' as usize] = 1;
    t[b'\\' as usize] = 1;
    t[b'<' as usize] = 1;
    t[b'>' as usize] = 1;
    t[b'&' as usize] = 1;
    t[0xE2] = 1;
    t
}
static ESC: [u8; 256] = esc_table();

/// True when any of the 8 bytes in `x` may need escaping (a byte below 0x20, '"', '\\', '<', '>', '&', or any byte
/// from 0x80). False hits are fine: the caller checks those bytes with `ESC`.
#[inline(always)]
fn word_needs_escape(x: u64) -> bool {
    const LO: u64 = 0x0101_0101_0101_0101;
    const HI: u64 = 0x8080_8080_8080_8080;
    let has = |c: u8| {
        let y = x ^ (LO * c as u64);
        y.wrapping_sub(LO) & !y
    };
    let below_space = x.wrapping_sub(LO * 0x20) & !x;
    ((x | below_space | has(b'"') | has(b'\\') | has(b'<') | has(b'>') | has(b'&')) & HI) != 0
}

/// Appends `v` as a JSON string literal with Go's escaping.
pub fn write_str(v: &str, out: &mut String) {
    out.reserve(v.len() + 2);
    out.push('"');
    let b = v.as_bytes();
    let mut start = 0;
    let mut i = 0;
    // bytes before `slow_end` are checked one by one
    let mut slow_end = 0;
    while i < b.len() {
        if i >= slow_end {
            if i + 8 <= b.len() {
                let w = &b[i..i + 8];
                let x = u64::from_le_bytes([w[0], w[1], w[2], w[3], w[4], w[5], w[6], w[7]]);
                if !word_needs_escape(x) {
                    i += 8;
                    continue;
                }
                slow_end = i + 8;
            } else {
                slow_end = b.len();
            }
        }
        let c = b[i];
        if ESC[c as usize] == 0 {
            i += 1;
            continue;
        }
        let esc: Option<&str> = match c {
            b'"' => Some("\\\""),
            b'\\' => Some("\\\\"),
            b'\n' => Some("\\n"),
            b'\r' => Some("\\r"),
            b'\t' => Some("\\t"),
            b'<' => Some("\\u003c"),
            b'>' => Some("\\u003e"),
            b'&' => Some("\\u0026"),
            _ => None,
        };
        if let Some(e) = esc {
            out.push_str(&v[start..i]);
            out.push_str(e);
            i += 1;
            start = i;
            continue;
        }
        if c < 0x20 {
            out.push_str(&v[start..i]);
            out.push_str("\\u00");
            out.push(HEX[(c >> 4) as usize] as char);
            out.push(HEX[(c & 15) as usize] as char);
            i += 1;
            start = i;
            continue;
        }
        // U+2028 and U+2029: E2 80 A8 / E2 80 A9
        if c == 0xE2 && i + 2 < b.len() && b[i + 1] == 0x80 && (b[i + 2] == 0xA8 || b[i + 2] == 0xA9) {
            out.push_str(&v[start..i]);
            out.push_str(if b[i + 2] == 0xA8 { "\\u2028" } else { "\\u2029" });
            i += 3;
            start = i;
            continue;
        }
        i += 1;
    }
    out.push_str(&v[start..]);
    out.push('"');
}
const HEX: &[u8; 16] = b"0123456789abcdef";

/// `v` as a JSON string literal.
pub fn quote(v: &str) -> String {
    let mut s = String::with_capacity(v.len() + 2);
    write_str(v, &mut s);
    s
}

// ---- parser ----
fn err(msg: &'static str, offset: usize) -> JsonError {
    JsonError { msg, offset }
}
fn skip_ws(b: &[u8], p: &mut usize) {
    while *p < b.len() && matches!(b[*p], b' ' | b'\n' | b'\r' | b'\t') {
        *p += 1;
    }
}
fn parse_value(b: &[u8], t: &str, p: &mut usize, depth: usize) -> Result<Json, JsonError> {
    if depth > MAX_DEPTH {
        return Err(err("JSON nested too deep", *p));
    }
    skip_ws(b, p);
    if *p >= b.len() {
        return Err(err("unexpected end of JSON input", *p));
    }
    match b[*p] {
        b'{' => {
            *p += 1;
            let mut o: Vec<(String, Json)> = Vec::new();
            skip_ws(b, p);
            if *p < b.len() && b[*p] == b'}' {
                *p += 1;
                return Ok(Json::Obj(o));
            }
            loop {
                skip_ws(b, p);
                let k = parse_str(b, t, p)?;
                skip_ws(b, p);
                if *p >= b.len() || b[*p] != b':' {
                    return Err(err("expected ':' after object key", *p));
                }
                *p += 1;
                let v = parse_value(b, t, p, depth + 1)?;
                // A repeated key keeps its first position and takes the last value.
                let mut found = false;
                for e in o.iter_mut() {
                    if e.0 == k {
                        e.1 = v.clone();
                        found = true;
                        break;
                    }
                }
                if !found {
                    o.push((k, v));
                }
                skip_ws(b, p);
                if *p < b.len() && b[*p] == b',' {
                    *p += 1;
                    continue;
                }
                if *p < b.len() && b[*p] == b'}' {
                    *p += 1;
                    return Ok(Json::Obj(o));
                }
                return Err(err("expected ',' or '}' in object", *p));
            }
        }
        b'[' => {
            *p += 1;
            let mut a = Vec::new();
            skip_ws(b, p);
            if *p < b.len() && b[*p] == b']' {
                *p += 1;
                return Ok(Json::Arr(a));
            }
            loop {
                a.push(parse_value(b, t, p, depth + 1)?);
                skip_ws(b, p);
                if *p < b.len() && b[*p] == b',' {
                    *p += 1;
                    continue;
                }
                if *p < b.len() && b[*p] == b']' {
                    *p += 1;
                    return Ok(Json::Arr(a));
                }
                return Err(err("expected ',' or ']' in array", *p));
            }
        }
        b'"' => Ok(Json::Str(parse_str(b, t, p)?)),
        b't' if b[*p..].starts_with(b"true") => {
            *p += 4;
            Ok(Json::Bool(true))
        }
        b'f' if b[*p..].starts_with(b"false") => {
            *p += 5;
            Ok(Json::Bool(false))
        }
        b'n' if b[*p..].starts_with(b"null") => {
            *p += 4;
            Ok(Json::Null)
        }
        b'-' | b'0'..=b'9' => parse_num(b, t, p),
        _ => Err(err("invalid character looking for beginning of value", *p)),
    }
}
fn parse_num(b: &[u8], txt: &str, p: &mut usize) -> Result<Json, JsonError> {
    let s = *p;
    let mut is_float = false;
    if b[*p] == b'-' {
        *p += 1;
    }
    let ds = *p;
    while *p < b.len() && b[*p].is_ascii_digit() {
        *p += 1;
    }
    if *p == ds {
        return Err(err("invalid number", s));
    }
    if *p < b.len() && b[*p] == b'.' {
        is_float = true;
        *p += 1;
        let fs = *p;
        while *p < b.len() && b[*p].is_ascii_digit() {
            *p += 1;
        }
        if *p == fs {
            return Err(err("invalid number", s));
        }
    }
    if *p < b.len() && (b[*p] == b'e' || b[*p] == b'E') {
        is_float = true;
        *p += 1;
        if *p < b.len() && (b[*p] == b'+' || b[*p] == b'-') {
            *p += 1;
        }
        let es = *p;
        while *p < b.len() && b[*p].is_ascii_digit() {
            *p += 1;
        }
        if *p == es {
            return Err(err("invalid number", s));
        }
    }
    // the number is ASCII, so it is valid text when the input is
    let t = if txt.len() == b.len() {
        &txt[s..*p]
    } else {
        match std::str::from_utf8(&b[s..*p]) {
            Ok(t) => t,
            Err(_) => return Err(err("invalid number", s)),
        }
    };
    // "-0" is a float (-0.0) as in Go, so it writes back as "-0", not "0"
    if !is_float && t != "-0" {
        if let Ok(n) = t.parse::<i64>() {
            return Ok(Json::Int(n));
        }
    }
    match t.parse::<f64>() {
        Ok(f) => Ok(Json::Float(f)),
        Err(_) => Err(err("invalid number", s)),
    }
}
fn hex4(b: &[u8], p: &mut usize) -> Result<u32, JsonError> {
    if *p + 4 > b.len() {
        return Err(err("invalid \\u escape", *p));
    }
    let mut v = 0u32;
    for i in 0..4 {
        let c = b[*p + i];
        let d = match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => return Err(err("invalid \\u escape", *p)),
        };
        v = v * 16 + d as u32;
    }
    *p += 4;
    Ok(v)
}
fn parse_str(b: &[u8], txt: &str, p: &mut usize) -> Result<String, JsonError> {
    if *p >= b.len() || b[*p] != b'"' {
        return Err(err("expected string", *p));
    }
    *p += 1;
    let start = *p;
    // positions of '"' and '\\' are ASCII, so they are char boundaries of `txt`
    let valid = txt.len() == b.len();
    // fast path: no escapes
    while *p < b.len() && b[*p] != b'"' && b[*p] != b'\\' {
        *p += 1;
    }
    if *p < b.len() && b[*p] == b'"' {
        let s = if valid {
            txt[start..*p].to_string()
        } else {
            match std::str::from_utf8(&b[start..*p]) {
                Ok(s) => s.to_string(),
                Err(_) => String::from_utf8_lossy(&b[start..*p]).into_owned(),
            }
        };
        *p += 1;
        return Ok(s);
    }
    let mut out: Vec<u8> = b[start..*p].to_vec();
    loop {
        if *p >= b.len() {
            return Err(err("unterminated string", *p));
        }
        let c = b[*p];
        *p += 1;
        match c {
            b'"' => break,
            b'\\' => {
                if *p >= b.len() {
                    return Err(err("invalid escape", *p));
                }
                let e = b[*p];
                *p += 1;
                match e {
                    b'"' => out.push(b'"'),
                    b'\\' => out.push(b'\\'),
                    b'/' => out.push(b'/'),
                    b'b' => out.push(8),
                    b'f' => out.push(12),
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b't' => out.push(b'\t'),
                    b'u' => {
                        let mut v = hex4(b, p)?;
                        if (0xd800..0xdc00).contains(&v) && b[*p..].starts_with(b"\\u") {
                            let save = *p;
                            *p += 2;
                            let lo = hex4(b, p)?;
                            if (0xdc00..0xe000).contains(&lo) {
                                v = 0x10000 + ((v - 0xd800) << 10) + (lo - 0xdc00);
                            } else {
                                *p = save;
                            }
                        }
                        let ch = char::from_u32(v).unwrap_or('\u{fffd}');
                        let mut t = [0u8; 4];
                        out.extend_from_slice(ch.encode_utf8(&mut t).as_bytes());
                    }
                    _ => return Err(err("invalid escape", *p - 1)),
                }
            }
            _ => {
                // copy the run up to the next quote or backslash at once
                let s = *p - 1;
                while *p < b.len() && b[*p] != b'"' && b[*p] != b'\\' {
                    *p += 1;
                }
                out.extend_from_slice(&b[s..*p]);
            }
        }
    }
    match String::from_utf8(out) {
        Ok(s) => Ok(s),
        Err(e) => Ok(String::from_utf8_lossy(e.as_bytes()).into_owned()),
    }
}
