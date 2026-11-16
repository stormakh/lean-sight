//! Port of promlib `models/query_test.go` (TestParse, TestRateInterval, TestAlignTimeRange,
//! TestParseNumericFormatError, TestPromQueryFormatUnmarshalJSON). The scope, ad hoc filter and group-by cases
//! are listed in tests-data/not-ported.md.
use grafana::promlib::models::{DataQuery, HOUR, MIN, MS, Query, SEC, align_time_range, parse, parse_format};
use json::Json;

fn now() -> i64 {
    grafana::util::now_ms() * MS + 123_456 // not aligned to anything, like time.Now()
}

/// `queryContext(json, timeRange, queryInterval)`.
fn query_context(json: &str, from: i64, to: i64, interval: i64) -> DataQuery {
    DataQuery { ref_id: "A".to_string(), interval, from, to, json: Json::parse(json).expect("query json"), ..Default::default() }
}

fn parse_ok(q: &DataQuery, scrape: &str) -> Query {
    parse(q, scrape, false).expect("parse")
}

// TestParse
#[test]
fn parse_parsing_query_from_unified_alerting() {
    let n = now();
    let q = query_context(r#"{"expr": "go_goroutines", "refId": "A", "exemplar": true}"#, n, n + 12 * HOUR, 0);
    let res = parse(&q, "15s", true).expect("parse");
    assert!(!res.exemplar_query);
}

#[test]
fn parse_parsing_query_model_with_step() {
    let n = now();
    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "refId": "A"}"#, n, n + 12 * HOUR, MIN);
    assert_eq!(parse_ok(&q, "15s").step, 30 * SEC);
}

#[test]
fn parse_parsing_query_model_without_step_parameter() {
    let n = now();
    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + HOUR, MIN);
    assert_eq!(parse_ok(&q, "15s").step, 15 * SEC);
}

#[test]
fn parse_parsing_query_model_with_high_interval_factor() {
    let n = now();
    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "intervalFactor": 10, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    assert_eq!(parse_ok(&q, "15s").step, 20 * MIN);
}

#[test]
fn parse_parsing_query_model_with_low_interval_factor() {
    let n = now();
    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    assert_eq!(parse_ok(&q, "15s").step, 2 * MIN);
}

#[test]
fn parse_parsing_query_model_specified_scrape_interval_in_the_data_source() {
    let n = now();
    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    assert_eq!(parse_ok(&q, "240s").step, 4 * MIN);
}

#[test]
fn parse_parsing_query_model_with_interval_variable() {
    let n = now();
    let mut q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__interval]})", "format": "time_series", "intervalFactor": 1, "intervalMs": 60000, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    q.max_data_points = 2110;
    let res = parse_ok(&q, "15s");
    assert_eq!(res.expr, "rate(ALERTS{job=\"test\" [1m]})");
    assert_eq!(res.step, 60 * SEC);
}

#[test]
fn parse_parsing_query_model_with_interval_ms_variable() {
    let n = now();
    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__interval_ms]})", "format": "time_series", "intervalFactor": 1, "intervalMs": 60000, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [120000]})");
}

#[test]
fn parse_parsing_query_model_with_interval_ms_and_interval_variable() {
    let n = now();
    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__interval_ms]}) + rate(ALERTS{job=\"test\" [$__interval]})", "format": "time_series", "intervalFactor": 1, "intervalMs": 60000, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [120000]}) + rate(ALERTS{job=\"test\" [2m]})");
}

#[test]
fn parse_parsing_query_model_with_braced_interval_ms_and_interval_variable() {
    let n = now();
    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [${__interval_ms}]}) + rate(ALERTS{job=\"test\" [${__interval}]})", "format": "time_series", "intervalFactor": 1, "intervalMs": 60000, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [120000]}) + rate(ALERTS{job=\"test\" [2m]})");
}

#[test]
fn parse_parsing_query_model_with_range_variable() {
    let n = now();
    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__range]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [172800s]})");
}

