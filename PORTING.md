# Porting contract

This repo is a Rust rewrite of the Grafana 13.3.1 **server**. The web UI is the real, unchanged Grafana 13.3.1 frontend (`upstream-bin/grafana-13.3.1/public`), served by our server. `upstream/` is the Go and TypeScript source at tag v13.3.1 (commit 5db6ba88; the release binary reports build commit da3aa3ff). `upstream-prometheus/` is the Prometheus data-source plugin at v13.2.6 (commit 4911ab9), the version Grafana 13.3.1 installs; its backend is `pkg/promlib`. Both are read-only.

## Goals (frozen before any Rust code; see `bench/results/`)

- The real Grafana UI works against grafana-rs for every flow in `tests-data/recordings/ui-tour-v1.jsonl`: home, view a dashboard with Prometheus panels, dashboard list and search, data source list and edit, panel edit, Explore, profile, create and save a dashboard.
- At least 20X less memory (PSS of the whole process tree) than default Grafana, idle and under load.
- At least 30X less CPU per request than default Grafana on the `bench/load.sh` workloads, and 30X less idle CPU.
- Also report both numbers against the trimmed Grafana (`bench/grafana.sh trimmed`).

Frozen targets (from `bench/results/grafana-default-*`, 2026-11-16, fresh start, whole process tree PSS):

| Metric | Default Grafana | Trimmed Grafana | grafana-rs must reach (vs default) |
|---|---|---|---|
| Idle memory | 847 MB (15 procs) | 351 MB (2 procs) | <= 42 MB |
| Peak memory under `bench/load.sh` | 1063 MB | 545 MB | <= 53 MB |
| Idle CPU | 0.566% of a core | 0.346% | <= 0.019% |
| CPU per request: health / search / dashboard / ds_query | 1.25 / 23.6 / 25.9 / 37.8 ms | 1.12 / 23.9 / 25.9 / 37.6 ms | <= 0.042 / 0.79 / 0.86 / 1.26 ms |

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
- Concurrency: one OS thread per connection, 64 KB stack, at most 256 connections (more wait in accept), keep-alive with a 30 s idle timeout, blocking I/O. Bounded header size, body size, and upstream request time. No async runtime. No timers that wake when idle.

## Rules

- **The recording is the contract.** For each route, match the status code, the JSON keys and their order, value types, and the values the UI reads. Read the Go handler in `upstream/pkg/...` for behavior the recording does not show (errors, permissions, edge cases), and port that behavior too.
- Keep Grafana's error messages, status codes, and JSON field names. The UI shows them.
- Never weaken a check in `scripts/replay.mjs` to get a match. If a field cannot match (for example a timestamp or a random uid), add it to the named ignore list in `tests-data/replay-ignore.json` with the reason.
- Memory and CPU are the goal: no per-request `format!` on hot paths, no needless clones, stream large responses, keep idle work at zero (no timers that wake when nothing happens).
- No new crates without a real need. Write no automated tests other than the replay ledger.
- Keep each change focused. Do not run `cargo fmt` over files you do not own.

## Foundations already in place. Read these first

Built in phase 2. Keep these interfaces stable; small additions are fine, say so in your report.

### Run it

```sh
cargo build --release -p server                       # target/release/grafana-rs
GF_AUTH_ANONYMOUS_ENABLED=true GF_AUTH_ANONYMOUS_ORG_ROLE=Admin target/release/grafana-rs   # mode of ui-tour-v1
target/release/grafana-rs                             # anonymous off: mode of api-flows-v1 (login-v1)
```

- Defaults: port 3400, Grafana home `upstream-bin/grafana-13.3.1` (override: `GF_PATHS_HOME`), data dir `./data`
  (`GF_PATHS_DATA`), database `<data>/grafana-rs.db`. Delete `data/` for a fresh start; the first start creates
  org 1 "Main Org." and user admin/admin (`grafana::seed`).
- Settings: `<home>/conf/defaults.ini`, then `GF_RS_CONFIG` (or `<home>/conf/custom.ini`), then every
  `GF_<SECTION>_<KEY>` env var, as Grafana. `GF_RS_THREAD_STACK_KB` sets the connection thread stack (default 64;
  a debug build needs about 512). `GF_SERVER_ENABLE_GZIP=false` turns gzip off (it is on by default here).
