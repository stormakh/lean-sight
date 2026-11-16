//! Prometheus (and Loki) JSON responses to data frames. Port of promlib `converter/prom.go`
//! (`ReadPrometheusStyleResult`). Samples stream from the bytes into columns; no JSON tree is built.
use super::frame::{DataResponse, Field, Frame, Labels, Meta, Notice, QueryStat, SOURCE_DOWNSTREAM, Severity, Values, labels_set};
use super::jsonpull::{Iter, Kind, R};
use json::Json;

fn rsp_err(e: String) -> DataResponse {
    DataResponse::err(e, SOURCE_DOWNSTREAM)
}

/// Go `fmt.Sprintf("%v", v)` for a decoded JSON value.
pub fn go_fmt_v(v: &Json) -> String {
    match v {
        Json::Null => "<nil>".to_string(),
        Json::Bool(b) => b.to_string(),
        Json::Str(s) => s.clone(),
        Json::Int(i) => go_fmt_g(*i as f64),
        Json::Float(f) => go_fmt_g(*f),
        Json::Arr(a) => {
            let parts: Vec<String> = a.iter().map(go_fmt_v).collect();
            format!("[{}]", parts.join(" "))
        }
        Json::Obj(o) => {
            let mut idx: Vec<usize> = (0..o.len()).collect();
            json::sort::sort_by(&mut idx, |a, b| o[*a].0.cmp(&o[*b].0));
            let parts: Vec<String> = idx.iter().map(|i| format!("{}:{}", o[*i].0, go_fmt_v(&o[*i].1))).collect();
            format!("map[{}]", parts.join(" "))
        }
    }
}

/// Go `%v` of a float64: `strconv.FormatFloat(f, 'g', -1, 64)` (exponent when exp < -4 or exp >= 21).
pub fn go_fmt_g(f: f64) -> String {
    if f.is_nan() {
        return "NaN".to_string();
    }
    if f.is_infinite() {
        return if f > 0.0 { "+Inf".to_string() } else { "-Inf".to_string() };
    }
    if f == 0.0 {
        return if f.is_sign_negative() { "-0".to_string() } else { "0".to_string() };
    }
    let e = format!("{:e}", f); // shortest digits: "1.5e3"
    let (mant, exp) = match e.split_once('e') {
        Some((m, x)) => (m.to_string(), x.parse::<i32>().unwrap_or(0)),
        None => (e.clone(), 0),
    };
    if exp < -4 || exp >= 21 {
        let sign = if exp < 0 { '-' } else { '+' };
        let a = exp.unsigned_abs();
        if a < 10 { format!("{mant}e{sign}0{a}") } else { format!("{mant}e{sign}{a}") }
    } else {
        format!("{}", f)
    }
}

/// `timeFromFloat`: seconds (float) to Unix nanoseconds, truncated to milliseconds as Go does.
#[inline]
pub fn time_from_float(fv: f64) -> i64 {
    ((fv * 1000.0) as i64).wrapping_mul(1_000_000)
}

/// `timeFromLokiString`: a nanosecond timestamp string.
pub fn time_from_loki_string(s: &str) -> R<i64> {
    let n = s.len();
    if n < 19 || (n == 19 && s.as_bytes()[0] == b'1') {
        if let Ok(ns) = s.parse::<i64>() {
            return Ok(ns);
        }
    }
    if n < 10 {
        return Err(format!("unexpected time format '{s}' in response. response may have been truncated"));
    }
    let ss: i64 = s[0..10].parse().unwrap_or(0);
    let ns: i64 = s[10..].parse().unwrap_or(0);
    Ok(ss.wrapping_mul(1_000_000_000).wrapping_add(ns))
}

fn custom_result_type(rt: &str) -> Option<Json> {
    Some(Json::obj().with("resultType", rt))
}

