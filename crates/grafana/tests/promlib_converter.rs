//! Port of promlib `converter/prom_test.go`: one test per upstream case, same inputs, same golden outputs.
mod common;

use common::{check_golden, read_bytes};
use grafana::promlib::converter::{read_prometheus_style_result, time_from_float, time_from_loki_string};
use grafana::promlib::frame::{DataResponse, Severity, Values};
use json::Json;

/// `runScenario`.
fn run_scenario(name: &str) {
    let b = read_bytes(&format!("converter/{name}.json"));
    let rsp = read_prometheus_style_result(&b);
    if name.contains("error") {
        assert!(rsp.error.is_some(), "{name}: expected an error");
        return;
    }
    let has = |sev: Severity| rsp.frames.as_ref().map(|fs| fs.iter().any(|f| f.meta.as_ref().map(|m| m.notices.iter().any(|n| n.severity == sev)).unwrap_or(false))).unwrap_or(false);
    if name.contains("warnings") {
        assert!(has(Severity::Warning), "{name}: expected a warning notice");
    }
    if name.contains("infos") {
        assert!(has(Severity::Info), "{name}: expected an info notice");
    }
    assert!(rsp.error.is_none(), "{name}: {:?}", rsp.error);
    check_golden(&format!("converter/{name}-frame.jsonc"), &rsp);
}

// TestReadPromFrames
#[test]
fn read_prom_frames_prom_labels() {
    run_scenario("prom-labels");
}
#[test]
fn read_prom_frames_prom_matrix() {
    run_scenario("prom-matrix");
}
#[test]
fn read_prom_frames_prom_matrix_with_nans() {
    run_scenario("prom-matrix-with-nans");
}
#[test]
fn read_prom_frames_prom_matrix_histogram_no_labels() {
    run_scenario("prom-matrix-histogram-no-labels");
}
#[test]
fn read_prom_frames_prom_matrix_histogram_partitioned() {
    run_scenario("prom-matrix-histogram-partitioned");
}
#[test]
fn read_prom_frames_prom_vector_histogram_no_labels() {
    run_scenario("prom-vector-histogram-no-labels");
}
#[test]
fn read_prom_frames_prom_vector() {
    run_scenario("prom-vector");
}
#[test]
fn read_prom_frames_prom_string() {
    run_scenario("prom-string");
}
#[test]
fn read_prom_frames_prom_scalar() {
    run_scenario("prom-scalar");
}
#[test]
fn read_prom_frames_prom_series() {
    run_scenario("prom-series");
}
#[test]
fn read_prom_frames_prom_warnings() {
    run_scenario("prom-warnings");
}
#[test]
fn read_prom_frames_prom_warnings_no_data() {
    run_scenario("prom-warnings-no-data");
}
#[test]
fn read_prom_frames_prom_infos() {
    run_scenario("prom-infos");
}
#[test]
fn read_prom_frames_prom_infos_no_data() {
    run_scenario("prom-infos-no-data");
}
#[test]
fn read_prom_frames_prom_error() {
    run_scenario("prom-error");
}
#[test]
fn read_prom_frames_prom_exemplars_a() {
    run_scenario("prom-exemplars-a");
}
#[test]
fn read_prom_frames_prom_exemplars_b() {
    run_scenario("prom-exemplars-b");
}
#[test]
fn read_prom_frames_prom_exemplars_diff_labels() {
    run_scenario("prom-exemplars-diff-labels");
}
#[test]
fn read_prom_frames_prom_query_range() {
    run_scenario("prom-query-range");
}
#[test]
fn read_prom_frames_prom_query_range_big() {
    run_scenario("prom-query-range-big");
}
#[test]
fn read_prom_frames_loki_streams_a() {
    run_scenario("loki-streams-a");
}
#[test]
fn read_prom_frames_loki_streams_b() {
    run_scenario("loki-streams-b");
}
#[test]
fn read_prom_frames_loki_streams_c() {
    run_scenario("loki-streams-c");
}