- Release build, idle: PSS 3.1 MB, about 4 MB after 10 000 requests. Startup to the first `/api/health`: under 10 ms
  (46 ms on the very first start, which hashes the admin password).

### Crates

- `json`: `Json` enum `Null | Bool | Int(i64) | Float(f64) | Str | Arr(Vec<Json>) | Obj(Vec<(String, Json)>)`, keys
  in insertion order. Integer literals that fit i64 stay `Int` (int64 ids keep every digit); floats write as Go does
  (`1e+21`, `1e-7`, `0.00002`); strings escape `<`, `>`, `&`, U+2028/9 as Go's `json.Marshal`.
  Build: `Json::obj().with("id", 1).with("title", "x")`, `Json::arr().push_item(..)`, `set`, `push`, `remove`.
  Read: `get(k)`, `get_mut`, `at(i)`, `path("spec.panels.0.title")`, `as_str/as_i64/as_f64/as_bool/as_arr/as_obj`,
  `str_of(k)` / `i64_of(k)` / `bool_of(k)` (default "" / 0 / false). Text: `Json::parse(&str)`, `parse_bytes`,
  `stringify()`, `write(&mut String)`, `stringify_pretty("  ")` (Go `MarshalIndent`). Streaming helpers:
  `json::write_str`, `json::quote`, `json::write_i64`, `json::write_f64` (write JSON text without a tree on hot paths).
- `crypto`: `sha256`, `Sha256` (streaming), `HmacSha256::new(key).sign(d)`, `hmac_sha256`, `pbkdf2_sha256(pw, salt,
  iters, len)`, `encode_password(pw, salt)` (Grafana `util.EncodePassword`), `ct_eq`, `md5`, `sha1`, `crc32`,
  `aes_gcm_encrypt/decrypt`, `seal/open` (AES-256-GCM with a random nonce), `random_bytes`, `random_string(n, alphabet)`
  (`util.GetRandomString`), `random_hex(n)`, `hex`, `hex_decode`, `base64`, `base64url`, `base64_decode`.
- `http`: `Request { method, path (raw), query (raw), headers (lowercase names), body, remote_ip, params }` with
  `header(n)`, `param(n)`, `query_get(n)`, `query_all(n)`, `query_pairs()`, `cookie(n)`, `form()`, `json()`.
  `Response { status, headers, body: Body::Bytes | Body::File }` with `Response::json(status, &Json)`,
  `json_text(status, String)`, `text(status, content_type, body)`, `html`, `redirect(status, loc)` (Go's body),
  `with_header`, `set_header`, `set_cookie(&Cookie)`. `Router<T>`: patterns `/api/dashboards/uid/:uid`, `/public/*`,
  `/api/datasources/uid/:uid/resources/*rest`; the most literal segments win; HEAD matches GET. `Server` (thread per
  connection, 64 KB stack, 256 connections, 30 s keep-alive idle, 64 KB headers, 32 MB body, chunked bodies,
  `Expect: 100-continue`, gzip >= 1400 bytes for compressible types). `Client` (plain http, pool of 8 idle
  keep-alive connections, deadline per request): `app.client.request("POST", url, &[("Content-Type", "...")], body,
  Duration)` -> `ClientResponse { status, headers, body }`. `staticfile::serve(req, root, rel, cache_control)`.
  `util::{url_encode, url_decode, parse_query, http_date, parse_http_date, now_unix, civil_from_days}`.
- `grafana`: the app. `store-sqlite`: the Store on SQLite (cargo only). `server`: `main`, malloc tuning
  (`M_ARENA_MAX=2`, fixed mmap threshold, `malloc_trim` when the last connection closes, at most once a second).

### Add a route

Each area has `crates/grafana/src/api/<area>.rs` with `pub fn routes(r: &mut Routes)`, already called by
`api::register`: boot, auth, users (agent A); dashboards, folders, search (agent B); datasources, prometheus (agent C);
annotations, stubs (shared: add only your rows, keep functions in separate blocks). Health, metrics, and static files
are done (`api/health.rs`, `api/public.rs`).

```rust
use crate::ctx::{Access, Ctx, Role, Routes};
use crate::error::ApiError;
use http::Response;
use json::Json;

pub fn routes(r: &mut Routes) {
    r.get("/api/dashboards/uid/:uid", Access::SignedIn, get_dashboard);
    r.post("/api/dashboards/db", Access::Role(Role::Editor), post_dashboard);
}

fn get_dashboard(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let d = c.app.store.dashboard_by_uid(c.user.org_id, c.param("uid"))?; // StoreError::NotFound -> 404
    Ok(Response::json(200, &Json::obj().with("uid", d.uid)))
}
```

- `Handler = fn(&mut Ctx<'_>) -> Result<Response, ApiError>`. `Ctx { app: &App, req: Request (owned), user:
  SignedInUser }`; helpers `param`, `param_i64`, `body_json`, `require_role`.
- `Access`: `Public` (no auth: static, health), `Optional` (identify if possible, never reject: login page),
  `SignedIn` (`reqSignedIn`, anonymous allowed when enabled), `NoAnonymous`, `Role(Role::Editor | Role::Admin)`,
  `GrafanaAdmin`. Failures answer as Grafana's middleware: API paths (`/api*`) get the 401/403 JSON, pages get
  `302 /login?redirectTo=...` or `302 /`. Fine-grained RBAC checks go in the handler.
- `App` adds Grafana's default headers to every routed response (`Cache-Control: no-store` except cachable paths,
  `X-Content-Type-Options`, `X-Frame-Options`, `X-Xss-Protection`). No route: `/api*` -> 404 `{"message":"Not found"}`,
  other paths -> the index page with status 404.
