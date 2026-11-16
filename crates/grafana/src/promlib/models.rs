//! Query model, step and variable interpolation, and the time helpers they need. Port of promlib
//! `models/query.go`, `intervalv2/intervalv2.go`, plugin SDK `backend/gtime` (`ParseInterval`, `FormatInterval`,
//! `RoundInterval`, `GetIntervalFrom`) and Go's `time.Duration` text forms. Durations and times are nanoseconds.
use json::Json;

pub const NS: i64 = 1;
pub const US: i64 = 1_000;
pub const MS: i64 = 1_000_000;
pub const SEC: i64 = 1_000_000_000;
pub const MIN: i64 = 60 * SEC;
pub const HOUR: i64 = 60 * MIN;
pub const DAY: i64 = 24 * HOUR;

/// `backend.DataQuery` as the plugin receives it.
#[derive(Debug, Clone, Default)]
pub struct DataQuery {
    pub ref_id: String,
    pub query_type: String,
    pub max_data_points: i64,
    /// `intervalMs` as a duration.
    pub interval: i64,
    /// Time range, Unix nanoseconds.
    pub from: i64,
    pub to: i64,
    /// The query JSON object.
    pub json: Json,
}

/// The internal query (`models.Query`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Query {
    pub expr: String,
    pub step: i64,
    pub legend_format: String,
    pub start: i64,
    pub end: i64,
    pub ref_id: String,
    pub instant_query: bool,
    pub range_query: bool,
    pub exemplar_query: bool,
    pub utc_offset_sec: i64,
}

impl Query {
    /// (start, end, step) aligned to the step (`Query.TimeRange`).
    pub fn time_range(&self) -> (i64, i64, i64) {
        (align_time_range(self.start, self.step, self.utc_offset_sec), align_time_range(self.end, self.step, self.utc_offset_sec), self.step)
    }
}

/// Response format (`PromQueryFormat`), with its lenient JSON decoding.
pub fn parse_format(v: Option<&Json>) -> &'static str {
    match v {
        Some(Json::Str(s)) => match s.as_str() {
            "time_series" => "time_series",
            "table" => "table",
            "heatmap" => "heatmap",
            _ => "time_series",
        },
        Some(Json::Int(n)) => match *n {
            2 => "table",
            3 => "heatmap",
            _ => "time_series",
        },
        _ => "time_series",
    }
}

// ---------------------------------------------------------------- Go durations

/// Go `time.Duration.String()`: "1h0m0s", "1m30s", "1.5s", "500ms", "0s".
pub fn duration_string(d: i64) -> String {
    if d == 0 {
        return "0s".to_string();
    }
    let neg = d < 0;
    let u = d.unsigned_abs();
    let mut s = String::new();
    if neg {
        s.push('-');
    }
    if u < SEC as u64 {
        // sub-second: ns, µs or ms with a fraction
        let (unit, div, prec) = if u < US as u64 {
            ("ns", 1u64, 0)
        } else if u < MS as u64 {
            ("µs", 1_000u64, 3)
        } else {
            ("ms", 1_000_000u64, 6)
        };
        s.push_str(&(u / div).to_string());
        push_frac(&mut s, u % div, prec);
        s.push_str(unit);
        return s;
    }
    let total_secs = u / SEC as u64;
    let frac = u % SEC as u64;
    let h = total_secs / 3600;
    let m = (total_secs / 60) % 60;
    let sec = total_secs % 60;
    if h > 0 {
        s.push_str(&h.to_string());
        s.push('h');
    }
    if h > 0 || m > 0 {
        s.push_str(&m.to_string());
        s.push('m');
    }
    s.push_str(&sec.to_string());
    push_frac(&mut s, frac, 9);
    s.push('s');
    s
}

