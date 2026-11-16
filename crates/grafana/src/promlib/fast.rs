//! The hot path of `/api/ds/query`: a successful matrix or vector answer converted and written as frame JSON in
//! one pass, without building frames, and with no allocation per series or sample (labels are borrowed from the
//! answer, columns reuse two buffers). It writes exactly what `parse_response` + `DataResponse::write_json`
//! write (a test compares both on every golden input). Anything else (errors, warnings, stats, histograms,
//! escaped strings, empty results) is declined, and the caller uses the general path on the same answer.
use super::client::HttpResp;
use super::frame::write_go_f64;
use super::jsonpull::{Iter, Kind, R};
use super::models::{MS, Query};
use super::querydata::executed_query_string;

/// Reusable buffers: the value column text, NaN/Inf positions (index, 0 NaN / 1 +Inf / 2 -Inf), a name.
#[derive(Default)]
pub struct Scratch {
    vals: Vec<u8>,
    special: Vec<(usize, u8)>,
    text: String,
}

/// True when `t` is exactly what Go's JSON encoder writes for its value (`strconv.AppendFloat(f, 'f', -1)` for
/// 1e-6 <= |f| < 1e21 or 0): plain decimal, no leading or trailing zeros, at most 17 significant digits.
/// Prometheus (and the other Go servers) write sample values this way, so the text is copied, not re-formatted.
fn go_json_float_form(t: &[u8]) -> bool {
    let mut i = 0;
    if t.first() == Some(&b'-') {
        i = 1;
    }
    let int_start = i;
    while i < t.len() && t[i].is_ascii_digit() {
        i += 1;
    }
    let int_len = i - int_start;
    if int_len == 0 || int_len > 21 || (int_len > 1 && t[int_start] == b'0') {
        return false;
    }
    let int_zero = int_len == 1 && t[int_start] == b'0';
    let mut sig = if int_zero { 0 } else { int_len };
    if i == t.len() {
        return sig <= 17;
    }
    if t[i] != b'.' {
        return false;
    }
    i += 1;
    let frac_start = i;
    while i < t.len() && t[i].is_ascii_digit() {
        i += 1;
    }
    if i != t.len() || i == frac_start || t[i - 1] == b'0' {
        return false;
    }
    if int_zero {
        let mut z = 0;
        while t[frac_start + z] == b'0' {
            z += 1;
        }
        if z > 5 {
            return false; // below 1e-6: Go writes an exponent
        }
        sig = i - frac_start - z;
    } else {
        sig += i - frac_start;
    }
    sig <= 17
}

enum Out {
    Done,
    Decline,
}

/// Appends the frames of one answer to `out` (each preceded by a comma unless `*first`). Returns false, with
/// `out` unchanged, when the answer needs the general path.
pub fn write_frames(q: &Query, ref_id: &str, res: &HttpResp, first: &mut bool, out: &mut Vec<u8>, s: &mut Scratch) -> bool {
    if !(200..300).contains(&res.status) || res.header("server-timing").is_some() {
        return false;
    }
    let start = out.len();
    let first_before = *first;
    match run(q, ref_id, &res.body, first, out, s) {
        Ok(Out::Done) => true,
        _ => {
            out.truncate(start);
            *first = first_before;
            false
        }
    }
}

fn run(q: &Query, ref_id: &str, body: &[u8], first: &mut bool, out: &mut Vec<u8>, s: &mut Scratch) -> R<Out> {
    // checked once here, so strings are borrowed without a check each; anything else needs the general path
    let text = match std::str::from_utf8(body) {
        Ok(t) => t,
        Err(_) => return Ok(Out::Decline),
    };
    let mut it = Iter::new_str(text);
    let mut success = false;
    let mut wrote = false;
    while let Some(k) = it.read_object_raw()? {
        match k {
            Some("status") => match it.read_str_raw()? {
                Some("success") => success = true,
                _ => return Ok(Out::Decline),
            },
            Some("data") => {
                if wrote || it.what_is_next() != Kind::Obj {
                    return Ok(Out::Decline);
                }
                match data(q, ref_id, &mut it, first, out, s)? {
                    Out::Done => wrote = true,
                    Out::Decline => return Ok(Out::Decline),
                }
            }
            Some("warnings") | Some("infos") => {
                // an empty list adds nothing; any notice needs the general path
                if it.what_is_next() == Kind::Arr {
                    if it.read_array()? {
                        return Ok(Out::Decline);
                    }
                } else {
                    it.skip()?;
                }
            }
            Some("error") | Some("errorType") | None => return Ok(Out::Decline),
            Some(_) => it.skip()?,
        }
    }
    if !success || !wrote { Ok(Out::Decline) } else { Ok(Out::Done) }
}

