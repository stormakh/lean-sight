//! Data frames as the Grafana plugin SDK (grafana-plugin-sdk-go v0.297.0 `data`, `backend`) models them, with the
//! same JSON encoding: `backend.DataResponse` (`writeDataResponseJSON`), `data.Frame` (`writeDataFrame`:
//! schema + data, NaN/Inf in `entities`, sub-millisecond times in `nanos`), `FrameMeta` and `FieldConfig` with
//! their omitempty rules, and maps (labels, `meta.custom`) with sorted keys as `ConfigCompatibleWithStandardLibrary`.
//! The JSON is written as text straight from the columns (no tree).
use json::Json;

/// Column values. Times are Unix nanoseconds (Go `time.Time`).
#[derive(Debug, Clone, PartialEq)]
pub enum Values {
    Time(Vec<i64>),
    F64(Vec<f64>),
    Str(Vec<String>),
    /// `json.RawMessage` (type "other"): each item is JSON text.
    Json(Vec<String>),
    I8(Vec<i8>),
}

impl Values {
    pub fn len(&self) -> usize {
        match self {
            Values::Time(v) => v.len(),
            Values::F64(v) => v.len(),
            Values::Str(v) => v.len(),
            Values::Json(v) => v.len(),
            Values::I8(v) => v.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Go `Field.Extend(n)`: appends n zero values.
    pub fn extend_zero(&mut self, n: usize) {
        for _ in 0..n {
            match self {
                Values::Time(v) => v.push(0),
                Values::F64(v) => v.push(0.0),
                Values::Str(v) => v.push(String::new()),
                Values::Json(v) => v.push(String::new()),
                Values::I8(v) => v.push(0),
            }
        }
    }
}

/// `data.FieldConfig`, only the members this plugin sets.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FieldConfig {
    pub display_name: String,
    pub display_name_from_ds: String,
    pub unit: String,
    pub interval: f64,
}

/// Labels: sorted by name, unique names (Go `data.Labels`, a map written with sorted keys).
pub type Labels = Vec<(String, String)>;

/// Sets `k` in sorted labels.
pub fn labels_set(l: &mut Labels, k: &str, v: &str) {
    match l.binary_search_by(|p| p.0.as_str().cmp(k)) {
        Ok(i) => l[i].1 = v.to_string(),
        Err(i) => l.insert(i, (k.to_string(), v.to_string())),
    }
}
pub fn labels_get<'a>(l: &'a Labels, k: &str) -> Option<&'a str> {
    match l.binary_search_by(|p| p.0.as_str().cmp(k)) {
        Ok(i) => Some(l[i].1.as_str()),
        Err(_) => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    /// None: no `labels` key; Some(empty): `"labels":{}`.
    pub labels: Option<Labels>,
    pub config: Option<FieldConfig>,
    pub values: Values,
}

impl Field {
    pub fn new(name: &str, values: Values) -> Field {
        Field { name: name.to_string(), labels: None, config: None, values }
    }
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub severity: Severity,
    pub text: String,
}

/// `data.QueryStat`: a FieldConfig (display name, unit) and a value.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryStat {
    pub display_name: String,
    pub unit: String,
    pub value: f64,
}

/// `data.FrameMeta`, the members this plugin sets.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meta {
    pub typ: String,
    pub type_version: [u32; 2],
    /// `custom`: a JSON object (written with sorted keys) or any JSON value; None omits it.
    pub custom: Option<Json>,
    pub stats: Vec<QueryStat>,
    pub notices: Vec<Notice>,
    pub executed_query_string: String,
}