fn push_frac(s: &mut String, mut v: u64, prec: usize) {
    if v == 0 || prec == 0 {
        return;
    }
    let mut digits = vec![b'0'; prec];
    for i in (0..prec).rev() {
        digits[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    while digits.last() == Some(&b'0') {
        digits.pop();
    }
    s.push('.');
    s.push_str(std::str::from_utf8(&digits).unwrap_or(""));
}

/// Go `time.ParseDuration`.
pub fn parse_go_duration(s: &str) -> Result<i64, String> {
    let orig = s;
    let bad = || format!("time: invalid duration {}", go_quote(orig));
    let mut b = s.as_bytes();
    let mut neg = false;
    if let Some(&c) = b.first() {
        if c == b'-' || c == b'+' {
            neg = c == b'-';
            b = &b[1..];
        }
    }
    if b == b"0" {
        return Ok(0);
    }
    if b.is_empty() {
        return Err(bad());
    }
    let mut total: f64 = 0.0;
    let mut total_i: i128 = 0;
    while !b.is_empty() {
        // number
        let mut i = 0;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        let int_part = &b[..i];
        let mut frac_part: &[u8] = &[];
        let mut rest = &b[i..];
        if rest.first() == Some(&b'.') {
            let mut j = 1;
            while j < rest.len() && rest[j].is_ascii_digit() {
                j += 1;
            }
            frac_part = &rest[1..j];
            rest = &rest[j..];
        }
        if int_part.is_empty() && frac_part.is_empty() {
            return Err(bad());
        }
        let mut k = 0;
        while k < rest.len() && rest[k] != b'.' && !rest[k].is_ascii_digit() {
            k += 1;
        }
        let unit = std::str::from_utf8(&rest[..k]).unwrap_or("");
        let mult: i64 = match unit {
            "ns" => NS,
            "us" | "µs" | "μs" => US,
            "ms" => MS,
            "s" => SEC,
            "m" => MIN,
            "h" => HOUR,
            "" => return Err(format!("time: missing unit in duration {}", go_quote(orig))),
            _ => return Err(format!("time: unknown unit {} in duration {}", go_quote(unit), go_quote(orig))),
        };
        let iv: i128 = std::str::from_utf8(int_part).ok().filter(|t| !t.is_empty()).and_then(|t| t.parse::<i128>().ok()).unwrap_or(0);
        total_i += iv * mult as i128;
        if !frac_part.is_empty() {
            let fv: f64 = format!("0.{}", std::str::from_utf8(frac_part).unwrap_or("0")).parse().unwrap_or(0.0);
            total += fv * mult as f64;
        }
        if total_i > i64::MAX as i128 {
            return Err(bad());
        }
        b = &rest[k..];
    }
    let v = total_i as i64 + total as i64;
    Ok(if neg { -v } else { v })
}

fn go_quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// gtime `parse`: a Go duration, or `<n>[dwMy]` returned as (n, unit).
fn gtime_parse(inp: &str) -> Result<(i64, char), String> {
    if inp.is_empty() {
        return Err("empty input".to_string());
    }
    let last = inp.as_bytes()[inp.len() - 1];
    if last != b'd' && last != b'w' && last != b'M' && last != b'y' {
        return parse_go_duration(inp).map(|d| (d, ' '));
    }
    let num = &inp[..inp.len() - 1];
    if num.bytes().all(|c| c.is_ascii_digit()) {
        return match num.parse::<i64>() {
            Ok(n) => Ok((n, last as char)),
            Err(_) => Err(format!("strconv.Atoi: parsing {}: invalid syntax", go_quote(num))),
        };
    }
    parse_go_duration(inp).map(|d| (d, ' '))
}

/// gtime `ParseDuration`: Go durations plus d, w, M (year/12), y (365.25 days).
pub fn gtime_parse_duration(inp: &str) -> Result<i64, String> {
    let (d, unit) = gtime_parse(inp)?;
    let year = (DAY as f64 * 365.25) as i64;
    Ok(match unit {
        ' ' => d,
        'd' => d * DAY,
        'w' => d * 7 * DAY,
        'M' => d * (year / 12),
        'y' => d * year,
        _ => return Err(format!("invalid duration {}", go_quote(inp))),
    })
}

/// gtime `ParseIntervalStringToTimeDuration`: `<10s>` brackets stripped, a bare number is seconds.
pub fn parse_interval_string(interval: &str) -> Result<i64, String> {
    if interval.is_empty() {
        return Err("invalid interval".to_string());
    }
    let mut s = interval;
    if let Some(r) = s.strip_prefix('<') {
        s = r;
    }
    if let Some(r) = s.strip_suffix('>') {
        s = r;
    }
    if s.bytes().all(|c| c.is_ascii_digit()) {
        return match s.parse::<i64>() {
            Ok(n) => Ok(n * SEC),
            Err(_) => Err(format!("strconv.ParseInt: parsing {}: invalid syntax", go_quote(s))),
        };
    }
    gtime_parse_duration(s)
}

/// gtime `FormatInterval`.
pub fn format_interval(d: i64) -> String {
    let year = 365 * DAY;
    if d >= year {
        format!("{}y", d / year)
    } else if d >= DAY {
        format!("{}d", d / DAY)
    } else if d >= HOUR {
        format!("{}h", d / HOUR)
    } else if d >= MIN {
        format!("{}m", d / MIN)
    } else if d >= SEC {
        format!("{}s", d / SEC)
    } else if d >= MS {
        format!("{}ms", d / MS)
    } else {
        "1ms".to_string()
    }
}

/// gtime `RoundInterval`.
pub fn round_interval(i: i64) -> i64 {
    let ms = |n: i64| n * MS;
    let table: [(i64, i64); 30] = [
        (10, 1),
        (15, 10),
        (35, 20),
        (75, 50),
        (150, 100),
        (350, 200),
        (750, 500),
        (1500, 1000),
        (3500, 2000),
        (7500, 5000),
        (12500, 10000),
        (17500, 15000),
        (25000, 20000),
        (45000, 30000),
        (90000, 60000),
        (210000, 120000),
        (450000, 300000),
        (750000, 600000),
        (1050000, 900000),
        (1500000, 1200000),
        (2700000, 1800000),
        (5400000, 3600000),
        (9000000, 7200000),
        (16200000, 10800000),
        (32400000, 21600000),
        (86400000, 43200000),
        (172800000, 86400000),
        (604800000, 86400000),
        (1814400000, 604800000),
        (0, 0),
    ];
    for (le, v) in table.iter() {
        if *le == 0 {
            break;
        }
        if i <= ms(*le) {
            return ms(*v);
        }
    }
    if i < ms(63072000000) { ms(2592000000) } else { ms(31536000000) }
}

/// gtime `GetIntervalFrom`.
pub fn get_interval_from(ds_interval: &str, query_interval: &str, query_interval_ms: i64, default: i64) -> Result<i64, String> {
    let mut interval = query_interval;
    if interval == "0s" {
        interval = "";
    }
    if interval.is_empty() && query_interval_ms != 0 {
        return Ok(query_interval_ms * MS);
    }
    if interval.is_empty() && !ds_interval.is_empty() {
        interval = ds_interval;
    }
    if interval.is_empty() {
        return Ok(default);
    }
    parse_interval_string(interval)
}

/// intervalv2 `Calculate`.
pub fn calculate_interval(from: i64, to: i64, min_interval: i64, max_data_points: i64) -> i64 {
    let resolution = if max_data_points == 0 { 1500 } else { max_data_points };
    let calculated = (to - from) / resolution;
    if calculated < min_interval {
        return min_interval;
    }
    round_interval(calculated)
}

/// intervalv2 `CalculateSafeInterval`.
pub fn calculate_safe_interval(from: i64, to: i64, safe_res: i64) -> i64 {
    round_interval((to - from) / safe_res)
}

// ---------------------------------------------------------------- query parsing

const SAFE_RESOLUTION: i64 = 11000;

fn is_rate_interval_variable(i: &str) -> bool {
    i == "$__rate_interval" || i == "${__rate_interval}" || i == "$__rate_interval_ms" || i == "${__rate_interval_ms}"
}

fn is_variable_interval(i: &str) -> bool {
    i == "$__interval" || i == "${__interval}" || i == "$__interval_ms" || i == "${__interval_ms}" || is_rate_interval_variable(i)
}

/// `CalculatePrometheusInterval`.
pub fn calculate_prometheus_interval(query_interval: &str, ds_scrape_interval: &str, interval_ms: i64, interval_factor: i64, q: &DataQuery) -> Result<i64, String> {
    let original = query_interval;
    let qi = if is_variable_interval(query_interval) { "" } else { query_interval };
    let min_interval = get_interval_from(ds_scrape_interval, qi, interval_ms, 15 * SEC)?;
    let calculated = calculate_interval(q.from, q.to, min_interval, q.max_data_points);
    let safe = calculate_safe_interval(q.from, q.to, SAFE_RESOLUTION);
    let adjusted = calculated.max(safe);
    if is_rate_interval_variable(original) {
        return Ok(calculate_rate_interval(adjusted, ds_scrape_interval));
    }
    let factor = if interval_factor == 0 { 1 } else { interval_factor };
    Ok(adjusted.wrapping_mul(factor))
}

/// `calculateRateInterval`.
pub fn calculate_rate_interval(query_interval: i64, requested_min_step: &str) -> i64 {
    let scrape = if requested_min_step.is_empty() { "15s" } else { requested_min_step };
    let d = match parse_interval_string(scrape) {
        Ok(d) => d,
        Err(_) => return 0,
    };
    (query_interval + d).max(4 * d)
}

/// `InterpolateVariables`.
pub fn interpolate_variables(expr: &str, query_interval: i64, calculated_step: i64, requested_min_step: &str, ds_scrape_interval: &str, time_range: i64) -> String {
    if !expr.contains('$') {
        return expr.to_string();
    }
    let range_ms = time_range / MS;
    let range_s_rounded = (range_ms as f64 / 1000.0).round() as i64;
    let rate_interval = if requested_min_step == "$__rate_interval" || requested_min_step == "${__rate_interval}" {
        calculated_step
    } else {
        let mut rms = requested_min_step.to_string();
        if rms == "$__interval" || rms == "${__interval}" {
            rms = duration_string(calculated_step);
        }
        if rms.is_empty() {
            rms = ds_scrape_interval.to_string();
        }
        calculate_rate_interval(query_interval, &rms)
    };
    let mut e = expr.to_string();
    let rep = |e: String, a: &str, b: &str, v: &str| -> String { e.replace(a, v).replace(b, v) };
    e = rep(e, "$__interval_ms", "${__interval_ms}", &(calculated_step / MS).to_string());
    e = rep(e, "$__interval", "${__interval}", &format_interval(calculated_step));
    e = rep(e, "$__range_ms", "${__range_ms}", &range_ms.to_string());
    e = rep(e, "$__range_s", "${__range_s}", &range_s_rounded.to_string());
    e = rep(e, "$__range", "${__range}", &format!("{range_s_rounded}s"));
    e = rep(e, "$__rate_interval_ms", "${__rate_interval_ms}", &(rate_interval / MS).to_string());
    e = rep(e, "$__rate_interval", "${__rate_interval}", &duration_string(rate_interval));
    e
}

/// `AlignTimeRange`: rounds down to a multiple of the step, with the UTC offset (float math as Go).
pub fn align_time_range(t: i64, step: i64, offset: i64) -> i64 {
    let offset_nano = (offset * 1_000_000_000) as f64;
    let step_nano = step as f64;
    (((t as f64 + offset_nano) / step_nano).floor() * step_nano - offset_nano) as i64
}

/// A member of the query object, matched as Go's encoding/json does (exact name, then case-insensitive).
pub fn get_ci<'a>(o: &'a Json, k: &str) -> Option<&'a Json> {
    let obj = o.as_obj()?;
    let mut found: Option<&'a Json> = None;
    for (key, v) in obj.iter() {
        if key == k {
            return Some(v);
        }
        if found.is_none() && key.eq_ignore_ascii_case(k) {
            found = Some(v);
        }
    }
    found
}