fn data(q: &Query, ref_id: &str, it: &mut Iter<'_>, first: &mut bool, out: &mut Vec<u8>, s: &mut Scratch) -> R<Out> {
    let mut rt: Option<&'static str> = None;
    let mut wrote = false;
    let mut idx = 0usize;
    while let Some(k) = it.read_object_raw()? {
        match k {
            Some("resultType") => {
                rt = match it.read_str_raw()? {
                    Some("matrix") => Some("matrix"),
                    Some("vector") => Some("vector"),
                    _ => return Ok(Out::Decline),
                };
            }
            Some("result") => {
                let r = match rt {
                    Some(r) => r,
                    None => return Ok(Out::Decline),
                };
                if wrote {
                    return Ok(Out::Decline);
                }
                while it.read_array()? {
                    if let Out::Decline = series(q, ref_id, r, idx, it, first, out, s)? {
                        return Ok(Out::Decline);
                    }
                    idx += 1;
                }
                wrote = true;
            }
            Some("stats") | Some("encodingFlags") | None => return Ok(Out::Decline),
            Some(_) => it.skip()?,
        }
    }
    if !wrote || idx == 0 { Ok(Out::Decline) } else { Ok(Out::Done) }
}

#[allow(clippy::too_many_arguments)]
fn series<'a>(q: &Query, ref_id: &str, rt: &'static str, idx: usize, it: &mut Iter<'a>, first: &mut bool, out: &mut Vec<u8>, s: &mut Scratch) -> R<Out> {
    s.vals.clear();
    s.special.clear();
    let mut labels: Vec<(&'a str, &'a str)> = Vec::new();
    let mut has_metric = false;
    let mut started = false;
    let mut n = 0usize;
    while let Some(k) = it.read_object_raw()? {
        match k {
            Some("metric") => {
                if started {
                    return Ok(Out::Decline); // labels after samples: the general path handles it
                }
                has_metric = true;
                while let Some(lk) = it.read_object_raw()? {
                    let lk = match lk {
                        Some(x) => x,
                        None => return Ok(Out::Decline),
                    };
                    match it.read_str_raw()? {
                        Some(v) => labels.push((lk, v)),
                        None => return Ok(Out::Decline),
                    }
                }
            }
            Some("value") | Some("values") => {
                if !started {
                    if !begin(q, ref_id, rt, idx == 0, has_metric, &mut labels, first, out, s) {
                        return Ok(Out::Decline);
                    }
                    started = true;
                } else if n > 0 {
                    return Ok(Out::Decline);
                }
                if k == Some("value") {
                    sample(it, &mut n, out, s)?;
                } else if it.read_array()? {
                    values(it, &mut n, out, s)?;
                }
            }
            Some("histogram") | Some("histograms") | None => return Ok(Out::Decline),
            Some(_) => it.skip()?,
        }
    }
    if !started && !begin(q, ref_id, rt, idx == 0, has_metric, &mut labels, first, out, s) {
        return Ok(Out::Decline);
    }
    finish(out, s);
    Ok(Out::Done)
}

