//! Port of promlib `querydata/framing_test.go` (golden files in `testdata/`) and `querydata/response_test.go`.
mod common;

use common::{check_golden, read_bytes};
use grafana::promlib::client::{HttpReq, HttpResp, Instance, TransportError};
use grafana::promlib::frame::{DataResponse, Field, FieldConfig, Frame, Meta, Values};
use grafana::promlib::models::{DataQuery, Query, SEC};
use grafana::promlib::querydata::{QueryType, execute, parse_query_stats, parse_response, process_exemplars};
use grafana::promlib::settings::parse_prom_options;
use json::Json;

/// `setup()`: a data source at localhost:9090 with `timeInterval: 15s`.
fn instance() -> Instance {
    Instance { url: "http://localhost:9090".to_string(), opts: parse_prom_options(r#"{"timeInterval": "15s"}"#).expect("options"), timeout_secs: 30, ..Default::default() }
}

/// `loadStoredQuery` + `runQuery`: the stored query as a request, every Prometheus call answered with `response`.
fn golden_scenario(name: &str) {
    let sq = Json::parse(&String::from_utf8(read_bytes(&format!("querydata/{name}.query.json"))).expect("utf8")).expect("query json");
    let step = sq.i64_of("Step");
    let qm = Json::obj()
        .with("expr", sq.str_of("Expr"))
        .with("range", sq.bool_of("RangeQuery"))
        .with("exemplar", sq.bool_of("ExemplarQuery"))
        .with("legendFormat", sq.str_of("LegendFormat"))
        .with("intervalMs", step * 1000)
        .with("interval", format!("{step}s"));
    let q = DataQuery { ref_id: sq.str_of("RefId").to_string(), from: sq.i64_of("Start") * SEC, to: sq.i64_of("End") * SEC, interval: step * SEC, json: qm, ..Default::default() };
    let body = read_bytes(&format!("querydata/{name}.result.json"));
    let t = |_r: &HttpReq| -> Result<HttpResp, TransportError> { Ok(HttpResp { status: 200, headers: Vec::new(), body: body.clone() }) };
    let res = execute(&instance(), std::slice::from_ref(&q), &t, false);
    assert_eq!(res.len(), 1);
    let (ref_id, dr) = &res[0];
    assert_eq!(ref_id, "A");
    check_golden(&format!("querydata/{name}.result.golden.jsonc"), dr);
}

// TestRangeResponses
#[test]
fn range_parse_a_simple_matrix_response() {
    golden_scenario("range_simple");
}
#[test]
fn range_parse_a_simple_matrix_response_with_value_missing_steps() {
    golden_scenario("range_missing");
}
#[test]
fn range_parse_a_matrix_response_with_infinity() {
    golden_scenario("range_infinity");
}
#[test]
fn range_parse_a_matrix_response_with_nan() {
    golden_scenario("range_nan");
}
#[test]
fn range_parse_a_response_with_legend_format_auto() {
    golden_scenario("range_auto");
}

// TestExemplarResponses
#[test]
fn exemplar_parse_an_exemplar_response() {
    golden_scenario("exemplar");
}

// TestQueryData_parseResponse
const SERIES: &str = r#"[{"metric":{"__name__":"some_name","environment":"some_env","id":"some_id","instance":"some_instance:1234","job":"some_job","name":"another_name","region":"some_region"},"value":[1.1,"2"]}]"#;

fn parse(body: &str, status: u16) -> DataResponse {
    let res = HttpResp { status, headers: Vec::new(), body: body.as_bytes().to_vec() };
    parse_response(&Query::default(), &res, QueryType::Range)
}

#[test]
fn parse_response_result_type_before_result() {
    let r = parse(&format!(r#"{{"data":{{"resultType":"vector", "result":{SERIES}}},"status":"success"}}"#), 200);
    assert!(r.error.is_none());
    assert_eq!(r.frame_count(), 1);
}

#[test]
fn parse_response_result_type_after_result() {
    let r = parse(&format!(r#"{{"data":{{"result":{SERIES},"resultType":"vector"}},"status":"success"}}"#), 200);
    assert!(r.error.is_none());
    assert_eq!(r.frame_count(), 1);
}

#[test]
fn parse_response_no_result_type() {
    let r = parse(&format!(r#"{{"data":{{"result":{SERIES}}},"status":"success"}}"#), 200);
    assert_eq!(r.error.as_deref(), Some("no resultType found"));
}

#[test]
fn parse_response_empty_result_type_before_result() {
    let r = parse(&format!(r#"{{"data":{{"resultType":"", "result":{SERIES}}},"status":"success"}}"#), 200);
    assert_eq!(r.error.as_deref(), Some("unknown result type: "));
}

#[test]
fn parse_response_empty_result_type_after_result() {
    let r = parse(&format!(r#"{{"data":{{"result":{SERIES},"resultType":""}},"status":"success"}}"#), 200);
    assert_eq!(r.error.as_deref(), Some("unknown result type: "));
}

// TestAddMetadataToMultiFrame
#[test]
fn add_metadata_when_you_have_native_histogram_result() {
    let body = r#"{"status":"success","data":{"resultType":"matrix","result":[{"metric":{"__name__":"rpc_durations_native_histogram_seconds","instance":"nativehisto:8080","job":"prometheus"},"histograms":[[1729529685,{"count":"7243102","sum":"72460202.93145595","buckets":[[0,"1.8340080864093422","2","10"],[0,"2","2.1810154653305154","68"]]}],[1729529700,{"count":"7243490","sum":"72464056.03309634","buckets":[[0,"1.8340080864093422","2","10"],[0,"2","2.1810154653305154","68"]]}],[1729529715,{"count":"7243880","sum":"72467935.35871512","buckets":[[0,"1.8340080864093422","2","10"],[0,"2","2.1810154653305154","68"]]}]]}]}}"#;
    let r = parse(body, 200);
    assert!(r.error.is_none());
    assert_eq!(r.frame_count(), 1);
    assert_eq!(r.frames.as_ref().expect("frames")[0].fields[1].name, "yMin");
}

// TestParseQueryStats
fn stats(lines: &[&str], qt: QueryType) -> Vec<(String, String, f64)> {
    let headers = lines.iter().map(|l| ("server-timing".to_string(), l.to_string())).collect();
    let res = HttpResp { status: 200, headers, body: Vec::new() };
    parse_query_stats(&res, qt).into_iter().map(|s| (s.display_name, s.unit, s.value)).collect()
}
fn st(d: &str, u: &str, v: f64) -> (String, String, f64) {
    (d.to_string(), u.to_string(), v)
}

#[test]
fn query_stats_absent_header() {
    assert_eq!(stats(&[], QueryType::Range), vec![]);
}
#[test]
fn query_stats_all_known_mimir_metrics() {
    assert_eq!(
        stats(&["bytes_processed;val=11188007, equivalent_samples_read;val=20000, querier_wall_time;dur=5215.074756, response_time;dur=1061.890603, samples_processed;val=17647"], QueryType::Range),
        vec![
            st("Range: Bytes processed", "decbytes", 11188007.0),
            st("Range: Equivalent samples read", "short", 20000.0),
            st("Range: Querier wall time", "ms", 5215.074756),
            st("Range: Response time", "ms", 1061.890603),
            st("Range: Samples processed", "short", 17647.0),
        ]
    );
}
#[test]
fn query_stats_unknown_metrics_are_skipped() {
    assert_eq!(stats(&["future_metric;val=42, bytes_processed;val=11188007"], QueryType::Range), vec![st("Range: Bytes processed", "decbytes", 11188007.0)]);
}
#[test]
fn query_stats_malformed_entries_are_skipped() {
    assert_eq!(stats(&["querier_wall_time, bytes_processed;val=notanumber, response_time;dur=10"], QueryType::Range), vec![st("Range: Response time", "ms", 10.0)]);
}
#[test]
fn query_stats_instant_query_stats_are_labeled_instant() {
    assert_eq!(stats(&["response_time;dur=10"], QueryType::Instant), vec![st("Instant: Response time", "ms", 10.0)]);
}
#[test]
fn query_stats_exemplar_query_stats_are_labeled_exemplar() {
    assert_eq!(stats(&["response_time;dur=10"], QueryType::Exemplar), vec![st("Exemplar: Response time", "ms", 10.0)]);
}
#[test]
fn query_stats_multiple_header_lines_are_all_read() {
    assert_eq!(stats(&["querier_wall_time;dur=5215.074756", "bytes_processed;val=11188007"], QueryType::Range), vec![st("Range: Querier wall time", "ms", 5215.074756), st("Range: Bytes processed", "decbytes", 11188007.0)]);
}
#[test]
fn query_stats_a_second_entry_param_is_ignored() {
    assert_eq!(stats(&[r#"response_time;dur=10;desc="foo""#], QueryType::Range), vec![st("Range: Response time", "ms", 10.0)]);
}
#[test]
fn query_stats_a_stray_space_before_the_semicolon_still_matches() {
    assert_eq!(stats(&["bytes_processed ;val=5"], QueryType::Range), vec![st("Range: Bytes processed", "decbytes", 5.0)]);
}
#[test]
fn query_stats_a_metric_repeated_within_one_line_keeps_the_first() {
    assert_eq!(stats(&["bytes_processed;val=1, bytes_processed;val=2"], QueryType::Range), vec![st("Range: Bytes processed", "decbytes", 1.0)]);
}
#[test]
fn query_stats_a_metric_repeated_across_lines_keeps_the_first() {
    assert_eq!(stats(&["bytes_processed;val=1", "bytes_processed;val=2"], QueryType::Range), vec![st("Range: Bytes processed", "decbytes", 1.0)]);
}

// TestParseResponse_QueryStats
const UP: &str = r#"{"data":{"resultType":"vector","result":[{"metric":{"__name__":"up"},"value":[1.1,"2"]}]},"status":"success"}"#;

fn with_timing(header: Option<&str>, body: &str) -> Vec<(String, String, f64)> {
    let headers = match header {
        Some(h) => vec![("server-timing".to_string(), h.to_string())],
        None => Vec::new(),
    };
    let res = HttpResp { status: 200, headers, body: body.as_bytes().to_vec() };
    let r = parse_response(&Query::default(), &res, QueryType::Range);
    assert!(r.error.is_none());
    let frames = r.frames.expect("frames");
    assert_eq!(frames.len(), 1);
    frames[0].meta.as_ref().expect("meta").stats.iter().map(|s| (s.display_name.clone(), s.unit.clone(), s.value)).collect()
}

#[test]
fn parse_response_populates_frame_meta_stats_from_server_timing() {
    assert_eq!(with_timing(Some("equivalent_samples_read;val=20000"), UP), vec![st("Range: Equivalent samples read", "short", 20000.0)]);
}

#[test]
fn parse_response_leaves_stats_nil_when_header_absent() {
    assert_eq!(with_timing(None, UP), vec![]);
}

#[test]
fn parse_response_keeps_prometheus_stats_alongside_server_timing_stats() {
    let body = r#"{"data":{"resultType":"vector","result":[{"metric":{"__name__":"up"},"value":[1.1,"2"]}],"stats":{"samples":{"totalQueryableSamples":30}}},"status":"success"}"#;
    assert_eq!(with_timing(Some("equivalent_samples_read;val=20000"), body), vec![st("Total queryable samples", "", 30.0), st("Range: Equivalent samples read", "short", 20000.0)]);
}

// TestParseResponse_ErrorCases
fn error_case(status: u16, body: &str) {
    let r = parse(body, status);
    let e = r.error.as_deref().expect("error");
    assert!(e.contains("unexpected response"), "{e}");
    assert_eq!(r.frame_count(), 1);
}
#[test]
fn parse_response_error_500() {
    error_case(500, r#"{"error":"internal server error"}"#);
}
#[test]
fn parse_response_error_404() {
    error_case(404, r#"{"error":"not found"}"#);
}
#[test]
fn parse_response_error_401() {
    error_case(401, r#"{"error":"unauthorized"}"#);
}

// TestProcessExemplars
fn new_exemplar_frame(meta: Meta) -> Frame {
    let mut tf = Field::new("Time", Values::Time(vec![SEC]));
    tf.config = Some(FieldConfig { interval: 15000.0, ..Default::default() });
    let mut vf = Field::new("Value", Values::F64(vec![1.0]));
    vf.labels = Some(vec![("__name__".to_string(), "up".to_string())]);
    let mut f = Frame::with_fields("", vec![tf, vf]);
    f.ref_id = "A".to_string();
    f.meta = Some(meta);
    f
}

#[test]
fn process_exemplars_preserves_the_first_exemplar_frames_meta() {
    let custom = Some(Json::obj().with("resultType", "exemplar"));
    let f1 = new_exemplar_frame(Meta { executed_query_string: "Expr: up\nStep: 15s".to_string(), custom: custom.clone(), ..Default::default() });
    let f2 = new_exemplar_frame(Meta { custom, ..Default::default() });
    let r = process_exemplars(DataResponse { frames: Some(vec![f1, f2]), ..Default::default() });
    assert!(r.error.is_none());
    let frames = r.frames.expect("frames");
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].meta.as_ref().map(|m| m.executed_query_string.as_str()), Some("Expr: up\nStep: 15s"));
}