fn type_name(v: &Json) -> &'static str {
    match v {
        Json::Null => "null",
        Json::Bool(_) => "bool",
        Json::Int(_) | Json::Float(_) => "number",
        Json::Str(_) => "string",
        Json::Arr(_) => "array",
        Json::Obj(_) => "object",
    }
}

fn unmarshal_err(v: &Json, field: &str, go_type: &str) -> String {
    let what = match v {
        Json::Int(_) | Json::Float(_) => {
            let mut s = String::new();
            v.write(&mut s);
            format!("number {s}")
        }
        _ => type_name(v).to_string(),
    };
    format!("error unmarshaling query: json: cannot unmarshal {what} into Go struct field internalQueryModel.{field} of type {go_type}")
}

fn get_str(o: &Json, k: &str) -> Result<String, String> {
    match get_ci(o, k) {
        None | Some(Json::Null) => Ok(String::new()),
        Some(Json::Str(s)) => Ok(s.clone()),
        Some(v) => Err(unmarshal_err(v, k, "string")),
    }
}
fn get_bool(o: &Json, k: &str) -> Result<bool, String> {
    match get_ci(o, k) {
        None | Some(Json::Null) => Ok(false),
        Some(Json::Bool(b)) => Ok(*b),
        Some(v) => Err(unmarshal_err(v, k, "bool")),
    }
}
fn get_i64(o: &Json, k: &str) -> Result<i64, String> {
    match get_ci(o, k) {
        None | Some(Json::Null) => Ok(0),
        Some(Json::Int(i)) => Ok(*i),
        Some(v) => Err(unmarshal_err(v, k, "int64")),
    }
}
fn get_f64(o: &Json, k: &str) -> Result<f64, String> {
    match get_ci(o, k) {
        None | Some(Json::Null) => Ok(0.0),
        Some(Json::Int(i)) => Ok(*i as f64),
        Some(Json::Float(f)) => Ok(*f),
        Some(v) => Err(unmarshal_err(v, k, "float64")),
    }
}