/// The rest of a `values` array, from its first element: compact samples in a tight loop, others via `sample`.
#[inline]
fn values(it: &mut Iter<'_>, n: &mut usize, out: &mut Vec<u8>, s: &mut Scratch) -> R<()> {
    let b = it.bytes();
    loop {
        match compact_sample(b, it.pos, *n, out, &mut s.vals) {
            Some(p) => {
                *n += 1;
                // the common separators, read here; anything else goes through read_array
                match b.get(p) {
                    Some(b',') => {
                        it.pos = p + 1;
                        continue;
                    }
                    Some(b']') => {
                        it.pos = p + 1;
                        return Ok(());
                    }
                    _ => it.pos = p,
                }
            }
            None => sample(it, n, out, s)?,
        }
        if !it.read_array()? {
            return Ok(());
        }
    }
}

/// The common sample form `[1700000000,"1.5"]` (whole seconds, a value in Go's JSON float form, no spaces),
/// read and written in one scan. None (nothing written) for any other form: `sample` handles it.
#[inline(always)]
fn compact_sample(b: &[u8], start: usize, n: usize, out: &mut Vec<u8>, vals: &mut Vec<u8>) -> Option<usize> {
    if start + 16 <= b.len() {
        let h = &b[start..start + 16];
        // `[1700000000,"0"]`: the most common sample of all
        if h[0] == b'[' && h[11] == b',' && h[12] == b'"' && h[13] == b'0' && h[14] == b'"' && h[15] == b']' && h[1] < b'9' && ten_digits(&h[1..11]) {
            if n > 0 {
                out.push(b',');
                vals.push(b',');
            }
            out.extend_from_slice(&h[1..11]);
            out.extend_from_slice(b"000");
            vals.push(b'0');
            return Some(start + 16);
        }
    }
    let mut i = start;
    if *b.get(i)? != b'[' {
        return None;
    }
    i += 1;
    let ts = i;
    let tl;
    if ts + 12 <= b.len() && b[ts + 10] == b',' && b[ts + 11] == b'"' && b[ts] < b'9' && ten_digits(&b[ts..ts + 10]) {
        // the usual 10-digit time, checked without a loop exit per digit
        tl = 10;
        i = ts + 10;
    } else {
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        tl = i - ts;
        if tl == 0 || tl > 10 || (tl == 10 && b[ts] >= b'9') || *b.get(i)? != b',' || *b.get(i + 1)? != b'"' {
            return None;
        }
    }
    i += 2;
    let vs = i;
    if *b.get(i)? == b'-' {
        i += 1;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let int_len = i - int_start;
    if int_len == 0 || int_len > 17 || (int_len > 1 && b[int_start] == b'0') {
        return None;
    }
    let int_zero = int_len == 1 && b[int_start] == b'0';
    let mut sig = if int_zero { 0 } else { int_len };
    if *b.get(i)? == b'.' {
        i += 1;
        let fs = i;
        let mut z = 0;
        if int_zero {
            while i < b.len() && b[i] == b'0' {
                i += 1;
            }
            z = i - fs;
            if z > 5 {
                return None; // below 1e-6: Go writes an exponent
            }
        }
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == fs + z || b[i - 1] == b'0' {
            return None;
        }
        sig += i - fs - z;
    }
    if sig > 17 || *b.get(i)? != b'"' || *b.get(i + 1)? != b']' {
        return None;
    }
    // the bytes checked above are ASCII digits, '-' and '.': copied as they are
    if n > 0 {
        out.push(b',');
        vals.push(b',');
    }
    push_seconds_as_ms(&b[ts..ts + tl], out);
    if i - vs == 1 {
        vals.push(b[vs]);
    } else {
        vals.extend_from_slice(&b[vs..i]);
    }
    Some(i + 2)
}

/// True when all 10 bytes are ASCII digits.
#[inline]
fn ten_digits(d: &[u8]) -> bool {
    let mut ok = true;
    for c in d.iter().take(10) {
        ok &= c.wrapping_sub(b'0') < 10;
    }
    ok
}

/// Whole seconds (1 to 10 ASCII digits) as milliseconds: the same digits and "000" (timeFromFloat is exact here).
#[inline(always)]
fn push_seconds_as_ms(d: &[u8], out: &mut Vec<u8>) {
    if d == b"0" {
        out.push(b'0');
    } else if d.len() == 10 {
        out.extend_from_slice(&d[..10]);
        out.extend_from_slice(b"000");
    } else {
        out.extend_from_slice(d);
        out.extend_from_slice(b"000");
    }
}

/// One `[time, "value"]`: the time goes to `out`, the value to `s.vals`.
fn sample(it: &mut Iter<'_>, n: &mut usize, out: &mut Vec<u8>, s: &mut Scratch) -> R<()> {
    it.read_array()?;
    let (a, b) = it.number_raw()?;
    let bytes = it.bytes();
    let tok = &bytes[a..b];
    if *n > 0 {
        out.push(b',');
        s.vals.push(b',');
    }
    if !tok.is_empty() && tok.len() <= 10 && tok.iter().all(|c| c.is_ascii_digit()) && (tok.len() < 10 || tok[0] < b'9') {
        push_seconds_as_ms(tok, out);
    } else {
        match std::str::from_utf8(tok).ok().and_then(|x| x.parse::<f64>().ok()) {
            Some(f) => push_i64((f * 1000.0) as i64, out),
            None => return Err("ReadFloat64: invalid number".to_string()),
        }
    }
    it.read_array()?;
    let (a, b, escaped) = it.string_raw()?;
    if escaped {
        return Err("escaped value".to_string());
    }
    let raw = &bytes[a..b];
    if go_json_float_form(raw) {
        s.vals.extend_from_slice(raw);
    } else {
        let txt = std::str::from_utf8(raw).unwrap_or("");
        match super::jsonpull::parse_go_float(txt) {
            Some(f) if f.is_finite() => {
                let mut t = String::new();
                write_go_f64(f, &mut t);
                s.vals.extend_from_slice(t.as_bytes());
            }
            Some(f) => {
                s.special.push((*n, if f.is_nan() { 0 } else if f > 0.0 { 1 } else { 2 }));
                s.vals.extend_from_slice(b"null");
            }
            None => return Err(format!("strconv.ParseFloat: parsing {}: invalid syntax", json::quote(txt))),
        }
    }
    it.read_array()?;
    *n += 1;
    Ok(())
}

/// Appends `n` in decimal.
fn push_i64(n: i64, out: &mut Vec<u8>) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    let neg = n < 0;
    let mut u = n.unsigned_abs();
    loop {
        i -= 1;
        buf[i] = b'0' + (u % 10) as u8;
        u /= 10;
        if u == 0 {
            break;
        }
    }
    if neg {
        out.push(b'-');
    }
    out.extend_from_slice(&buf[i..]);
}