impl Meta {
    /// Sets `custom[k] = v`, making `custom` an object first.
    pub fn set_custom(&mut self, k: &str, v: Json) {
        match &mut self.custom {
            Some(c) if c.as_obj().is_some() => c.set(k, v),
            _ => self.custom = Some(Json::obj().with(k, v)),
        }
    }
    pub fn custom_str(&self, k: &str) -> &str {
        match &self.custom {
            Some(c) => c.get(k).and_then(|v| v.as_str()).unwrap_or(""),
            None => "",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Frame {
    pub name: String,
    pub ref_id: String,
    pub meta: Option<Meta>,
    pub fields: Vec<Field>,
}

impl Frame {
    pub fn new(name: &str) -> Frame {
        Frame { name: name.to_string(), ref_id: String::new(), meta: None, fields: Vec::new() }
    }
    pub fn with_fields(name: &str, fields: Vec<Field>) -> Frame {
        Frame { name: name.to_string(), ref_id: String::new(), meta: None, fields }
    }
    /// The frame meta, created empty when absent.
    pub fn meta_mut(&mut self) -> &mut Meta {
        if self.meta.is_none() {
            self.meta = Some(Meta::default());
        }
        match &mut self.meta {
            Some(m) => m,
            None => unreachable!(),
        }
    }
    pub fn row_len(&self) -> usize {
        match self.fields.first() {
            Some(f) => f.len(),
            None => 0,
        }
    }
}

/// Where an error comes from (`backend.ErrorSource`).
pub const SOURCE_DOWNSTREAM: &str = "downstream";
pub const SOURCE_PLUGIN: &str = "plugin";

/// `backend.DataResponse`. `frames: None` is a nil slice (no `frames` key).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DataResponse {
    pub frames: Option<Vec<Frame>>,
    pub error: Option<String>,
    pub error_source: String,
    /// HTTP-like status; 0 is unset.
    pub status: u16,
}

impl DataResponse {
    pub fn err(msg: String, source: &str) -> DataResponse {
        DataResponse { frames: None, error: Some(msg), error_source: source.to_string(), status: 0 }
    }
    pub fn frames_mut(&mut self) -> &mut Vec<Frame> {
        if self.frames.is_none() {
            self.frames = Some(Vec::new());
        }
        match &mut self.frames {
            Some(f) => f,
            None => unreachable!(),
        }
    }
    pub fn frame_count(&self) -> usize {
        match &self.frames {
            Some(f) => f.len(),
            None => 0,
        }
    }

    /// The response JSON as `writeDataResponseJSON` writes it.
    pub fn write_json(&self, out: &mut String) {
        out.push('{');
        let mut started = false;
        let mut status = self.status as i64;
        if let Some(e) = &self.error {
            out.push_str("\"error\":");
            json::write_str(e, out);
            if !(100..600).contains(&status) {
                status = 500; // statusFromError: StatusUnknown
            }
            out.push_str(",\"errorSource\":");
            json::write_str(&self.error_source, out);
            started = true;
        }
        if (100..600).contains(&status) || status == 0 {
            if started {
                out.push(',');
            }
            out.push_str("\"status\":");
            json::write_i64(if status == 0 { 200 } else { status }, out);
            started = true;
        }
        if let Some(frames) = &self.frames {
            if started {
                out.push(',');
            }
            out.push_str("\"frames\":[");
            for (i, f) in frames.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_frame(f, out);
            }
            out.push(']');
        }
        out.push('}');
    }

    pub fn to_json_string(&self) -> String {
        let mut s = String::new();
        self.write_json(&mut s);
        s
    }
}

/// Go's float64 JSON text (jsoniter `WriteFloat64` == encoding/json): shortest round-trip digits, 'e' notation
/// below 1e-6 and from 1e21, exponent without leading zero. The caller handles NaN and infinities.
/// ECMAScript `Number::toString` (ryu-js) gives the same text for every finite value except -0.
pub fn write_go_f64(f: f64, out: &mut String) {
    let a = f.abs();
    if a < 1e15 && f == f.trunc() {
        if f == 0.0 && f.is_sign_negative() {
            out.push_str("-0");
        } else {
            push_i64(f as i64, out);
        }
    } else {
        let mut b = ryu_js::Buffer::new();
        out.push_str(b.format(f));
    }
}

/// Appends `n` in decimal (no allocation).
pub fn push_i64(n: i64, out: &mut String) {
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
        out.push('-');
    }
    out.push_str(std::str::from_utf8(&buf[i..]).unwrap_or(""));
}

fn write_labels(l: &Labels, out: &mut String) {
    out.push('{');
    for (i, (k, v)) in l.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        json::write_str(k, out);
        out.push(':');
        json::write_str(v, out);
    }
    out.push('}');
}

/// `FieldConfig` with omitempty, in struct order (displayName, displayNameFromDS, ..., unit, ..., interval).
fn write_field_config(c: &FieldConfig, out: &mut String, trailing_value: Option<f64>) {
    out.push('{');
    let mut started = false;
    let mut key = |out: &mut String, k: &str| {
        if started {
            out.push(',');
        }
        started = true;
        out.push('"');
        out.push_str(k);
        out.push_str("\":");
    };
    if !c.display_name.is_empty() {
        key(out, "displayName");
        json::write_str(&c.display_name, out);
    }
    if !c.display_name_from_ds.is_empty() {
        key(out, "displayNameFromDS");
        json::write_str(&c.display_name_from_ds, out);
    }
    if !c.unit.is_empty() {
        key(out, "unit");
        json::write_str(&c.unit, out);
    }
    if c.interval != 0.0 {
        key(out, "interval");
        write_go_f64(c.interval, out);
    }
    if let Some(v) = trailing_value {
        key(out, "value");
        write_go_f64(v, out);
    }
    out.push('}');
}