- `ApiError`: `new(status, msg)` -> `{"message"}`; `with_trace` -> `{"message","traceID":""}` (Go `response.Error`
  with an err); `errutil(status, messageId, msg)` -> `{"message","messageId","statusCode","traceID"}`;
  `raw(status, Json)` for other shapes (Kubernetes `Status`); `not_found()`, `unauthorized()`, `forbidden()`,
  `bad_request(msg)`, `internal(msg)`. `?` converts `StoreError` (NotFound 404, Conflict 409, Other 500) and
  `JsonError` (400 "bad request data").
- `SignedInUser { user_id, uid, org_id, org_name, org_role: Role, login, name, email, theme, help_flags1,
  is_grafana_admin, is_anonymous, is_service_account, is_disabled, last_seen_at, auth: AuthMethod, session_id,
  api_key_id }`; `is_signed_in()`, `has_role(r)`, `identity()` ("user:1", "anonymous:0").
  `Role`: `None < Viewer < Editor < Admin`, `Role::parse("Editor")`, `as_str()`.
- Settings: `c.app.cfg` (`Settings`): typed fields (`app_url`, `app_sub_url`, `secret_key`, `login_cookie_name`,
  `anonymous_*`, `admin_*`, `build_version`, `build_commit`, ...) and `get(section, key)`, `get_bool`, `get_int`.
- Utilities: `grafana::util::{now, now_ms, rfc3339, parse_rfc3339, short_uid, valid_uid, slugify, html_escape}`;
  `grafana::secrets::{encrypt_map, decrypt_map, secure_fields}` for data source `secureJsonData`;
  `grafana::template::Template` (the Go-template subset of `public/views/index.html`, used by `api::boot::index`).

### Authentication (`grafana::auth`)

Order as upstream authn: `Authorization: Bearer glsa_...` (service account token, checked by CRC and
`EncodePassword(secret, checksum)` against `api_key.key`), `Authorization: Basic` (login or email + password;
`api_key:<token>` works too), the `grafana_session` cookie (sha256(token + secret_key) against `user_auth_token`,
revoked / expired / needs-rotation as upstream: rotation after `token_rotation_interval_minutes`, or 1 minute for a
token the client has not seen), then anonymous. `X-Grafana-Org-Id` / `orgId` pick another org the user belongs to.
For agent A: `auth::create_session(app, user_id, ip, user_agent) -> (token, row)`, `auth::session_cookie(app,
&token)`, `auth::clear_session_cookie(app)`, `auth::hash_token`, `auth::identity_for_user`.
Different from Grafana: successful basic-auth checks are cached for 60 s (keyed by a MAC of the credentials, valid
only while the stored password hash is unchanged), so basic auth costs one PBKDF2 per minute instead of per request;
there is no login-attempt rate limit yet; api key `last_used_at` is written at most once a minute.