/// Per byte: bit 1 when `json::write_str` escapes it (or may: 0xE2 starts U+2028/9), bit 2 when `go_quote_q`
/// escapes it. Controls, `"` and `\\` have both; `<`, `>`, `&`, 0xE2 only bit 1; 0x7f only bit 2.
static ESCAPE: [u8; 256] = escape_table();

const fn escape_table() -> [u8; 256] {
    let mut t = [0u8; 256];
    let mut i = 0;
    while i < 0x20 {
        t[i] = 3;
        i += 1;
    }
    t[b'"' as usize] = 3;
    t[b'\\' as usize] = 3;
    t[b'&' as usize] = 1;
    t[b'<' as usize] = 1;
    t[b'>' as usize] = 1;
    t[0xE2] = 1;
    t[0x7f] = 2;
    t
}

/// True when neither `json::write_str` nor `go_quote_q` changes anything in `v` (they only add quotes).
fn plain(v: &str) -> bool {
    v.bytes().all(|c| ESCAPE[c as usize] == 0)
}

/// `"v"` for a `plain` string.
#[inline]
fn wplain(v: &str, out: &mut Vec<u8>) {
    out.push(b'"');
    out.extend_from_slice(v.as_bytes());
    out.push(b'"');
}

/// `json::write_str` into bytes (same output): runs without escapes are copied whole.
fn wstr(v: &str, out: &mut Vec<u8>) {
    let b = v.as_bytes();
    out.push(b'"');
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        // copy the run of bytes that need no escape in one go
        let mut j = i;
        while j < b.len() && ESCAPE[b[j] as usize] & 1 == 0 {
            j += 1;
        }
        if j == b.len() {
            break;
        }
        i = j;
        let c = b[i];
        let esc: &[u8] = match c {
            b'"' => b"\\\"",
            b'\\' => b"\\\\",
            b'\n' => b"\\n",
            b'\r' => b"\\r",
            b'\t' => b"\\t",
            b'<' => b"\\u003c",
            b'>' => b"\\u003e",
            b'&' => b"\\u0026",
            0xE2 => {
                if i + 2 < b.len() && b[i + 1] == 0x80 && (b[i + 2] == 0xA8 || b[i + 2] == 0xA9) {
                    out.extend_from_slice(&b[start..i]);
                    out.extend_from_slice(if b[i + 2] == 0xA8 { b"\\u2028" } else { b"\\u2029" });
                    i += 3;
                    start = i;
                } else {
                    i += 1;
                }
                continue;
            }
            _ => {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                out.extend_from_slice(&b[start..i]);
                out.extend_from_slice(b"\\u00");
                out.push(HEX[(c >> 4) as usize]);
                out.push(HEX[(c & 15) as usize]);
                i += 1;
                start = i;
                continue;
            }
        };
        out.extend_from_slice(&b[start..i]);
        out.extend_from_slice(esc);
        i += 1;
        start = i;
    }
    out.extend_from_slice(&b[start..]);
    out.push(b'"');
}

