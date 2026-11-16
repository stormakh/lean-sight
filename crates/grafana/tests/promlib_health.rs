//! Port of promlib `healthcheck_test.go` (Test_healthcheck), `heuristics_test.go` (Test_GetHeuristics),
//! `client/transport_test.go` (TestCreateTransportOptions) and `middleware/force_http_get_test.go`
//! (TestEnsureHttpMethodMiddleware). The Go round trippers become transports; the instance is built by
//! `Instance::new`, the port of `newInstanceSettings`.
use grafana::promlib::client::{DsSettings, HttpReq, HttpResp, Instance, TransportError, forwarded_headers};
use json::Json;
use std::cell::RefCell;

/// `getPromInstanceSettings()`: URL http://promurl:9090, basic auth as admin, JSONData `{}` (httpMethod POST).
fn prom_instance() -> Instance {
    Instance::new(&DsSettings { url: "http://promurl:9090", basic_auth: true, basic_auth_user: "admin", json_data: "{}", default_timeout_secs: 30, ..Default::default() }).expect("instance")
}

fn answer(status: u16, body: &str) -> Result<HttpResp, TransportError> {
    Ok(HttpResp { status, headers: Vec::new(), body: body.as_bytes().to_vec() })
}

// Test_healthcheck
#[test]
fn healthcheck_should_do_a_successful_health_check() {
    // healthCheckSuccessRoundTripper: a scalar answer to every request
    let rt = |_r: &HttpReq| answer(200, "{\n\t\t\t\"status\": \"success\",\n\t\t\t\"data\": {\n\t\t\t\t\"resultType\": \"scalar\",\n\t\t\t\t\"result\": [\n\t\t\t\t\t1692969348.331,\n\t\t\t\t\t\"3\"\n\t\t\t\t]\n\t\t\t}\n\t\t}");
    let (ok, _message, _details) = grafana::promlib::check_health(&prom_instance(), &rt);
    assert!(ok);
}

#[test]
fn healthcheck_should_return_an_error_for_an_unsuccessful_health_check() {
    // healthCheckFailRoundTripper: 400 with no body
    let rt = |_r: &HttpReq| answer(400, "");
    let (ok, _message, _details) = grafana::promlib::check_health(&prom_instance(), &rt);
    assert!(!ok);
}

// Test_GetHeuristics
fn heuristics_of(body: &str) -> Option<Json> {
    let rt = |_r: &HttpReq| answer(200, body);
    grafana::promlib::heuristics(&prom_instance(), &rt)
}

#[test]
fn heuristics_should_return_prometheus() {
    let res = heuristics_of("{\"status\":\"success\",\"data\":{\"version\":\"1.0\"}}").expect("heuristics");
    assert_eq!(res.str_of("application"), "Prometheus");
    assert_eq!(res.get("features").map(|f| f.stringify()), Some("{\"rulerApiEnabled\":false}".to_string()));
}

#[test]
fn heuristics_should_return_mimir() {
    let res = heuristics_of("{\"status\":\"success\",\"data\":{\"features\":{\"foo\":\"bar\"},\"version\":\"1.0\"}}").expect("heuristics");
    assert_eq!(res.str_of("application"), "Mimir");
    assert_eq!(res.get("features").map(|f| f.stringify()), Some("{\"rulerApiEnabled\":true}".to_string()));
}

#[test]
fn heuristics_buildinfo_request_is_always_sent_as_get_even_for_post_configured_datasources() {
    let inst = prom_instance();
    assert_eq!(inst.opts.http_method, "POST");
    let got: RefCell<Option<HttpReq>> = RefCell::new(None);
    let rt = |r: &HttpReq| {
        *got.borrow_mut() = Some(r.clone());
        answer(200, r#"{"status":"success","data":{"version":"1.0"}}"#)
    };
    let res = grafana::promlib::heuristics(&inst, &rt);
    assert!(res.is_some());
    let req = got.borrow().clone().expect("a request");
    assert_eq!(req.method, "GET");
    let path = req.url.strip_prefix("http://promurl:9090").expect("data source URL").split('?').next().unwrap_or("").to_string();
    assert_eq!(path, "/api/v1/status/buildinfo");
}

// TestCreateTransportOptions
#[test]
fn transport_creates_correct_options_object() {
    let secure = Json::obj().with("httpHeaderValue1", "bar");
    let inst = Instance::new(&DsSettings { json_data: r#"{"httpHeaderName1": "foo"}"#, secure: Some(&secure), default_timeout_secs: 30, ..Default::default() }).expect("instance");
    // Go: http.Header{"Foo": []string{"bar"}} (header names compare without case)
    assert_eq!(inst.headers.len(), 1);
    assert!(inst.headers[0].0.eq_ignore_ascii_case("Foo"));
    assert_eq!(inst.headers[0].1, "bar");
}

#[test]
fn transport_enables_forward_http_headers_so_grafana_headers_are_forwarded_to_the_datasource() {
    // ForwardHTTPHeaders: the headers Grafana sets on the plugin request (X-Dashboard-*, X-Panel-*, X-Grafana-*,
    // X-Query-Group-Id, X-Datasource-Uid) reach the data source; other incoming headers do not.
    let incoming: Vec<(String, String)> = [
        ("x-dashboard-uid", "dash1"),
        ("x-panel-id", "2"),
        ("x-grafana-org-id", "1"),
        ("x-datasource-uid", "prom1"),
        ("x-query-group-id", "q1"),
        ("x-panel-plugin-id", "timeseries"),
        ("x-dashboard-title", "Caf\u{e9}"),
        ("cookie", "grafana_session=x"),
        ("authorization", "Basic eA=="),
        ("x-panel-title", ""),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let fwd = forwarded_headers(&incoming);
    let names: Vec<&str> = fwd.iter().map(|h| h.0.as_str()).collect();
    assert_eq!(names, vec!["X-Query-Group-Id", "X-Panel-Id", "X-Dashboard-Uid", "X-Datasource-Uid", "X-Grafana-Org-Id", "X-Panel-Plugin-Id", "X-Dashboard-Title"]);
    assert_eq!(fwd.iter().find(|h| h.0 == "X-Dashboard-Title").map(|h| h.1.as_str()), Some("Caf%C3%A9"));
}

// TestEnsureHttpMethodMiddleware ("Name should be correct" checks the SDK middleware name: nothing to port)
#[test]
fn force_http_get_should_force_get_method() {
    let inst = Instance::new(&DsSettings { url: "http://example.com", json_data: r#"{"httpMethod":"GET"}"#, default_timeout_secs: 30, ..Default::default() }).expect("instance");
    let sent = RefCell::new(0);
    let final_round_tripper = |_r: &HttpReq| {
        *sent.borrow_mut() += 1;
        answer(200, "")
    };
    let req = HttpReq { method: "POST".to_string(), url: "http://example.com".to_string(), ..Default::default() };
    let res = inst.round_trip(&req, &final_round_tripper).expect("response");
    assert_eq!(res.status, 405);
    assert_eq!(*sent.borrow(), 0);
}