/// `ReadPrometheusStyleResult`.
pub fn read_prometheus_style_result(b: &[u8]) -> DataResponse {
    let mut it = Iter::new(b);
    let mut rsp = DataResponse::default();
    let mut status = String::from("unknown");
    let mut error_type = String::new();
    let mut prom_err = String::new();
    let mut warnings: Vec<Notice> = Vec::new();
    let mut infos: Vec<Notice> = Vec::new();
    loop {
        let key = match it.read_object() {
            Ok(Some(k)) => k,
            Ok(None) => break,
            Err(e) => return rsp_err(format!("response from prometheus couldn't be parsed. it is non-json: {e}")),
        };
        match key.as_str() {
            "status" => match it.read_string() {
                Ok(s) => status = s,
                Err(e) => return rsp_err(e),
            },
            "data" => {
                rsp = read_prometheus_data(&mut it);
                if rsp.error.is_some() {
                    return rsp;
                }
            }
            "error" => match it.read_string() {
                Ok(s) => prom_err = s,
                Err(e) => return rsp_err(e),
            },
            "errorType" => match it.read_string() {
                Ok(s) => error_type = s,
                Err(e) => return rsp_err(e),
            },
            "warnings" => match read_annotations(&mut it, Severity::Warning) {
                Ok(w) => warnings = w,
                Err(e) => return rsp_err(e),
            },
            "infos" => match read_annotations(&mut it, Severity::Info) {
                Ok(w) => infos = w,
                Err(e) => return rsp_err(e),
            },
            _ => {
                if let Err(e) = it.skip() {
                    return DataResponse::err(e, "");
                }
            }
        }
    }
    if status == "error" {
        return DataResponse::err(format!("{error_type}: {prom_err}"), "");
    }
    warnings.extend(infos);
    if !warnings.is_empty() {
        if rsp.frame_count() == 0 {
            rsp.frames_mut().push(Frame::new("Warnings"));
        }
        for f in rsp.frames_mut().iter_mut() {
            f.meta_mut().notices = warnings.clone();
        }
    }
    rsp
}

fn read_annotations(it: &mut Iter<'_>, sev: Severity) -> R<Vec<Notice>> {
    let mut out = Vec::new();
    if it.what_is_next() != Kind::Arr {
        it.skip()?;
        return Ok(out);
    }
    while it.read_array()? {
        if it.what_is_next() == Kind::Str {
            out.push(Notice { severity: sev, text: it.read_string()? });
        } else {
            it.skip()?;
        }
    }
    Ok(out)
}

fn read_prometheus_data(it: &mut Iter<'_>) -> DataResponse {
    let t = it.what_is_next();
    if t == Kind::Arr {
        return read_array_data(it);
    }
    if t != Kind::Obj {
        return DataResponse::err("expected object type".to_string(), "");
    }
    let mut rsp = DataResponse::default();
    let mut result_type = String::new();
    let mut result_type_found = false;
    let mut result_bytes: Option<Vec<u8>> = None;
    let mut stats: Option<Json> = None;
    let mut encoding_flags: Vec<String> = Vec::new();
    loop {
        let key = match it.read_object() {
            Ok(Some(k)) => k,
            Ok(None) => break,
            Err(e) => return rsp_err(e),
        };
        match key.as_str() {
            "encodingFlags" => loop {
                match it.read_array() {
                    Ok(true) => match it.read_string() {
                        Ok(s) => encoding_flags.push(s),
                        Err(e) => return rsp_err(e),
                    },
                    Ok(false) => break,
                    Err(e) => return rsp_err(e),
                }
            },
            "resultType" => {
                match it.read_string() {
                    Ok(s) => result_type = s,
                    Err(e) => return rsp_err(e),
                }
                result_type_found = true;
                if let Some(rb) = result_bytes.take() {
                    if !rb.is_empty() {
                        let mut ji = Iter::new(&rb);
                        rsp = read_result(&result_type, &mut ji, &encoding_flags);
                    }
                }
            }
            "result" => {
                if result_type_found {
                    rsp = read_result(&result_type, it, &encoding_flags);
                } else {
                    match it.skip_and_return_bytes() {
                        Ok(b) => result_bytes = Some(b.to_vec()),
                        Err(_) => result_bytes = Some(Vec::new()),
                    }
                }
            }
            "stats" => match it.read_any() {
                Ok(v) => stats = Some(v),
                Err(e) => return rsp_err(e),
            },
            _ => {
                if let Err(e) = it.skip() {
                    return rsp_err(e);
                }
            }
        }
    }
    if !result_type_found {
        return rsp_err("no resultType found".to_string());
    }
    if let Some(s) = stats {
        attach_query_stats(&mut rsp, s);
    }
    rsp
}

