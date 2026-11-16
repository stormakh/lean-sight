//! A pull JSON reader over a byte slice, with jsoniter's call pattern (`ReadObject` returns the next key or none at
//! the end, `ReadArray` returns whether another element follows). It reads numbers and number strings without
//! allocating, so the Prometheus converter can stream samples straight into columns.
use json::Json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Invalid,
    Str,
    Num,
    Null,
    Bool,
    Arr,
    Obj,
}

pub struct Iter<'a> {
    b: &'a [u8],
    /// The input as text when the caller checked it is UTF-8: strings are then sliced, not checked again.
    text: Option<&'a str>,
    pub pos: usize,
    /// Nesting depth of `skip` / `read_any`, bounded so a hostile answer cannot overflow the 64 KB thread stack.
    depth: u32,
}

const MAX_DEPTH: u32 = 64;

pub type R<T> = Result<T, String>;

impl<'a> Iter<'a> {
    pub fn new(b: &'a [u8]) -> Iter<'a> {
        Iter { b, text: None, pos: 0, depth: 0 }
    }

    /// An iterator over text already known to be UTF-8.
    pub fn new_str(t: &'a str) -> Iter<'a> {
        Iter { b: t.as_bytes(), text: Some(t), pos: 0, depth: 0 }
    }

    fn ws(&mut self) {
        while self.pos < self.b.len() {
            match self.b[self.pos] {
                b' ' | b'\n' | b'\r' | b'\t' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.b.get(self.pos).copied()
    }

    fn err<T>(&self, what: &str) -> R<T> {
        let start = self.pos.saturating_sub(10);
        let end = (self.pos + 10).min(self.b.len());
        Err(format!("{what}, error found in #10 byte of ...|{}|..., bigger context ...|{}|...", String::from_utf8_lossy(&self.b[start..end]), String::from_utf8_lossy(&self.b[self.pos.saturating_sub(50)..(self.pos + 50).min(self.b.len())])))
    }

    pub fn at_end(&mut self) -> bool {
        self.peek().is_none()
    }

    pub fn what_is_next(&mut self) -> Kind {
        match self.peek() {
            Some(b'"') => Kind::Str,
            Some(b'-') | Some(b'0'..=b'9') => Kind::Num,
            Some(b'n') => Kind::Null,
            Some(b't') | Some(b'f') => Kind::Bool,
            Some(b'[') => Kind::Arr,
            Some(b'{') => Kind::Obj,
            _ => Kind::Invalid,
        }
    }

    fn expect_lit(&mut self, lit: &[u8]) -> R<()> {
        if self.b.len() >= self.pos + lit.len() && &self.b[self.pos..self.pos + lit.len()] == lit {
            self.pos += lit.len();
            Ok(())
        } else {
            self.err("invalid literal")
        }
    }

    /// Next object key, or None at the end of the object. The first call reads the `{`. `null` reads as an
    /// empty object.
    pub fn read_object(&mut self) -> R<Option<String>> {
        match self.peek() {
            Some(b'{') => {
                self.pos += 1;
                if self.peek() == Some(b'}') {
                    self.pos += 1;
                    return Ok(None);
                }
                self.read_key().map(Some)
            }
            Some(b',') => {
                self.pos += 1;
                self.read_key().map(Some)
            }
            Some(b'}') => {
                self.pos += 1;
                Ok(None)
            }
            Some(b'n') => {
                self.expect_lit(b"null")?;
                Ok(None)
            }
            _ => self.err("ReadObject: expect { or , or } or n"),
        }
    }

    fn read_key(&mut self) -> R<String> {
        if self.peek() != Some(b'"') {
            return self.err("ReadObject: expect \" after {");
        }
        let k = self.read_string()?;
        if self.peek() != Some(b':') {
            return self.err("ReadObject: expect :");
        }
        self.pos += 1;
        Ok(k)
    }

    /// True when another element follows. The first call reads the `[`. `null` reads as an empty array.
    pub fn read_array(&mut self) -> R<bool> {
        match self.peek() {
            Some(b'[') => {
                self.pos += 1;
                if self.peek() == Some(b']') {
                    self.pos += 1;
                    return Ok(false);
                }
                Ok(true)
            }
            Some(b',') => {
                self.pos += 1;
                Ok(true)
            }
            Some(b']') => {
                self.pos += 1;
                Ok(false)
            }
            Some(b'n') => {
                self.expect_lit(b"null")?;
                Ok(false)
            }
            _ => self.err("ReadArray: expect [ or , or ] or n"),
        }
    }

    /// The raw bytes of a string without escapes (the common case), or None when it has escapes.
    fn string_span(&mut self) -> R<(usize, usize, bool)> {
        if self.peek() != Some(b'"') {
            return self.err("ReadString: expects \" or n");
        }
        let start = self.pos + 1;
        let mut i = start;
        let mut escaped = false;
        while i < self.b.len() {
            match self.b[i] {
                b'"' => {
                    self.pos = i + 1;
                    return Ok((start, i, escaped));
                }
                b'\\' => {
                    escaped = true;
                    i += 2;
                }
                _ => i += 1,
            }
        }
        self.pos = self.b.len();
        self.err("incomplete string")
    }

    /// The input bytes.
    pub fn bytes(&self) -> &'a [u8] {
        self.b
    }

    /// The span (start, end) of a number token.
    pub fn number_raw(&mut self) -> R<(usize, usize)> {
        self.ws();
        let start = self.pos;
        let b = self.b;
        let mut i = start;
        while i < b.len() && matches!(b[i], b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E') {
            i += 1;
        }
        if i == start {
            return self.err("readNumberAsString: invalid number");
        }
        self.pos = i;
        Ok((start, i))
    }

    /// The span (start, end) of a string's contents and whether it has escapes.
    pub fn string_raw(&mut self) -> R<(usize, usize, bool)> {
        self.string_span()
    }

    /// A string without escapes, borrowed from the input; None when it has escapes (the caller declines).
    pub fn read_str_raw(&mut self) -> R<Option<&'a str>> {
        let (s, e, escaped) = self.string_span()?;
        if escaped {
            return Ok(None);
        }
        // the span starts after a '"' and ends at one, so it is on character boundaries
        if let Some(t) = self.text {
            return Ok(t.get(s..e));
        }
        let b: &'a [u8] = self.b;
        match std::str::from_utf8(&b[s..e]) {
            Ok(t) => Ok(Some(t)),
            Err(_) => Ok(None),
        }
    }

    /// Like `read_object`, but the key is borrowed. `Some(None)` is a key with escapes.
    pub fn read_object_raw(&mut self) -> R<Option<Option<&'a str>>> {
        let first = match self.peek() {
            Some(b'{') => {
                self.pos += 1;
                if self.peek() == Some(b'}') {
                    self.pos += 1;
                    return Ok(None);
                }
                true
            }
            Some(b',') => {
                self.pos += 1;
                false
            }
            Some(b'}') => {
                self.pos += 1;
                return Ok(None);
            }
            Some(b'n') => {
                self.expect_lit(b"null")?;
                return Ok(None);
            }
            _ => return self.err("ReadObject: expect { or , or } or n"),
        };
        let _ = first;
        if self.peek() != Some(b'"') {
            return self.err("ReadObject: expect \" after {");
        }
        let k = self.read_str_raw()?;
        if self.peek() != Some(b':') {
            return self.err("ReadObject: expect :");
        }
        self.pos += 1;
        Ok(Some(k))
    }

    /// A JSON number that is a whole number of seconds, or seconds with a fraction, as Unix nanoseconds truncated
    /// to milliseconds exactly as `timeFromFloat(f)` (float math for fractions, exact for integers).
    pub fn read_time(&mut self) -> R<i64> {
        let t = self.number_span()?;
        let b = t.as_bytes();
        if !b.is_empty() && b.len() <= 10 && b.iter().all(|c| c.is_ascii_digit()) && (b.len() < 10 || b[0] < b'9') {
            let mut v: i64 = 0;
            for c in b {
                v = v * 10 + (c - b'0') as i64;
            }
            return Ok(v * 1_000_000_000);
        }
        match t.parse::<f64>() {
            Ok(f) => Ok(((f * 1000.0) as i64).wrapping_mul(1_000_000)),
            Err(_) => Err(format!("ReadFloat64: invalid number {t}")),
        }
    }

    pub fn read_string(&mut self) -> R<String> {
        if self.peek() == Some(b'n') {
            self.expect_lit(b"null")?;
            return Ok(String::new());
        }
        let (s, e, escaped) = self.string_span()?;
        if !escaped {
            return Ok(String::from_utf8_lossy(&self.b[s..e]).into_owned());
        }
        unescape(&self.b[s..e]).ok_or_else(|| "invalid escape in string".to_string())
    }

    /// A string holding a float (`"1.5"`, `"NaN"`, `"+Inf"`), parsed as Go's `strconv.ParseFloat`.
    pub fn read_f64_string(&mut self) -> R<f64> {
        let (s, e, escaped) = self.string_span()?;
        if escaped {
            let t = unescape(&self.b[s..e]).unwrap_or_default();
            return parse_go_float(&t).ok_or_else(|| format!("strconv.ParseFloat: parsing {}: invalid syntax", json::quote(&t)));
        }
        let t = std::str::from_utf8(&self.b[s..e]).unwrap_or("");
        match parse_go_float(t) {
            Some(v) => Ok(v),
            None => Err(format!("strconv.ParseFloat: parsing {}: invalid syntax", json::quote(t))),
        }
    }

    fn number_span(&mut self) -> R<&'a str> {
        self.ws();
        let start = self.pos;
        while self.pos < self.b.len() {
            match self.b[self.pos] {
                b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E' => self.pos += 1,
                _ => break,
            }
        }
        if start == self.pos {
            return self.err("readNumberAsString: invalid number");
        }
        let b: &'a [u8] = self.b;
        Ok(std::str::from_utf8(&b[start..self.pos]).unwrap_or(""))
    }

