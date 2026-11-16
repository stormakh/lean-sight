//! Port of promlib `querydata/exemplar/sampler_stddev_test.go` and `sampler_noop_test.go` (golden files).
mod common;

use common::check_golden;
use grafana::promlib::exemplar::{Exemplar, Framer, NoOpSampler, Sampler, StdDevSampler};
use grafana::promlib::frame::DataResponse;
use grafana::promlib::models::SEC;

/// `generateTestExemplars`: one exemplar per second from start (inclusive) to end (exclusive), value = Unix seconds.
fn generate(start: i64, end: i64) -> Vec<Exemplar> {
    let mut out = Vec::new();
    let mut t = start;
    while t < end {
        out.push(Exemplar { series_labels: Vec::new(), source: usize::MAX, row_idx: 0, value: (t / SEC) as f64, timestamp: t });
        t += SEC;
    }
    out
}

/// `CheckGoldenJSONFramer` = `backend.FrameResponse(framer)`.
fn framer_response(sampler: &mut dyn Sampler) -> DataResponse {
    let frames = Framer::new().frames(sampler);
    DataResponse { frames: Some(frames), ..Default::default() }
}

#[test]
fn std_dev_sampler_standard_deviation_sampler() {
    let mut s = StdDevSampler::default();
    s.set_step(600 * SEC);
    for e in generate(0, 100000 * SEC) {
        s.add(e);
    }
    check_golden("exemplar/stddev_sampler.jsonc", &framer_response(&mut s));
}

#[test]
fn no_op_sampler_no_op_sampler() {
    let mut s = NoOpSampler::default();
    for e in generate(0, 2000 * SEC) {
        s.add(e);
    }
    check_golden("exemplar/noop_sampler.jsonc", &framer_response(&mut s));
}