fn attach_query_stats(rsp: &mut DataResponse, stats: Json) {
    if rsp.frame_count() == 0 {
        rsp.frames_mut().push(Frame::new("Query statistics"));
    }
    let total = stats.get("samples").and_then(|s| s.get("totalQueryableSamples")).and_then(|t| match t {
        Json::Float(f) => Some(*f),
        Json::Int(i) => Some(*i as f64),
        _ => None,
    });
    let frame = &mut rsp.frames_mut()[0];
    let meta = frame.meta_mut();
    meta.set_custom("stats", stats);
    if let Some(t) = total {
        meta.stats.push(QueryStat { display_name: "Total queryable samples".to_string(), unit: String::new(), value: t });
    }
}

fn read_result(result_type: &str, it: &mut Iter<'_>, flags: &[String]) -> DataResponse {
    match result_type {
        "matrix" | "vector" => read_matrix_or_vector_multi(it, result_type),
        "streams" => {
            if flags.iter().any(|f| f == "categorize-labels") {
                read_categorized_stream(it)
            } else {
                read_stream(it)
            }
        }
        "string" => read_string_result(it),
        "scalar" => read_scalar(it),
        _ => {
            if let Err(e) = it.skip() {
                return rsp_err(e);
            }
            DataResponse::err(format!("unknown result type: {result_type}"), "")
        }
    }
}

/// Strings, label sets (series) or exemplars.
fn read_array_data(it: &mut Iter<'_>) -> DataResponse {
    let mut rsp = DataResponse::default();
    let mut label_frame: Option<usize> = None; // index in frames
    let mut lookup: Vec<String> = Vec::new(); // field names of the label frame, by insertion
    let mut strings: Vec<String> = Vec::new();
    loop {
        match it.read_array() {
            Ok(true) => {}
            Ok(false) => break,
            Err(_) => break,
        }
        match it.what_is_next() {
            Kind::Str => match it.read_string() {
                Ok(s) => strings.push(s),
                Err(e) => return rsp_err(e),
            },
            Kind::Obj => {
                let (exemplar, pairs) = match read_labels_or_exemplars(it) {
                    Ok(v) => v,
                    Err(e) => return rsp_err(e),
                };
                if let Some(f) = exemplar {
                    rsp.frames_mut().push(f);
                } else if !pairs.is_empty() {
                    let mut max = 0usize;
                    for (k, v) in pairs.iter() {
                        let fi = match lookup.iter().position(|n| n == k) {
                            Some(i) => i,
                            None => {
                                lookup.push(k.clone());
                                if label_frame.is_none() {
                                    rsp.frames_mut().push(Frame::new(""));
                                    label_frame = Some(rsp.frame_count() - 1);
                                }
                                let lf = label_frame.unwrap_or(0);
                                rsp.frames_mut()[lf].fields.push(Field::new(k, Values::Str(Vec::new())));
                                lookup.len() - 1
                            }
                        };
                        let lf = label_frame.unwrap_or(0);
                        let field = &mut rsp.frames_mut()[lf].fields[fi];
                        if let Values::Str(s) = &mut field.values {
                            s.push(v.clone());
                        }
                        if field.len() > max {
                            max = field.len();
                        }
                    }
                    if let Some(lf) = label_frame {
                        for f in rsp.frames_mut()[lf].fields.iter_mut() {
                            let l = f.len();
                            if max > l {
                                f.values.extend_zero(max - l);
                            }
                        }
                    }
                }
            }
            _ => match it.read_any() {
                Ok(v) => strings.push(go_fmt_v(&v)),
                Err(e) => return rsp_err(e),
            },
        }
    }
    if !strings.is_empty() {
        rsp.frames_mut().push(Frame::with_fields("", vec![Field::new("Value", Values::Str(strings))]));
    }
    rsp
}

fn read_labels_as_pairs(it: &mut Iter<'_>) -> R<Vec<(String, String)>> {
    let mut pairs = Vec::new();
    while let Some(k) = it.read_object()? {
        let v = it.read_string()?;
        pairs.push((k, v));
    }
    Ok(pairs)
}