    pub fn read_f64(&mut self) -> R<f64> {
        let t = self.number_span()?;
        match t.parse::<f64>() {
            Ok(v) => Ok(v),
            Err(_) => Err(format!("ReadFloat64: invalid number {t}")),
        }
    }

    /// jsoniter `ReadInt8`.
    pub fn read_i8(&mut self) -> R<i8> {
        let t = self.number_span()?;
        match t.parse::<i8>() {
            Ok(v) => Ok(v),
            Err(_) => Err(format!("ReadInt8: invalid number {t}")),
        }
    }

    pub fn skip(&mut self) -> R<()> {
        if self.depth >= MAX_DEPTH {
            return self.err("Skip: nesting too deep");
        }
        self.depth += 1;
        let r = self.skip_inner();
        self.depth -= 1;
        r
    }

    fn skip_inner(&mut self) -> R<()> {
        match self.what_is_next() {
            Kind::Str => {
                self.string_span()?;
                Ok(())
            }
            Kind::Num => {
                self.number_span()?;
                Ok(())
            }
            Kind::Null => self.expect_lit(b"null"),
            Kind::Bool => {
                if self.b.get(self.pos) == Some(&b't') {
                    self.expect_lit(b"true")
                } else {
                    self.expect_lit(b"false")
                }
            }
            Kind::Arr => {
                while self.read_array()? {
                    self.skip()?;
                }
                Ok(())
            }
            Kind::Obj => {
                while self.read_object()?.is_some() {
                    self.skip()?;
                }
                Ok(())
            }
            Kind::Invalid => self.err("Skip: do not know how to skip"),
        }
    }