#[test]
fn parse_parsing_query_model_with_range_s_variable() {
    let n = now();
    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__range_s]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [172800]})");
}

#[test]
fn parse_parsing_query_model_with_braced_range_s_variable() {
    let n = now();
    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [${__range_s}s]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [172800s]})");
}

#[test]
fn parse_parsing_query_model_with_range_s_variable_below_half_a_second() {
    let n = now();
    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__range_s]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 40 * MS, MIN);
    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [0]})");
}

#[test]
fn parse_parsing_query_model_with_range_s_variable_between_half_and_one_second() {
    let n = now();
    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__range_s]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 800 * MS, MIN);
    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [1]})");
}

#[test]
fn parse_parsing_query_model_with_range_ms_variable() {
    let n = now();
    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__range_ms]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [172800000]})");
}

#[test]
fn parse_parsing_query_model_with_range_ms_variable_below_1s() {
    let n = now();
    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__range_ms]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 20 * MS, MIN);
    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [20]})");
}

#[test]
fn parse_parsing_query_model_with_rate_interval_variable() {
    let n = now();
    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__rate_interval]})", "format": "time_series", "intervalFactor": 1, "interval": "5m", "refId": "A"}"#, n, n + 5 * MIN, MIN);
    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [20m0s]})");
}

#[test]
fn parse_parsing_query_model_with_rate_interval_variable_in_expr_and_interval() {
    let n = now();
    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__rate_interval]})", "format": "time_series", "intervalFactor": 1, "interval": "$__rate_interval", "refId": "A"}"#, n, n + 5 * MIN, MIN);
    let res = parse_ok(&q, "15s");
    assert_eq!(res.expr, "rate(ALERTS{job=\"test\" [1m0s]})");
    assert_eq!(res.step, MIN);
}

#[test]
fn parse_parsing_query_model_with_rate_interval_ms_variable() {
    let n = now();
    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__rate_interval_ms]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, 2 * MIN);
    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [135000]})");
}

#[test]
fn parse_parsing_query_model_with_rate_interval_ms_and_rate_interval_variable() {
    let n = now();
    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__rate_interval_ms]}) + rate(ALERTS{job=\"test\" [$__rate_interval]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, 2 * MIN);
    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [135000]}) + rate(ALERTS{job=\"test\" [2m15s]})");
}

#[test]
fn parse_parsing_query_model_with_legacy_datasource_reference() {
    let n = now();
    let q = query_context(r#"{"datasource": "hello", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, 2 * MIN);
    assert_eq!(parse_ok(&q, "15s").ref_id, "A");
}

#[test]
fn parse_parsing_query_model_with_braced_rate_interval_ms_and_rate_interval_variable() {
    let n = now();
    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [${__rate_interval_ms}]}) + rate(ALERTS{job=\"test\" [${__rate_interval}]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, 2 * MIN);
    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [135000]}) + rate(ALERTS{job=\"test\" [2m15s]})");
}

#[test]
fn parse_parsing_query_model_of_range_query() {
    let n = now();
    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "intervalFactor": 1, "refId": "A", "range": true}"#, n, n + 48 * HOUR, MIN);
    assert!(parse_ok(&q, "15s").range_query);
}

#[test]
fn parse_parsing_query_model_of_range_and_instant_query() {
    let n = now();
    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "intervalFactor": 1, "refId": "A", "range": true, "instant": true}"#, n, n + 48 * HOUR, MIN);
    let res = parse_ok(&q, "15s");
    assert!(res.range_query);
    assert!(res.instant_query);
}

#[test]
fn parse_parsing_query_model_of_with_no_query_type() {
    let n = now();
    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    assert!(parse_ok(&q, "15s").range_query);
}

