//! Query execution and response post-processing: promlib `querydata/request.go` and `querydata/response.go`.
use super::client::{HttpResp, Instance, Transport, TransportError};
use super::converter::read_prometheus_style_result;
use super::exemplar::{Exemplar, Framer, StdDevSampler};
use super::frame::{DataResponse, Field, FieldConfig, Frame, QueryStat, SOURCE_DOWNSTREAM, SOURCE_PLUGIN, Values, labels_get};
use super::models::{self, DataQuery, MS, Query};
use json::Json;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum QueryType {
    Range,
    Instant,
    Exemplar,
}

/// `QueryData.Execute`: one response per refId, in request order (the JSON writer sorts by refId).
pub fn execute(inst: &Instance, queries: &[DataQuery], t: Transport<'_>, from_alert: bool) -> Vec<(String, DataResponse)> {
    let mut out: Vec<(String, DataResponse)> = Vec::with_capacity(queries.len());
    for q in queries {
        let r = handle_query(inst, q, t, from_alert);
        match out.iter().position(|p| p.0 == q.ref_id) {
            Some(i) => out[i].1 = r,
            None => out.push((q.ref_id.clone(), r)),
        }
    }
    out
}

fn handle_query(inst: &Instance, bq: &DataQuery, t: Transport<'_>, from_alert: bool) -> DataResponse {
    let q = match models::parse(bq, &inst.opts.time_interval, from_alert) {
        Ok(q) => q,
        Err(e) => {
            let src = if e.contains("not supported by grafana-rs") { SOURCE_PLUGIN } else { SOURCE_DOWNSTREAM };
            return DataResponse::err(e, src);
        }
    };
    fetch(inst, &q, t)
}

/// `fetch`: instant, range and exemplar requests of one query (run one after the other; Go runs them at once
/// and appends in completion order, which is this order for a single Prometheus).
pub fn fetch(inst: &Instance, q: &Query, t: Transport<'_>) -> DataResponse {
    let mut dr = DataResponse { frames: Some(Vec::new()), ..Default::default() };
    if q.instant_query {
        let res = run(inst, q, t, QueryType::Instant);
        add_data_response(res, &mut dr);
    }
    if q.range_query {
        let res = run(inst, q, t, QueryType::Range);
        add_data_response(res, &mut dr);
    }
    if q.exemplar_query {
        let res = run(inst, q, t, QueryType::Exemplar);
        // an exemplar error is only logged
        if let Some(f) = res.frames {
            dr.frames_mut().extend(f);
        }
    }
    dr
}

fn add_data_response(res: DataResponse, dr: &mut DataResponse) {
    if let Some(e) = res.error {
        dr.error = Some(match dr.error.take() {
            None => e,
            Some(prev) => format!("{prev} {e}"),
        });
        dr.error_source = source_from_http_status(res.status).to_string();
        dr.status = res.status;
    }
    if let Some(f) = res.frames {
        dr.frames_mut().extend(f);
    }
}

/// `status.SourceFromHTTPStatus`.
pub fn source_from_http_status(s: u16) -> &'static str {
    match s {
        405 | 406 | 412 | 413 | 431 | 414 | 417 | 426 | 416 | 501 => SOURCE_PLUGIN,
        _ => SOURCE_DOWNSTREAM,
    }
}

fn run(inst: &Instance, q: &Query, t: Transport<'_>, qt: QueryType) -> DataResponse {
    let req = match qt {
        QueryType::Range => inst.range_request(q),
        QueryType::Instant => inst.instant_request(q),
        QueryType::Exemplar => inst.exemplar_request(q),
    };
    let res = match t(&req) {
        Ok(r) => r,
        Err(e) => return transport_error(e, qt),
    };
    // health check fallback: a non-200 instant answer is an error with the status text
    if qt == QueryType::Instant && res.status != 200 && q.ref_id == "__healthcheck__" {
        return DataResponse { frames: None, error: Some(format!("{} {}", res.status, http::reason(res.status))), error_source: source_from_http_status(res.status).to_string(), status: 0 };
    }
    parse_response(q, &res, qt)
}

fn transport_error(e: TransportError, qt: QueryType) -> DataResponse {
    let mut r = DataResponse::err(e.message, SOURCE_DOWNSTREAM);
    if qt != QueryType::Exemplar {
        r.status = 502;
    }
    r
}