/// `models.Parse`. Scope filters, ad hoc filters and group-by keys are not applied (they need a PromQL parser):
/// a query with any of them is answered with an error, never silently run unfiltered.
pub fn parse(q: &DataQuery, ds_scrape_interval: &str, from_alert: bool) -> Result<Query, String> {
    let m = &q.json;
    if m.as_obj().is_none() {
        return Err(format!("error unmarshaling query: json: cannot unmarshal {} into Go value of type models.internalQueryModel", type_name(m)));
    }
    let expr = get_str(m, "expr")?;
    let mut range = get_bool(m, "range")?;
    let instant = get_bool(m, "instant")?;
    let mut exemplar = get_bool(m, "exemplar")?;
    let interval_factor = get_i64(m, "intervalFactor")?;
    let legend_format = get_str(m, "legendFormat")?;
    let interval_ms = get_f64(m, "intervalMs")?;
    let utc_offset_sec = get_i64(m, "utcOffsetSec")?;
    let interval = get_str(m, "interval")?;
    let has = |k: &str| match get_ci(m, k) {
        Some(Json::Arr(a)) => !a.is_empty(),
        _ => false,
    };
    if has("scopes") || has("adhocFilters") || has("groupByKeys") {
        return Err("scope filters, ad hoc filters and group-by keys are not supported by grafana-rs".to_string());
    }
    let step = calculate_prometheus_interval(&interval, ds_scrape_interval, interval_ms as i64, interval_factor, q)?;
    let expr = interpolate_variables(&expr, q.interval, step, &interval, ds_scrape_interval, q.to - q.from);
    if !instant && !range {
        range = true;
    }
    if from_alert {
        exemplar = false;
    }
    Ok(Query { expr, step, legend_format, start: q.from, end: q.to, ref_id: q.ref_id.clone(), instant_query: instant, range_query: range, exemplar_query: exemplar, utc_offset_sec })
}