// TestReadPrometheusQueryStats
fn read_ok(payload: &str) -> DataResponse {
    let rsp = read_prometheus_style_result(payload.as_bytes());
    assert!(rsp.error.is_none(), "{:?}", rsp.error);
    rsp
}

const INSTANT_RESULT: &str = r#"[{"metric":{"__name__":"up","instance":"localhost"},"value":[1710000000,"1"]}]"#;
const PER_STEP: &str = "[[1710000000,12],[1710000060,18]]";

fn check_stats_case(data: &str) {
    let rsp = read_ok(&format!(r#"{{"status":"success","data":{data}}}"#));
    let frames = rsp.frames.as_ref().expect("frames");
    assert_eq!(frames.len(), 1);
    let meta = frames[0].meta.as_ref().expect("meta");
    assert_eq!(meta.stats.len(), 1);
    assert_eq!(meta.stats[0].display_name, "Total queryable samples");
    assert_eq!(meta.stats[0].unit, "");
    assert_eq!(meta.stats[0].value, 30.0);
    let custom = meta.custom.as_ref().expect("custom");
    assert_eq!(custom.str_of("resultType"), "vector");
    let samples = custom.path("stats.samples").expect("samples");
    assert_eq!(samples.get("totalQueryableSamples").and_then(|v| v.as_f64()), Some(30.0));
    assert_eq!(samples.get("totalQueryableSamplesPerStep").and_then(|v| v.as_arr()).map(|a| a.len()), Some(2));
}

#[test]
fn query_stats_stats_after_result() {
    check_stats_case(&format!(r#"{{"resultType":"vector","result":{INSTANT_RESULT},"stats":{{"samples":{{"totalQueryableSamples":30,"totalQueryableSamplesPerStep":{PER_STEP}}}}}}}"#));
}

#[test]
fn query_stats_stats_before_result() {
    check_stats_case(&format!(r#"{{"stats":{{"samples":{{"totalQueryableSamples":30,"totalQueryableSamplesPerStep":{PER_STEP}}}}},"resultType":"vector","result":{INSTANT_RESULT}}}"#));
}

#[test]
fn query_stats_stats_and_result_before_result_type() {
    check_stats_case(&format!(r#"{{"stats":{{"samples":{{"totalQueryableSamples":30,"totalQueryableSamplesPerStep":{PER_STEP}}}}},"result":{INSTANT_RESULT},"resultType":"vector"}}"#));
}

#[test]
fn query_stats_zero_samples_is_retained_as_a_typed_stat() {
    let rsp = read_ok(r#"{"status":"success","data":{"resultType":"vector","result":[],"stats":{"samples":{"totalQueryableSamples":0}}}}"#);
    let frames = rsp.frames.as_ref().expect("frames");
    assert_eq!(frames.len(), 1);
    let meta = frames[0].meta.as_ref().expect("meta");
    assert_eq!(meta.stats.len(), 1);
    assert_eq!(meta.stats[0].value, 0.0);
}

#[test]
fn query_stats_range_stats_are_attached_to_the_first_frame_only() {
    let rsp = read_ok(r#"{"status":"success","data":{"resultType":"matrix","result":[{"metric":{"job":"one"},"values":[[1710000000,"1"]]},{"metric":{"job":"two"},"values":[[1710000000,"2"]]}],"stats":{"samples":{"totalQueryableSamples":2}}}}"#);
    let frames = rsp.frames.as_ref().expect("frames");
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].meta.as_ref().map(|m| m.stats.len()), Some(1));
    assert!(frames[1].meta.as_ref().map(|m| m.stats.is_empty()).unwrap_or(true));
}

#[test]
fn query_stats_empty_result_creates_a_metadata_only_frame() {
    let rsp = read_ok(r#"{"status":"success","data":{"resultType":"matrix","result":[],"stats":{"samples":{"totalQueryableSamples":7}}}}"#);
    let frames = rsp.frames.as_ref().expect("frames");
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].name, "Query statistics");
    assert!(frames[0].fields.is_empty());
    assert_eq!(frames[0].meta.as_ref().map(|m| m.stats[0].value), Some(7.0));
}

#[test]
fn query_stats_malformed_stats_do_not_fail_a_valid_query() {
    let rsp = read_ok(&format!(r#"{{"status":"success","data":{{"resultType":"vector","result":{INSTANT_RESULT},"stats":"not-an-object"}}}}"#));
    let frames = rsp.frames.as_ref().expect("frames");
    assert_eq!(frames.len(), 1);
    let meta = frames[0].meta.as_ref().expect("meta");
    assert!(meta.stats.is_empty());
    assert_eq!(meta.custom.as_ref().and_then(|c| c.get("stats")).and_then(|s| s.as_str()), Some("not-an-object"));
}

#[test]
fn query_stats_malformed_total_omits_only_the_typed_stat() {
    let rsp = read_ok(&format!(r#"{{"status":"success","data":{{"resultType":"vector","result":{INSTANT_RESULT},"stats":{{"samples":{{"totalQueryableSamples":"unknown","totalQueryableSamplesPerStep":{PER_STEP}}}}}}}}}"#));
    let meta = rsp.frames.as_ref().expect("frames")[0].meta.as_ref().expect("meta");
    assert!(meta.stats.is_empty());
    assert!(meta.custom.as_ref().and_then(|c| c.get("stats")).map(|s| !s.is_null()).unwrap_or(false));
}

#[test]
fn query_stats_missing_stats_leaves_existing_metadata_unchanged() {
    let rsp = read_ok(&format!(r#"{{"status":"success","data":{{"resultType":"vector","result":{INSTANT_RESULT}}}}}"#));
    let meta = rsp.frames.as_ref().expect("frames")[0].meta.as_ref().expect("meta");
    assert!(meta.stats.is_empty());
    assert_eq!(meta.custom, Some(Json::obj().with("resultType", "vector")));
}

// TestTimeConversions
#[test]
fn time_conversions() {
    // 2020-09-14 15:22:25.479 UTC, millisecond precision
    assert_eq!(time_from_float(1600096945.479), 1600096945479 * 1_000_000);
    // 2022-02-16 16:50:46.277587968 UTC
    assert_eq!(time_from_loki_string("1645030246277587968"), Ok(1645030246277587968));
    // 2033-05-18 03:33:20 UTC
    assert_eq!(time_from_loki_string("2000000000000000000"), Ok(2000000000 * 1_000_000_000));
}

// TestReadPrometheusNumericStrings
fn assert_numeric_string_values(v: &Values) {
    match v {
        Values::F64(x) => {
            assert_eq!(x.len(), 5);
            assert_eq!(x[0], 1.25);
            assert_eq!(x[1], 1.25e10);
            assert!(x[2].is_nan());
            assert_eq!(x[3], f64::INFINITY);
            assert_eq!(x[4], f64::NEG_INFINITY);
        }
        other => panic!("not float64: {other:?}"),
    }
}

#[test]
fn numeric_strings_matrix() {
    let rsp = read_ok(r#"{"status":"success","data":{"resultType":"matrix","result":[{"metric":{},"values":[[1,"1.25"],[2,"1.25e10"],[3,"NaN"],[4,"+Inf"],[5,"-Inf"]]}]}}"#);
    let frames = rsp.frames.as_ref().expect("frames");
    assert_eq!(frames.len(), 1);
    assert_numeric_string_values(&frames[0].fields[1].values);
}

#[test]
fn numeric_strings_exemplars() {
    let rsp = read_ok(r#"{"status":"success","data":[{"seriesLabels":{},"exemplars":[{"value":"1.25","timestamp":1},{"value":"1.25e10","timestamp":2},{"value":"NaN","timestamp":3},{"value":"+Inf","timestamp":4},{"value":"-Inf","timestamp":5}]}]}"#);
    let frames = rsp.frames.as_ref().expect("frames");
    assert_eq!(frames.len(), 1);
    assert_numeric_string_values(&frames[0].fields[1].values);
}