/// A JSON value as Go writes a decoded `any` with sorted map keys (ConfigCompatibleWithStandardLibrary).
pub fn write_sorted(v: &Json, out: &mut String) {
    match v {
        Json::Obj(o) => {
            let mut idx: Vec<usize> = (0..o.len()).collect();
            idx.sort_by(|a, b| o[*a].0.cmp(&o[*b].0));
            out.push('{');
            for (n, i) in idx.iter().enumerate() {
                if n > 0 {
                    out.push(',');
                }
                json::write_str(&o[*i].0, out);
                out.push(':');
                write_sorted(&o[*i].1, out);
            }
            out.push('}');
        }
        Json::Arr(a) => {
            out.push('[');
            for (n, x) in a.iter().enumerate() {
                if n > 0 {
                    out.push(',');
                }
                write_sorted(x, out);
            }
            out.push(']');
        }
        Json::Float(f) => {
            if f.is_finite() {
                write_go_f64(*f, out)
            } else {
                out.push_str("null")
            }
        }
        _ => v.write(out),
    }
}

fn write_meta(m: &Meta, out: &mut String) {
    out.push('{');
    if !m.typ.is_empty() {
        out.push_str("\"type\":");
        json::write_str(&m.typ, out);
        out.push(',');
    }
    out.push_str("\"typeVersion\":[");
    json::write_i64(m.type_version[0] as i64, out);
    out.push(',');
    json::write_i64(m.type_version[1] as i64, out);
    out.push(']');
    if let Some(c) = &m.custom {
        if !c.is_null() {
            out.push_str(",\"custom\":");
            write_sorted(c, out);
        }
    }
    if !m.stats.is_empty() {
        out.push_str(",\"stats\":[");
        for (i, s) in m.stats.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let c = FieldConfig { display_name: s.display_name.clone(), display_name_from_ds: String::new(), unit: s.unit.clone(), interval: 0.0 };
            write_field_config(&c, out, Some(s.value));
        }
        out.push(']');
    }
    if !m.notices.is_empty() {
        out.push_str(",\"notices\":[");
        for (i, n) in m.notices.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("{\"severity\":");
            out.push_str(match n.severity {
                Severity::Info => "\"info\"",
                Severity::Warning => "\"warning\"",
                Severity::Error => "\"error\"",
            });
            out.push_str(",\"text\":");
            json::write_str(&n.text, out);
            out.push('}');
        }
        out.push(']');
    }
    if !m.executed_query_string.is_empty() {
        out.push_str(",\"executedQueryString\":");
        json::write_str(&m.executed_query_string, out);
    }
    out.push('}');
}

fn type_strings(v: &Values) -> (&'static str, &'static str) {
    match v {
        Values::Time(_) => ("time", "time.Time"),
        Values::F64(_) => ("number", "float64"),
        Values::Str(_) => ("string", "string"),
        Values::Json(_) => ("other", "json.RawMessage"),
        Values::I8(_) => ("number", "int8"),
    }
}

/// One frame as `writeDataFrame(frame, stream, true, true)`.
pub fn write_frame(f: &Frame, out: &mut String) {
    out.push_str("{\"schema\":{");
    let mut started = false;
    if !f.name.is_empty() {
        out.push_str("\"name\":");
        json::write_str(&f.name, out);
        started = true;
    }
    if !f.ref_id.is_empty() {
        if started {
            out.push(',');
        }
        out.push_str("\"refId\":");
        json::write_str(&f.ref_id, out);
        started = true;
    }
    if let Some(m) = &f.meta {
        if started {
            out.push(',');
        }
        out.push_str("\"meta\":");
        write_meta(m, out);
        started = true;
    }
    if started {
        out.push(',');
    }
    out.push_str("\"fields\":[");
    for (i, fd) in f.fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('{');
        if !fd.name.is_empty() {
            out.push_str("\"name\":");
            json::write_str(&fd.name, out);
            out.push(',');
        }
        let (t, ft) = type_strings(&fd.values);
        out.push_str("\"type\":\"");
        out.push_str(t);
        out.push_str("\",\"typeInfo\":{\"frame\":\"");
        out.push_str(ft);
        out.push_str("\"}");
        if let Some(l) = &fd.labels {
            out.push_str(",\"labels\":");
            write_labels(l, out);
        }
        if let Some(c) = &fd.config {
            out.push_str(",\"config\":");
            write_field_config(c, out, None);
        }
        out.push('}');
    }
    out.push_str("]},\"data\":");
    write_frame_data(f, out);
    out.push('}');
}

