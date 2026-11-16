//! Small helpers: clock, time formats, uids, slugs.
use std::sync::atomic::{AtomicU64, Ordering};

/// Unix seconds.
pub fn now() -> i64 {
    http::util::now_unix()
}
/// Unix milliseconds.
pub fn now_ms() -> i64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(_) => 0,
    }
}

fn push_n(s: &mut String, n: i64, width: usize) {
    let t = n.to_string();
    for _ in t.len()..width {
        s.push('0');
    }
    s.push_str(&t);
}

/// RFC 3339 in UTC, as Go's `time.Time` marshals a UTC time: `2026-11-16T12:00:00Z`.
pub fn rfc3339(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86400);
    let secs = unix_secs.rem_euclid(86400);
    let (y, m, d) = http::util::civil_from_days(days);
    let mut s = String::with_capacity(20);
    push_n(&mut s, y, 4);
    s.push('-');
    push_n(&mut s, m as i64, 2);
    s.push('-');
    push_n(&mut s, d as i64, 2);
    s.push('T');
    push_n(&mut s, secs / 3600, 2);
    s.push(':');
    push_n(&mut s, secs / 60 % 60, 2);
    s.push(':');
    push_n(&mut s, secs % 60, 2);
    s.push('Z');
    s
}

/// Parses `YYYY-MM-DDTHH:MM:SS[.fff](Z|+hh:mm)` to unix seconds.
pub fn parse_rfc3339(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || (b[10] != b'T' && b[10] != b' ') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let y: i64 = s[0..4].parse().ok()?;
    let mo: u32 = s[5..7].parse().ok()?;
    let d: u32 = s[8..10].parse().ok()?;
    let h: i64 = s[11..13].parse().ok()?;
    let mi: i64 = s[14..16].parse().ok()?;
    let se: i64 = s[17..19].parse().ok()?;
    let mut rest = &s[19..];
    if let Some(r) = rest.strip_prefix('.') {
        let n = r.find(|c: char| !c.is_ascii_digit()).unwrap_or(r.len());
        rest = &r[n..];
    }
    let off = if rest == "Z" || rest == "z" {
        0
    } else if rest.len() == 6 && (rest.starts_with('+') || rest.starts_with('-')) {
        let oh: i64 = rest[1..3].parse().ok()?;
        let om: i64 = rest[4..6].parse().ok()?;
        let v = oh * 3600 + om * 60;
        if rest.starts_with('-') { -v } else { v }
    } else {
        return None;
    };
    Some(http::util::days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + se - off)
}

static UID_SEQ: AtomicU64 = AtomicU64::new(0);

fn base36(mut n: u64) -> String {
    const D: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".to_string();
    }
    let mut v = Vec::new();
    while n > 0 {
        v.push(D[(n % 36) as usize]);
        n /= 36;
    }
    v.reverse();
    String::from_utf8(v).unwrap_or_default()
}

/// Grafana's `util.GenerateShortUID`: a hex letter, a base36 snowflake id, a hex letter (about 14 characters).
pub fn short_uid() -> String {
    let mut r = [0u8; 3];
    crypto::random_bytes(&mut r);
    const HEXL: &[u8; 6] = b"abcdef";
    let ms = (now_ms() - 1288834974657).max(0) as u64;
    let node = (r[2] as u64 & 0x3) << 8 | r[1] as u64; // 10 bits
    let seq = UID_SEQ.fetch_add(1, Ordering::Relaxed) & 0xfff;
    let id = (ms << 22) | (node << 12) | seq;
    let mut s = String::with_capacity(16);
    s.push(HEXL[(r[0] % 6) as usize] as char);
    s.push_str(&base36(id));
    s.push(HEXL[((r[0] / 6) % 6) as usize] as char);
    s
}

/// Grafana's uid rule: 1 to 40 characters of `a-zA-Z0-9-_`.
pub fn valid_uid(uid: &str) -> bool {
    !uid.is_empty() && uid.len() <= 40 && uid.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

/// A URL slug of a title, as Grafana's slugify: lower case, runs of other characters become one '-'.
pub fn slugify(title: &str) -> String {
    let mut s = String::with_capacity(title.len());
    let mut dash = false;
    for c in title.chars() {
        if c.is_alphanumeric() {
            for l in c.to_lowercase() {
                s.push(l);
            }
            dash = false;
        } else if !s.is_empty() && !dash {
            s.push('-');
            dash = true;
        }
    }
    while s.ends_with('-') {
        s.pop();
    }
    s
}

/// HTML-escapes text for element content and attribute values.
pub fn html_escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&#34;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
}