// ---------------------------------------------------------------- time ranges ("now-1h", epoch ms)

/// gtime `TimeRange` parsing of `from`/`to`: epoch milliseconds, a duration back from now ("1h"), or date math
/// ("now", "now-6h", "now/d", "now-1d/d"). `round_up` is set for `to`. Unparsable input gives the Unix epoch (0).
pub fn parse_time(s: &str, now_ns: i64, round_up: bool) -> i64 {
    if let Ok(ms) = s.parse::<i64>() {
        return ms.wrapping_mul(MS);
    }
    if let Ok(d) = gtime_parse_duration(s) {
        return now_ns - d;
    }
    match datemath(s, now_ns, round_up) {
        Some(t) => t,
        None => 0,
    }
}

fn unit_ns(u: u8) -> Option<i64> {
    match u {
        b's' => Some(SEC),
        b'm' => Some(MIN),
        b'h' => Some(HOUR),
        b'd' => Some(DAY),
        b'w' => Some(7 * DAY),
        _ => None,
    }
}

/// Date math in UTC: `now`, `+N<unit>` / `-N<unit>` steps, `/unit` rounding (s m h d w M y).
fn datemath(s: &str, now_ns: i64, round_up: bool) -> Option<i64> {
    let rest = s.strip_prefix("now")?;
    let mut t = now_ns;
    let b = rest.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let op = b[i];
        i += 1;
        if op == b'/' {
            let u = *b.get(i)?;
            i += 1;
            t = round_time(t, u, round_up)?;
            continue;
        }
        if op != b'+' && op != b'-' {
            return None;
        }
        let st = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        let n: i64 = if st == i { 1 } else { rest[st..i].parse().ok()? };
        let u = *b.get(i)?;
        i += 1;
        let sign = if op == b'-' { -1 } else { 1 };
        t = match u {
            b'M' | b'y' => add_months(t, sign * n * if u == b'y' { 12 } else { 1 }),
            _ => t + sign * n * unit_ns(u)?,
        };
    }
    Some(t)
}