fn write_index_list(name: &str, v: &[usize], first: &mut bool, out: &mut String) {
    if v.is_empty() {
        return;
    }
    if !*first {
        out.push(',');
    }
    *first = false;
    out.push('"');
    out.push_str(name);
    out.push_str("\":[");
    for (i, n) in v.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        json::write_i64(*n as i64, out);
    }
    out.push(']');
}

/// `writeDataFrameData`: `{"values":[[...],...]}` plus `entities` (NaN/Inf positions) and `nanos`.
fn write_frame_data(f: &Frame, out: &mut String) {
    let rows = f.row_len();
    out.push_str("{\"values\":[");
    // per field: (NaN, Inf, NegInf) indexes; nanos
    let mut entities: Vec<Option<[Vec<usize>; 3]>> = Vec::new();
    let mut nanos: Vec<Option<Vec<i64>>> = Vec::new();
    let mut any_entity = false;
    let mut any_nanos = false;
    for (fi, fd) in f.fields.iter().enumerate() {
        if fi > 0 {
            out.push(',');
        }
        out.push('[');
        let mut ent: Option<[Vec<usize>; 3]> = None;
        let mut ns: Option<Vec<i64>> = None;
        match &fd.values {
            Values::Time(v) => {
                for i in 0..rows {
                    if i > 0 {
                        out.push(',');
                    }
                    let t = v.get(i).copied().unwrap_or(0);
                    push_i64(t.div_euclid(1_000_000), out);
                    let r = t.rem_euclid(1_000_000);
                    if r != 0 {
                        let n = ns.get_or_insert_with(|| vec![0; rows]);
                        n[i] = r;
                    }
                }
            }
            Values::F64(v) => {
                for i in 0..rows {
                    if i > 0 {
                        out.push(',');
                    }
                    let x = v.get(i).copied().unwrap_or(0.0);
                    if x.is_finite() {
                        write_go_f64(x, out);
                    } else {
                        let e = ent.get_or_insert_with(|| [Vec::new(), Vec::new(), Vec::new()]);
                        if x.is_nan() {
                            e[0].push(i);
                        } else if x > 0.0 {
                            e[1].push(i);
                        } else {
                            e[2].push(i);
                        }
                        out.push_str("null");
                    }
                }
            }
            Values::Str(v) => {
                for i in 0..rows {
                    if i > 0 {
                        out.push(',');
                    }
                    json::write_str(v.get(i).map(|s| s.as_str()).unwrap_or(""), out);
                }
            }
            Values::Json(v) => {
                for i in 0..rows {
                    if i > 0 {
                        out.push(',');
                    }
                    let s = v.get(i).map(|s| s.as_str()).unwrap_or("");
                    out.push_str(if s.is_empty() { "null" } else { s });
                }
            }
            Values::I8(v) => {
                for i in 0..rows {
                    if i > 0 {
                        out.push(',');
                    }
                    json::write_i64(v.get(i).copied().unwrap_or(0) as i64, out);
                }
            }
        }
        out.push(']');
        if ent.is_some() {
            any_entity = true;
        }
        if ns.is_some() {
            any_nanos = true;
        }
        entities.push(ent);
        nanos.push(ns);
    }
    out.push(']');
    if any_entity {
        out.push_str(",\"entities\":[");
        for (i, e) in entities.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            match e {
                None => out.push_str("null"),
                Some(e) => {
                    out.push('{');
                    let mut first = true;
                    write_index_list("NaN", &e[0], &mut first, out);
                    write_index_list("Inf", &e[1], &mut first, out);
                    write_index_list("NegInf", &e[2], &mut first, out);
                    out.push('}');
                }
            }
        }
        out.push(']');
    }
    if any_nanos {
        out.push_str(",\"nanos\":[");
        for (i, n) in nanos.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            match n {
                None => out.push_str("null"),
                Some(n) => {
                    out.push('[');
                    for (j, x) in n.iter().enumerate() {
                        if j > 0 {
                            out.push(',');
                        }
                        json::write_i64(*x, out);
                    }
                    out.push(']');
                }
            }
        }
        out.push(']');
    }
    out.push('}');
}