/// `parseResponse`.
pub fn parse_response(q: &Query, res: &HttpResp, qt: QueryType) -> DataResponse {
    let sc = res.status;
    if (200..300).contains(&sc) || sc == 400 || sc == 422 || sc == 503 {
        let mut r = read_prometheus_style_result(&res.body);
        r.status = sc;
        if r.frame_count() == 0 && !q.exemplar_query {
            r.frames_mut().push(Frame::new(""));
        }
        let stats = parse_query_stats(res, qt);
        if let Some(frames) = &mut r.frames {
            for (i, f) in frames.iter_mut().enumerate() {
                add_metadata_to_multi_frame(q, f);
                if i == 0 {
                    let m = f.meta_mut();
                    m.executed_query_string = executed_query_string(q);
                    match &m.custom {
                        None => m.custom = Some(Json::obj().with("calculatedMinStep", q.step / MS)),
                        Some(Json::Obj(_)) => m.set_custom("calculatedMinStep", Json::Int(q.step / MS)),
                        Some(_) => {}
                    }
                    m.stats.extend(stats.iter().cloned());
                }
            }
        }
        if r.error.is_none() {
            r = process_exemplars(r);
        }
        return r;
    }
    let n = res.body.len().min(1024);
    let tb = String::from_utf8_lossy(&res.body[..n]).into_owned();
    let mut f = Frame::new("");
    add_metadata_to_multi_frame(q, &mut f);
    f.meta_mut().executed_query_string = executed_query_string(q);
    DataResponse { frames: Some(vec![f]), error: Some(format!("unexpected response with status code {sc}: {tb}")), error_source: source_from_http_status(sc).to_string(), status: 0 }
}

/// Mimir `Server-Timing` stats (`parseQueryStats`).
pub fn parse_query_stats(res: &HttpResp, qt: QueryType) -> Vec<QueryStat> {
    let mut out: Vec<QueryStat> = Vec::new();
    let values = res.header_values("server-timing");
    if values.is_empty() {
        return out;
    }
    let prefix = match qt {
        QueryType::Range => "Range: ",
        QueryType::Instant => "Instant: ",
        QueryType::Exemplar => "Exemplar: ",
    };
    let mut seen: Vec<String> = Vec::new();
    for line in values {
        for entry in line.split(',') {
            let mut parts = entry.split(';');
            let name = parts.next().unwrap_or("").trim();
            let (disp, unit) = match name {
                "querier_wall_time" => ("Querier wall time", "ms"),
                "response_time" => ("Response time", "ms"),
                "bytes_processed" => ("Bytes processed", "decbytes"),
                "samples_processed" => ("Samples processed", "short"),
                "equivalent_samples_read" => ("Equivalent samples read", "short"),
                _ => continue,
            };
            if seen.iter().any(|s| s == name) {
                continue;
            }
            let mut value: Option<f64> = None;
            for p in parts {
                if let Some((k, v)) = p.trim().split_once('=') {
                    if k != "dur" && k != "val" {
                        continue;
                    }
                    if let Some(f) = super::jsonpull::parse_go_float(v) {
                        value = Some(f);
                        break;
                    }
                }
            }
            if let Some(v) = value {
                seen.push(name.to_string());
                out.push(QueryStat { display_name: format!("{prefix}{disp}"), unit: unit.to_string(), value: v });
            }
        }
    }
    out
}

fn is_exemplar_frame(f: &Frame) -> bool {
    match &f.meta {
        Some(m) => m.custom_str("resultType") == "exemplar",
        None => false,
    }
}

/// `processExemplars`: exemplar frames are sampled into one "exemplar" frame; others pass through. The status is
/// dropped, as upstream builds a new response.
pub fn process_exemplars(dr: DataResponse) -> DataResponse {
    let frames = match dr.frames {
        Some(f) => f,
        None => return DataResponse { frames: None, error: dr.error, ..Default::default() },
    };
    if !frames.iter().any(is_exemplar_frame) {
        return DataResponse { frames: Some(frames), error: dr.error, ..Default::default() };
    }
    let mut sampler = StdDevSampler::default();
    let mut framer = Framer::new();
    let mut meta_set = false;
    for f in frames {
        if !is_exemplar_frame(&f) {
            framer.add_frame(f);
            continue;
        }
        if !meta_set {
            framer.meta = f.meta.clone();
            framer.ref_id = f.ref_id.clone();
            meta_set = true;
        }
        let step = match f.fields.first().and_then(|x| x.config.as_ref()) {
            Some(c) => (c.interval as i64) * MS,
            None => 0,
        };
        super::exemplar::Sampler::set_step(&mut sampler, step);
        let series = f.fields.get(1).and_then(|x| x.labels.clone()).unwrap_or_default();
        framer.track_labels(&series);
        for x in f.fields.iter().skip(2) {
            framer.track_name(&x.name);
        }
        let src = framer.sources.len();
        if let (Some(Field { values: Values::Time(ts), .. }), Some(Field { values: Values::F64(vs), .. })) = (f.fields.first(), f.fields.get(1)) {
            for (row, t) in ts.iter().enumerate() {
                let ex = Exemplar { series_labels: series.clone(), source: src, row_idx: row, value: vs.get(row).copied().unwrap_or(0.0), timestamp: *t };
                super::exemplar::Sampler::add(&mut sampler, ex);
            }
        }
        framer.sources.push(f);
    }
    let frames = framer.frames(&mut sampler);
    DataResponse { frames: Some(frames), error: dr.error, ..Default::default() }
}

