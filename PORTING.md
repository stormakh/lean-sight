# Porting contract

This repo is a Rust rewrite of the Grafana 13.3.1 **server**. The web UI is the real, unchanged Grafana 13.3.1 frontend (`upstream-bin/grafana-13.3.1/public`), served by our server. `upstream/` is the Go and TypeScript source at tag v13.3.1 (commit 5db6ba88; the release binary reports build commit da3aa3ff). `upstream-prometheus/` is the Prometheus data-source plugin at v13.2.6 (commit 4911ab9), the version Grafana 13.3.1 installs; its backend is `pkg/promlib`. Both are read-only.

## Goals (frozen before any Rust code; see `bench/results/`)

- The real Grafana UI works against grafana-rs for every flow in `tests-data/recordings/ui-tour-v1.jsonl`: home, view a dashboard with Prometheus panels, dashboard list and search, data source list and edit, panel edit, Explore, profile, create and save a dashboard.
- At least 20X less memory (PSS of the whole process tree) than default Grafana, idle and under load.
- At least 30X less CPU per request than default Grafana on the `bench/load.sh` workloads, and 30X less idle CPU.
- Also report both numbers against the trimmed Grafana (`bench/grafana.sh trimmed`).

## Scope

In scope: HTTP server and static files, index page with boot data, login and sessions (cookie `grafana_session`, basic auth, API keys/service account tokens), users, orgs, preferences, stars, teams (read only), folders, dashboards (classic v1 JSON, versions), search, data sources (CRUD, health, secure fields), the Prometheus data-source plugin backend in Rust (`/api/ds/query`, resources, health), annotations (store and query), query history, OFREP flags, `/api/health`, `/metrics`.

Out of scope, answered with a valid empty or "disabled" result so the UI keeps working: alerting, live (websocket), plugin catalog and installs, app plugins, Advisor, provisioning, quotas, reporting, correlations, library panels, public dashboards, the v2 dashboard schema. Each stub returns the same JSON shape as the recording.

The v2 dashboard schema is avoided by sending `dashboardNewLayouts: false` (OFREP and boot `featureToggles`). The UI then uses `/apis/dashboard.grafana.app/v1/...`, with the classic dashboard JSON as `spec`.

## Layout

```
crates/json       JSON value with ordered keys, parser, writer          (lust)
crates/crypto     sha256, hmac, pbkdf2, md5, aes-gcm, random (from baileys-rs soft.rs) (lust)
crates/http       blocking HTTP/1.1 server, router, request/response, gzip, HTTP client (lust)
crates/grafana    all Grafana logic: settings, boot data, auth, API handlers, the Store trait,
                  the Prometheus plugin                                   (lust)
crates/store-sqlite  Store implementation on SQLite (rusqlite, bundled)   (cargo only)
crates/server     the `grafana-rs` binary: config, wiring, main            (cargo only)
```

## Commands

```sh
cargo build --release -p server                 # the binary: target/release/grafana-rs
LUST_DAEMON_IDLE=600 lust check crates/grafana/src/lib.rs --manifest-path crates/grafana/Cargo.toml --lib
node scripts/replay.mjs                         # replay recorded UI calls, print SCORE matched/total
bench/load.sh <pid> http://localhost:3400 grafana-rs
```

`lust` must accept every `(lust)` crate after each change. Run it first, it is fast. `cargo` builds the shipped binary.

## Language limits for `(lust)` crates

- No `async`, no proc macros, no serde, no `unsafe`, edition 2024. Only std and crates with a lust adapter (`~/code/baileys-rs/lust-lang/library/deps`: flate2, indexmap, jiff, memchr, smallvec, rustc-hash, ...).
- No `dyn` of std traits other than `Fn`, `Any`, `Error`, `Debug`, `Display`. Our own traits (`Store`) as `dyn` are fine.
- Lust has a stricter lifetime model than rustc. If it rejects a closure that keeps a borrowed argument, use a plain loop.
- Concurrency: a fixed pool of OS threads (default 4) and blocking I/O. No async runtime.

## Rules

- **The recording is the contract.** For each route, match the status code, the JSON keys and their order, value types, and the values the UI reads. Read the Go handler in `upstream/pkg/...` for behavior the recording does not show (errors, permissions, edge cases), and port that behavior too.
- Keep Grafana's error messages, status codes, and JSON field names. The UI shows them.
- Never weaken a check in `scripts/replay.mjs` to get a match. If a field cannot match (for example a timestamp or a random uid), add it to the named ignore list in `tests-data/replay-ignore.json` with the reason.
- Memory and CPU are the goal: no per-request `format!` on hot paths, no needless clones, stream large responses, keep idle work at zero (no timers that wake when nothing happens).
- No new crates without a real need. Write no automated tests other than the replay ledger.
- Keep each change focused. Do not run `cargo fmt` over files you do not own.

## Foundations already in place. Read these first

(filled in by the lead after phase 2)

## Reporting back

Report: the routes you implemented, the replay score for your routes, every route or field that does not match and why, every place the behavior differs from Grafana, and what another agent must know. Report failures plainly.