fn read_labels_or_exemplars(it: &mut Iter<'_>) -> R<(Option<Frame>, Vec<(String, String)>)> {
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut labels: Labels = Vec::new();
    let mut frame: Option<Frame> = None;
    while let Some(key) = it.read_object()? {
        match key.as_str() {
            "seriesLabels" => {
                it.read_labels(&mut labels)?;
                if let Some(f) = &mut frame {
                    f.fields[1].labels = Some(labels.clone());
                }
            }
            "exemplars" => {
                let mut times: Vec<i64> = Vec::new();
                let mut values: Vec<f64> = Vec::new();
                let mut extra: Vec<Field> = Vec::new();
                let mut notices: Vec<Notice> = Vec::new();
                let mut ex_count = 0usize;
                while it.read_array()? {
                    while let Some(k2) = it.read_object()? {
                        match k2.as_str() {
                            "value" => values.push(it.read_f64_string()?),
                            "timestamp" => times.push(time_from_float(it.read_f64()?)),
                            "labels" => {
                                let p = read_labels_as_pairs(it)?;
                                for (k, v) in p {
                                    match extra.iter().position(|f| f.name == k) {
                                        Some(i) => {
                                            if let Values::Str(s) = &mut extra[i].values {
                                                s.push(v);
                                            }
                                        }
                                        None => {
                                            let mut col = vec![String::new(); ex_count];
                                            col.push(v);
                                            extra.push(Field::new(&k, Values::Str(col)));
                                        }
                                    }
                                }
                                for f in extra.iter_mut() {
                                    let l = f.len();
                                    if ex_count + 1 > l {
                                        f.values.extend_zero(ex_count + 1 - l);
                                    }
                                }
                            }
                            _ => {
                                it.skip()?;
                                notices.push(Notice { severity: Severity::Error, text: format!("unable to parse key: {k2} in response body") });
                            }
                        }
                    }
                    ex_count += 1;
                }
                let mut tf = Field::new("Time", Values::Time(times));
                tf.labels = None;
                let mut vf = Field::new("Value", Values::F64(values));
                vf.labels = Some(labels.clone());
                let mut fields = vec![tf, vf];
                fields.extend(extra);
                let mut f = Frame::with_fields("", fields);
                let m = f.meta_mut();
                m.custom = custom_result_type("exemplar");
                m.notices = notices;
                frame = Some(f);
            }
            _ => {
                let v = it.read_any()?;
                pairs.push((key, go_fmt_v(&v)));
            }
        }
    }
    Ok((frame, pairs))
}

fn read_string_result(it: &mut Iter<'_>) -> DataResponse {
    let r = (|| -> R<(i64, String)> {
        it.read_array()?;
        let t = it.read_f64()?;
        it.read_array()?;
        let v = it.read_string()?;
        it.read_array()?;
        Ok((time_from_float(t), v))
    })();
    let (t, v) = match r {
        Ok(x) => x,
        Err(e) => return rsp_err(e),
    };
    let tf = Field::new("Time", Values::Time(vec![t]));
    let mut vf = Field::new("Value", Values::Str(vec![v]));
    vf.labels = Some(Vec::new());
    let mut f = Frame::with_fields("", vec![tf, vf]);
    let m = f.meta_mut();
    m.typ = "timeseries-multi".to_string();
    m.custom = custom_result_type("string");
    DataResponse { frames: Some(vec![f]), ..Default::default() }
}

fn read_time_value_pair(it: &mut Iter<'_>) -> R<(i64, f64)> {
    it.read_array()?;
    let t = it.read_time()?;
    it.read_array()?;
    let v = it.read_f64_string()?;
    it.read_array()?;
    Ok((t, v))
}

fn read_scalar(it: &mut Iter<'_>) -> DataResponse {
    let (t, v) = match read_time_value_pair(it) {
        Ok(x) => x,
        Err(e) => return DataResponse::err(e, ""),
    };
    let tf = Field::new("Time", Values::Time(vec![t]));
    let mut vf = Field::new("Value", Values::F64(vec![v]));
    vf.labels = Some(Vec::new());
    let mut f = Frame::with_fields("", vec![tf, vf]);
    let m = f.meta_mut();
    m.typ = "numeric-multi".to_string();
    m.custom = custom_result_type("scalar");
    m.type_version = [0, 1];
    DataResponse { frames: Some(vec![f]), ..Default::default() }
}

struct Hist {
    times: Vec<i64>,
    y_mins: Vec<f64>,
    y_maxes: Vec<f64>,
    counts: Vec<f64>,
    y_layouts: Vec<i8>,
}
impl Hist {
    fn new(cap: usize) -> Hist {
        Hist { times: Vec::with_capacity(cap), y_mins: Vec::with_capacity(cap), y_maxes: Vec::with_capacity(cap), counts: Vec::with_capacity(cap), y_layouts: Vec::with_capacity(cap) }
    }
}

