//! Port of promlib `models/settings_test.go`.
use grafana::promlib::settings::{PromOptions, parse_prom_options};

fn with_method(m: &str) -> Result<PromOptions, String> {
    parse_prom_options(&json::Json::obj().with("httpMethod", m).stringify())
}

fn method_ok(input: &str, want: &str) {
    assert_eq!(with_method(input).expect("options").http_method, want);
}

fn method_err(input: &str, want: &str) {
    let e = with_method(input).expect_err("error");
    assert!(e.contains(want), "{e}");
}

// TestParsePromOptions_HTTPMethod
#[test]
fn http_method_empty_defaults_to_post() {
    method_ok("", "POST");
}
#[test]
fn http_method_lowercase_get_normalised_to_get() {
    method_ok("get", "GET");
}
#[test]
fn http_method_mixed_case_post_normalised_to_post() {
    method_ok("Post", "POST");
}
#[test]
fn http_method_uppercase_post_accepted() {
    method_ok("POST", "POST");
}
#[test]
fn http_method_uppercase_get_accepted() {
    method_ok("GET", "GET");
}
#[test]
fn http_method_whitespace_padded_get_normalised() {
    method_ok("  get  ", "GET");
}
#[test]
fn http_method_put_rejected() {
    method_err("PUT", r#"invalid httpMethod "PUT""#);
}
#[test]
fn http_method_delete_rejected() {
    method_err("DELETE", r#"invalid httpMethod "DELETE""#);
}

// TestParsePromOptions_MalformedJSON
#[test]
fn malformed_json() {
    let e = parse_prom_options("{not valid json").expect_err("error");
    assert!(e.contains("error unmarshalling JSONData"), "{e}");
}

// TestParsePromOptions_EmptyJSONData
#[test]
fn empty_json_data_nil() {
    assert_eq!(parse_prom_options("").expect("options").http_method, "POST");
}
#[test]
fn empty_json_data_empty_slice() {
    assert_eq!(parse_prom_options("").expect("options").http_method, "POST");
}
#[test]
fn empty_json_data_empty_json_object() {
    assert_eq!(parse_prom_options("{}").expect("options").http_method, "POST");
}

// TestParsePromOptions_QueryStatsEnabled
#[test]
fn query_stats_enabled_true() {
    assert!(parse_prom_options(r#"{"queryStatsEnabled":true}"#).expect("options").query_stats_enabled);
}
#[test]
fn query_stats_enabled_false() {
    assert!(!parse_prom_options(r#"{"queryStatsEnabled":false}"#).expect("options").query_stats_enabled);
}
#[test]
fn query_stats_enabled_missing_defaults_to_false() {
    assert!(!parse_prom_options("{}").expect("options").query_stats_enabled);
}

// TestPromOptions_ApplyDefaults
fn defaults(input: &str, want: &str) {
    let mut o = PromOptions { http_method: input.to_string(), ..Default::default() };
    o.apply_defaults();
    assert_eq!(o.http_method, want);
}
#[test]
fn apply_defaults_empty_http_method_defaults_to_post() {
    defaults("", "POST");
}
#[test]
fn apply_defaults_whitespace_only_http_method_defaults_to_post() {
    defaults("   ", "POST");
}
#[test]
fn apply_defaults_lowercase_get_is_normalised_to_get() {
    defaults("get", "GET");
}
#[test]
fn apply_defaults_mixed_case_post_is_normalised_to_post() {
    defaults("Post", "POST");
}
#[test]
fn apply_defaults_whitespace_padded_value_is_trimmed_and_uppercased() {
    defaults("  get  ", "GET");
}
#[test]
fn apply_defaults_already_uppercase_value_is_preserved() {
    defaults("GET", "GET");
}

// TestPromOptions_ApplyDefaults_DoesNotMutateUnrelatedFields
#[test]
fn apply_defaults_does_not_mutate_unrelated_fields() {
    let mut o = PromOptions { time_interval: "30s".to_string(), query_timeout: "60s".to_string(), prometheus_type: "Prometheus".to_string(), series_limit: Some(42.0), ..Default::default() };
    o.apply_defaults();
    assert_eq!(o.time_interval, "30s");
    assert_eq!(o.query_timeout, "60s");
    assert_eq!(o.prometheus_type, "Prometheus");
    assert_eq!(o.series_limit, Some(42.0));
}

// TestPromOptions_Validate
fn validate(method: &str) -> Result<(), String> {
    PromOptions { http_method: method.to_string(), ..Default::default() }.validate()
}
fn validate_err(method: &str, want: &str) {
    let e = validate(method).expect_err("error");
    assert!(e.contains(want), "{e}");
}
#[test]
fn validate_empty_is_valid() {
    assert!(validate("").is_ok());
}
#[test]
fn validate_get_is_valid() {
    assert!(validate("GET").is_ok());
}
#[test]
fn validate_post_is_valid() {
    assert!(validate("POST").is_ok());
}
#[test]
fn validate_lowercase_get_is_valid() {
    assert!(validate("get").is_ok());
}
#[test]
fn validate_lowercase_post_is_valid() {
    assert!(validate("post").is_ok());
}
#[test]
fn validate_put_is_rejected() {
    validate_err("PUT", r#"invalid httpMethod "PUT""#);
}
#[test]
fn validate_delete_is_rejected() {
    validate_err("DELETE", r#"invalid httpMethod "DELETE""#);
}
#[test]
fn validate_patch_is_rejected() {
    validate_err("PATCH", r#"invalid httpMethod "PATCH""#);
}
#[test]
fn validate_arbitrary_string_is_rejected() {
    validate_err("foo", r#"invalid httpMethod "foo""#);
}