fn key(out: &mut Vec<u8>, k: &str) {
    out.push(b'"');
    out.extend_from_slice(k.as_bytes());
    out.extend_from_slice(b"\":");
}

/// `metricNameFromLabels` + `getName` into `s.text`.
fn display_name(q: &Query, labels: &[(&str, &str)], has_metric: bool, text: &mut String) {
    text.clear();
    let name = labels.iter().find(|p| p.0 == "__name__").map(|p| p.1);
    if q.legend_format == "__auto" && has_metric && !labels.is_empty() {
        return;
    }
    if !q.legend_format.is_empty() && q.legend_format != "__auto" {
        text.push_str(&super::querydata::legend_format(&q.legend_format, labels));
    } else {
        let n = if name.is_some() { labels.len() - 1 } else { labels.len() };
        if n == 0 {
            match name {
                Some(m) => text.push_str(m),
                None => text.push_str("{}"),
            }
        } else {
            text.push_str(name.unwrap_or(""));
            text.push('{');
            let mut sep = false;
            for (k, v) in labels.iter() {
                if *k == "__name__" {
                    continue;
                }
                if sep {
                    text.push_str(", ");
                }
                sep = true;
                text.push_str(k);
                text.push('=');
                super::querydata::go_quote_q(v, text);
            }
            text.push('}');
        }
    }
    if text == "{}" {
        text.clear();
        text.push_str(&q.expr);
    }
}