    /// Skips a value and returns its bytes.
    pub fn skip_and_return_bytes(&mut self) -> R<&'a [u8]> {
        self.ws();
        let start = self.pos;
        self.skip()?;
        let b: &'a [u8] = self.b;
        Ok(&b[start..self.pos])
    }

    /// Any value as a tree (jsoniter `Read`). Numbers become floats, as Go's `any`.
    pub fn read_any(&mut self) -> R<Json> {
        if self.depth >= MAX_DEPTH {
            return self.err("Read: nesting too deep");
        }
        self.depth += 1;
        let r = self.read_any_inner();
        self.depth -= 1;
        r
    }

    fn read_any_inner(&mut self) -> R<Json> {
        match self.what_is_next() {
            Kind::Str => Ok(Json::Str(self.read_string()?)),
            Kind::Num => Ok(Json::Float(self.read_f64()?)),
            Kind::Null => {
                self.expect_lit(b"null")?;
                Ok(Json::Null)
            }
            Kind::Bool => {
                if self.b.get(self.pos) == Some(&b't') {
                    self.expect_lit(b"true")?;
                    Ok(Json::Bool(true))
                } else {
                    self.expect_lit(b"false")?;
                    Ok(Json::Bool(false))
                }
            }
            Kind::Arr => {
                let mut a = Vec::new();
                while self.read_array()? {
                    a.push(self.read_any()?);
                }
                Ok(Json::Arr(a))
            }
            Kind::Obj => {
                let mut o: Vec<(String, Json)> = Vec::new();
                while let Some(k) = self.read_object()? {
                    let v = self.read_any()?;
                    match o.iter().position(|p| p.0 == k) {
                        Some(i) => o[i].1 = v,
                        None => o.push((k, v)),
                    }
                }
                Ok(Json::Obj(o))
            }
            Kind::Invalid => self.err("Read: unexpected value type"),
        }
    }

    /// An object of strings into sorted labels (`iter.ReadVal(&data.Labels)`). `null` gives an empty set.
    pub fn read_labels(&mut self, out: &mut super::frame::Labels) -> R<()> {
        while let Some(k) = self.read_object()? {
            let v = self.read_string()?;
            super::frame::labels_set(out, &k, &v);
        }
        Ok(())
    }
}

