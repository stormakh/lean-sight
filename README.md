# grafana-rs

The Grafana 13.3.1 server, rewritten in Rust. It serves the real, unchanged Grafana web UI.

Measured against default Grafana 13.3.1 on the same machine (`bench/report.html`): 155X less idle memory (5.5 MB vs 848 MB), 137X less peak memory, 37–99X less CPU per request, no idle CPU. 162/165 recorded UI calls and 77/77 recorded API calls match real Grafana; 328/328 ported upstream Prometheus plugin tests pass.

In scope: login and users, dashboards, folders, search, annotations, data sources, and the Prometheus data source (rewritten in Rust). Not ported: alerting, expressions, other data source types, ad hoc filters on Prometheus queries. See `PORTING.md` and the report for the full list.

## Run

```sh
cargo build --release -p server
GF_PATHS_HOME=/path/to/grafana-13.3.1 target/release/grafana-rs   # port 3400, data in ./data, admin/admin
```

`GF_PATHS_HOME` must point at an unpacked Grafana 13.3.1 release: grafana-rs serves its `public/` folder and its plugin files.

## Check

```sh
cargo test --release -p grafana                 # ported promlib tests
node scripts/replay.mjs --seed tests-data/recordings/api-flows-v1.jsonl   # against a running grafana-rs
bench/baseline.sh <label> "<start command>" <url> # the frozen benchmark
```