// TestRateInterval
/// `mockQuery`: default time range of one hour.
fn mock_query(expr: &str, interval: &str, interval_ms: i64, range: Option<(i64, i64)>) -> DataQuery {
    let n = now();
    let (from, to) = range.unwrap_or((n, n + HOUR));
    let json = format!(r#"{{"expr": "{expr}", "format": "time_series", "interval": "{interval}", "intervalMs": {interval_ms}, "intervalFactor": 1, "refId": "A"}}"#);
    DataQuery { ref_id: "A".to_string(), interval: interval_ms * MS, from, to, json: Json::parse(&json).expect("json"), ..Default::default() }
}

fn rate_case(expr: &str, interval: &str, interval_ms: i64, scrape: &str, range: Option<i64>, want_expr: &str, want_step: i64) {
    let n = now();
    let mut q = mock_query(expr, interval, interval_ms, range.map(|d| (n, n + d)));
    q.max_data_points = 12384;
    let res = parse_ok(&q, scrape);
    assert_eq!(res.expr, want_expr);
    assert_eq!(res.step, want_step);
}

const RATE: &str = "rate(rpc_durations_seconds_count[$__rate_interval])";

#[test]
fn rate_interval_interval_ms_100s_min_step_override_150s_and_scrape_interval_30s() {
    rate_case(RATE, "150s", 100000, "30s", None, "rate(rpc_durations_seconds_count[10m0s])", 150 * SEC);
}
#[test]
fn rate_interval_interval_ms_120s_min_step_override_150s_and_ds_scrape_interval_30s() {
    rate_case(RATE, "150s", 120000, "30s", None, "rate(rpc_durations_seconds_count[10m0s])", 150 * SEC);
}
#[test]
fn rate_interval_interval_ms_120s_min_step_auto_and_ds_scrape_interval_30s() {
    rate_case(RATE, "120s", 120000, "30s", None, "rate(rpc_durations_seconds_count[8m0s])", 120 * SEC);
}
#[test]
fn rate_interval_interval_and_min_step_automatic_ds_scrape_interval_30s_time_range_1h() {
    rate_case(RATE, "30s", 30000, "30s", Some(HOUR), "rate(rpc_durations_seconds_count[2m0s])", 30 * SEC);
}
#[test]
fn rate_interval_min_step_is_rate_interval_ds_scrape_interval_30s_time_range_1h() {
    rate_case(RATE, "$__rate_interval", 30000, "30s", Some(HOUR), "rate(rpc_durations_seconds_count[2m0s])", 2 * MIN);
}
#[test]
fn rate_interval_min_step_is_rate_interval_ds_scrape_interval_30s_time_range_2d() {
    rate_case(RATE, "$__rate_interval", 120000, "30s", Some(48 * HOUR), "rate(rpc_durations_seconds_count[2m30s])", 150 * SEC);
}
#[test]
fn rate_interval_min_step_is_interval_ds_scrape_interval_15s_time_range_2d() {
    rate_case(RATE, "$__interval", 120000, "15s", Some(48 * HOUR), "rate(rpc_durations_seconds_count[8m0s])", 120 * SEC);
}

#[test]
fn rate_interval_min_step_is_auto_and_ds_scrape_interval_30s_and_time_range_1_hour() {
    let n = now();
    let json = r#"{
        "datasource":{"type":"prometheus","uid":"zxS5e5W4k"},
        "datasourceId":38,
        "editorMode":"code",
        "exemplar":false,
        "expr":"sum(rate(process_cpu_seconds_total[$__rate_interval]))",
        "instant":false,
        "interval":"",
        "intervalMs":30000,
        "key":"Q-f96b6729-c47a-4ea8-8f71-a79774cf9bd5-0",
        "legendFormat":"__auto",
        "maxDataPoints":1613,
        "range":true,
        "refId":"G",
        "requestId":"1G",
        "utcOffsetSec":3600
    }"#;
    let q = DataQuery { ref_id: "G".to_string(), max_data_points: 1613, interval: 30 * SEC, from: n, to: n + HOUR, json: Json::parse(json).expect("json"), ..Default::default() };
    let res = parse_ok(&q, "30s");
    assert_eq!(res.expr, "sum(rate(process_cpu_seconds_total[2m0s]))");
    assert_eq!(res.step, 30 * SEC);
}