### The Store

`app.store: Arc<dyn Store>`. `Store` is the sum of one trait per area in `crates/grafana/src/store/<area>.rs`
(`OrgStore`, `UserStore`, `AuthStore`, `PrefStore`, `StarStore`, `FolderStore`, `DashboardStore`, `DataSourceStore`,
`AnnotationStore`, `QueryHistoryStore`, `KvStore`, `TeamStore`, `MetaStore`), implemented in
`crates/store-sqlite/src/<area>.rs`. To add a method: add it to your area's trait and implement it in the same-named
sqlite file; nobody else edits those files. Rules:

- Every method is atomic (one statement or one transaction via `self.tx(|c| ...)`). If an API call needs several
  writes together, add one method for them (example: `dashboard_save` writes the dashboard row, a version row and the
  tags in one transaction).
- Times are unix seconds (`i64`); annotation epochs are milliseconds (`*_ms`). JSON columns (dashboard `data`, data
  source `json_data`, preferences `json_data`, query history `queries`) are JSON text; parse when needed.
- Text columns are NOT NULL with "" for none. The schema (`crates/store-sqlite/src/schema.sql`) uses Grafana's table and
  column names. Annotation tags are a JSON array in `annotation.tags` (no tag tables). Folders live only in `folder`
  (not as dashboard rows). To change the schema: edit `schema.sql`, bump `SCHEMA_VERSION` in `store-sqlite/src/lib.rs`
  (no migrations yet: delete `data/` after a schema change).
- One SQLite connection behind a mutex; use `prepare_cached` and `concat!`-built SQL on hot paths.

### Replay scorer

```sh
# terminal 1 (fresh data, mode of the recording)
rm -rf data && GF_AUTH_ANONYMOUS_ENABLED=true GF_AUTH_ANONYMOUS_ORG_ROLE=Admin target/release/grafana-rs
# terminal 2
node scripts/replay.mjs --seed                                   # ui-tour-v1, prints per-route results and SCORE
node scripts/replay.mjs --seed -v --only /api/datasources        # one area, every row
rm -rf data && target/release/grafana-rs   # then:  node scripts/replay.mjs --seed tests-data/recordings/api-flows-v1.jsonl
```

`--seed` runs `bench/seed.sh <target>` (data source `prom1` and dashboard `bench1`, as admin:admin, the same seed as the
recordings). Use `--prom http://localhost:9391` with `node scripts/prom-fixture.mjs` running, so data frames compare
exactly. The scorer is strict (see the top of `scripts/replay.mjs`): its own JSON parser keeps number text, key order
and duplicate keys; status, Content-Type and Location must match; HTML pages must match line by line, with the boot
data compared as JSON. Rules in `tests-data/replay-ignore.json` never accept "any value": each one is a typed check
(`time`, `uid`, `id`, `expect`, `sortBy`, `score`, `maxOf`, `cost`) with a reason, plus named `substitutions` (the
recording origin and Prometheus URL) and `browserHeaders` (for ui-tour-v1, recorded without User-Agent). Cookies from the
target's `Set-Cookie` are replayed on rows that had a cookie in the recording. Phase 2 score: ui-tour-v1 0/165, both files 1/242 (every area route is empty).

### Lust status

`lust check` accepts `json`, `crypto`, `http`, and `grafana` (`PATH=$HOME/.cargo/bin:$PATH` is needed: lust runs
`cargo metadata`, and the mise shim fails). Lust has no `Read::take`; use a loop. `store-sqlite` and `server` are
cargo only (rusqlite, `unsafe` for mallopt).

## Reporting back

Report: the routes you implemented, the replay score for your routes, every route or field that does not match and why, every place the behavior differs from Grafana, and what another agent must know. Report failures plainly.
