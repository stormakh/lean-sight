# Lean Sight

A lean, compatible server for the Grafana 13.3.1 web UI, written in Rust. It serves the real, unchanged Grafana UI, from one process with no idle CPU.

**Real workload** (`bench/real-workload.html`): Node Exporter Full dashboard on live host metrics, 20 viewers for 30 minutes, side by side with Grafana 13.3.1 on the same machine:

| | Grafana 13.3.1 | Lean Sight | Better by |
|---|---|---|---|
| Memory, idle | 886 MB (15 processes) | 3.0 MB (1 process) | 298X |
| Memory, average under load | 962 MB | 31.9 MB | 30X |
| CPU under load | 23.6% of a core | 0.71% | 33X |
| Panel query latency, p95 | 32.5 ms | 11.8 ms | 3X |
| Dashboard open, p95 | 118 ms | 6 ms | 19X |
| Failed requests | 59 of 58,142 | 0 of 58,220 | |

**Frozen benchmark** (`bench/compare-2x.html`): 252X less idle memory, 72–302X less CPU per request. 162/165 recorded UI calls and 77/77 recorded API calls match real Grafana; 328/328 ported upstream Prometheus plugin tests pass.

In scope: login and users, dashboards, folders, search, annotations, data sources, and the Prometheus data source (rewritten in Rust). Not ported: alerting, expressions, other data source types, ad hoc filters on Prometheus queries. See `PORTING.md` for the full list.

Lean Sight is an independent project. It is not affiliated with or endorsed by Grafana Labs. Grafana is a trademark of Grafana Labs. License: AGPL-3.0, the same as Grafana.

## Run

```sh
cargo build --release -p server
GF_PATHS_HOME=/path/to/grafana-13.3.1 target/release/lean-sight   # port 3400, data in ./data, admin/admin
```

`GF_PATHS_HOME` must point at an unpacked Grafana 13.3.1 release: Lean Sight serves its `public/` folder and its plugin files.

## Check

```sh
cargo test --release -p grafana                 # ported promlib tests
node scripts/replay.mjs --seed tests-data/recordings/api-flows-v1.jsonl   # against a running Lean Sight
bench/baseline.sh <label> "<start command>" <url> # the frozen benchmark
```
