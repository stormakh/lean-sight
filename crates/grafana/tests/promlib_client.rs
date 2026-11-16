//! Port of promlib `client/client_test.go` (TestClient) and `middleware/custom_query_params_test.go`
//! (TestCustomQueryParametersMiddleware). The Rust client builds the request without sending it, so the request
//! the Go `MockDoer` / round tripper would see is the returned `HttpReq` (or the URL after `apply_middleware`).
use grafana::promlib::client::{HttpReq, Instance, parse_query};
use grafana::promlib::models::{Query, SEC};
use grafana::promlib::settings::PromOptions;

/// `NewClient(doer, method, "http://localhost:9090", "60s")`.
fn client(method: &str) -> Instance {
    let opts = PromOptions { http_method: method.to_string(), query_timeout: "60s".to_string(), ..Default::default() };
    Instance { url: "http://localhost:9090".to_string(), opts, ..Default::default() }
}

fn header<'a>(r: &'a HttpReq, name: &str) -> &'a str {
    r.headers.iter().find(|h| h.0.eq_ignore_ascii_case(name)).map(|h| h.1.as_str()).unwrap_or("")
}

fn raw_query(url: &str) -> &str {
    url.split_once('?').map(|p| p.1).unwrap_or("")
}

// TestClient / QueryResource
#[test]
fn query_resource_post_configured_client_forwards_post_request_as_post() {
    let c = client("POST");
    let r = c.resource_request("POST", "/api/v1/series", "", b"match%5B%5D=ALERTS&start=1655271408&end=1655293008");
    assert_eq!(r.method, "POST");
    assert_eq!(String::from_utf8(r.body).expect("utf8"), "match%5B%5D=ALERTS&start=1655271408&end=1655293008");
    assert_eq!(r.url, "http://localhost:9090/api/v1/series");
}

#[test]
fn query_resource_get_configured_client_converts_incoming_post_to_get() {
    let c = client("GET");
    let r = c.resource_request("POST", "/api/v1/query", "", b"query=up&time=1234567890");
    assert_eq!(r.method, "GET");
    assert!(raw_query(&r.url).contains("query=up"), "{}", r.url);
    assert!(raw_query(&r.url).contains("time=1234567890"), "{}", r.url);
}

#[test]
fn query_resource_get_request_is_always_respected_regardless_of_configured_method() {
    let c = client("POST");
    // CallResourceRequest URL "api/v1/series?match%5B%5D=ALERTS&start=1655272558&end=1655294158"
    let r = c.resource_request("GET", "/api/v1/series", "match%5B%5D=ALERTS&start=1655272558&end=1655294158", &[]);
    assert_eq!(r.method, "GET");
    assert_eq!(r.body, Vec::<u8>::new());
    assert_eq!(r.url, "http://localhost:9090/api/v1/series?match%5B%5D=ALERTS&start=1655272558&end=1655294158");
}

#[test]
fn query_resource_get_configured_client_preserves_existing_query_params_when_converting_post() {
    let c = client("GET");
    // CallResourceRequest URL "/api/v1/query?timeout=30"
    let r = c.resource_request("POST", "/api/v1/query", "timeout=30", b"query=up");
    assert_eq!(r.method, "GET");
    assert!(raw_query(&r.url).contains("timeout=30"), "{}", r.url);
    assert!(raw_query(&r.url).contains("query=up"), "{}", r.url);
}

// TestClient / QueryRange
fn range_query() -> Query {
    Query { expr: "rate(ALERTS{job=\"test\" [$__rate_interval]})".to_string(), start: 0, end: 1234 * SEC, range_query: true, step: SEC, ..Default::default() }
}

#[test]
fn query_range_sends_correct_post_query() {
    let r = client("POST").range_request(&range_query());
    assert_eq!(r.method, "POST");
    assert_eq!(header(&r, "Content-Type"), "application/x-www-form-urlencoded");
    assert_eq!(r.body, b"end=1234&query=rate%28ALERTS%7Bjob%3D%22test%22+%5B%24__rate_interval%5D%7D%29&start=0&step=1&timeout=60s".to_vec());
    assert_eq!(r.url, "http://localhost:9090/api/v1/query_range");
}

