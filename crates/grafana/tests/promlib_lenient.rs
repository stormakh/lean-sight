//! Port of promlib `models/lenient_test.go`. The warnings Go logs through `log.DefaultLogger.Warn` are the ones
//! `parse_prom_options_logged` returns, in the test recorder's "from->to outcome value" form.
//! TestPointerPropertiesAreAccountedFor walks the Go struct by reflection; in Rust `series_limit` is the only
//! `Option` member and the compiler lists the members, so it has no port.
use grafana::promlib::settings::{PromOptions, parse_prom_options, parse_prom_options_logged};

fn parse(json_data: &str) -> PromOptions {
    match parse_prom_options(json_data) {
        Ok(o) => o,
        Err(e) => panic!("jsonData {json_data} failed: {e}"),
    }
}

// TestParsePromOptions_LooselyTypedJSONData
#[test]
fn loosely_typed_booleans_stored_as_quoted_strings() {
    let o = parse(r#"{"seriesEndpoint":"true","disableRecordingRules":"false","oauthPassThru":"1"}"#);
    assert!(o.series_endpoint);
    assert!(!o.disable_recording_rules);
    assert!(o.oauth_pass_thru);
}
#[test]
fn loosely_typed_capitalised_booleans_are_not_silently_inverted() {
    let o = parse(r#"{"seriesEndpoint":"True","disableMetricsLookup":"TRUE","incrementalQuerying":"False"}"#);
    assert!(o.series_endpoint);
    assert!(o.disable_metrics_lookup);
    assert!(!o.incremental_querying);
}
#[test]
fn loosely_typed_an_unrecognised_boolean_spelling_falls_back_to_false() {
    let o = parse(r#"{"seriesEndpoint":"yes","oauthPassThru":"maybe"}"#);
    assert!(!o.series_endpoint);
    assert!(!o.oauth_pass_thru);
}
#[test]
fn loosely_typed_booleans_stored_as_0_1() {
    let o = parse(r#"{"queryStatsEnabled":1,"disableMetricsLookup":0}"#);
    assert!(o.query_stats_enabled);
    assert!(!o.disable_metrics_lookup);
}
#[test]
fn loosely_typed_strings_stored_as_bare_numbers_keep_their_text() {
    let o = parse(r#"{"incrementalQueryOverlapWindow":10,"prometheusVersion":2.4,"customQueryParameters":123}"#);
    assert_eq!(o.incremental_query_overlap_window, "10");
    assert_eq!(o.prometheus_version, "2.4");
    assert_eq!(o.custom_query_parameters, "123");
}
#[test]
fn loosely_typed_thresholds_stored_as_quoted_numbers() {
    let o = parse(r#"{"maxSamplesProcessedWarningThreshold":"100000","maxSamplesProcessedErrorThreshold":"200000"}"#);
    assert_eq!(o.max_samples_processed_warning_threshold, 100000.0);
    assert_eq!(o.max_samples_processed_error_threshold, 200000.0);
}
#[test]
fn loosely_typed_series_limit_stored_as_a_quoted_number() {
    assert_eq!(parse(r#"{"seriesLimit":"1000"}"#).series_limit, Some(1000.0));
}
#[test]
fn loosely_typed_series_limit_stored_as_a_fractional_literal() {
    assert_eq!(parse(r#"{"seriesLimit":1000.0}"#).series_limit, Some(1000.0));
}
#[test]
fn loosely_typed_an_exemplar_value_that_is_not_a_list_is_ignored() {
    let o = parse(r#"{"exemplarTraceIdDestinations":{"name":"traceID"},"queryStatsEnabled":"true"}"#);
    assert!(o.exemplar_trace_id_destinations.is_empty());
    assert!(o.query_stats_enabled);
}
#[test]
fn loosely_typed_a_well_formed_exemplar_list_still_decodes() {
    let o = parse(r#"{"exemplarTraceIdDestinations":[{"name":"traceID","datasourceUid":"abc"}]}"#);
    assert_eq!(o.exemplar_trace_id_destinations.len(), 1);
    assert_eq!(o.exemplar_trace_id_destinations[0].name, "traceID");
    assert_eq!(o.exemplar_trace_id_destinations[0].datasource_uid, "abc");
}
#[test]
fn loosely_typed_a_value_that_cannot_be_read_falls_back_to_the_zero_value() {
    let o = parse(r#"{"seriesEndpoint":{"a":1},"prometheusVersion":["x"],"queryStatsEnabled":"true"}"#);
    assert!(!o.series_endpoint);
    assert!(o.prometheus_version.is_empty());
    assert!(o.query_stats_enabled);
}
#[test]
fn loosely_typed_a_mistyped_field_does_not_discard_the_fields_around_it() {
    let o = parse(r#"{"httpMethod":"GET","timeInterval":"30s","seriesLimit":"5","queryStatsEnabled":"true"}"#);
    assert_eq!(o.http_method, "GET");
    assert_eq!(o.time_interval, "30s");
    assert_eq!(o.series_limit, Some(5.0));
    assert!(o.query_stats_enabled);
}

// TestParsePromOptions_PreexistingStrictFieldsStayStrict
fn strict(json_data: &str) {
    match parse_prom_options(json_data) {
        Ok(_) => panic!("jsonData {json_data} must fail"),
        Err(e) => assert!(e.contains("error unmarshalling JSONData"), "{e}"),
    }
}
#[test]
fn strict_http_method() {
    strict(r#"{"httpMethod":30}"#);
}
#[test]
fn strict_time_interval() {
    strict(r#"{"timeInterval":30}"#);
}
#[test]
fn strict_query_timeout() {
    strict(r#"{"queryTimeout":60}"#);
}

// TestParsePromOptions_LenientFieldsCannotFailTheDatasource: every json key of the upstream PromOptions struct
const JSON_DATA_KEYS: [&str; 28] = [
    "allowAsRecordingRulesTarget",
    "cacheLevel",
    "customQueryParameters",
    "defaultEditor",
    "disableMetricsLookup",
    "disableRecordingRules",
    "enableSecureSocksProxy",
    "exemplarTraceIdDestinations",
    "httpMethod",
    "incrementalQuerying",
    "incrementalQueryOverlapWindow",
    "keepCookies",
    "manageAlerts",
    "maxSamplesProcessedErrorThreshold",
    "maxSamplesProcessedWarningThreshold",
    "oauthPassThru",
    "prometheusType",
    "prometheusVersion",
    "queryStatsEnabled",
    "queryTimeout",
    "seriesEndpoint",
    "seriesLimit",
    "serverName",
    "timeInterval",
    "timeout",
    "tlsAuth",
    "tlsAuthWithCACert",
    "tlsSkipVerify",
];
const STRICT_JSON_DATA_FIELDS: [&str; 3] = ["httpMethod", "timeInterval", "queryTimeout"];

#[test]
fn lenient_fields_cannot_fail_the_datasource() {
    let values = [
        r#""true""#, r#""false""#, r#""True""#, r#""nonsense""#, "true", "false", "1", "0", "30.5", "30.0", "60", r#""60""#, r#""30s""#, r#""abc""#, r#""""#, "null", "[]", r#"["a"]"#, "[1]",
        "{}", r#"{"a":1}"#,
    ];
    for key in JSON_DATA_KEYS.iter() {
        if STRICT_JSON_DATA_FIELDS.contains(key) {
            continue;
        }
        for value in values.iter() {
            let json_data = format!("{{{}:{}}}", json::quote(key), value);
            if let Err(e) = parse_prom_options(&json_data) {
                panic!("jsonData {json_data} must not fail the datasource: {e}");
            }
        }
    }
}

// TestLenientTypes_LogOnlyWhenLenient (require.ElementsMatch: the same warnings in any order)
fn logs(json_data: &str, want: &[&str]) {
    let (r, mut got) = parse_prom_options_logged(json_data);
    assert!(r.is_ok(), "{json_data}: {r:?}");
    let mut want: Vec<String> = want.iter().map(|s| s.to_string()).collect();
    got.sort();
    want.sort();
    assert_eq!(got, want, "{json_data}");
}
#[test]
fn log_correctly_typed_values_log_nothing() {
    logs(r#"{"seriesEndpoint":true,"seriesLimit":10,"prometheusVersion":"2.50.1"}"#, &[]);
}
#[test]
fn log_null_is_absence_not_a_type_mismatch() {
    logs(r#"{"seriesEndpoint":null,"seriesLimit":null,"prometheusVersion":null}"#, &[]);
}
#[test]
fn log_undeclared_properties_log_nothing() {
    logs(r#"{"sigV4Auth":123,"someLegacyField":{"a":1}}"#, &[]);
}
#[test]
fn log_a_salvaged_boolean_is_coerced() {
    logs(r#"{"seriesEndpoint":"True"}"#, &[r#"string->bool coerced "True""#]);
}
#[test]
fn log_an_unreadable_boolean_is_dropped() {
    logs(r#"{"seriesEndpoint":"banana"}"#, &[r#"string->bool dropped "banana""#]);
}
#[test]
fn log_every_lenient_value_is_reported_not_just_the_first() {
    logs(r#"{"seriesEndpoint":"true","seriesLimit":"10","prometheusVersion":2.4}"#, &[r#"string->bool coerced "true""#, r#"string->float64 coerced "10""#, "float64->string coerced 2.4"]);
}
#[test]
fn log_any_number_shape_is_accepted_without_coercion() {
    logs(r#"{"seriesLimit":1000}"#, &[]);
}
#[test]
fn log_a_fractional_literal_needs_no_coercion_either() {
    logs(r#"{"seriesLimit":1000.0}"#, &[]);
}
#[test]
fn log_nor_does_exponent_notation() {
    logs(r#"{"seriesLimit":1e3}"#, &[]);
}
#[test]
fn log_a_number_read_as_a_boolean_is_distinguishable_from_a_string() {
    logs(r#"{"seriesEndpoint":1}"#, &["float64->bool coerced 1"]);
}
#[test]
fn log_a_boolean_read_as_a_string_names_bool_as_the_source() {
    logs(r#"{"prometheusVersion":true}"#, &["bool->string coerced true"]);
}
#[test]
fn log_an_unparseable_number_string_is_reported_as_a_string_not_unknown() {
    logs(r#"{"seriesLimit":"ten","maxSamplesProcessedWarningThreshold":"lots"}"#, &[r#"string->float64 dropped "ten""#, r#"string->float64 dropped "lots""#]);
}
#[test]
fn log_an_unusable_shape_is_dropped() {
    logs(r#"{"seriesEndpoint":["true"],"oauthPassThru":{"a":1}}"#, &[r#"unknown->bool dropped ["true"]"#, r#"unknown->bool dropped {"a":1}"#]);
}
#[test]
fn log_a_non_list_exemplar_value_is_dropped() {
    logs(r#"{"exemplarTraceIdDestinations":{"name":"x"}}"#, &[r#"unknown->exemplarDestinations dropped {"name":"x"}"#]);
}

// TestParsePromOptions_DroppedPointerIsLeftUnset
fn series_limit(json_data: &str, want: Option<f64>) {
    assert_eq!(parse(json_data).series_limit, want, "{json_data}");
}
#[test]
fn dropped_pointer_absent_stays_unset() {
    series_limit("{}", None);
}
#[test]
fn dropped_pointer_null_stays_unset() {
    series_limit(r#"{"seriesLimit":null}"#, None);
}
#[test]
fn dropped_pointer_ignored_string_is_left_unset() {
    series_limit(r#"{"seriesLimit":"ten"}"#, None);
}
#[test]
fn dropped_pointer_ignored_object_is_left_unset() {
    series_limit(r#"{"seriesLimit":{}}"#, None);
}
#[test]
fn dropped_pointer_ignored_array_is_left_unset() {
    series_limit(r#"{"seriesLimit":[]}"#, None);
}
#[test]
fn dropped_pointer_ignored_boolean_is_left_unset() {
    series_limit(r#"{"seriesLimit":true}"#, None);
}
#[test]
fn dropped_pointer_a_stored_number_is_kept() {
    series_limit(r#"{"seriesLimit":1000}"#, Some(1000.0));
}
#[test]
fn dropped_pointer_an_explicit_zero_is_kept_not_mistaken_for_unset() {
    series_limit(r#"{"seriesLimit":0}"#, Some(0.0));
}
#[test]
fn dropped_pointer_a_quoted_number_is_coerced_and_kept() {
    series_limit(r#"{"seriesLimit":"1000"}"#, Some(1000.0));
}
#[test]
fn dropped_pointer_a_quoted_zero_is_coerced_and_kept() {
    series_limit(r#"{"seriesLimit":"0"}"#, Some(0.0));
}
#[test]
fn dropped_pointer_a_fractional_number_is_kept() {
    series_limit(r#"{"seriesLimit":1000.5}"#, Some(1000.5));
}
#[test]
fn dropped_pointer_exponent_notation_is_kept() {
    series_limit(r#"{"seriesLimit":1e3}"#, Some(1000.0));
}
