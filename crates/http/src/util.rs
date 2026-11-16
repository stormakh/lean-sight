//! URL encoding, query parsing and HTTP dates.

fn hexval(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Percent-decodes `s` and turns `+` into a space (query and form rules). Bad escapes stay as they are.
pub fn url_decode(s: &str) -> String {
    decode(s, true)
}
/// Percent-decodes a path segment (`+` stays `+`).
pub fn path_decode(s: &str) -> String {
    decode(s, false)
}
fn decode(s: &str, plus: bool) -> String {
    let b = s.as_bytes();
    if !b.iter().any(|c| *c == b'%' || (plus && *c == b'+')) {
        return s.to_string();
    }
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hexval(b[i + 1]), hexval(b[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        if plus && c == b'+' {
            out.push(b' ');
        } else {
            out.push(c);
        }
        i += 1;
    }
    match String::from_utf8(out) {
        Ok(s) => s,
        Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
    }
}

/// Percent-encodes everything but unreserved characters (Go `url.QueryEscape`, space as `+`).
pub fn url_encode(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.bytes() {
        if c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c == b'.' || c == b'~' {
            out.push(c as char);
        } else if c == b' ' {
            out.push('+');
        } else {
            out.push('%');
            out.push(HEX[(c >> 4) as usize] as char);
            out.push(HEX[(c & 15) as usize] as char);
        }
    }
    out
}

/// Decoded `k=v` pairs of a query string or form body, in order. A key without `=` has an empty value.
pub fn parse_query(q: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for part in q.split('&') {
        if part.is_empty() {
            continue;
        }
        match part.split_once('=') {
            Some((k, v)) => out.push((url_decode(k), url_decode(v))),
            None => out.push((url_decode(part), String::new())),
        }
    }
    out
}

const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// (year, month 1-12, day 1-31) from days since 1970-01-01.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
/// Days since 1970-01-01 from a civil date.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn push2(s: &mut String, n: u32) {
    s.push((b'0' + (n / 10) as u8) as char);
    s.push((b'0' + (n % 10) as u8) as char);
}

/// IMF-fixdate, as in `Date` and `Last-Modified`: `Sun, 06 Nov 1994 08:49:37 GMT`.
pub fn http_date(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86400);
    let secs = unix_secs.rem_euclid(86400) as u32;
    let (y, m, d) = civil_from_days(days);
    let mut s = String::with_capacity(29);
    s.push_str(DAYS[days.rem_euclid(7) as usize]);
    s.push_str(", ");
    push2(&mut s, d);
    s.push(' ');
    s.push_str(MONTHS[(m - 1) as usize]);
    s.push(' ');
    s.push_str(&y.to_string());
    s.push(' ');
    push2(&mut s, secs / 3600);
    s.push(':');
    push2(&mut s, secs / 60 % 60);
    s.push(':');
    push2(&mut s, secs % 60);
    s.push_str(" GMT");
    s
}

/// Parses an IMF-fixdate. Returns unix seconds.
pub fn parse_http_date(s: &str) -> Option<i64> {
    // "Sun, 06 Nov 1994 08:49:37 GMT"
    let rest = s.split_once(", ")?.1;
    let mut it = rest.split(' ');
    let d: u32 = it.next()?.parse().ok()?;
    let mon = it.next()?;
    let mut m = 0u32;
    for (i, name) in MONTHS.iter().enumerate() {
        if *name == mon {
            m = i as u32 + 1;
        }
    }
    if m == 0 {
        return None;
    }
    let y: i64 = it.next()?.parse().ok()?;
    let t = it.next()?;
    let mut tp = t.split(':');
    let hh: i64 = tp.next()?.parse().ok()?;
    let mm: i64 = tp.next()?.parse().ok()?;
    let ss: i64 = tp.next()?.parse().ok()?;
    Some(days_from_civil(y, m, d) * 86400 + hh * 3600 + mm * 60 + ss)
}

/// Seconds since the unix epoch.
pub fn now_unix() -> i64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(_) => 0,
    }
}