fn read_matrix_or_vector_multi(it: &mut Iter<'_>, result_type: &str) -> DataResponse {
    match read_matrix_inner(it, result_type) {
        Ok(r) => r,
        Err(e) => rsp_err(e),
    }
}

fn read_matrix_inner(it: &mut Iter<'_>, result_type: &str) -> R<DataResponse> {
    let mut rsp = DataResponse::default();
    let mut size = 0usize;
    let mut prev_hist = usize::MAX;
    let mut hist_hint = 0usize;
    while it.read_array()? {
        let mut times: Vec<i64> = Vec::with_capacity(size);
        let mut values: Vec<f64> = Vec::with_capacity(size);
        let mut labels: Option<Labels> = None;
        let mut hist: Option<Hist> = None;
        while let Some(k) = it.read_object()? {
            match k.as_str() {
                "metric" => {
                    let mut l = Vec::new();
                    it.read_labels(&mut l)?;
                    labels = Some(l);
                }
                "value" => {
                    let (t, v) = read_time_value_pair(it)?;
                    times.push(t);
                    values.push(v);
                }
                "values" => {
                    while it.read_array()? {
                        let (t, v) = read_time_value_pair(it)?;
                        times.push(t);
                        values.push(v);
                    }
                }
                "histogram" => {
                    if hist.is_none() {
                        hist = Some(Hist::new(hist_hint));
                    }
                    if let Some(h) = &mut hist {
                        read_histogram(it, h)?;
                    }
                }
                "histograms" => {
                    if hist.is_none() {
                        hist = Some(Hist::new(hist_hint));
                    }
                    while it.read_array()? {
                        if let Some(h) = &mut hist {
                            read_histogram(it, h)?;
                        }
                    }
                }
                _ => it.skip()?,
            }
        }
        if let Some(h) = hist {
            let n = h.times.len();
            let mk = |name: &str, v: Values| Field { name: name.to_string(), labels: labels.clone(), config: None, values: v };
            let fields = vec![mk("xMax", Values::Time(h.times)), mk("yMin", Values::F64(h.y_mins)), mk("yMax", Values::F64(h.y_maxes)), mk("count", Values::F64(h.counts)), mk("yLayout", Values::I8(h.y_layouts))];
            let mut f = Frame::with_fields("", fields);
            f.meta_mut().typ = "heatmap-cells".to_string();
            rsp.frames_mut().push(f);
            hist_hint = if n == prev_hist { n } else { 0 };
            prev_hist = n;
        } else {
            size = times.len();
            let tf = Field::new("Time", Values::Time(times));
            let vf = Field { name: "Value".to_string(), labels, config: None, values: Values::F64(values) };
            let mut f = Frame::with_fields("", vec![tf, vf]);
            let m = f.meta_mut();
            m.typ = if result_type == "vector" { "numeric-multi".to_string() } else { "timeseries-multi".to_string() };
            m.custom = custom_result_type(result_type);
            m.type_version = [0, 1];
            rsp.frames_mut().push(f);
        }
    }
    Ok(rsp)
}

/// One sparse histogram: `[time, {count, sum, buckets: [[layout, "min", "max", "count"], ...]}]`.
fn read_histogram(it: &mut Iter<'_>, h: &mut Hist) -> R<()> {
    it.read_array()?;
    let t = time_from_float(it.read_f64()?);
    it.read_array()?;
    while let Some(k) = it.read_object()? {
        match k.as_str() {
            "buckets" => {
                while it.read_array()? {
                    h.times.push(t);
                    it.read_array()?;
                    h.y_layouts.push(it.read_i8()?);
                    it.read_array()?;
                    h.y_mins.push(it.read_f64_string()?);
                    it.read_array()?;
                    h.y_maxes.push(it.read_f64_string()?);
                    it.read_array()?;
                    h.counts.push(it.read_f64_string()?);
                    if it.read_array()? {
                        return Err("expected close array".to_string());
                    }
                }
            }
            _ => it.skip()?,
        }
    }
    if it.read_array()? {
        return Err("expected to be done".to_string());
    }
    Ok(())
}