#[test]
fn query_range_sends_correct_get_query() {
    let r = client("GET").range_request(&range_query());
    assert_eq!(r.method, "GET");
    assert_eq!(r.body, Vec::<u8>::new());
    assert_eq!(r.url, "http://localhost:9090/api/v1/query_range?end=1234&query=rate%28ALERTS%7Bjob%3D%22test%22+%5B%24__rate_interval%5D%7D%29&start=0&step=1&timeout=60s");
}

// TestCustomQueryParametersMiddleware

/// `CustomQueryParameters(logger, customQueryParams, warnVal, errVal, queryStatsEnabled)` applied to `url`.
fn mw(custom: &str, warn: f64, err: f64, stats: bool, url: &str) -> String {
    let opts = PromOptions {
        custom_query_parameters: custom.to_string(),
        max_samples_processed_warning_threshold: warn,
        max_samples_processed_error_threshold: err,
        query_stats_enabled: stats,
        ..Default::default()
    };
    let inst = Instance { opts, ..Default::default() };
    let mut u = url.to_string();
    inst.apply_middleware(&mut u);
    u
}

/// Go `req.URL.Query()`.
fn query_of(url: &str) -> Vec<(String, String)> {
    parse_query(raw_query(url)).unwrap_or_default()
}
/// Go `url.Values.Get`: the first value or "".
fn get(q: &[(String, String)], k: &str) -> String {
    q.iter().find(|p| p.0 == k).map(|p| p.1.clone()).unwrap_or_default()
}
fn all(q: &[(String, String)], k: &str) -> Vec<String> {
    q.iter().filter(|p| p.0 == k).map(|p| p.1.clone()).collect()
}
fn escape(s: &str) -> String {
    let mut out = String::new();
    grafana::promlib::client::query_escape(s, &mut out);
    out
}

#[test]
fn middleware_without_custom_query_parameters_set_should_not_apply_middleware() {
    assert_eq!(mw("", 0.0, 0.0, false, "http://test.com/query?hello=name"), "http://test.com/query?hello=name");
}

#[test]
fn middleware_with_invalid_custom_query_parameters_should_not_apply_middleware() {
    assert_eq!(mw("custom=%%abc&test=abc", 0.0, 0.0, false, "http://test.com/query?hello=name"), "http://test.com/query?hello=name");
}

#[test]
fn middleware_with_custom_query_parameters_for_request_url_containing_query_parameters() {
    let u = mw("custom=par/am&second=f oo", 0.0, 0.0, false, "http://test.com/query?hello=name");
    assert!(u.starts_with("http://test.com/query?"), "{u}");
    let q = query_of(&u);
    let mut keys: Vec<&str> = q.iter().map(|p| p.0.as_str()).collect();
    keys.dedup();
    assert_eq!(keys.len(), 3, "{u}");
    assert_eq!(escape(&get(&q, "hello")), "name");
    assert_eq!(escape(&get(&q, "custom")), "par%2Fam");
    assert_eq!(escape(&get(&q, "second")), "f+oo");
}

#[test]
fn middleware_with_custom_query_parameters_for_request_url_not_containing_query_parameters() {
    assert_eq!(mw("custom=par/am&second=f oo", 0.0, 0.0, false, "http://test.com/query"), "http://test.com/query?custom=par%2Fam&second=f+oo");
}

#[test]
fn middleware_with_sample_thresholds_only_should_apply_middleware() {
    let q = query_of(&mw("", 500.0, 1000.0, false, "http://test.com/api/v1/query_range?query=up"));
    assert_eq!(get(&q, "max_samples_processed_warning_threshold"), "500");
    assert_eq!(get(&q, "max_samples_processed_error_threshold"), "1000");
    assert_eq!(get(&q, "query"), "up");
}