fn add_months(t: i64, months: i64) -> i64 {
    let days = t.div_euclid(DAY);
    let rem = t.rem_euclid(DAY);
    let (y, m, d) = http::util::civil_from_days(days);
    let total = y * 12 + (m as i64 - 1) + months;
    let ny = total.div_euclid(12);
    let nm = (total.rem_euclid(12) + 1) as u32;
    http::util::days_from_civil(ny, nm, d) * DAY + rem
}

fn round_time(t: i64, u: u8, up: bool) -> Option<i64> {
    let days = t.div_euclid(DAY);
    let start = match u {
        b's' | b'm' | b'h' | b'd' => {
            let n = unit_ns(u)?;
            t.div_euclid(n) * n
        }
        b'w' => {
            // weeks start on Monday; 1970-01-01 was a Thursday
            let wd = (days + 3).rem_euclid(7);
            (days - wd) * DAY
        }
        b'M' => {
            let (y, m, _) = http::util::civil_from_days(days);
            http::util::days_from_civil(y, m, 1) * DAY
        }
        b'y' => {
            let (y, _, _) = http::util::civil_from_days(days);
            http::util::days_from_civil(y, 1, 1) * DAY
        }
        _ => return None,
    };
    if !up {
        return Some(start);
    }
    let next = match u {
        b'w' => start + 7 * DAY,
        b'M' => add_months(start, 1),
        b'y' => add_months(start, 12),
        _ => start + unit_ns(u)?,
    };
    Some(next - MS)
}