/// `labelsToRawJson`: labels as compact JSON with sorted keys.
fn labels_json(l: &Labels) -> String {
    let mut s = String::from("{");
    for (i, (k, v)) in l.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        json::write_str(k, &mut s);
        s.push(':');
        json::write_str(v, &mut s);
    }
    s.push('}');
    s
}

fn read_stream(it: &mut Iter<'_>) -> DataResponse {
    let mut lab: Vec<String> = Vec::new();
    let mut times: Vec<i64> = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    let mut tss: Vec<String> = Vec::new();
    let mut label_json = "{}".to_string();
    let r = (|| -> R<()> {
        while it.read_array()? {
            while let Some(k) = it.read_object()? {
                match k.as_str() {
                    "stream" => {
                        let mut l = Vec::new();
                        it.read_labels(&mut l)?;
                        label_json = labels_json(&l);
                    }
                    "values" => {
                        while it.read_array()? {
                            it.read_array()?;
                            let ts = it.read_string()?;
                            it.read_array()?;
                            let line = it.read_string()?;
                            it.read_array()?;
                            let t = time_from_loki_string(&ts)?;
                            lab.push(label_json.clone());
                            times.push(t);
                            lines.push(line);
                            tss.push(ts);
                        }
                    }
                    _ => it.skip()?,
                }
            }
        }
        Ok(())
    })();
    if let Err(e) = r {
        return rsp_err(e);
    }
    let fields = vec![Field::new("__labels", Values::Json(lab)), Field::new("Time", Values::Time(times)), Field::new("Line", Values::Str(lines)), Field::new("TS", Values::Str(tss))];
    let mut f = Frame::with_fields("", fields);
    f.meta = Some(Meta::default());
    DataResponse { frames: Some(vec![f]), ..Default::default() }
}

fn read_categorized_stream(it: &mut Iter<'_>) -> DataResponse {
    let mut lab: Vec<String> = Vec::new();
    let mut lab_types: Vec<String> = Vec::new();
    let mut times: Vec<i64> = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    let mut tss: Vec<String> = Vec::new();
    let mut indexed: Labels = Vec::new();
    let r = (|| -> R<()> {
        while it.read_array()? {
            while let Some(k) = it.read_object()? {
                match k.as_str() {
                    "stream" => {
                        indexed = Vec::new();
                        it.read_labels(&mut indexed)?;
                    }
                    "values" => {
                        while it.read_array()? {
                            it.read_array()?;
                            let ts = it.read_string()?;
                            it.read_array()?;
                            let line = it.read_string()?;
                            it.read_array()?;
                            let mut parsed: Labels = Vec::new();
                            let mut structured: Labels = Vec::new();
                            while let Some(sk) = it.read_object()? {
                                match sk.as_str() {
                                    "parsed" => it.read_labels(&mut parsed)?,
                                    "structuredMetadata" => it.read_labels(&mut structured)?,
                                    _ => it.skip()?,
                                }
                            }
                            it.read_array()?;
                            let t = time_from_loki_string(&ts)?;
                            let mut types: Labels = Vec::new();
                            let mut all: Labels = Vec::new();
                            for (k, v) in indexed.iter() {
                                labels_set(&mut types, k, "I");
                                labels_set(&mut all, k, v);
                            }
                            for (k, v) in structured.iter() {
                                labels_set(&mut all, k, v);
                                labels_set(&mut types, k, "S");
                            }
                            for (k, v) in parsed.iter() {
                                labels_set(&mut all, k, v);
                                labels_set(&mut types, k, "P");
                            }
                            lab.push(labels_json(&all));
                            lab_types.push(labels_json(&types));
                            times.push(t);
                            lines.push(line);
                            tss.push(ts);
                        }
                    }
                    _ => it.skip()?,
                }
            }
        }
        Ok(())
    })();
    if let Err(e) = r {
        return rsp_err(e);
    }
    let fields = vec![
        Field::new("__labels", Values::Json(lab)),
        Field::new("Time", Values::Time(times)),
        Field::new("Line", Values::Str(lines)),
        Field::new("TS", Values::Str(tss)),
        Field::new("__labelTypes", Values::Json(lab_types)),
    ];
    let mut f = Frame::with_fields("", fields);
    f.meta = Some(Meta::default());
    DataResponse { frames: Some(vec![f]), ..Default::default() }
}
