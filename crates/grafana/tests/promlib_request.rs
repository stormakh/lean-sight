//! Port of promlib `querydata/request_test.go` (TestPrometheus_parseTimeSeriesResponse,
//! TestPrometheus_executedQueryString). Go's `fakeHttpClientProvider` answers `/api/v1/query_range`,
//! `/api/v1/query` and `/api/v1/query_exemplars` with canned bodies built by `toAPIResponse`; here a transport
//! closure does the same. Go's "timestamps are UTC zoned" checks have no Rust counterpart (times are Unix ns).
use grafana::promlib::client::{HttpReq, HttpResp, Instance, TransportError};
use grafana::promlib::frame::{Field, Frame, Values};
use grafana::promlib::models::{DataQuery, HOUR, MIN, MS, SEC};
use grafana::promlib::querydata::execute;
use grafana::promlib::settings::parse_prom_options;
use json::Json;

/// `setup()`: a data source at localhost:9090 with `timeInterval: 15s`.
fn instance() -> Instance {
    Instance { url: "http://localhost:9090".to_string(), opts: parse_prom_options(r#"{"timeInterval": "15s"}"#).expect("options"), ..Default::default() }
}

/// `toAPIResponse(d)`: `{"status":"success","data":<d>}` (`null` for a nil result).
fn api(data: Option<&str>) -> Vec<u8> {
    format!(r#"{{"status":"success","data":{}}}"#, data.unwrap_or("null")).into_bytes()
}

/// `execute(tctx, query, rqr, iqr, eqr)`: the frames of the query's response.
fn run(query: DataQuery, range: Option<&str>, instant: Option<&str>, exemplar: Option<&str>) -> Vec<Frame> {
    let (rb, ib, eb) = (api(range), api(instant), api(exemplar));
    let t = |r: &HttpReq| -> Result<HttpResp, TransportError> {
        let no_q = r.url.split('?').next().unwrap_or("");
        let path = no_q.strip_prefix("http://localhost:9090").unwrap_or(no_q);
        let body = match path {
            "/api/v1/query_range" => rb.clone(),
            "/api/v1/query" => ib.clone(),
            "/api/v1/query_exemplars" => eb.clone(),
            _ => return Err(TransportError { message: format!("no such path: {path}"), timeout: false }),
        };
        Ok(HttpResp { status: 200, headers: Vec::new(), body })
    };
    let ref_id = query.ref_id.clone();
    let res = execute(&instance(), std::slice::from_ref(&query), &t, false);
    let dr = res.into_iter().find(|r| r.0 == ref_id).expect("response for refId").1;
    dr.frames.unwrap_or_default()
}

/// Go `data.Labels.String()`: `k=v` sorted by key, joined by ", ".
fn labels_string(f: &Field) -> String {
    f.labels.as_ref().map(|l| l.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", ")).unwrap_or_default()
}
fn labels_len(f: &Field) -> usize {
    f.labels.as_ref().map(|l| l.len()).unwrap_or(0)
}
fn display_name_from_ds(f: &Field) -> String {
    f.config.as_ref().map(|c| c.display_name_from_ds.clone()).unwrap_or_default()
}
fn f64_at(f: &Field, i: usize) -> f64 {
    match &f.values {
        Values::F64(v) => v[i],
        other => panic!("field {} is not float64: {other:?}", f.name),
    }
}
fn time_at(f: &Field, i: usize) -> i64 {
    match &f.values {
        Values::Time(v) => v[i],
        other => panic!("field {} is not time: {other:?}", f.name),
    }
}

/// A query whose JSON is `json` over [from, to] (seconds).
fn query(json: Json, from: i64, to: i64) -> DataQuery {
    DataQuery { from: from * SEC, to: to * SEC, json, ..Default::default() }
}

// TestPrometheus_parseTimeSeriesResponse
#[test]
fn parse_ts_exemplars_response_should_be_sampled_and_parsed_normally() {
    let now_ms = grafana::util::now_ms();
    let ts = |ago: i64| -> String { format!("{}", (now_ms - ago * MIN / MS) as f64 / 1000.0) };
    let exemplars = format!(
        r#"[{{"seriesLabels":{{"__name__":"tns_request_duration_seconds_bucket","instance":"app:80","job":"tns/app"}},"exemplars":[{{"labels":{{"traceID":"test1"}},"value":"0.003535405","timestamp":{}}},{{"labels":{{"traceID":"test2"}},"value":"0.005555605","timestamp":{}}},{{"labels":{{"traceID":"test3"}},"value":"0.007545445","timestamp":{}}},{{"labels":{{"traceID":"test4"}},"value":"0.009545445","timestamp":{}}}]}}]"#,
        ts(2),
        ts(4),
        ts(6),
        ts(7)
    );
    let range = r#"{"resultType":"matrix","result":[{"metric":{"app":"Application","tag2":"tag2"},"values":[[1,"1"],[4,"4"],[7,"6"],[1.1,"8"]]}]}"#;
    let qm = Json::obj().with("utcOffsetSec", 0).with("legendFormat", "legend {{app}}").with("exemplar", true);
    let q = DataQuery { ref_id: "A".to_string(), json: qm, ..Default::default() };
    let res = run(q, Some(range), None, Some(&exemplars));

    assert_eq!(res.len(), 2);
    let exemplar_frame = res.iter().find(|f| f.name == "exemplar").expect("exemplar frame");
    let _range_frame = res.iter().find(|f| f.name != "exemplar").expect("range frame");
    assert_eq!(exemplar_frame.fields[0].name, "Time");
    assert_eq!(exemplar_frame.fields[1].name, "Value");
    assert_eq!(exemplar_frame.fields.len(), 6);

    assert_eq!(exemplar_frame.fields[1].len(), 4);
    assert_eq!(f64_at(&exemplar_frame.fields[1], 0), 0.009545445);
    assert_eq!(f64_at(&exemplar_frame.fields[1], 3), 0.003535405);
}

#[test]
fn parse_ts_matrix_response_should_be_parsed_normally() {
    let range = r#"{"resultType":"matrix","result":[{"metric":{"app":"Application","tag2":"tag2"},"values":[[1,"1"],[2,"2"],[3,"3"],[4,"4"],[5,"5"]]}]}"#;
    let qm = Json::obj().with("utcOffsetSec", 0).with("range", true).with("legendFormat", "legend {{app}}");
    let res = run(query(qm, 1, 5), Some(range), None, None);

    assert_eq!(res.len(), 1);
    assert_eq!(res[0].fields.len(), 2);
    assert_eq!(labels_len(&res[0].fields[0]), 0);
    assert_eq!(res[0].fields[0].name, "Time");
    assert_eq!(labels_len(&res[0].fields[1]), 2);
    assert_eq!(labels_string(&res[0].fields[1]), "app=Application, tag2=tag2");
    assert_eq!(display_name_from_ds(&res[0].fields[1]), "legend Application");
}

#[test]
fn parse_ts_matrix_response_with_missed_data_points_should_be_parsed_correctly() {
    let range = r#"{"resultType":"matrix","result":[{"metric":{"app":"Application","tag2":"tag2"},"values":[[1,"1"],[4,"4"]]}]}"#;
    let qm = Json::obj().with("utcOffsetSec", 0).with("range", true).with("legendFormat", "");
    let res = run(query(qm, 1, 4), Some(range), None, None);

    assert_eq!(res.len(), 1);
    assert_eq!(res[0].fields[0].len(), 2);
    assert_eq!(time_at(&res[0].fields[0], 0), SEC);
    assert_eq!(time_at(&res[0].fields[0], 1), 4 * SEC);
    assert_eq!(res[0].fields[1].len(), 2);
    assert_eq!(f64_at(&res[0].fields[1], 0), 1.0);
    assert_eq!(f64_at(&res[0].fields[1], 1), 4.0);
}

#[test]
fn parse_ts_matrix_response_with_from_alerting_missed_data_points_should_be_parsed_correctly() {
    let range = r#"{"resultType":"matrix","result":[{"metric":{"app":"Application","tag2":"tag2"},"values":[[1,"1"],[4,"4"]]}]}"#;
    let qm = Json::obj().with("utcOffsetSec", 0).with("range", true).with("legendFormat", "");
    let res = run(query(qm, 1, 4), Some(range), None, None);

    assert_eq!(res.len(), 1);
    assert_eq!(res[0].fields.len(), 2);
    assert_eq!(labels_len(&res[0].fields[0]), 0);
    assert_eq!(res[0].fields[0].name, "Time");
    assert_eq!(labels_len(&res[0].fields[1]), 2);
    assert_eq!(labels_string(&res[0].fields[1]), "app=Application, tag2=tag2");
    assert_eq!(display_name_from_ds(&res[0].fields[1]), r#"{app="Application", tag2="tag2"}"#);
}

#[test]
fn parse_ts_matrix_response_with_nan_value_should_be_changed_to_null() {
    let range = r#"{"resultType":"matrix","result":[{"metric":{"app":"Application"},"values":[[1,"NaN"]]}]}"#;
    let qm = Json::obj().with("utcOffsetSec", 0).with("range", true).with("legendFormat", "");
    let res = run(query(qm, 1, 4), Some(range), None, None);

    assert_eq!(display_name_from_ds(&res[0].fields[1]), r#"{app="Application"}"#);
    assert!(f64_at(&res[0].fields[1], 0).is_nan());
}

#[test]
fn parse_ts_vector_response_should_be_parsed_normally() {
    let instant = r#"{"resultType":"vector","result":[{"metric":{"app":"Application","tag2":"tag2"},"value":[0.123,"1"]}]}"#;
    let qm = Json::obj().with("utcOffsetSec", 0).with("instant", true).with("legendFormat", "legend {{app}}");
    let res = run(DataQuery { json: qm, ..Default::default() }, None, Some(instant), None);

    assert_eq!(res.len(), 1);
    assert_eq!(res[0].fields.len(), 2);
    assert_eq!(labels_len(&res[0].fields[0]), 0);
    assert_eq!(res[0].fields[0].name, "Time");
    assert_eq!(res[0].fields[0].name, "Time");
    assert_eq!(labels_len(&res[0].fields[1]), 2);
    assert_eq!(labels_string(&res[0].fields[1]), "app=Application, tag2=tag2");
    assert_eq!(display_name_from_ds(&res[0].fields[1]), "legend Application");
    assert_eq!(time_at(&res[0].fields[0], 0) / MS, 123);
}

#[test]
fn parse_ts_scalar_response_should_be_parsed_normally() {
    let instant = r#"{"resultType":"scalar","result":[0.123,"1"]}"#;
    let qm = Json::obj().with("utcOffsetSec", 0).with("instant", true).with("legendFormat", "");
    let res = run(DataQuery { json: qm, ..Default::default() }, None, Some(instant), None);

    assert_eq!(res.len(), 1);
    assert_eq!(res[0].fields.len(), 2);
    assert_eq!(labels_len(&res[0].fields[0]), 0);
    assert_eq!(res[0].fields[0].name, "Time");
    assert_eq!(res[0].fields[1].name, "Value");
    assert_eq!(f64_at(&res[0].fields[1], 0), 1.0);
    assert_eq!(time_at(&res[0].fields[0], 0) / MS, 123);
}

// TestPrometheus_executedQueryString
fn executed_query_string(interval_ms: i64, max_data_points: i64) -> String {
    let range = r#"{"resultType":"matrix","result":[{"metric":{"app":"Application"},"values":[[1,"1"],[2,"2"]]}]}"#;
    let qj = format!(r#"{{"expr": "test_metric", "format": "time_series", "intervalFactor": 1, "interval": "2m", "intervalMs": {interval_ms}, "maxDataPoints": {max_data_points}, "refId": "A", "range": true}}"#);
    let now = grafana::util::now_ms() * MS + 123_456;
    let q = DataQuery { ref_id: "A".to_string(), max_data_points, interval: interval_ms * MS, from: now, to: now + 48 * HOUR, json: Json::parse(&qj).expect("query json"), ..Default::default() };
    let res = run(q, Some(range), None, None);
    assert_eq!(res.len(), 1);
    res[0].meta.as_ref().expect("meta").executed_query_string.clone()
}

#[test]
fn executed_query_string_with_interval_ms_300_000() {
    assert_eq!(executed_query_string(300_000, 761), "Expr: test_metric\nStep: 5m0s");
}

#[test]
fn executed_query_string_with_interval_ms_900_000() {
    assert_eq!(executed_query_string(900_000, 175), "Expr: test_metric\nStep: 15m0s");
}