/// Writes the frame up to the time column (`... "data":{"values":[[`). False on duplicate label names.
#[allow(clippy::too_many_arguments)]
fn begin(q: &Query, ref_id: &str, rt: &'static str, first_of_answer: bool, has_metric: bool, labels: &mut [(&str, &str)], first: &mut bool, out: &mut Vec<u8>, s: &mut Scratch) -> bool {
    // labels sorted by name, names unique (as a Go map)
    labels.sort_by(|a, b| a.0.cmp(b.0));
    for w in labels.windows(2) {
        if w[0].0 == w[1].0 {
            return false;
        }
    }
    if !*first {
        out.push(b',');
    }
    *first = false;
    let step_ms = q.step / MS;
    out.extend_from_slice(b"{\"schema\":{");
    if !ref_id.is_empty() {
        key(out, "refId");
        wstr(ref_id, out);
        out.push(b',');
    }
    out.extend_from_slice(if rt == "vector" { "\"meta\":{\"type\":\"numeric-multi\",\"typeVersion\":[0,1],\"custom\":{" } else { "\"meta\":{\"type\":\"timeseries-multi\",\"typeVersion\":[0,1],\"custom\":{" }.as_bytes());
    if first_of_answer {
        key(out, "calculatedMinStep");
        push_i64(step_ms, out);
        out.push(b',');
    }
    out.extend_from_slice(b"\"resultType\":\"");
    out.extend_from_slice(rt.as_bytes());
    out.extend_from_slice(b"\"}");
    if first_of_answer {
        out.push(b',');
        key(out, "executedQueryString");
        wstr(&executed_query_string(q), out);
    }
    out.extend_from_slice(b"},\"fields\":[{\"name\":\"Time\",\"type\":\"time\",\"typeInfo\":{\"frame\":\"time.Time\"},\"config\":{\"interval\":");
    push_i64(step_ms, out);
    out.extend_from_slice(b"}},{\"name\":");
    let name = labels.iter().find(|p| p.0 == "__name__").map(|p| p.1);
    // the usual case: no label needs escaping, so strings are copied between quotes
    let all_plain = labels.iter().all(|(k, v)| plain(k) && plain(v));
    let w = if all_plain { wplain } else { wstr };
    w(name.unwrap_or("Value"), out);
    out.extend_from_slice(b",\"type\":\"number\",\"typeInfo\":{\"frame\":\"float64\"}");
    if has_metric {
        out.extend_from_slice(b",\"labels\":{");
        for (i, (k, v)) in labels.iter().enumerate() {
            if i > 0 {
                out.push(b',');
            }
            w(k, out);
            out.push(b':');
            w(v, out);
        }
        out.push(b'}');
    }
    let others = if name.is_some() { labels.len() - 1 } else { labels.len() };
    if all_plain && q.legend_format.is_empty() && others > 0 {
        // `metricNameFromLabels` written straight as JSON: `name{k=\"v\", ...}`
        out.extend_from_slice(b",\"config\":{\"displayNameFromDS\":\"");
        out.extend_from_slice(name.unwrap_or("").as_bytes());
        out.push(b'{');
        let mut sep = false;
        for (k, v) in labels.iter() {
            if *k == "__name__" {
                continue;
            }
            if sep {
                out.extend_from_slice(b", ");
            }
            sep = true;
            out.extend_from_slice(k.as_bytes());
            out.extend_from_slice(b"=\\\"");
            out.extend_from_slice(v.as_bytes());
            out.extend_from_slice(b"\\\"");
        }
        out.extend_from_slice(b"}\"}}]},\"data\":{\"values\":[[");
        return true;
    }
    let mut text = std::mem::take(&mut s.text);
    display_name(q, labels, has_metric, &mut text);
    if !text.is_empty() {
        out.extend_from_slice(b",\"config\":{\"displayNameFromDS\":");
        wstr(&text, out);
        out.push(b'}');
    }
    s.text = text;
    out.extend_from_slice(b"}]},\"data\":{\"values\":[[");
    true
}

/// The value column, `entities` for NaN/+Inf/-Inf, and the end of the frame.
fn finish(out: &mut Vec<u8>, s: &Scratch) {
    out.extend_from_slice(b"],[");
    out.extend_from_slice(&s.vals);
    out.extend_from_slice(b"]]");
    if !s.special.is_empty() {
        out.extend_from_slice(b",\"entities\":[null,{");
        let mut first_list = true;
        for (name, pick) in [("NaN", 0u8), ("Inf", 1), ("NegInf", 2)] {
            let mut started = false;
            for (i, kind) in s.special.iter() {
                if *kind != pick {
                    continue;
                }
                if !started {
                    if !first_list {
                        out.push(b',');
                    }
                    first_list = false;
                    key(out, name);
                    out.push(b'[');
                    started = true;
                } else {
                    out.push(b',');
                }
                push_i64(*i as i64, out);
            }
            if started {
                out.push(b']');
            }
        }
        out.extend_from_slice(b"}]");
    }
    out.extend_from_slice(b"}}");
}