fn hexval(c: u8) -> Option<u32> {
    match c {
        b'0'..=b'9' => Some((c - b'0') as u32),
        b'a'..=b'f' => Some((c - b'a' + 10) as u32),
        b'A'..=b'F' => Some((c - b'A' + 10) as u32),
        _ => None,
    }
}

fn unescape(b: &[u8]) -> Option<String> {
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c != b'\\' {
            out.push(c);
            i += 1;
            continue;
        }
        let e = *b.get(i + 1)?;
        i += 2;
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
                if i + 4 > b.len() {
                    return None;
                }
                let mut cp = 0u32;
                for k in 0..4 {
                    cp = cp * 16 + hexval(b[i + k])?;
                }
                i += 4;
                if (0xD800..0xDC00).contains(&cp) && i + 6 <= b.len() && b[i] == b'\\' && b[i + 1] == b'u' {
                    let mut lo = 0u32;
                    let mut ok = true;
                    for k in 0..4 {
                        match hexval(b[i + 2 + k]) {
                            Some(h) => lo = lo * 16 + h,
                            None => ok = false,
                        }
                    }
                    if ok && (0xDC00..0xE000).contains(&lo) {
                        cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                        i += 6;
                    }
                }
                let ch = char::from_u32(cp).unwrap_or('\u{FFFD}');
                let mut buf = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            }
            _ => return None,
        }
    }
    Some(String::from_utf8_lossy(&out).into_owned())
}

/// Go `strconv.ParseFloat(s, 64)` for the forms Prometheus writes: decimals, exponents, `NaN`, `[+-]Inf`.
pub fn parse_go_float(t: &str) -> Option<f64> {
    if let Ok(v) = t.parse::<f64>() {
        // Rust also accepts "infinity"/"inf"/"nan" in any case, as Go does
        return Some(v);
    }
    None
}