/// `addMetadataToMultiFrame`.
pub fn add_metadata_to_multi_frame(q: &Query, f: &mut Frame) {
    f.meta_mut();
    if f.fields.len() < 2 {
        return;
    }
    f.fields[0].config = Some(FieldConfig { interval: (q.step / MS) as f64, ..Default::default() });
    let name = get_name(q, &f.fields[1]);
    if !name.is_empty() {
        f.fields[1].config = Some(FieldConfig { display_name_from_ds: name, ..Default::default() });
    }
    let heatmap = match &f.meta {
        Some(m) => m.typ == "heatmap-cells",
        None => false,
    };
    if heatmap {
        return;
    }
    let vf = &mut f.fields[1];
    let n = match &vf.labels {
        Some(l) => labels_get(l, "__name__").map(|s| s.to_string()),
        None => None,
    };
    if let Some(n) = n {
        vf.name = n;
    }
}

/// `metricNameFromLabels`: `name{a="1", b="2"}` (labels sorted), `{}` when there is nothing.
fn metric_name_from_labels(f: &Field) -> String {
    let empty = Vec::new();
    let labels = f.labels.as_ref().unwrap_or(&empty);
    let name = labels_get(labels, "__name__");
    let n = if name.is_some() { labels.len() - 1 } else { labels.len() };
    if n == 0 {
        return match name {
            Some(m) => m.to_string(),
            None => "{}".to_string(),
        };
    }
    let mut s = String::new();
    s.push_str(name.unwrap_or(""));
    s.push('{');
    let mut first = true;
    for (k, v) in labels.iter() {
        if k == "__name__" {
            continue;
        }
        if !first {
            s.push_str(", ");
        }
        first = false;
        s.push_str(k);
        s.push('=');
        go_quote_q(v, &mut s);
    }
    s.push('}');
    s
}

/// Go `%q` (strconv.Quote) of a label value.
fn go_quote_q(v: &str, out: &mut String) {
    out.push('"');
    for c in v.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{7}' => out.push_str("\\a"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\u{b}' => out.push_str("\\v"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// `executedQueryString`.
pub fn executed_query_string(q: &Query) -> String {
    let mut s = String::with_capacity(q.expr.len() + 20);
    s.push_str("Expr: ");
    s.push_str(&q.expr);
    s.push_str("\nStep: ");
    s.push_str(&models::duration_string(q.step));
    s
}

/// The legend: `legendFormat` with `{{label}}` replaced, `__auto`, or the series name.
pub fn get_name(q: &Query, f: &Field) -> String {
    let mut legend = metric_name_from_labels(f);
    if q.legend_format == "__auto" {
        if f.labels.as_ref().map(|l| !l.is_empty()).unwrap_or(false) {
            legend = String::new();
        }
    } else if !q.legend_format.is_empty() {
        let empty = Vec::new();
        let labels = f.labels.as_ref().unwrap_or(&empty);
        legend = apply_legend_format(&q.legend_format, labels);
    }
    if legend == "{}" {
        return q.expr.clone();
    }
    legend
}

/// `legendFormatRegexp` (`\{\{\s*(.+?)\s*\}\}`) replacement.
fn apply_legend_format(fmt: &str, labels: &super::frame::Labels) -> String {
    let b = fmt.as_bytes();
    let mut out = String::with_capacity(fmt.len());
    let mut i = 0;
    let mut last = 0;
    while i + 1 < b.len() {
        if b[i] == b'{' && b[i + 1] == b'{' {
            // the shortest match: at least one character, then "}}", no newline in between
            let mut j = i + 3;
            let mut found: Option<usize> = None;
            while j + 1 < b.len() {
                if b[j] == b'}' && b[j + 1] == b'}' {
                    found = Some(j);
                    break;
                }
                j += 1;
            }
            if let Some(end) = found {
                if !fmt[i + 2..end].contains('\n') {
                    out.push_str(&fmt[last..i]);
                    let inner = fmt[i..end + 2].replacen("{{", "", 1).replacen("}}", "", 1);
                    let name = inner.trim();
                    if let Some(v) = labels_get(labels, name) {
                        out.push_str(v);
                    }
                    i = end + 2;
                    last = i;
                    continue;
                }
            }
        }
        i += 1;
    }
    out.push_str(&fmt[last..]);
    out
}
