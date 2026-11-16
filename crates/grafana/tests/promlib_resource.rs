//! Port of promlib `resource/allowlist_test.go` (TestIsAllowedResourcePath).
use grafana::promlib::is_allowed_resource_path;

fn allowed(path: &str) {
    assert!(is_allowed_resource_path(path), "expected {path:?} to be allowed");
}
fn rejected(path: &str) {
    assert!(!is_allowed_resource_path(path), "expected {path:?} to be rejected");
}

// TestIsAllowedResourcePath: allowed
#[test]
fn allowed_api_v1_query() {
    allowed("api/v1/query");
}
#[test]
fn allowed_slash_api_v1_query() {
    allowed("/api/v1/query");
}
#[test]
fn allowed_api_v1_query_range() {
    allowed("api/v1/query_range");
}
#[test]
fn allowed_api_v1_query_exemplars() {
    allowed("api/v1/query_exemplars");
}
#[test]
fn allowed_api_v1_series() {
    allowed("api/v1/series");
}
#[test]
fn allowed_api_v1_labels() {
    allowed("api/v1/labels");
}
#[test]
fn allowed_api_v1_label_instance_values() {
    allowed("api/v1/label/instance/values");
}
#[test]
fn allowed_api_v1_label_name_values() {
    allowed("api/v1/label/__name__/values");
}
#[test]
fn allowed_api_v1_metadata() {
    allowed("api/v1/metadata");
}
#[test]
fn allowed_api_v1_rules() {
    allowed("api/v1/rules");
}
#[test]
fn allowed_api_v1_status_buildinfo() {
    allowed("api/v1/status/buildinfo");
}
#[test]
fn allowed_api_v1_search_metric_names() {
    allowed("api/v1/search/metric_names");
}
#[test]
fn allowed_api_v1_search_label_names() {
    allowed("api/v1/search/label_names");
}
#[test]
fn allowed_api_v1_search_label_values() {
    allowed("api/v1/search/label_values");
}

// TestIsAllowedResourcePath: rejected
#[test]
fn rejected_empty() {
    rejected("");
}
#[test]
fn rejected_slash() {
    rejected("/");
}
#[test]
fn rejected_reload() {
    rejected("-/reload");
}
#[test]
fn rejected_quit() {
    rejected("-/quit");
}
#[test]
fn rejected_federate() {
    rejected("federate");
}
#[test]
fn rejected_admin_tsdb_delete_series() {
    rejected("api/v1/admin/tsdb/delete_series");
}
#[test]
fn rejected_search_trailing_slash() {
    rejected("api/v1/search/");
}
#[test]
fn rejected_search_unknown_endpoint() {
    rejected("api/v1/search/unknown_endpoint");
}
#[test]
fn rejected_label_empty_name() {
    rejected("api/v1/label//values");
}
#[test]
fn rejected_label_name_with_slash() {
    rejected("api/v1/label/foo/bar/values");
}
#[test]
fn rejected_dot_dot_admin() {
    rejected("api/v1/../admin");
}
#[test]
fn rejected_label_dot_dot_reload() {
    rejected("api/v1/label/../../-/reload/values");
}
#[test]
fn rejected_dot_prefix() {
    rejected("./api/v1/labels");
}
#[test]
fn rejected_encoded_slashes() {
    rejected("api%2Fv1%2Flabels");
}
#[test]
fn rejected_encoded_dot_dot() {
    rejected("api/v1/labels%2F..%2F..%2F-%2Freload");
}

// resource/resource_test.go: TestNewResource, TestResource_Execute, TestResource_ExecuteRejectsDisallowedPath
use grafana::promlib::client::{DsSettings, HttpReq, HttpResp, Instance, TransportError};

/// `setup()` + `newResource`: settings {ID: 1, URL: "http://mock-server", JSONData: {}} through the constructor.
fn new_resource_instance() -> Result<Instance, String> {
    Instance::new(&DsSettings { id: 1, url: "http://mock-server", json_data: "{}", default_timeout_secs: 30, ..Default::default() })
}

/// `mockRoundTripper` with `setup()`'s response: 200 `{"message": "success"}`.
fn mock_round_tripper(_r: &HttpReq) -> Result<HttpResp, TransportError> {
    Ok(HttpResp { status: 200, headers: Vec::new(), body: br#"{"message": "success"}"#.to_vec() })
}

#[test]
fn new_resource() {
    let res = new_resource_instance();
    assert!(res.is_ok(), "{res:?}");
}

#[test]
fn resource_execute() {
    let res = new_resource_instance().expect("resource");
    let resp = grafana::promlib::call_resource(&res, "", "/api/v1/labels", "/api/v1/labels", &[], None, &mock_round_tripper).expect("response");
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, br#"{"message": "success"}"#.to_vec());
}

#[test]
fn resource_execute_rejects_disallowed_path() {
    let res = new_resource_instance().expect("resource");
    let resp = grafana::promlib::call_resource(&res, "", "-/reload", "/-/reload", &[], None, &mock_round_tripper).expect("response");
    assert_eq!(resp.status, 403);
}