#[test]
fn middleware_with_zero_sample_thresholds_should_not_add_threshold_query_params() {
    let q = query_of(&mw("custom=value", 0.0, 0.0, false, "http://test.com/query?hello=name"));
    assert_eq!(get(&q, "hello"), "name");
    assert_eq!(get(&q, "custom"), "value");
    assert_eq!(get(&q, "max_samples_processed_warning_threshold"), "");
    assert_eq!(get(&q, "max_samples_processed_error_threshold"), "");
}

#[test]
fn middleware_with_custom_query_parameters_and_sample_thresholds_should_merge_query_string() {
    let q = query_of(&mw("timeout=30s", 42.0, 0.0, false, "http://test.com/query"));
    assert_eq!(get(&q, "timeout"), "30s");
    assert_eq!(get(&q, "max_samples_processed_warning_threshold"), "42");
    assert_eq!(get(&q, "max_samples_processed_error_threshold"), "");
}

#[test]
fn middleware_with_explicit_threshold_fields_and_matching_custom_query_parameters_should_prefer_threshold_fields() {
    let q = query_of(&mw("max_samples_processed_warning_threshold=9&max_samples_processed_error_threshold=17", 42.0, 88.0, false, "http://test.com/query"));
    assert_eq!(get(&q, "max_samples_processed_warning_threshold"), "42");
    assert_eq!(get(&q, "max_samples_processed_error_threshold"), "88");
}

#[test]
fn middleware_with_query_statistics_disabled_should_not_add_stats() {
    let q = query_of(&mw("", 0.0, 0.0, false, "http://test.com/api/v1/query?query=up"));
    assert_eq!(get(&q, "stats"), "");
}

/// "With query statistics enabled should add stats for <name>" (the method does not change the URL middleware).
fn stats_added(path: &str) {
    let q = query_of(&mw("", 0.0, 0.0, true, &format!("http://test.com{path}?query=up&existing=value")));
    assert_eq!(get(&q, "stats"), "all");
    assert_eq!(get(&q, "query"), "up");
    assert_eq!(get(&q, "existing"), "value");
}
#[test]
fn middleware_stats_enabled_should_add_stats_for_get_instant_query() {
    stats_added("/api/v1/query");
}
#[test]
fn middleware_stats_enabled_should_add_stats_for_post_instant_query() {
    stats_added("/api/v1/query");
}
#[test]
fn middleware_stats_enabled_should_add_stats_for_get_range_query() {
    stats_added("/api/v1/query_range");
}
#[test]
fn middleware_stats_enabled_should_add_stats_for_post_range_query() {
    stats_added("/api/v1/query_range");
}
#[test]
fn middleware_stats_enabled_should_add_stats_for_query_with_workspace_base_path() {
    stats_added("/workspaces/ws-123/api/v1/query");
}

/// "With query statistics enabled should not add stats to non-query endpoint <path>".
fn stats_not_added(path: &str) {
    let q = query_of(&mw("", 0.0, 0.0, true, &format!("http://test.com{path}")));
    assert_eq!(get(&q, "stats"), "");
}
#[test]
fn middleware_stats_enabled_should_not_add_stats_to_query_exemplars() {
    stats_not_added("/api/v1/query_exemplars");
}
#[test]
fn middleware_stats_enabled_should_not_add_stats_to_metadata() {
    stats_not_added("/api/v1/metadata");
}
#[test]
fn middleware_stats_enabled_should_not_add_stats_to_labels() {
    stats_not_added("/api/v1/labels");
}
#[test]
fn middleware_stats_enabled_should_not_add_stats_to_series() {
    stats_not_added("/api/v1/series");
}
#[test]
fn middleware_stats_enabled_should_not_add_stats_to_status_buildinfo() {
    stats_not_added("/api/v1/status/buildinfo");
}

#[test]
fn middleware_explicit_query_statistics_setting_should_override_a_custom_stats_value() {
    let q = query_of(&mw("stats=none&timeout=30s", 0.0, 0.0, true, "http://test.com/api/v1/query_range"));
    assert_eq!(all(&q, "stats"), vec!["all".to_string()]);
    assert_eq!(get(&q, "timeout"), "30s");
}