#[test]
fn rate_interval_min_step_is_auto_and_ds_scrape_interval_15s_and_time_range_5_minutes() {
    let n = now();
    let json = r#"{
        "datasource": {"type": "prometheus", "uid": "2z9d6ElGk"},
        "editorMode": "code",
        "expr": "sum(rate(cache_requests_total[$__rate_interval]))",
        "legendFormat": "__auto",
        "range": true,
        "refId": "A",
        "exemplar": false,
        "requestId": "1A",
        "utcOffsetSec": 0,
        "interval": "",
        "datasourceId": 508,
        "intervalMs": 15000,
        "maxDataPoints": 1055
    }"#;
    let q = DataQuery { ref_id: "A".to_string(), max_data_points: 1055, interval: 15 * SEC, from: n, to: n + 5 * MIN, json: Json::parse(json).expect("json"), ..Default::default() };
    let res = parse_ok(&q, "15s");
    assert_eq!(res.expr, "sum(rate(cache_requests_total[1m0s]))");
    assert_eq!(res.step, 15 * SEC);
}

// TestAlignTimeRange
#[test]
fn align_time_range_second_step() {
    assert_eq!(align_time_range(1664816826 * SEC, 10 * SEC, 0), 1664816820 * SEC);
}
#[test]
fn align_time_range_millisecond_step() {
    assert_eq!(align_time_range(1664816825 * SEC + 5 * MS, 10 * MS, 0), 1664816825 * SEC);
}
#[test]
fn align_time_range_second_step_with_offset() {
    assert_eq!(align_time_range(1664816825 * SEC + 5 * MS, 2 * SEC, -3), 1664816825 * SEC);
}
#[test]
fn align_time_range_1w_step_with_range_date_of_monday_changes_the_range_to_a_thursday() {
    let monday = 1704672000;
    let thursday = 1704326400;
    assert_eq!(align_time_range(monday * SEC, 604800 * SEC, 0), thursday * SEC);
}

// TestParseNumericFormatError
fn numeric_format(format: &str) {
    let n = now();
    let q = query_context(&format!(r#"{{"expr": "up", "format": {format}, "refId": "A"}}"#), n, n + 12 * HOUR, 0);
    let res = parse_ok(&q, "15s");
    assert_eq!(res.expr, "up");
}
#[test]
fn numeric_format_0_should_be_handled_gracefully() {
    numeric_format("0");
}
#[test]
fn numeric_format_1_should_default_to_time_series() {
    numeric_format("1");
}
#[test]
fn numeric_format_2_should_map_to_table() {
    numeric_format("2");
}
#[test]
fn numeric_format_3_should_map_to_heatmap() {
    numeric_format("3");
}
#[test]
fn numeric_format_unknown_number_should_default_to_time_series() {
    numeric_format("999");
}
#[test]
fn numeric_format_float_should_be_handled_gracefully() {
    numeric_format("1.5");
}

// TestPromQueryFormatUnmarshalJSON
fn format_of(json: &str) -> &'static str {
    let v = Json::parse(json).expect("json");
    parse_format(v.get("format"))
}
#[test]
fn query_format_valid_time_series() {
    assert_eq!(format_of(r#"{"format": "time_series"}"#), "time_series");
}
#[test]
fn query_format_valid_table() {
    assert_eq!(format_of(r#"{"format": "table"}"#), "table");
}
#[test]
fn query_format_valid_heatmap() {
    assert_eq!(format_of(r#"{"format": "heatmap"}"#), "heatmap");
}
#[test]
fn query_format_invalid_unknown_format_string() {
    assert_eq!(format_of(r#"{"format": "invalid_format"}"#), "time_series");
}
#[test]
fn query_format_invalid_empty_string() {
    assert_eq!(format_of(r#"{"format": ""}"#), "time_series");
}
#[test]
fn query_format_invalid_random_string() {
    assert_eq!(format_of(r#"{"format": "foobar"}"#), "time_series");
}
#[test]
fn query_format_invalid_typo_in_format() {
    assert_eq!(format_of(r#"{"format": "time_serie"}"#), "time_series");
}
