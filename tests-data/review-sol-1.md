Reading additional input from stdin...
OpenAI Codex v0.163.0
--------
workdir: /home/kamrot/code/grafana-rs
model: gpt-6.1-sol
provider: openai
approval: never
sandbox: read-only
reasoning effort: high
reasoning summaries: none
session id: 01a1e551-c526-7b10-9fb3-2281928205d2
--------
user
Independent review of grafana-rs (Rust rewrite of the Grafana 13.3.1 server; read PORTING.md and ~/code/baileys-rs/PORTING-PLAYBOOK.md phase 6). Be adversarial; do not trust agent claims. Check: (1) tests-data/replay-ignore.json: list every rule that is broader than needed or hides a real mismatch (e.g. ignoring all values of a field, 'any' mode, wildcard routes); for each, say what real bug it could hide and the narrower rule. Note OFREP flags.*.value uses mode any because Go map order is random: is a sorted-by-key comparison possible in scripts/replay.mjs instead? (2) scripts/replay.mjs: any comparison logic that is weaker than PORTING.md says (key order, types, null vs missing, array order, HTML boot data comparison, skipped rows). (3) crates/grafana/tests/promlib_*.rs vs the upstream Go tests in upstream-prometheus/pkg/promlib: find tests that were weakened, changed inputs/expected values, or only check setup; and check tests-data/not-ported.md reasons. Sample at least 25 tests across files. (4) Any place where the implementation special-cases recorded values (uids like bench1/prom1/gsshcd, fixed ids, ports) to pass replay. Cite file:line. Under 900 words. State confidence.
hook: SessionStart
hook: SessionStart Completed
hook: UserPromptSubmit
hook: UserPromptSubmit Completed
codex
I’ll read the porting rules, inspect the replay comparison code and ignore rules, then compare at least 25 Rust tests with the Go tests. I’ll keep the review read-only and cite file lines.

hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "pwd; rg --files -g AGENTS.md -g PORTING.md -g 'promlib_*.rs' -g replay.mjs -g replay-ignore.json -g not-ported.md" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
/home/kamrot/code/grafana-rs
crates/grafana/tests/promlib_settings.rs
crates/grafana/tests/promlib_resource.rs
crates/grafana/tests/promlib_request.rs
crates/grafana/tests/promlib_querydata.rs
crates/grafana/tests/promlib_models.rs
crates/grafana/tests/promlib_interval.rs
crates/grafana/tests/promlib_exemplar.rs
crates/grafana/tests/promlib_converter.rs
crates/grafana/tests/promlib_client.rs
scripts/replay.mjs
PORTING.md
tests-data/not-ported.md
tests-data/replay-ignore.json

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "cat PORTING.md; sed -n '/[Pp]hase 6/,/[Pp]hase 7/p' /home/kamrot/code/baileys-rs/PORTING-PLAYBOOK.md" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
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
recordings), so it works once agents A to C have `POST /api/datasources` and `POST /api/dashboards/db`. The 302 of
`/logout` and `/profile` is compared as recorded (fetch does not follow redirects). Matching rules and ignore modes
(`value`, `any`, `map` for generated ids that later URLs use) are at the top of `scripts/replay.mjs`; add exceptions
to `tests-data/replay-ignore.json` with route, path and reason. Cookies from the target's `Set-Cookie` are replayed on
rows that had a cookie in the recording. Phase 2 score: ui-tour-v1 0/165, both files 1/242 (every area route is empty).

### Lust status

`lust check` accepts `json`, `crypto`, `http`, and `grafana` (`PATH=$HOME/.cargo/bin:$PATH` is needed: lust runs
`cargo metadata`, and the mise shim fails). Lust has no `Read::take`; use a loop. `store-sqlite` and `server` are
cargo only (rusqlite, `unsafe` for mallopt).

## Reporting back

Report: the routes you implemented, the replay score for your routes, every route or field that does not match and why, every place the behavior differs from Grafana, and what another agent must know. Report failures plainly.
## Phase 6: independent review

Get a review that is not anchored on your own reading: a second agent or a different model (here GPT Sol through `codex exec`, read-only). Ask it to compare each Rust test with its upstream test and find weakened or changed tests. It found tests that had become sequential or only checked setup, and those went into the report as caveats instead of being hidden.

## Phase 7: the real world

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc 'nl -ba tests-data/replay-ignore.json; nl -ba scripts/replay.mjs' in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
     1	{
     2	  "rules": [
     3	    {
     4	      "route": "POST /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
     5	      "path": "metadata.name",
     6	      "mode": "map",
     7	      "reason": "generated dashboard uid (generateName); later rows use it in URLs"
     8	    },
     9	    {
    10	      "route": "POST /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
    11	      "path": "metadata.uid",
    12	      "mode": "map",
    13	      "reason": "generated Kubernetes object uid"
    14	    },
    15	    {
    16	      "route": "POST /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
    17	      "path": "metadata.resourceVersion",
    18	      "mode": "value",
    19	      "reason": "write time in microseconds"
    20	    },
    21	    {
    22	      "route": "POST /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
    23	      "path": "metadata.creationTimestamp",
    24	      "mode": "value",
    25	      "reason": "write time"
    26	    },
    27	    {
    28	      "route": "POST /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
    29	      "path": [
    30	        "metadata",
    31	        "labels",
    32	        "grafana.app/deprecatedInternalID"
    33	      ],
    34	      "mode": "value",
    35	      "reason": "generated snowflake id"
    36	    },
    37	    {
    38	      "route": "POST /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
    39	      "path": "metadata.managedFields.*.time",
    40	      "mode": "value",
    41	      "reason": "write time"
    42	    },
    43	    {
    44	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards/*/dto",
    45	      "path": "metadata.uid",
    46	      "mode": "map",
    47	      "reason": "generated Kubernetes object uid"
    48	    },
    49	    {
    50	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards/*/dto",
    51	      "path": "metadata.resourceVersion",
    52	      "mode": "value",
    53	      "reason": "write time in microseconds"
    54	    },
    55	    {
    56	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards/*/dto",
    57	      "path": "metadata.creationTimestamp",
    58	      "mode": "value",
    59	      "reason": "write time"
    60	    },
    61	    {
    62	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards/*/dto",
    63	      "path": [
    64	        "metadata",
    65	        "labels",
    66	        "grafana.app/deprecatedInternalID"
    67	      ],
    68	      "mode": "value",
    69	      "reason": "generated snowflake id"
    70	    },
    71	    {
    72	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards/*/dto",
    73	      "path": "metadata.managedFields.*.time",
    74	      "mode": "value",
    75	      "reason": "write time"
    76	    },
    77	    {
    78	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards/*/dto",
    79	      "path": [
    80	        "metadata",
    81	        "annotations",
    82	        "grafana.app/createdBy"
    83	      ],
    84	      "mode": "value",
    85	      "reason": "user:<uid> of the seeding admin; the user uid is random per install"
    86	    },
    87	    {
    88	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards/*/dto",
    89	      "path": [
    90	        "metadata",
    91	        "annotations",
    92	        "grafana.app/updatedBy"
    93	      ],
    94	      "mode": "value",
    95	      "reason": "user:<uid> of the seeding admin; the user uid is random per install"
    96	    },
    97	    {
    98	      "route": "POST /api/query-history",
    99	      "path": "result.uid",
   100	      "mode": "map",
   101	      "reason": "generated query history uid"
   102	    },
   103	    {
   104	      "route": "POST /api/query-history",
   105	      "path": "result.createdAt",
   106	      "mode": "value",
   107	      "reason": "write time"
   108	    },
   109	    {
   110	      "route": "GET /apis/advisor.grafana.app/v0alpha1/namespaces/*/checktypes/*",
   111	      "path": "metadata.uid",
   112	      "mode": "value",
   113	      "reason": "generated Kubernetes object uid (grafana-rs serves the uid of a capture from real Grafana 13.3.1)"
   114	    },
   115	    {
   116	      "route": "GET /apis/advisor.grafana.app/v0alpha1/namespaces/*/checktypes/*",
   117	      "path": "metadata.resourceVersion",
   118	      "mode": "value",
   119	      "reason": "write time in microseconds when Grafana registered the check type at startup"
   120	    },
   121	    {
   122	      "route": "GET /apis/advisor.grafana.app/v0alpha1/namespaces/*/checktypes/*",
   123	      "path": "metadata.creationTimestamp",
   124	      "mode": "value",
   125	      "reason": "time when Grafana registered the check type at startup"
   126	    },
   127	    {
   128	      "route": "GET /apis/advisor.grafana.app/v0alpha1/namespaces/*/checktypes/*",
   129	      "path": "metadata.managedFields.*.time",
   130	      "mode": "value",
   131	      "reason": "time when Grafana registered the check type at startup"
   132	    },
   133	    {
   134	      "route": "POST /ofrep/v1/evaluate/flags",
   135	      "path": "flags.*.key",
   136	      "mode": "value",
   137	      "reason": "Go map order is random in Grafana, so the array order differs between calls"
   138	    },
   139	    {
   140	      "route": "POST /ofrep/v1/evaluate/flags",
   141	      "path": "flags.*.value",
   142	      "mode": "any",
   143	      "reason": "Go map order is random in Grafana, so the array order differs between calls"
   144	    },
   145	    {
   146	      "route": "GET /**",
   147	      "path": [
   148	        "settings",
   149	        "featureToggles",
   150	        "dashboardNewLayouts"
   151	      ],
   152	      "mode": "any",
   153	      "reason": "grafana-rs sends dashboardNewLayouts=false explicitly (classic v1 dashboards); Grafana lists only enabled toggles"
   154	    },
   155	    {
   156	      "route": "GET /api/frontend/settings",
   157	      "path": [
   158	        "featureToggles",
   159	        "dashboardNewLayouts"
   160	      ],
   161	      "mode": "any",
   162	      "reason": "grafana-rs sends dashboardNewLayouts=false explicitly (classic v1 dashboards); Grafana lists only enabled toggles"
   163	    },
   164	    {
   165	      "route": "GET /api/frontend/settings",
   166	      "path": "liveEnabled",
   167	      "mode": "value",
   168	      "reason": "live (websocket) is out of scope: grafana-rs reports it disabled so the UI does not retry /api/live/ws"
   169	    },
   170	    {
   171	      "route": "GET /api/frontend/settings",
   172	      "path": "appUrl",
   173	      "mode": "value",
   174	      "reason": "the recording server ran on another port (3300/3310)"
   175	    },
   176	    {
   177	      "route": "GET /api/frontend/settings",
   178	      "path": "pluginCatalogPreinstalledPlugins.*.id",
   179	      "mode": "value",
   180	      "reason": "Grafana builds this list from a Go map, so its order is random"
   181	    },
   182	    {
   183	      "route": "GET /api/user",
   184	      "path": "uid",
   185	      "mode": "map",
   186	      "reason": "random user uid per install"
   187	    },
   188	    {
   189	      "route": "GET /api/user",
   190	      "path": "updatedAt",
   191	      "mode": "value",
   192	      "reason": "time of the user's creation or last request"
   193	    },
   194	    {
   195	      "route": "GET /api/user",
   196	      "path": "createdAt",
   197	      "mode": "value",
   198	      "reason": "time of the user's creation or last request"
   199	    },
   200	    {
   201	      "route": "GET /api/org/users",
   202	      "path": "*.uid",
   203	      "mode": "map",
   204	      "reason": "random user uid per install"
   205	    },
   206	    {
   207	      "route": "GET /api/org/users",
   208	      "path": "*.lastSeenAt",
   209	      "mode": "value",
   210	      "reason": "time of the user's creation or last request"
   211	    },
   212	    {
   213	      "route": "GET /api/org/users",
   214	      "path": "*.created",
   215	      "mode": "value",
   216	      "reason": "time of the user's creation or last request"
   217	    },
   218	    {
   219	      "route": "GET /api/users/search",
   220	      "path": "users.*.uid",
   221	      "mode": "map",
   222	      "reason": "random user uid per install"
   223	    },
   224	    {
   225	      "route": "GET /api/users/search",
   226	      "path": "users.*.lastSeenAt",
   227	      "mode": "value",
   228	      "reason": "time of the user's creation or last request"
   229	    },
   230	    {
   231	      "route": "GET /api/users/search",
   232	      "path": "users.*.created",
   233	      "mode": "value",
   234	      "reason": "time of the user's creation or last request"
   235	    },
   236	    {
   237	      "route": "GET /apis/dashboard.grafana.app/v0alpha1/namespaces/*/search",
   238	      "path": "hits.*.score",
   239	      "mode": "value",
   240	      "reason": "bleve BM25 relevance score; grafana-rs scores by its own formula that keeps the same order"
   241	    },
   242	    {
   243	      "route": "GET /apis/dashboard.grafana.app/v0alpha1/namespaces/*/search",
   244	      "path": "maxScore",
   245	      "mode": "value",
   246	      "reason": "bleve BM25 relevance score"
   247	    },
   248	    {
   249	      "route": "GET /apis/dashboard.grafana.app/v0alpha1/namespaces/*/search",
   250	      "path": "queryCost",
   251	      "mode": "value",
   252	      "reason": "bleve query cost counter"
   253	    },
   254	    {
   255	      "route": "POST /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
   256	      "path": "metadata.managedFields.*.manager",
   257	      "mode": "value",
   258	      "reason": "the apiserver names the field manager after the User-Agent, which replay.mjs does not send"
   259	    },
   260	    {
   261	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards/*/dto",
   262	      "path": "metadata.managedFields.*.manager",
   263	      "mode": "value",
   264	      "reason": "the apiserver names the field manager after the User-Agent, which replay.mjs does not send"
   265	    },
   266	    {
   267	      "route": "POST /api/folders",
   268	      "path": "id",
   269	      "mode": "value",
   270	      "reason": "generated snowflake id (grafana.app/deprecatedInternalID)"
   271	    },
   272	    {
   273	      "route": "POST /api/folders",
   274	      "path": "created",
   275	      "mode": "value",
   276	      "reason": "write time"
   277	    },
   278	    {
   279	      "route": "POST /api/folders",
   280	      "path": "updated",
   281	      "mode": "value",
   282	      "reason": "write time"
   283	    },
   284	    {
   285	      "route": "GET /api/folders/*",
   286	      "path": "id",
   287	      "mode": "value",
   288	      "reason": "generated snowflake id (grafana.app/deprecatedInternalID)"
   289	    },
   290	    {
   291	      "route": "GET /api/folders/*",
   292	      "path": "created",
   293	      "mode": "value",
   294	      "reason": "write time"
   295	    },
   296	    {
   297	      "route": "GET /api/folders/*",
   298	      "path": "updated",
   299	      "mode": "value",
   300	      "reason": "write time"
   301	    },
   302	    {
   303	      "route": "GET /api/folders",
   304	      "path": "*.id",
   305	      "mode": "value",
   306	      "reason": "generated snowflake id (grafana.app/deprecatedInternalID)"
   307	    },
   308	    {
   309	      "route": "GET /apis/folder.grafana.app/v1/namespaces/*/folders/*",
   310	      "path": "metadata.uid",
   311	      "mode": "value",
   312	      "reason": "generated Kubernetes object uid"
   313	    },
   314	    {
   315	      "route": "GET /apis/folder.grafana.app/v1/namespaces/*/folders/*",
   316	      "path": "metadata.resourceVersion",
   317	      "mode": "value",
   318	      "reason": "write time in microseconds"
   319	    },
   320	    {
   321	      "route": "GET /apis/folder.grafana.app/v1/namespaces/*/folders/*",
   322	      "path": "metadata.creationTimestamp",
   323	      "mode": "value",
   324	      "reason": "write time"
   325	    },
   326	    {
   327	      "route": "GET /apis/folder.grafana.app/v1/namespaces/*/folders/*",
   328	      "path": [
   329	        "metadata",
   330	        "labels",
   331	        "grafana.app/deprecatedInternalID"
   332	      ],
   333	      "mode": "value",
   334	      "reason": "generated snowflake id (grafana.app/deprecatedInternalID)"
   335	    },
   336	    {
   337	      "route": "GET /apis/folder.grafana.app/v1/namespaces/*/folders/*",
   338	      "path": "metadata.managedFields.*.time",
   339	      "mode": "value",
   340	      "reason": "write time"
   341	    },
   342	    {
   343	      "route": "GET /apis/folder.grafana.app/v1/namespaces/*/folders/*",
   344	      "path": [
   345	        "metadata",
   346	        "annotations",
   347	        "grafana.app/createdBy"
   348	      ],
   349	      "mode": "value",
   350	      "reason": "user:<uid> of the writing user; the user uid is random per install"
   351	    },
   352	    {
   353	      "route": "GET /apis/folder.grafana.app/v1/namespaces/*/folders/*",
   354	      "path": [
   355	        "metadata",
   356	        "annotations",
   357	        "grafana.app/updatedBy"
   358	      ],
   359	      "mode": "value",
   360	      "reason": "user:<uid> of the writing user; the user uid is random per install"
   361	    },
   362	    {
   363	      "route": "POST /api/dashboards/db",
   364	      "path": "id",
   365	      "mode": "value",
   366	      "reason": "generated snowflake id (grafana.app/deprecatedInternalID)"
   367	    },
   368	    {
   369	      "route": "POST /api/dashboards/db",
   370	      "path": "uid",
   371	      "mode": "map",
   372	      "reason": "generated dashboard uid (generateName \"a\"); later rows use it"
   373	    },
   374	    {
   375	      "route": "GET /api/dashboards/uid/*",
   376	      "path": "dashboard.id",
   377	      "mode": "value",
   378	      "reason": "generated snowflake id (grafana.app/deprecatedInternalID)"
   379	    },
   380	    {
   381	      "route": "GET /api/dashboards/uid/*",
   382	      "path": "meta.folderId",
   383	      "mode": "value",
   384	      "reason": "generated snowflake id (grafana.app/deprecatedInternalID)"
   385	    },
   386	    {
   387	      "route": "GET /api/dashboards/uid/*",
   388	      "path": "meta.created",
   389	      "mode": "value",
   390	      "reason": "write time"
   391	    },
   392	    {
   393	      "route": "GET /api/dashboards/uid/*",
   394	      "path": "meta.updated",
   395	      "mode": "value",
   396	      "reason": "write time"
   397	    },
   398	    {
   399	      "route": "GET /api/dashboards/uid/*/versions",
   400	      "path": "versions.*.dashboardId",
   401	      "mode": "value",
   402	      "reason": "generated snowflake id (grafana.app/deprecatedInternalID)"
   403	    },
   404	    {
   405	      "route": "GET /api/dashboards/uid/*/versions",
   406	      "path": "versions.*.created",
   407	      "mode": "value",
   408	      "reason": "write time"
   409	    },
   410	    {
   411	      "route": "GET /api/dashboards/uid/*/versions/*",
   412	      "path": "dashboardId",
   413	      "mode": "value",
   414	      "reason": "generated snowflake id (grafana.app/deprecatedInternalID)"
   415	    },
   416	    {
   417	      "route": "GET /api/dashboards/uid/*/versions/*",
   418	      "path": "created",
   419	      "mode": "value",
   420	      "reason": "write time"
   421	    },
   422	    {
   423	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards/*",
   424	      "path": "metadata.uid",
   425	      "mode": "value",
   426	      "reason": "generated Kubernetes object uid"
   427	    },
   428	    {
   429	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards/*",
   430	      "path": "metadata.resourceVersion",
   431	      "mode": "value",
   432	      "reason": "write time in microseconds"
   433	    },
   434	    {
   435	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards/*",
   436	      "path": "metadata.creationTimestamp",
   437	      "mode": "value",
   438	      "reason": "write time"
   439	    },
   440	    {
   441	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards/*",
   442	      "path": [
   443	        "metadata",
   444	        "labels",
   445	        "grafana.app/deprecatedInternalID"
   446	      ],
   447	      "mode": "value",
   448	      "reason": "generated snowflake id (grafana.app/deprecatedInternalID)"
   449	    },
   450	    {
   451	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards/*",
   452	      "path": "metadata.managedFields.*.time",
   453	      "mode": "value",
   454	      "reason": "write time"
   455	    },
   456	    {
   457	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards/*",
   458	      "path": [
   459	        "metadata",
   460	        "annotations",
   461	        "grafana.app/createdBy"
   462	      ],
   463	      "mode": "value",
   464	      "reason": "user:<uid> of the writing user; the user uid is random per install"
   465	    },
   466	    {
   467	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards/*",
   468	      "path": [
   469	        "metadata",
   470	        "annotations",
   471	        "grafana.app/updatedBy"
   472	      ],
   473	      "mode": "value",
   474	      "reason": "user:<uid> of the writing user; the user uid is random per install"
   475	    },
   476	    {
   477	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards/*",
   478	      "path": [
   479	        "metadata",
   480	        "annotations",
   481	        "grafana.app/updatedTimestamp"
   482	      ],
   483	      "mode": "value",
   484	      "reason": "write time"
   485	    },
   486	    {
   487	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards/*/dto",
   488	      "path": [
   489	        "metadata",
   490	        "annotations",
   491	        "grafana.app/updatedTimestamp"
   492	      ],
   493	      "mode": "value",
   494	      "reason": "write time"
   495	    },
   496	    {
   497	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
   498	      "path": "metadata.resourceVersion",
   499	      "mode": "value",
   500	      "reason": "write time in microseconds"
   501	    },
   502	    {
   503	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
   504	      "path": "items.*.metadata.uid",
   505	      "mode": "value",
   506	      "reason": "generated Kubernetes object uid"
   507	    },
   508	    {
   509	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
   510	      "path": "items.*.metadata.resourceVersion",
   511	      "mode": "value",
   512	      "reason": "write time in microseconds"
   513	    },
   514	    {
   515	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
   516	      "path": "items.*.metadata.creationTimestamp",
   517	      "mode": "value",
   518	      "reason": "write time"
   519	    },
   520	    {
   521	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
   522	      "path": [
   523	        "items",
   524	        "*",
   525	        "metadata",
   526	        "labels",
   527	        "grafana.app/deprecatedInternalID"
   528	      ],
   529	      "mode": "value",
   530	      "reason": "generated snowflake id (grafana.app/deprecatedInternalID)"
   531	    },
   532	    {
   533	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
   534	      "path": "items.*.metadata.managedFields.*.time",
   535	      "mode": "value",
   536	      "reason": "write time"
   537	    },
   538	    {
   539	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
   540	      "path": [
   541	        "items",
   542	        "*",
   543	        "metadata",
   544	        "annotations",
   545	        "grafana.app/createdBy"
   546	      ],
   547	      "mode": "value",
   548	      "reason": "user:<uid> of the writing user; the user uid is random per install"
   549	    },
   550	    {
   551	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
   552	      "path": [
   553	        "items",
   554	        "*",
   555	        "metadata",
   556	        "annotations",
   557	        "grafana.app/updatedBy"
   558	      ],
   559	      "mode": "value",
   560	      "reason": "user:<uid> of the writing user; the user uid is random per install"
   561	    },
   562	    {
   563	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
   564	      "path": [
   565	        "items",
   566	        "*",
   567	        "metadata",
   568	        "annotations",
   569	        "grafana.app/updatedTimestamp"
   570	      ],
   571	      "mode": "value",
   572	      "reason": "write time"
   573	    },
   574	    {
   575	      "route": "GET /api/search",
   576	      "path": "*.id",
   577	      "mode": "value",
   578	      "reason": "generated snowflake id (grafana.app/deprecatedInternalID)"
   579	    },
   580	    {
   581	      "route": "GET /api/search",
   582	      "path": "*.folderId",
   583	      "mode": "value",
   584	      "reason": "generated snowflake id (grafana.app/deprecatedInternalID)"
   585	    },
   586	    {
   587	      "route": "GET /api/annotations",
   588	      "path": "*.dashboardId",
   589	      "mode": "value",
   590	      "reason": "generated snowflake id (grafana.app/deprecatedInternalID)"
   591	    },
   592	    {
   593	      "route": "GET /api/annotations",
   594	      "path": "*.created",
   595	      "mode": "value",
   596	      "reason": "write time"
   597	    },
   598	    {
   599	      "route": "GET /api/annotations",
   600	      "path": "*.updated",
   601	      "mode": "value",
   602	      "reason": "write time"
   603	    },
   604	    {
   605	      "route": "GET /api/annotations",
   606	      "path": "*.userUID",
   607	      "mode": "value",
   608	      "reason": "uid of the admin user; random per install"
   609	    },
   610	    {
   611	      "route": "GET /api/annotations",
   612	      "path": "*.avatarUrl",
   613	      "mode": "value",
   614	      "reason": "sha256 of the admin email; Grafana ships admin@localhost, the seed may differ"
   615	    },
   616	    {
   617	      "route": "GET /api/query-history",
   618	      "path": "result.queryHistory.*.createdAt",
   619	      "mode": "value",
   620	      "reason": "write time in unix seconds; differs on every run"
   621	    }
   622	  ]
   623	}
     1	// Replay scorer: replays recorded UI traffic against a server and compares each response with the recording.
     2	//
     3	// usage: node scripts/replay.mjs [--target http://localhost:3400] [--seed] [--prom <url>] [--verbose] [--only <substring>] [file.jsonl ...]
     4	//   --prom: the URL of the seeded data source (default http://localhost:9390; a fixture Prometheus for exact frames).
     5	//   default file: tests-data/recordings/ui-tour-v1.jsonl. Several files replay in the order given, sharing ids and cookies.
     6	//
     7	// The recordings were made with anonymous Admin access (bench/grafana.sh capture-v1), so start the target the same way:
     8	//   GF_AUTH_ANONYMOUS_ENABLED=true GF_AUTH_ANONYMOUS_ORG_ROLE=Admin target/release/grafana-rs
     9	// and seed it like the recording (--seed runs bench/seed.sh <target>): data source prom1, dashboard bench1.
    10	//
    11	// A row matches when the status is equal and the body matches:
    12	//   JSON: same value types, same object keys in the same order, equal values, recursively.
    13	//   HTML: the window.grafanaBootData parts (user, settings, navTree, assets) have the same structure
    14	//         (keys, key order, types, array lengths); values are not compared.
    15	//   other text: equal.
    16	// Exceptions live in tests-data/replay-ignore.json, each with a reason. Rule modes:
    17	// A rule: {route: "METHOD /path/with/*/or/**", path: "a.*.b" or ["a", "key.with.dots"], mode, reason}.
    18	//   "value": the value may differ, the type must not.          "any": the value and type may differ (or be missing).
    19	//   "map":   a generated id: the first recorded->actual pair is remembered and replaced in later URLs and bodies,
    20	//            and later comparisons of that recorded value expect the mapped value.
    21	// Cookies: the target's own Set-Cookie values are sent on rows that had a cookie in the recording (login flows).
    22	// UPGRADE rows (websocket) are skipped. Every other row counts, in scope or not.
    23	import fs from 'node:fs';
    24	import path from 'node:path';
    25	
    26	const root = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
    27	const args = process.argv.slice(2);
    28	let target = 'http://localhost:3400';
    29	let seed = false, verbose = false, only = '', promUrl = 'http://localhost:9390';
    30	const files = [];
    31	for (let i = 0; i < args.length; i++) {
    32	  const a = args[i];
    33	  if (a === '--target') target = args[++i];
    34	  else if (a === '--seed') seed = true;
    35	  else if (a === '--verbose' || a === '-v') verbose = true;
    36	  else if (a === '--only') only = args[++i];
    37	  else if (a === '--prom') promUrl = args[++i];
    38	  else files.push(a);
    39	}
    40	if (!files.length) files.push(path.join(root, 'tests-data/recordings/ui-tour-v1.jsonl'));
    41	target = target.replace(/\/$/, '');
    42	
    43	const ignoreFile = path.join(root, 'tests-data/replay-ignore.json');
    44	const rules = fs.existsSync(ignoreFile) ? JSON.parse(fs.readFileSync(ignoreFile, 'utf8')).rules || [] : [];
    45	for (const r of rules) {
    46	  if (!r.route || !r.path || !r.mode || !r.reason) throw new Error(`replay-ignore.json: rule needs route, path, mode, reason: ${JSON.stringify(r)}`);
    47	  if (!['value', 'any', 'map'].includes(r.mode)) throw new Error(`replay-ignore.json: bad mode ${r.mode}`);
    48	  const [m, p] = r.route.split(' ');
    49	  r._method = m;
    50	  r._re = new RegExp('^' + p.split('/').map((s) => (s === '**' ? '.*' : s === '*' ? '[^/]+' : s.replace(/[.+?^${}()|[\]\\]/g, '\\$&'))).join('/') + '$');
    51	  r._path = Array.isArray(r.path) ? r.path.map(String) : r.path.split('.');
    52	}
    53	
    54	// ---- id mapping ----
    55	const idMap = new Map(); // recorded string -> actual string
    56	function applyMap(s) {
    57	  if (typeof s !== 'string' || !idMap.size) return s;
    58	  for (const [from, to] of idMap) if (from.length >= 4) s = s.split(from).join(to);
    59	  return s;
    60	}
    61	
    62	// ---- cookies: the target's Set-Cookie values are sent on rows that carried a cookie in the recording ----
    63	const jar = new Map();
    64	function cookieFor(recorded) {
    65	  const names = recorded.split(';').map((p) => p.split('=')[0].trim());
    66	  const out = names.filter((n) => jar.has(n)).map((n) => `${n}=${jar.get(n)}`);
    67	  return out.length ? out.join('; ') : undefined;
    68	}
    69	
    70	// ---- helpers ----
    71	const routeKey = (method, url) => {
    72	  const p = url.split('?')[0];
    73	  const norm = p
    74	    .replace(/\/d\/[^/]+(\/[^/]+)?$/, '/d/:uid/:slug')
    75	    .replace(/(dashboards|folders|datasources\/uid|query-history)\/[A-Za-z0-9_-]{6,}(?=\/|$)/, '$1/:uid')
    76	    .replace(/\/plugins\/[a-z0-9-]+\/settings$/, '/plugins/:id/settings');
    77	  return `${method} ${norm}`;
    78	};
    79	const typeOf = (v) => (v === null ? 'null' : Array.isArray(v) ? 'array' : typeof v);
    80	
    81	function rulesFor(method, url) {
    82	  const p = url.split('?')[0];
    83	  return rules.filter((r) => (r._method === '*' || r._method === method) && r._re.test(p));
    84	}
    85	function ruleAt(rs, jpath) {
    86	  for (const r of rs) {
    87	    if (r._path.length !== jpath.length) continue;
    88	    let ok = true;
    89	    for (let i = 0; i < jpath.length; i++) if (r._path[i] !== '*' && r._path[i] !== String(jpath[i])) { ok = false; break; }
    90	    if (ok) return r;
    91	  }
    92	  return null;
    93	}
    94	
    95	// compares recorded `a` with actual `b`; returns the first difference as a string, or null
    96	function cmp(a, b, rs, jp, structOnly) {
    97	  const r = ruleAt(rs, jp);
    98	  const where = '$' + (jp.length ? '.' + jp.join('.') : '');
    99	  if (r && r.mode === 'any') return null;
   100	  const ta = typeOf(a), tb = typeOf(b);
   101	  if (r && r.mode === 'map') {
   102	    if (ta !== tb) return `${where}: type ${tb}, want ${ta}`;
   103	    const ka = String(a), kb = String(b);
   104	    if (idMap.has(ka)) return idMap.get(ka) === kb ? null : `${where}: ${JSON.stringify(b)}, want mapped ${JSON.stringify(idMap.get(ka))}`;
   105	    if (ka !== kb) idMap.set(ka, kb);
   106	    return null;
   107	  }
   108	  if (ta !== tb) return `${where}: type ${tb}, want ${ta}`;
   109	  if (ta === 'array') {
   110	    if (a.length !== b.length && !(r && r.mode === 'value')) return `${where}: length ${b.length}, want ${a.length}`;
   111	    const n = Math.min(a.length, b.length);
   112	    for (let i = 0; i < n; i++) { const d = cmp(a[i], b[i], rs, [...jp, i], structOnly); if (d) return d; }
   113	    return null;
   114	  }
   115	  if (ta === 'object') {
   116	    const ka = Object.keys(a), kb = Object.keys(b);
   117	    const missing = ka.filter((k) => !(k in b) && !(ruleAt(rs, [...jp, k])?.mode === 'any'));
   118	    if (missing.length) return `${where}: missing keys ${missing.slice(0, 5).join(',')}`;
   119	    const extra = kb.filter((k) => !(k in a) && !(ruleAt(rs, [...jp, k])?.mode === 'any'));
   120	    if (extra.length) return `${where}: extra keys ${extra.slice(0, 5).join(',')}`;
   121	    const oa = ka.filter((k) => k in b), ob = kb.filter((k) => k in a);
   122	    for (let i = 0; i < oa.length; i++) if (oa[i] !== ob[i]) return `${where}: key order ${ob.slice(0, 6).join(',')}..., want ${oa.slice(0, 6).join(',')}...`;
   123	    for (const k of oa) { const d = cmp(a[k], b[k], rs, [...jp, k], structOnly); if (d) return d; }
   124	    return null;
   125	  }
   126	  if (structOnly || (r && r.mode === 'value')) return null;
   127	  const am = typeof a === 'string' ? applyMap(a) : a;
   128	  if (am !== b) return `${where}: ${JSON.stringify(b)?.slice(0, 80)}, want ${JSON.stringify(am)?.slice(0, 80)}`;
   129	  return null;
   130	}
   131	
   132	function bootData(html) {
   133	  const out = {};
   134	  for (const k of ['user', 'settings', 'navTree', 'assets']) {
   135	    const m = html.match(new RegExp(`\\n\\s*${k}: (.*?),?\\n`));
   136	    if (!m) return null;
   137	    try { out[k] = JSON.parse(m[1].replace(/,$/, '')); } catch { return null; }
   138	  }
   139	  return out;
   140	}
   141	function parseJson(t) { try { return { ok: true, v: JSON.parse(t) }; } catch { return { ok: false }; } }
   142	
   143	async function call(method, url, headers, body) {
   144	  const ctrl = new AbortController();
   145	  const timer = setTimeout(() => ctrl.abort(), 15000);
   146	  try {
   147	    const res = await fetch(target + url, { method, headers, body: body === undefined || method === 'GET' || method === 'HEAD' ? undefined : body, redirect: 'manual', signal: ctrl.signal });
   148	    const text = await res.text();
   149	    for (const c of res.headers.getSetCookie?.() || []) {
   150	      const [nv, ...attrs] = c.split(';');
   151	      const i = nv.indexOf('=');
   152	      const name = nv.slice(0, i).trim(), value = nv.slice(i + 1).trim();
   153	      const gone = attrs.some((a) => /^\s*max-age=(0|-)/i.test(a)) || value === '';
   154	      if (gone) jar.delete(name); else jar.set(name, value);
   155	    }
   156	    return { status: res.status, ct: res.headers.get('content-type') || '', text };
   157	  } catch (e) {
   158	    return { status: 0, ct: '', text: String(e.cause?.code || e.message) };
   159	  } finally { clearTimeout(timer); }
   160	}
   161	
   162	// Seeds the target exactly as the recordings were seeded: bench/seed.sh (prom1 and bench1, as admin:admin).
   163	async function doSeed() {
   164	  const { spawnSync } = await import('node:child_process');
   165	  const r = spawnSync('bash', [path.join(root, 'bench/seed.sh'), target, promUrl], { encoding: 'utf8' });
   166	  console.log(`seed: bench/seed.sh ${target} -> exit ${r.status}${(r.stdout + r.stderr).trim() ? ': ' + (r.stdout + r.stderr).trim() : ''}`);
   167	  // ui-tour-v1 was recorded on the instance of ui-tour (v2), which had saved dashboard gsshcd "Recorded save"
   168	  if (!files.some((f) => path.basename(f).startsWith('ui-tour'))) return;
   169	  const h = { authorization: 'Basic ' + Buffer.from('admin:admin').toString('base64'), 'content-type': 'application/json' };
   170	  const g = await call('POST', '/api/dashboards/db', h, JSON.stringify({ dashboard: { uid: 'gsshcd', title: 'Recorded save', panels: [], schemaVersion: 42 } }));
   171	  console.log(`seed: POST /api/dashboards/db (gsshcd) -> ${g.status} ${g.text.slice(0, 120)}`);
   172	}
   173	
   174	// ---- main ----
   175	if (seed) await doSeed();
   176	const results = new Map(); // route -> {ok, total, fails: []}
   177	let matched = 0, total = 0, skipped = 0;
   178	for (const f of files) {
   179	  const rows = fs.readFileSync(f, 'utf8').split('\n').filter(Boolean).map((l) => JSON.parse(l));
   180	  for (const row of rows) {
   181	    if (row.method === 'UPGRADE') { skipped++; continue; }
   182	    if (only && !row.url.includes(only)) continue;
   183	    const url = applyMap(row.url);
   184	    const headers = {};
   185	    for (const [k, v] of Object.entries(row.reqHeaders || {})) {
   186	      if (v === undefined || k === 'authorization' || k === 'cookie' || k === 'user-agent') continue;
   187	      headers[k] = applyMap(v);
   188	    }
   189	    if (row.reqHeaders?.cookie) { const c = cookieFor(row.reqHeaders.cookie); if (c) headers.cookie = c; }
   190	    const res = await call(row.method, url, headers, row.reqBody === undefined ? undefined : applyMap(row.reqBody));
   191	    let diff = null;
   192	    if (res.status !== row.status) diff = `status ${res.status}, want ${row.status}${res.status === 0 ? ` (${res.text})` : ''}`;
   193	    else {
   194	      const rs = rulesFor(row.method, row.url.split('?')[0]);
   195	      const rct = row.resHeaders?.['content-type'] || '';
   196	      const body = row.resBody ?? '';
   197	      if (rct.includes('json')) {
   198	        const a = parseJson(body), b = parseJson(res.text);
   199	        if (!a.ok) diff = cmp(body, res.text, rs, [], false);
   200	        else if (!b.ok) diff = `body is not JSON: ${res.text.slice(0, 80)}`;
   201	        else diff = cmp(a.v, b.v, rs, [], false);
   202	      } else if (rct.includes('html') && body.includes('grafanaBootData')) {
   203	        const a = bootData(body), b = bootData(res.text);
   204	        if (!b) diff = 'no window.grafanaBootData in the page';
   205	        else diff = cmp(a, b, rs, [], true);
   206	      } else if (/^<\d+ bytes>$/.test(body)) {
   207	        diff = null; // binary in the recording: status only
   208	      } else {
   209	        diff = cmp(body, res.text, rs, [], false);
   210	      }
   211	    }
   212	    const key = routeKey(row.method, row.url);
   213	    const e = results.get(key) || { ok: 0, total: 0, fails: [] };
   214	    e.total++; total++;
   215	    if (!diff) { e.ok++; matched++; } else e.fails.push(`${row.url.slice(0, 90)}: ${diff}`);
   216	    results.set(key, e);
   217	    if (verbose) console.log(`${diff ? 'FAIL' : 'ok  '} ${row.method} ${row.url.slice(0, 100)}${diff ? '\n       ' + diff : ''}`);
   218	  }
   219	}
   220	console.log('');
   221	const keys = [...results.keys()].sort();
   222	for (const k of keys) {
   223	  const e = results.get(k);
   224	  console.log(`${e.ok === e.total ? 'PASS' : 'FAIL'} ${String(e.ok).padStart(3)}/${String(e.total).padEnd(3)} ${k}`);
   225	  if (e.fails.length) console.log(`        ${e.fails[0]}`);
   226	}
   227	console.log(`\n${skipped} websocket rows skipped`);
   228	console.log(`SCORE ${matched}/${total}`);

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "nl -ba tests-data/not-ported.md; rg --files upstream-prometheus/pkg/promlib | sort; wc -l crates/grafana/tests/promlib_*.rs; rg -n 'bench1|prom1|gsshcd|3400|3300|9090|record|replay|fixture' crates/grafana/src crates/store-sqlite/src crates/server/src" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
     1	# Upstream promlib tests not ported
     2	
     3	Source: `upstream-prometheus/pkg/promlib` (grafana-prometheus-datasource v13.2.6). Ported tests live in
     4	`crates/grafana/tests/promlib_*.rs` (one `#[test]` per upstream case) with the golden files copied to
     5	`tests-data/promlib/`. These upstream cases are not ported:
     6	
     7	| Upstream test | Cases | Reason |
     8	|---|---|---|
     9	| `models/query_test.go` TestParseWithAdhocFilters, TestParseWithScopes, TestParseWithScopesAndAdhocFilters, TestParseWithGroupByKeys, TestParseComplexScenariosWithFilters | 14 | They rewrite PromQL (`ApplyFiltersAndGroupBy`), which needs the Prometheus PromQL parser and printer. grafana-rs has no PromQL parser: a query with scopes, ad hoc filters or group-by keys gets an error response instead of an unfiltered result. |
    10	| `models/scope_test.go` (all) | 7 functions | Same: PromQL rewriting. |
    11	| `resource/range_modifiers_test.go`, `resource/resource_test.go` TestResource_GetSuggestions* | 5 functions | The `suggestions` resource extracts metric names with the PromQL parser. grafana-rs answers `suggestions` with 500 "Failed to call resource". |
    12	| `models/query_test.go` TestQueryTypeDefinitions | 1 | Go schema code generation (`schemabuilder`), nothing to port. |
    13	| `models/lenient_test.go` | 6 functions | Not in the ported set: they check the metrics and logs written for each lenient conversion (instrumentation that grafana-rs does not have). The lenient parsing itself is ported in `promlib::settings` and covered by `promlib_settings.rs`. |
    14	| `resource/search_test.go`, `utils/utils_test.go`, `resource/resource_test.go` (gzip, zstd, framing headers, HTTP boundary), `resource/transport_error_test.go` | 21 functions | They test the Go HTTP transport: streaming NDJSON search bodies, gzip and zstd decoding, context cancellation. grafana-rs asks Prometheus for plain bodies and buffers search answers. |
    15	| `admission_handler_test.go` (Test_ValidateAdmission, Test_MutateAdmission, Test_getBadRequest), `instrumentation/instrumentation_test.go` | 5 functions | Kubernetes admission hooks and Prometheus metrics of the plugin process; grafana-rs has neither. |
    16	| `client/transport_test.go`, `middleware/force_http_get_test.go`, `library_test.go`, `healthcheck_test.go`, `heuristics_test.go` | 5 functions | Plugin SDK wiring (instance manager, middleware objects, admission and conversion handlers, Prometheus metrics). Health check and heuristics behavior is covered by the replay rows `GET /api/datasources/uid/*/health`. |
    17	| `querydata/framing_bench_test.go`, `converter/prom_bench_test.go` | benchmarks | Benchmarks, not checks. |
    18	
    19	Golden files copied but not used by any upstream test: `converter/testdata/prom-exemplars.json`,
    20	`prom-exemplars-frame.jsonc`, `prom-exemplars-a-frame.json`, `prom-exemplars-b-frame.json`,
    21	`loki-streams-structured-metadata.json` and its frame (upstream's `files` list does not name them).
upstream-prometheus/pkg/promlib/CHANGELOG.md
upstream-prometheus/pkg/promlib/README.md
upstream-prometheus/pkg/promlib/admission_handler.go
upstream-prometheus/pkg/promlib/admission_handler_test.go
upstream-prometheus/pkg/promlib/client/client.go
upstream-prometheus/pkg/promlib/client/client_test.go
upstream-prometheus/pkg/promlib/client/transport.go
upstream-prometheus/pkg/promlib/client/transport_test.go
upstream-prometheus/pkg/promlib/conversion_handler.go
upstream-prometheus/pkg/promlib/converter/prom.go
upstream-prometheus/pkg/promlib/converter/prom_bench_test.go
upstream-prometheus/pkg/promlib/converter/prom_test.go
upstream-prometheus/pkg/promlib/converter/testdata/loki-streams-a-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/loki-streams-a.json
upstream-prometheus/pkg/promlib/converter/testdata/loki-streams-b-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/loki-streams-b.json
upstream-prometheus/pkg/promlib/converter/testdata/loki-streams-c-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/loki-streams-c.json
upstream-prometheus/pkg/promlib/converter/testdata/loki-streams-structured-metadata-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/loki-streams-structured-metadata.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-error-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-error.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-exemplars-a-frame.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-exemplars-a-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-exemplars-a.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-exemplars-b-frame.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-exemplars-b-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-exemplars-b.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-exemplars-diff-labels-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-exemplars-diff-labels.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-exemplars-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-exemplars.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-infos-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-infos-no-data-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-infos-no-data.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-infos.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-labels-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-labels.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-matrix-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-matrix-histogram-no-labels-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-matrix-histogram-no-labels.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-matrix-histogram-partitioned-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-matrix-histogram-partitioned.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-matrix-with-nans-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-matrix-with-nans.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-matrix.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-query-range-big-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-query-range-big.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-query-range-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-query-range.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-scalar-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-scalar.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-series-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-series.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-string-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-string.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-vector-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-vector-histogram-no-labels-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-vector-histogram-no-labels.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-vector.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-warnings-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-warnings-no-data-frame.jsonc
upstream-prometheus/pkg/promlib/converter/testdata/prom-warnings-no-data.json
upstream-prometheus/pkg/promlib/converter/testdata/prom-warnings.json
upstream-prometheus/pkg/promlib/go.mod
upstream-prometheus/pkg/promlib/go.sum
upstream-prometheus/pkg/promlib/healthcheck.go
upstream-prometheus/pkg/promlib/healthcheck_test.go
upstream-prometheus/pkg/promlib/heuristics.go
upstream-prometheus/pkg/promlib/heuristics_test.go
upstream-prometheus/pkg/promlib/instrumentation/instrumentation.go
upstream-prometheus/pkg/promlib/instrumentation/instrumentation_test.go
upstream-prometheus/pkg/promlib/intervalv2/intervalv2.go
upstream-prometheus/pkg/promlib/intervalv2/intervalv2_test.go
upstream-prometheus/pkg/promlib/library.go
upstream-prometheus/pkg/promlib/library_test.go
upstream-prometheus/pkg/promlib/middleware/custom_query_params.go
upstream-prometheus/pkg/promlib/middleware/custom_query_params_test.go
upstream-prometheus/pkg/promlib/middleware/force_http_get.go
upstream-prometheus/pkg/promlib/middleware/force_http_get_test.go
upstream-prometheus/pkg/promlib/models/lenient.go
upstream-prometheus/pkg/promlib/models/lenient_test.go
upstream-prometheus/pkg/promlib/models/prometheus_interval_test.go
upstream-prometheus/pkg/promlib/models/query.go
upstream-prometheus/pkg/promlib/models/query.panel.example.json
upstream-prometheus/pkg/promlib/models/query.panel.schema.json
upstream-prometheus/pkg/promlib/models/query.request.example.json
upstream-prometheus/pkg/promlib/models/query.request.schema.json
upstream-prometheus/pkg/promlib/models/query_test.go
upstream-prometheus/pkg/promlib/models/result.go
upstream-prometheus/pkg/promlib/models/scope.go
upstream-prometheus/pkg/promlib/models/scope_test.go
upstream-prometheus/pkg/promlib/models/settings.go
upstream-prometheus/pkg/promlib/models/settings_test.go
upstream-prometheus/pkg/promlib/models/v0alpha1/query.examples.json
upstream-prometheus/pkg/promlib/models/v0alpha1/query.types.json
upstream-prometheus/pkg/promlib/querydata/exemplar/framer.go
upstream-prometheus/pkg/promlib/querydata/exemplar/labels.go
upstream-prometheus/pkg/promlib/querydata/exemplar/sampler.go
upstream-prometheus/pkg/promlib/querydata/exemplar/sampler_stddev.go
upstream-prometheus/pkg/promlib/querydata/exemplar/sampler_stddev_test.go
upstream-prometheus/pkg/promlib/querydata/exemplar/sampler_test.go
upstream-prometheus/pkg/promlib/querydata/exemplar/testdata/noop_sampler.jsonc
upstream-prometheus/pkg/promlib/querydata/exemplar/testdata/stddev_sampler.jsonc
upstream-prometheus/pkg/promlib/querydata/framing_bench_test.go
upstream-prometheus/pkg/promlib/querydata/framing_test.go
upstream-prometheus/pkg/promlib/querydata/request.go
upstream-prometheus/pkg/promlib/querydata/request_test.go
upstream-prometheus/pkg/promlib/querydata/response.go
upstream-prometheus/pkg/promlib/querydata/response_test.go
upstream-prometheus/pkg/promlib/resource/allowlist.go
upstream-prometheus/pkg/promlib/resource/allowlist_test.go
upstream-prometheus/pkg/promlib/resource/range_modifiers_test.go
upstream-prometheus/pkg/promlib/resource/resource.go
upstream-prometheus/pkg/promlib/resource/resource_test.go
upstream-prometheus/pkg/promlib/resource/search.go
upstream-prometheus/pkg/promlib/resource/search_test.go
upstream-prometheus/pkg/promlib/resource/transport_error_test.go
upstream-prometheus/pkg/promlib/testdata/exemplar.query.json
upstream-prometheus/pkg/promlib/testdata/exemplar.result.golden.jsonc
upstream-prometheus/pkg/promlib/testdata/exemplar.result.json
upstream-prometheus/pkg/promlib/testdata/range_auto.query.json
upstream-prometheus/pkg/promlib/testdata/range_auto.result.golden.jsonc
upstream-prometheus/pkg/promlib/testdata/range_auto.result.json
upstream-prometheus/pkg/promlib/testdata/range_infinity.query.json
upstream-prometheus/pkg/promlib/testdata/range_infinity.result.golden.jsonc
upstream-prometheus/pkg/promlib/testdata/range_infinity.result.json
upstream-prometheus/pkg/promlib/testdata/range_missing.query.json
upstream-prometheus/pkg/promlib/testdata/range_missing.result.golden.jsonc
upstream-prometheus/pkg/promlib/testdata/range_missing.result.json
upstream-prometheus/pkg/promlib/testdata/range_nan.query.json
upstream-prometheus/pkg/promlib/testdata/range_nan.result.golden.jsonc
upstream-prometheus/pkg/promlib/testdata/range_nan.result.json
upstream-prometheus/pkg/promlib/testdata/range_simple.query.json
upstream-prometheus/pkg/promlib/testdata/range_simple.result.golden.jsonc
upstream-prometheus/pkg/promlib/testdata/range_simple.result.json
upstream-prometheus/pkg/promlib/utils/utils.go
upstream-prometheus/pkg/promlib/utils/utils_test.go
  242 crates/grafana/tests/promlib_client.rs
  258 crates/grafana/tests/promlib_converter.rs
   43 crates/grafana/tests/promlib_exemplar.rs
  282 crates/grafana/tests/promlib_interval.rs
  394 crates/grafana/tests/promlib_models.rs
  254 crates/grafana/tests/promlib_querydata.rs
  205 crates/grafana/tests/promlib_request.rs
  156 crates/grafana/tests/promlib_resource.rs
  171 crates/grafana/tests/promlib_settings.rs
 2005 total
crates/server/src/main.rs:4://! GF_SERVER_HTTP_PORT (default 3400 here), GF_SERVER_HTTP_ADDR, GF_AUTH_ANONYMOUS_ENABLED,
crates/server/src/main.rs:57:    let cfg = Settings::load(home, &[("server", "http_port", "3400"), ("paths", "data", DEFAULT_DATA)]);
crates/grafana/src/app.rs:85:                        // errutil body with "extra" (recorded in api-flows-v1: GET /api/user after logout).
crates/grafana/src/access.rs:196:/// Org role Admin, captured from Grafana 13.3.1 (anonymous Admin; equal to the ui-tour-v1 recording).
crates/grafana/src/access.rs:368:/// Added for a Grafana server admin (the api-flows-v1 recording of admin minus ADMIN).
crates/grafana/src/api/stubs.rs:1://! Out-of-scope routes answered with an empty or disabled result of the recorded shape (alerting, live, advisor,
crates/grafana/src/api/stubs.rs:74:/// The metrics are not recorded here; the body is only checked to be `{"events": [...]}` JSON as Grafana binds it.
crates/grafana/src/api/ofrep.rs:3://! values (`captured/ofrep-flags.json`, from the recording), then the `[feature_toggles]` settings (`enable = a,b` and
crates/grafana/src/api/captured/ofrep-flags.json:1:{"flags":[{"key":"authz.userPermissions","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesAuthInfoRedirect","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"dashboard.vectorSearch","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"dataviz.experimentalColorSchemes","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"features.legacyOverrideLookupBypass","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"datasources.queryGateway","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"cloudWatchBatchQueries","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"dashboardUndoRedo","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"sqlExpressionsColumnAutoComplete","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.publishLegacySQLEvents","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingIgnorePendingForNoDataAndError","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"useKubernetesShortURLsAPI","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"secretsKeeperUI","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"savedQueriesRBAC","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"lokiLabelNamesQueryApi","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"text.newFeatures","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"sseExpressionErrorIsolation","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"table.refreshNewFeatures","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"datasources.querier.newName","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.dashboardAutoGridDefault","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"pluginProxyPreserveTrailingSlash","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"reloadDashboardsOnParamsChange","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"heatmapNegativeLogBuckets","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"onlyStoreServiceAccountActionSets","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"plugins.useMTPlugins","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.thresholdsInterpolation","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"datasources.gatewayGuardrails","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"reportRenderBinding","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"dashboardTemplatesAssistantButton","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"authZGRPCServer","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertEnrichmentPreview","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesExternalGroupMappingsApi","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"datasource.useNewCRUDAPIs","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.frontendLegacyAPIHandling","value":"off","reason":"static provider evaluation result","variant":"default"},{"key":"influxqlStreamingParser","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"dashboards.libraryPanelRepeatFromServerResolution","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingAIGenTemplates","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"teamlbac.usageFromAppPlatform","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesAuthzRolesAndRoleBindingsRedirect","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesUsersRedirectNoFallback","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"appplugins.loadAppManifest","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.navigationInfoArchRefresh","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"live.runAPIServer","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"configurableSchedulerTick","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"crashDetection","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"lokiShardSplitting","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"useSessionStorageForRedirection","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"rolePickerDrawer","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"enableSCIM","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingAIAnalyzeCentralStateHistory","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesAuthzGlobalRolesApi","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingImportAlertmanagerAPI","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"cloudWatchCrossAccountQuerying","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingEnrichmentAssistantInvestigations","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"tempoAlerting","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"dashboardLevelTimeMacros","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"pyroscopeUTF8LabelNames","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"rawPrometheus.tableNg","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"awsAssumeRolePerDatasourceExternalId","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingUIUseFullyCompatBackendFilters","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesAggregatorCapTokenAuth","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.savedQueriesPage","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesAuthzResourcePermissionApis","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"enableExtensionsAdminPage","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"pieChartGradientColorScheme","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"appplugins.handleProxyRequests","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"plugins.marketplaceLicensing","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alerting.stateManagerRequireWarm","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.pluginPathNesting","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.recordedQueriesDisableCreate","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"feedbackButton","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"alerting.manualAssistantInvestigation","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"pluginsAutoUpdate","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertEnrichmentConditional","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"graphiteBackendMode","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.enableScopesFirstMode","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"disableSSEDataplane","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"queryHistory.localOnly","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"enableScopesInMetricsExplore","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"teamHttpHeadersFromAppPlatform","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.vectorSearchCmdk","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"queryServiceRewrite","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"playlistsRBAC","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"datasourceConnectionsTab","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"panelTimeSettings","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"awsAsyncQueryCaching","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"datasources.queryTypes","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"useNewAPIsForDatasourceCRUD","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"sqlExpressions","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"vizActionsAuth","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"multiTenantTempCredentials","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertEnrichment","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesUsersReadApi","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"publicDashboardsEmailSharing","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"externalSnapshotsK8SAPIPush","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"zanzanaNoLegacyClient","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"datasources.config.ui.useNewDatasourceCRUDAPIs","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alerting.rulesAPIV2","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"reporting.redirectReportSettingsToK8SApi","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.dashboardsAutoHeightPanels","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"faroDatasourceSelector","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"reporting.pdfTablesFrontend","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"secretsManagementAppPlatformUI","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"exploreLogsShardSplitting","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"groupAttributeSync","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"azureMonitorEnableUserAuth","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"datasources.azureMonitorBatchAPI","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesAuthzServiceAccountResourcePermissions","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.useRouterMiddleware","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"lokiRunQueriesInParallel","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alerting.dataSourceManagedRouteProxy","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"infinityRunQueriesInParallel","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesAuthzRolesApi","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"awsDatasourcesHttpProxy","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"smoothingTransformation","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"datasourcesApiServerEnableHealthEndpointRedirect","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertmanagerRemotePrimary","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"streamingForwardTeamHeadersTempo","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"assistant.fullscreenWorkspace","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.queryVarEditorRedesign","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"datasources.gatewayAuditPolicy","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alerting.ruleStatusSync","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.frontendLegacyFeatureToggleHandling","value":"off","reason":"static provider evaluation result","variant":"default"},{"key":"reportingHeaderSettings","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"snapshots.kubernetesSnapshots","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingUIUseBackendFilters","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"cloudWatchNewLabelParsing","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesAuthZResourcePermissionsRedirect","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesAuthzTeamLBACRuleApi","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"scopeApi","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"newDashboardWithFiltersAndGroupBy","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"newShareReportDrawer","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"iam.onlyStoreDatasourceActionSets","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"deletedFolderResourceCleanup","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.scenesFlickeringFix","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"frontendService.reducedBootDataAPI","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"externalSnapshotsSupportLegacyAPI","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"perPanelNonApplicableDrilldowns","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"azureResourcePickerUpdates","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"transformationsEmptyPlaceholder","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"logsTablePanelNG","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.cmdkHybridSearch","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"datasources.queryGatewaySlimAudit","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"mlExpressions","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingQueryOptimization","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"recentlyViewedDashboards","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingSyncDispatchTimer","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"enableColorblindSafePanelOptions","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alerting.hybridSearch","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"splunk.useLegacyResultsApi","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesFolderCascadeDelete","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"createAlertRuleFromPanel","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"groupByVariable","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"cloudWatchRoundUpEndTime","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"interactiveLearning","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"ttlPluginInstanceManager","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.dashboardVariablesMigration","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"splashScreen","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingRuleGroupSortByFolderFullpath","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"oauthRequireSubClaim","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingEnrichmentPerRule","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingPolicyRoutingSettings","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"inlineLogDetailsNoScrolls","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"querycaching.redirectToK8SApi","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alerting.syncExternalAlertmanager","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.logLevelInference","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.filterablePanels","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"secretsManagementAppPlatformAwsKeeper","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesUsersRedirect","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.multiTenantNavTree","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.exploreMetricsSidebar","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"authz.listFoldersViaSearch","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.extensionSidebarFloating","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"externalServiceAccounts","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.customDashboardTemplates","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"rememberUserOrgForSso","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"datasourcesApiServerEnableHealthEndpoint","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.preserveLoginTabTitle","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"reporting.redirectReportsToK8SApi","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.otelLogDetails","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"reporting.removeLogoUrlField","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"provisioningExport","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesCorrelations","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingSaveStatePeriodic","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingMigrationWizardUI","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"cdnPluginsLoadFirst","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesTeamsApi","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"querycaching.useInQueryService","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"reporting.anyPageReporting","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.dashboardPreviewMode","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"canvasPanelPanZoom","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"prometheusSpecialCharsInLabelValues","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"datasources.teamHttpHeadersWriteGuard","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"lokiQueryLimitsContext","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.dashboardGlobalVariables","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"queryWithAssistant","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"queryEditorNext","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafanaAPIServerWithExperimentalAPIs","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"dashboardLibrary","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"dashboardValidatorApp","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"suggestedDashboardsAssistantButton","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.starredFolders","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"pluginInstallAPISync","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"appplugins.registerAPIServer","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"cacheConfigUnifiedStorageMigration","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertmanagerRemoteSecondary","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"teamHttpHeadersTempo","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"favoriteDatasources","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"influxDBConfigValidation","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"table.autoColumnWidths","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"table.refresh","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.multiTenantUserPermissions","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"sseGroupByDatasource","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"dashboard.notebooks","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesUsersDeterministicUID","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.growthHomepage","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.geomapProtomapsBasemap","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"canvasPanelNesting","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"datasourcesRerouteLegacyCRUDAPIs","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"auditLoggingAppPlatform","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.savedQueriesSearch","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"appPlatformGrpcClientAuth","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"datasources.teamHttpHeadersFromAppPlatformMT","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"datasources.useNewStackInfoToSettingsCache","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"auth.tokenRotationGracePeriod","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.newPanelQueryErrorsUI","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"zanzana","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesAuthzZanzanaSync","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"lokiAlignedQuerySplitting","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesReporting","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"dashboardNewLayouts","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingSyncNotifiersApiMigration","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"plugins.initDataSourcesAsync","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alerting.folderHasRulesLabel","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"foldersAppPlatformAPI","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"alertmanagerRemoteSecondaryWithRemoteState","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesAuthInfoApi","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"flameGraph.tableNg","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alerting.notificationsAPIV1Beta1","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"datasourcesApiserverEnableResourceEndpointRedirect","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"disableScriptedDashboards","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"connectionsFilterSidebar","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesServiceAccountsApi","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"managedPluginsV2","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.customizableMegaMenu","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"assistant.dashboardPlanning","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"saml.gosaml2Provider","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"reportingFooterSettings","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertRuleRestore","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"unifiedNavbars","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.onDemandDiagnostics","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"renderAuthJWT","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"scopeFilters","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingAlertListPanelEnhancements","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alerting.ruleQuality","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"cujTracking","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"datasources.apiServerConnectToHostedGrafanaDatabases","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"influxdbRunQueriesInParallel","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.newTextPanel","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"pluginStoreServiceLoading","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.scopesDashboardsMegaMenu","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"dataviz.tabularNums","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"authz.useLegacyCheck","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingUseNewSimplifiedRoutingHashAlgorithm","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesAuthzDatasourceResourcePermissions","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesUsersApi","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.visualDesignRefresh","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"queryeditor.coauthoringUi","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"dashboard.recentlyDeletedViaTrash","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"reporting.legacyServiceUsesK8SClient","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"queryHistory.recentQueriesUI","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"prometheusAzureOverrideAudience","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingJiraIntegration","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingNotificationHistory","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.dedicatedGrafanaComProxyAPIToken","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"onlyStoreActionSets","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.panelEditNextFeedbackEvent","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"pluginsForceTls13","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"datasources.loadOpenAPI","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"reportingV2Layouts","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesAggregator","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"queryLibrary","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"azureMonitorPrometheusExemplars","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"exploreLogsAggregatedMetrics","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"elasticsearchImprovedParsing","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"profilesHeatmap","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"dashboards.publicDashboardBadgeFromApi","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.smtpSettingsPage","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingNavigationV2","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"excludeRedundantManagedPermissions","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.useDefaultScopesEndpoint","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"libraryElements.folderTreeViaSearch","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"disableNumericMetricsSortingInExpressions","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"assistant.dashboardPlanBuildingScreen","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.panelPluginTransformations","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"dashboardTemplates","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingPrometheusRulesPrimary","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"prometheusTypeMigration","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"cdnPluginsUrls","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"pluginInsights","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"plugins.useMTPluginSettings","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"zanzanaMergeUserPermissions","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafanaAssistantInProfilesDrilldown","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.correlationsSkipLegacy","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"provisioningFolderMetadata","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"datasources.chunkedQueryStreaming","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"datasourcesApiServerEnableProxyEndpoint","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingProvenanceLockWrites","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"logQLScope","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"scopeSearchAllLevels","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesServiceAccountTokensApi","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.globalHomePreference","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"provisioning.performance","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"datasourceLegacyIdApi","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"cloudRBACRoles","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"sqlExpressionsCodeMirror","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"azureMonitorDisableLogLimit","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"jaegerEnableGrpcEndpoint","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"search.perResourceFolderTree","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.dynamicTraceToLogs","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"queryService","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"fetchRulesUsingPost","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"azureMonitorLogsBuilderEditor","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.dashboardSettingsRedesign","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.pluginExtensionReactElementProps","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.mtFallback","value":{},"reason":"static provider evaluation result","variant":"default"},{"key":"jitterAlertRulesWithinGroups","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"suggestedDashboards","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingListViewV2","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"libraryelements.kubernetesLibraryPanels","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingDisableSendAlertsExternal","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"datasources.teamHttpHeadersFromAppPlatformST","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingRuleRecoverDeleted","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"otelLogsFormatting","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingTriage","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"analyticsFramework","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesQueryCaching","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"authz.legacyUserPermissions","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesAuthzRoleBindingsApi","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"lokiQuerySplitting","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"refreshTokenRequired","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingListViewV2PreviewToggle","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"preferLibraryPanelTitle","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"paneledit.buttonLabels","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"tableSharedCrosshair","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"stateTimeline.nameAboveBars","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafanaManagedRecordingRules","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"newClickhouseConfigPageDesign","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"globalDashboardVariables","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesExternalGroupMappingsRedirect","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesTeamSync","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.viewPanelPane","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"lokiExperimentalStreaming","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"faroSessionReplay","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"fetchRulesInCompactMode","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesSsoSettingsApi","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.unifiedDataSourcePicker","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"folderPickerCreatePermission","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"queryServiceWithConnections","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"annotationPermissionUpdate","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"useScopesNavigationEndpoint","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"metricsFromProfiles","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"libraryElementFolderUIDRepair","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"querycaching.enableConnectionsClient","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertingBacktesting","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesLogsDrilldown","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"newInfluxDSConfigPageDesign","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesTeamsRedirect","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alerting.gmaOnlyListView","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"assistant.dashboard.artifacts","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"live.keepHAPrefixInCloud","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"datasourcesApiServerEnableResourceEndpoint","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.savedDashboardViews","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.secretsReferenceValueUI","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"preserveDashboardStateWhenNavigating","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alertEnrichmentMultiStep","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"tracesDrilldownTimeSeeker","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"table.inspectDataTableNG","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"awsDatasourcesTempCredentials","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"kubernetesFolderCountsLegacyStorage","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.kubernetesAnnotationsClient","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"pdfTables","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"alerting.ruleReviews","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"clickHouseConfigValidation","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"assistant.frontend.tools.dashboardTemplates","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.meticulousAIMode","value":"off","reason":"static provider evaluation result","variant":"default"},{"key":"panelTitleSearch","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"lokiLogsDataplane","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"timeComparison","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"newSavedQueriesExperience","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"alerting.ruleQualityFix","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.ssoSettingsToMTSettings","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"table.paginationPageSize","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"datetime.useLuxon","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"reporting.legacySettingServiceUsesK8SClient","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.logDetailsDisplayedFieldControls","value":false,"reason":"static provider evaluation result","variant":"default"},{"key":"grafana.announcementBannerDismissible","value":true,"reason":"static provider evaluation result","variant":"default"},{"key":"dashboards.libraryPanelHorizontalRepeatFullWidth","value":false,"reason":"static provider evaluation result","variant":"default"}]}
crates/grafana/src/api/boot.rs:402:        .with("recordedQueries", Json::obj().with("enabled", cfg_bool(cfg, "recorded_queries", "enabled", true)))
crates/grafana/src/api/boot.rs:408:            Json::parse(r#"{"minInterval":"10s","stateHistory":{"backend":"annotations","prometheusMetricName":"GRAFANA_ALERTS"},"recordingRulesEnabled":true,"notificationHistoryEnabled":false,"alertStateHistoryBackend":"annotations"}"#).unwrap_or(Json::obj()),
crates/grafana/src/api/boot.rs:432:/// Preinstalled plugins of `[plugins] preinstall` (Grafana builds this list from a map; the order is the recording's).
crates/grafana/src/api/boot.rs:878:const W_DRILLDOWN: i64 = -3400;
crates/grafana/src/api/boot.rs:1006:    // Alerting (unified alerting enabled, navigation V2 as the recorded feature flags say)
crates/grafana/src/api/captured/discovery.json:1:{"groups":[{"name":"dashboard.grafana.app","versions":[{"groupVersion":"dashboard.grafana.app/v2","version":"v2"},{"groupVersion":"dashboard.grafana.app/v2beta1","version":"v2beta1"},{"groupVersion":"dashboard.grafana.app/v2alpha1","version":"v2alpha1"},{"groupVersion":"dashboard.grafana.app/v0alpha1","version":"v0alpha1"},{"groupVersion":"dashboard.grafana.app/v1","version":"v1"},{"groupVersion":"dashboard.grafana.app/v1beta1","version":"v1beta1"}],"preferredVersion":{"groupVersion":"dashboard.grafana.app/v2","version":"v2"}},{"name":"folder.grafana.app","versions":[{"groupVersion":"folder.grafana.app/v1","version":"v1"},{"groupVersion":"folder.grafana.app/v1beta1","version":"v1beta1"}],"preferredVersion":{"groupVersion":"folder.grafana.app/v1","version":"v1"}},{"name":"iam.grafana.app","versions":[{"groupVersion":"iam.grafana.app/v0alpha1","version":"v0alpha1"}],"preferredVersion":{"groupVersion":"iam.grafana.app/v0alpha1","version":"v0alpha1"}},{"name":"userstorage.grafana.app","versions":[{"groupVersion":"userstorage.grafana.app/v0alpha1","version":"v0alpha1"}],"preferredVersion":{"groupVersion":"userstorage.grafana.app/v0alpha1","version":"v0alpha1"}},{"name":"preferences.grafana.app","versions":[{"groupVersion":"preferences.grafana.app/v1","version":"v1"},{"groupVersion":"preferences.grafana.app/v1alpha1","version":"v1alpha1"}],"preferredVersion":{"groupVersion":"preferences.grafana.app/v1","version":"v1"}},{"name":"collections.grafana.app","versions":[{"groupVersion":"collections.grafana.app/v1alpha1","version":"v1alpha1"}],"preferredVersion":{"groupVersion":"collections.grafana.app/v1alpha1","version":"v1alpha1"}},{"name":"provisioning.grafana.app","versions":[{"groupVersion":"provisioning.grafana.app/v0alpha1","version":"v0alpha1"},{"groupVersion":"provisioning.grafana.app/v1beta1","version":"v1beta1"}],"preferredVersion":{"groupVersion":"provisioning.grafana.app/v0alpha1","version":"v0alpha1"}},{"name":"playlist.grafana.app","versions":[{"groupVersion":"playlist.grafana.app/v1","version":"v1"},{"groupVersion":"playlist.grafana.app/v0alpha1","version":"v0alpha1"}],"preferredVersion":{"groupVersion":"playlist.grafana.app/v1","version":"v1"}},{"name":"plugins.grafana.app","versions":[{"groupVersion":"plugins.grafana.app/v0alpha1","version":"v0alpha1"}],"preferredVersion":{"groupVersion":"plugins.grafana.app/v0alpha1","version":"v0alpha1"}},{"name":"example.grafana.app","versions":[{"groupVersion":"example.grafana.app/v1alpha1","version":"v1alpha1"},{"groupVersion":"example.grafana.app/v0alpha1","version":"v0alpha1"}],"preferredVersion":{"groupVersion":"example.grafana.app/v1alpha1","version":"v1alpha1"}},{"name":"quotas.grafana.app","versions":[],"preferredVersion":{"groupVersion":"quotas.grafana.app/v0alpha1","version":"v0alpha1"}},{"name":"shorturl.grafana.app","versions":[{"groupVersion":"shorturl.grafana.app/v1beta1","version":"v1beta1"}],"preferredVersion":{"groupVersion":"shorturl.grafana.app/v1beta1","version":"v1beta1"}},{"name":"rules.alerting.grafana.app","versions":[{"groupVersion":"rules.alerting.grafana.app/v1beta1","version":"v1beta1"},{"groupVersion":"rules.alerting.grafana.app/v0alpha1","version":"v0alpha1"}],"preferredVersion":{"groupVersion":"rules.alerting.grafana.app/v1beta1","version":"v1beta1"}},{"name":"notifications.alerting.grafana.app","versions":[{"groupVersion":"notifications.alerting.grafana.app/v1beta1","version":"v1beta1"},{"groupVersion":"notifications.alerting.grafana.app/v0alpha1","version":"v0alpha1"}],"preferredVersion":{"groupVersion":"notifications.alerting.grafana.app/v1beta1","version":"v1beta1"}},{"name":"advisor.grafana.app","versions":[{"groupVersion":"advisor.grafana.app/v0alpha1","version":"v0alpha1"}],"preferredVersion":{"groupVersion":"advisor.grafana.app/v0alpha1","version":"v0alpha1"}},{"name":"historian.alerting.grafana.app","versions":[{"groupVersion":"historian.alerting.grafana.app/v0alpha1","version":"v0alpha1"}],"preferredVersion":{"groupVersion":"historian.alerting.grafana.app/v0alpha1","version":"v0alpha1"}}],"resources":{"advisor.grafana.app/v0alpha1":[{"name":"checks","singularName":"check","namespaced":true,"kind":"Check","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"checks/status","singularName":"","namespaced":true,"kind":"Check","verbs":["get","patch","update"]},{"name":"checktypes","singularName":"checktype","namespaced":true,"kind":"CheckType","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"checktypes/status","singularName":"","namespaced":true,"kind":"CheckType","verbs":["get","patch","update"]}],"collections.grafana.app/v1alpha1":[{"name":"stars","singularName":"stars","namespaced":true,"kind":"Stars","verbs":["create","delete","get","list","patch","update"]},{"name":"stars/update","singularName":"","namespaced":true,"kind":"Stars","verbs":["delete","update"]}],"dashboard.grafana.app/v0alpha1":[{"name":"dashboards","singularName":"dashboard","namespaced":true,"kind":"Dashboard","verbs":["create","delete","get","list","patch","update"]},{"name":"dashboards/dto","singularName":"","namespaced":true,"kind":"DashboardWithAccessInfo","verbs":["get"]},{"name":"librarypanels","singularName":"librarypanel","namespaced":true,"kind":"LibraryPanel","verbs":["create","delete","get","list","patch","update"]},{"name":"snapshots","singularName":"snapshot","namespaced":true,"kind":"Snapshot","verbs":["delete","get","list"]},{"name":"snapshots/dashboard","singularName":"","namespaced":true,"kind":"Dashboard","verbs":["get"]},{"name":"snapshots/deletekey","singularName":"","namespaced":true,"kind":"DashboardSnapshotWithDeleteKey","verbs":["get"]}],"dashboard.grafana.app/v1beta1":[{"name":"dashboards","singularName":"dashboard","namespaced":true,"kind":"Dashboard","verbs":["create","delete","get","list","patch","update"]},{"name":"dashboards/dto","singularName":"","namespaced":true,"kind":"DashboardWithAccessInfo","verbs":["get"]}],"dashboard.grafana.app/v1":[{"name":"dashboards","singularName":"dashboard","namespaced":true,"kind":"Dashboard","verbs":["create","delete","get","list","patch","update"]},{"name":"dashboards/dto","singularName":"","namespaced":true,"kind":"DashboardWithAccessInfo","verbs":["get"]}],"dashboard.grafana.app/v2alpha1":[{"name":"dashboards","singularName":"dashboard","namespaced":true,"kind":"Dashboard","verbs":["create","delete","get","list","patch","update"]},{"name":"dashboards/dto","singularName":"","namespaced":true,"kind":"DashboardWithAccessInfo","verbs":["get"]}],"dashboard.grafana.app/v2beta1":[{"name":"dashboards","singularName":"dashboard","namespaced":true,"kind":"Dashboard","verbs":["create","delete","get","list","patch","update"]},{"name":"dashboards/dto","singularName":"","namespaced":true,"kind":"DashboardWithAccessInfo","verbs":["get"]},{"name":"notebooks","singularName":"notebook","namespaced":true,"kind":"Notebook","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"variables","singularName":"variable","namespaced":true,"kind":"Variable","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]}],"dashboard.grafana.app/v2":[{"name":"dashboards","singularName":"dashboard","namespaced":true,"kind":"Dashboard","verbs":["create","delete","get","list","patch","update"]},{"name":"dashboards/dto","singularName":"","namespaced":true,"kind":"DashboardWithAccessInfo","verbs":["get"]}],"example.grafana.app/v0alpha1":[{"name":"examples","singularName":"example","namespaced":true,"kind":"Example","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"examples/status","singularName":"","namespaced":true,"kind":"Example","verbs":["get","patch","update"]}],"example.grafana.app/v1alpha1":[{"name":"examples","singularName":"example","namespaced":true,"kind":"Example","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"examples/custom","singularName":"","namespaced":true,"kind":"Example","verbs":["get","patch","update"]},{"name":"examples/foo","singularName":"","namespaced":true,"kind":"ResourceCallOptions","verbs":["get"]},{"name":"examples/status","singularName":"","namespaced":true,"kind":"Example","verbs":["get","patch","update"]}],"folder.grafana.app/v1beta1":[{"name":"folders","singularName":"folder","namespaced":true,"kind":"Folder","verbs":["create","delete","get","list","patch","update"]},{"name":"folders/access","singularName":"","namespaced":true,"kind":"FolderAccessInfo","verbs":["get"]},{"name":"folders/children","singularName":"","namespaced":true,"kind":"FolderList","verbs":["get"]},{"name":"folders/counts","singularName":"","namespaced":true,"kind":"DescendantCounts","verbs":["get"]},{"name":"folders/parents","singularName":"","namespaced":true,"kind":"FolderInfoList","verbs":["get"]}],"folder.grafana.app/v1":[{"name":"folders","singularName":"folder","namespaced":true,"kind":"Folder","verbs":["create","delete","get","list","patch","update"]},{"name":"folders/access","singularName":"","namespaced":true,"kind":"FolderAccessInfo","verbs":["get"]},{"name":"folders/children","singularName":"","namespaced":true,"kind":"FolderList","verbs":["get"]},{"name":"folders/counts","singularName":"","namespaced":true,"kind":"DescendantCounts","verbs":["get"]},{"name":"folders/parents","singularName":"","namespaced":true,"kind":"FolderInfoList","verbs":["get"]}],"historian.alerting.grafana.app/v0alpha1":[{"name":"dummys","singularName":"dummy","namespaced":true,"kind":"Dummy","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"dummys/status","singularName":"","namespaced":true,"kind":"Dummy","verbs":["get","patch","update"]}],"iam.grafana.app/v0alpha1":[{"name":"teamlbacrules","singularName":"teamlbacrule","namespaced":true,"kind":"TeamLBACRule","verbs":["create","delete","get","list","patch","update","watch"]},{"name":"teamlbacrules/for-subject","singularName":"","namespaced":true,"kind":"GetTeamLBACRulesForSubjectRouteResponse","verbs":["get"]}],"notifications.alerting.grafana.app/v0alpha1":[{"name":"configs","singularName":"config","namespaced":true,"kind":"Config","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"configs/status","singularName":"","namespaced":true,"kind":"Config","verbs":["get","patch","update"]},{"name":"inhibitionrules","singularName":"inhibitionrule","namespaced":true,"kind":"InhibitionRule","verbs":["create","delete","deletecollection","get","list","patch","update"]},{"name":"receivers","singularName":"receiver","namespaced":true,"kind":"Receiver","verbs":["create","delete","deletecollection","get","list","patch","update"]},{"name":"receivers/test","singularName":"","namespaced":true,"kind":"ResourceCallOptions","verbs":["create"]},{"name":"routingtrees","singularName":"routingtree","namespaced":true,"kind":"RoutingTree","verbs":["create","delete","deletecollection","get","list","patch","update"]},{"name":"templategroups","singularName":"templategroup","namespaced":true,"kind":"TemplateGroup","verbs":["create","delete","deletecollection","get","list","patch","update"]},{"name":"timeintervals","singularName":"timeinterval","namespaced":true,"kind":"TimeInterval","verbs":["create","delete","deletecollection","get","list","patch","update"]}],"notifications.alerting.grafana.app/v1beta1":[{"name":"configs","singularName":"config","namespaced":true,"kind":"Config","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"configs/status","singularName":"","namespaced":true,"kind":"Config","verbs":["get","patch","update"]},{"name":"inhibitionrules","singularName":"inhibitionrule","namespaced":true,"kind":"InhibitionRule","verbs":["create","delete","deletecollection","get","list","patch","update"]},{"name":"receivers","singularName":"receiver","namespaced":true,"kind":"Receiver","verbs":["create","delete","deletecollection","get","list","patch","update"]},{"name":"receivers/test","singularName":"","namespaced":true,"kind":"ResourceCallOptions","verbs":["create"]},{"name":"routingtrees","singularName":"routingtree","namespaced":true,"kind":"RoutingTree","verbs":["create","delete","deletecollection","get","list","patch","update"]},{"name":"templategroups","singularName":"templategroup","namespaced":true,"kind":"TemplateGroup","verbs":["create","delete","deletecollection","get","list","patch","update"]},{"name":"timeintervals","singularName":"timeinterval","namespaced":true,"kind":"TimeInterval","verbs":["create","delete","deletecollection","get","list","patch","update"]}],"playlist.grafana.app/v0alpha1":[{"name":"playlists","singularName":"playlist","namespaced":true,"kind":"Playlist","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"playlists/status","singularName":"","namespaced":true,"kind":"Playlist","verbs":["get","patch","update"]}],"playlist.grafana.app/v1":[{"name":"playlists","singularName":"playlist","namespaced":true,"kind":"Playlist","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"playlists/status","singularName":"","namespaced":true,"kind":"Playlist","verbs":["get","patch","update"]}],"plugins.grafana.app/v0alpha1":[{"name":"metas","singularName":"meta","namespaced":true,"kind":"Meta","verbs":["get","list"]},{"name":"metas/status","singularName":"","namespaced":true,"kind":"Meta","verbs":["get","patch","update"]},{"name":"plugins","singularName":"plugin","namespaced":true,"kind":"Plugin","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"plugins/status","singularName":"","namespaced":true,"kind":"Plugin","verbs":["get","patch","update"]}],"preferences.grafana.app/v1alpha1":[{"name":"preferences","singularName":"preferences","namespaced":true,"kind":"Preferences","verbs":["create","delete","get","list","patch","update"]}],"preferences.grafana.app/v1":[{"name":"preferences","singularName":"preferences","namespaced":true,"kind":"Preferences","verbs":["create","delete","get","list","patch","update"]}],"provisioning.grafana.app/v0alpha1":[{"name":"connections","singularName":"connection","namespaced":true,"kind":"Connection","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"connections/authorize","singularName":"","namespaced":true,"kind":"ConnectionAuthorizeRequest","verbs":["create"]},{"name":"connections/repositories","singularName":"","namespaced":true,"kind":"ExternalRepositoryList","verbs":["get"]},{"name":"connections/status","singularName":"","namespaced":true,"kind":"Connection","verbs":["get","patch","update"]},{"name":"historicjobs","singularName":"historicjob","namespaced":true,"kind":"HistoricJob","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"jobs","singularName":"job","namespaced":true,"kind":"Job","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"repositories","singularName":"repository","namespaced":true,"kind":"Repository","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"repositories/files","singularName":"","namespaced":true,"kind":"ResourceWrapper","verbs":["create","delete","get","update"]},{"name":"repositories/history","singularName":"","namespaced":true,"kind":"HistoryList","verbs":["get"]},{"name":"repositories/jobs","singularName":"","namespaced":true,"kind":"Repository","verbs":["create","get"]},{"name":"repositories/refs","singularName":"","namespaced":true,"kind":"RefList","verbs":["get"]},{"name":"repositories/render","singularName":"","namespaced":true,"kind":"Repository","verbs":["get"]},{"name":"repositories/resources","singularName":"","namespaced":true,"kind":"ResourceList","verbs":["get"]},{"name":"repositories/status","singularName":"","namespaced":true,"kind":"Repository","verbs":["get","patch","update"]},{"name":"repositories/test","singularName":"","namespaced":true,"kind":"TestResults","verbs":["create"]},{"name":"repositories/webhook","singularName":"","namespaced":true,"kind":"WebhookResponse","verbs":["create"]}],"provisioning.grafana.app/v1beta1":[{"name":"connections","singularName":"connection","namespaced":true,"kind":"Connection","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"connections/authorize","singularName":"","namespaced":true,"kind":"ConnectionAuthorizeRequest","verbs":["create"]},{"name":"connections/repositories","singularName":"","namespaced":true,"kind":"ExternalRepositoryList","verbs":["get"]},{"name":"connections/status","singularName":"","namespaced":true,"kind":"Connection","verbs":["get","patch","update"]},{"name":"historicjobs","singularName":"historicjob","namespaced":true,"kind":"HistoricJob","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"jobs","singularName":"job","namespaced":true,"kind":"Job","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"repositories","singularName":"repository","namespaced":true,"kind":"Repository","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"repositories/files","singularName":"","namespaced":true,"kind":"ResourceWrapper","verbs":["create","delete","get","update"]},{"name":"repositories/history","singularName":"","namespaced":true,"kind":"HistoryList","verbs":["get"]},{"name":"repositories/jobs","singularName":"","namespaced":true,"kind":"Repository","verbs":["create","get"]},{"name":"repositories/refs","singularName":"","namespaced":true,"kind":"RefList","verbs":["get"]},{"name":"repositories/render","singularName":"","namespaced":true,"kind":"Repository","verbs":["get"]},{"name":"repositories/resources","singularName":"","namespaced":true,"kind":"ResourceList","verbs":["get"]},{"name":"repositories/status","singularName":"","namespaced":true,"kind":"Repository","verbs":["get","patch","update"]},{"name":"repositories/test","singularName":"","namespaced":true,"kind":"TestResults","verbs":["create"]},{"name":"repositories/webhook","singularName":"","namespaced":true,"kind":"WebhookResponse","verbs":["create"]}],"rules.alerting.grafana.app/v0alpha1":[{"name":"alertrules","singularName":"alertrule","namespaced":true,"kind":"AlertRule","verbs":["create","delete","get","list","patch","update"]},{"name":"alertrules/status","singularName":"","namespaced":true,"kind":"AlertRule","verbs":["get","patch","update"]},{"name":"configs","singularName":"config","namespaced":true,"kind":"Config","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"configs/status","singularName":"","namespaced":true,"kind":"Config","verbs":["get","patch","update"]},{"name":"recordingrules","singularName":"recordingrule","namespaced":true,"kind":"RecordingRule","verbs":["create","delete","deletecollection","get","list","patch","update"]},{"name":"recordingrules/status","singularName":"","namespaced":true,"kind":"RecordingRule","verbs":["get","patch","update"]},{"name":"rulesequences","singularName":"rulesequence","namespaced":true,"kind":"RuleSequence","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"rulesequences/status","singularName":"","namespaced":true,"kind":"RuleSequence","verbs":["get","patch","update"]}],"rules.alerting.grafana.app/v1beta1":[{"name":"alertrules","singularName":"alertrule","namespaced":true,"kind":"AlertRule","verbs":["create","delete","get","list","patch","update"]},{"name":"alertrules/status","singularName":"","namespaced":true,"kind":"AlertRule","verbs":["get","patch","update"]},{"name":"recordingrules","singularName":"recordingrule","namespaced":true,"kind":"RecordingRule","verbs":["create","delete","deletecollection","get","list","patch","update"]},{"name":"recordingrules/status","singularName":"","namespaced":true,"kind":"RecordingRule","verbs":["get","patch","update"]},{"name":"rulesequences","singularName":"rulesequence","namespaced":true,"kind":"RuleSequence","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"rulesequences/status","singularName":"","namespaced":true,"kind":"RuleSequence","verbs":["get","patch","update"]}],"shorturl.grafana.app/v1beta1":[{"name":"shorturls","singularName":"shorturl","namespaced":true,"kind":"ShortURL","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]},{"name":"shorturls/goto","singularName":"","namespaced":true,"kind":"ResourceCallOptions","verbs":["get"]},{"name":"shorturls/status","singularName":"","namespaced":true,"kind":"ShortURL","verbs":["get","patch","update"]}],"userstorage.grafana.app/v0alpha1":[{"name":"user-storage","singularName":"user-storage","namespaced":true,"kind":"UserStorage","verbs":["create","delete","deletecollection","get","list","patch","update","watch"]}]}}

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc 'nl -ba scripts/replay.mjs' in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
     1	// Replay scorer: replays recorded UI traffic against a server and compares each response with the recording.
     2	//
     3	// usage: node scripts/replay.mjs [--target http://localhost:3400] [--seed] [--prom <url>] [--verbose] [--only <substring>] [file.jsonl ...]
     4	//   --prom: the URL of the seeded data source (default http://localhost:9390; a fixture Prometheus for exact frames).
     5	//   default file: tests-data/recordings/ui-tour-v1.jsonl. Several files replay in the order given, sharing ids and cookies.
     6	//
     7	// The recordings were made with anonymous Admin access (bench/grafana.sh capture-v1), so start the target the same way:
     8	//   GF_AUTH_ANONYMOUS_ENABLED=true GF_AUTH_ANONYMOUS_ORG_ROLE=Admin target/release/grafana-rs
     9	// and seed it like the recording (--seed runs bench/seed.sh <target>): data source prom1, dashboard bench1.
    10	//
    11	// A row matches when the status is equal and the body matches:
    12	//   JSON: same value types, same object keys in the same order, equal values, recursively.
    13	//   HTML: the window.grafanaBootData parts (user, settings, navTree, assets) have the same structure
    14	//         (keys, key order, types, array lengths); values are not compared.
    15	//   other text: equal.
    16	// Exceptions live in tests-data/replay-ignore.json, each with a reason. Rule modes:
    17	// A rule: {route: "METHOD /path/with/*/or/**", path: "a.*.b" or ["a", "key.with.dots"], mode, reason}.
    18	//   "value": the value may differ, the type must not.          "any": the value and type may differ (or be missing).
    19	//   "map":   a generated id: the first recorded->actual pair is remembered and replaced in later URLs and bodies,
    20	//            and later comparisons of that recorded value expect the mapped value.
    21	// Cookies: the target's own Set-Cookie values are sent on rows that had a cookie in the recording (login flows).
    22	// UPGRADE rows (websocket) are skipped. Every other row counts, in scope or not.
    23	import fs from 'node:fs';
    24	import path from 'node:path';
    25	
    26	const root = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
    27	const args = process.argv.slice(2);
    28	let target = 'http://localhost:3400';
    29	let seed = false, verbose = false, only = '', promUrl = 'http://localhost:9390';
    30	const files = [];
    31	for (let i = 0; i < args.length; i++) {
    32	  const a = args[i];
    33	  if (a === '--target') target = args[++i];
    34	  else if (a === '--seed') seed = true;
    35	  else if (a === '--verbose' || a === '-v') verbose = true;
    36	  else if (a === '--only') only = args[++i];
    37	  else if (a === '--prom') promUrl = args[++i];
    38	  else files.push(a);
    39	}
    40	if (!files.length) files.push(path.join(root, 'tests-data/recordings/ui-tour-v1.jsonl'));
    41	target = target.replace(/\/$/, '');
    42	
    43	const ignoreFile = path.join(root, 'tests-data/replay-ignore.json');
    44	const rules = fs.existsSync(ignoreFile) ? JSON.parse(fs.readFileSync(ignoreFile, 'utf8')).rules || [] : [];
    45	for (const r of rules) {
    46	  if (!r.route || !r.path || !r.mode || !r.reason) throw new Error(`replay-ignore.json: rule needs route, path, mode, reason: ${JSON.stringify(r)}`);
    47	  if (!['value', 'any', 'map'].includes(r.mode)) throw new Error(`replay-ignore.json: bad mode ${r.mode}`);
    48	  const [m, p] = r.route.split(' ');
    49	  r._method = m;
    50	  r._re = new RegExp('^' + p.split('/').map((s) => (s === '**' ? '.*' : s === '*' ? '[^/]+' : s.replace(/[.+?^${}()|[\]\\]/g, '\\$&'))).join('/') + '$');
    51	  r._path = Array.isArray(r.path) ? r.path.map(String) : r.path.split('.');
    52	}
    53	
    54	// ---- id mapping ----
    55	const idMap = new Map(); // recorded string -> actual string
    56	function applyMap(s) {
    57	  if (typeof s !== 'string' || !idMap.size) return s;
    58	  for (const [from, to] of idMap) if (from.length >= 4) s = s.split(from).join(to);
    59	  return s;
    60	}
    61	
    62	// ---- cookies: the target's Set-Cookie values are sent on rows that carried a cookie in the recording ----
    63	const jar = new Map();
    64	function cookieFor(recorded) {
    65	  const names = recorded.split(';').map((p) => p.split('=')[0].trim());
    66	  const out = names.filter((n) => jar.has(n)).map((n) => `${n}=${jar.get(n)}`);
    67	  return out.length ? out.join('; ') : undefined;
    68	}
    69	
    70	// ---- helpers ----
    71	const routeKey = (method, url) => {
    72	  const p = url.split('?')[0];
    73	  const norm = p
    74	    .replace(/\/d\/[^/]+(\/[^/]+)?$/, '/d/:uid/:slug')
    75	    .replace(/(dashboards|folders|datasources\/uid|query-history)\/[A-Za-z0-9_-]{6,}(?=\/|$)/, '$1/:uid')
    76	    .replace(/\/plugins\/[a-z0-9-]+\/settings$/, '/plugins/:id/settings');
    77	  return `${method} ${norm}`;
    78	};
    79	const typeOf = (v) => (v === null ? 'null' : Array.isArray(v) ? 'array' : typeof v);
    80	
    81	function rulesFor(method, url) {
    82	  const p = url.split('?')[0];
    83	  return rules.filter((r) => (r._method === '*' || r._method === method) && r._re.test(p));
    84	}
    85	function ruleAt(rs, jpath) {
    86	  for (const r of rs) {
    87	    if (r._path.length !== jpath.length) continue;
    88	    let ok = true;
    89	    for (let i = 0; i < jpath.length; i++) if (r._path[i] !== '*' && r._path[i] !== String(jpath[i])) { ok = false; break; }
    90	    if (ok) return r;
    91	  }
    92	  return null;
    93	}
    94	
    95	// compares recorded `a` with actual `b`; returns the first difference as a string, or null
    96	function cmp(a, b, rs, jp, structOnly) {
    97	  const r = ruleAt(rs, jp);
    98	  const where = '$' + (jp.length ? '.' + jp.join('.') : '');
    99	  if (r && r.mode === 'any') return null;
   100	  const ta = typeOf(a), tb = typeOf(b);
   101	  if (r && r.mode === 'map') {
   102	    if (ta !== tb) return `${where}: type ${tb}, want ${ta}`;
   103	    const ka = String(a), kb = String(b);
   104	    if (idMap.has(ka)) return idMap.get(ka) === kb ? null : `${where}: ${JSON.stringify(b)}, want mapped ${JSON.stringify(idMap.get(ka))}`;
   105	    if (ka !== kb) idMap.set(ka, kb);
   106	    return null;
   107	  }
   108	  if (ta !== tb) return `${where}: type ${tb}, want ${ta}`;
   109	  if (ta === 'array') {
   110	    if (a.length !== b.length && !(r && r.mode === 'value')) return `${where}: length ${b.length}, want ${a.length}`;
   111	    const n = Math.min(a.length, b.length);
   112	    for (let i = 0; i < n; i++) { const d = cmp(a[i], b[i], rs, [...jp, i], structOnly); if (d) return d; }
   113	    return null;
   114	  }
   115	  if (ta === 'object') {
   116	    const ka = Object.keys(a), kb = Object.keys(b);
   117	    const missing = ka.filter((k) => !(k in b) && !(ruleAt(rs, [...jp, k])?.mode === 'any'));
   118	    if (missing.length) return `${where}: missing keys ${missing.slice(0, 5).join(',')}`;
   119	    const extra = kb.filter((k) => !(k in a) && !(ruleAt(rs, [...jp, k])?.mode === 'any'));
   120	    if (extra.length) return `${where}: extra keys ${extra.slice(0, 5).join(',')}`;
   121	    const oa = ka.filter((k) => k in b), ob = kb.filter((k) => k in a);
   122	    for (let i = 0; i < oa.length; i++) if (oa[i] !== ob[i]) return `${where}: key order ${ob.slice(0, 6).join(',')}..., want ${oa.slice(0, 6).join(',')}...`;
   123	    for (const k of oa) { const d = cmp(a[k], b[k], rs, [...jp, k], structOnly); if (d) return d; }
   124	    return null;
   125	  }
   126	  if (structOnly || (r && r.mode === 'value')) return null;
   127	  const am = typeof a === 'string' ? applyMap(a) : a;
   128	  if (am !== b) return `${where}: ${JSON.stringify(b)?.slice(0, 80)}, want ${JSON.stringify(am)?.slice(0, 80)}`;
   129	  return null;
   130	}
   131	
   132	function bootData(html) {
   133	  const out = {};
   134	  for (const k of ['user', 'settings', 'navTree', 'assets']) {
   135	    const m = html.match(new RegExp(`\\n\\s*${k}: (.*?),?\\n`));
   136	    if (!m) return null;
   137	    try { out[k] = JSON.parse(m[1].replace(/,$/, '')); } catch { return null; }
   138	  }
   139	  return out;
   140	}
   141	function parseJson(t) { try { return { ok: true, v: JSON.parse(t) }; } catch { return { ok: false }; } }
   142	
   143	async function call(method, url, headers, body) {
   144	  const ctrl = new AbortController();
   145	  const timer = setTimeout(() => ctrl.abort(), 15000);
   146	  try {
   147	    const res = await fetch(target + url, { method, headers, body: body === undefined || method === 'GET' || method === 'HEAD' ? undefined : body, redirect: 'manual', signal: ctrl.signal });
   148	    const text = await res.text();
   149	    for (const c of res.headers.getSetCookie?.() || []) {
   150	      const [nv, ...attrs] = c.split(';');
   151	      const i = nv.indexOf('=');
   152	      const name = nv.slice(0, i).trim(), value = nv.slice(i + 1).trim();
   153	      const gone = attrs.some((a) => /^\s*max-age=(0|-)/i.test(a)) || value === '';
   154	      if (gone) jar.delete(name); else jar.set(name, value);
   155	    }
   156	    return { status: res.status, ct: res.headers.get('content-type') || '', text };
   157	  } catch (e) {
   158	    return { status: 0, ct: '', text: String(e.cause?.code || e.message) };
   159	  } finally { clearTimeout(timer); }
   160	}
   161	
   162	// Seeds the target exactly as the recordings were seeded: bench/seed.sh (prom1 and bench1, as admin:admin).
   163	async function doSeed() {
   164	  const { spawnSync } = await import('node:child_process');
   165	  const r = spawnSync('bash', [path.join(root, 'bench/seed.sh'), target, promUrl], { encoding: 'utf8' });
   166	  console.log(`seed: bench/seed.sh ${target} -> exit ${r.status}${(r.stdout + r.stderr).trim() ? ': ' + (r.stdout + r.stderr).trim() : ''}`);
   167	  // ui-tour-v1 was recorded on the instance of ui-tour (v2), which had saved dashboard gsshcd "Recorded save"
   168	  if (!files.some((f) => path.basename(f).startsWith('ui-tour'))) return;
   169	  const h = { authorization: 'Basic ' + Buffer.from('admin:admin').toString('base64'), 'content-type': 'application/json' };
   170	  const g = await call('POST', '/api/dashboards/db', h, JSON.stringify({ dashboard: { uid: 'gsshcd', title: 'Recorded save', panels: [], schemaVersion: 42 } }));
   171	  console.log(`seed: POST /api/dashboards/db (gsshcd) -> ${g.status} ${g.text.slice(0, 120)}`);
   172	}
   173	
   174	// ---- main ----
   175	if (seed) await doSeed();
   176	const results = new Map(); // route -> {ok, total, fails: []}
   177	let matched = 0, total = 0, skipped = 0;
   178	for (const f of files) {
   179	  const rows = fs.readFileSync(f, 'utf8').split('\n').filter(Boolean).map((l) => JSON.parse(l));
   180	  for (const row of rows) {
   181	    if (row.method === 'UPGRADE') { skipped++; continue; }
   182	    if (only && !row.url.includes(only)) continue;
   183	    const url = applyMap(row.url);
   184	    const headers = {};
   185	    for (const [k, v] of Object.entries(row.reqHeaders || {})) {
   186	      if (v === undefined || k === 'authorization' || k === 'cookie' || k === 'user-agent') continue;
   187	      headers[k] = applyMap(v);
   188	    }
   189	    if (row.reqHeaders?.cookie) { const c = cookieFor(row.reqHeaders.cookie); if (c) headers.cookie = c; }
   190	    const res = await call(row.method, url, headers, row.reqBody === undefined ? undefined : applyMap(row.reqBody));
   191	    let diff = null;
   192	    if (res.status !== row.status) diff = `status ${res.status}, want ${row.status}${res.status === 0 ? ` (${res.text})` : ''}`;
   193	    else {
   194	      const rs = rulesFor(row.method, row.url.split('?')[0]);
   195	      const rct = row.resHeaders?.['content-type'] || '';
   196	      const body = row.resBody ?? '';
   197	      if (rct.includes('json')) {
   198	        const a = parseJson(body), b = parseJson(res.text);
   199	        if (!a.ok) diff = cmp(body, res.text, rs, [], false);
   200	        else if (!b.ok) diff = `body is not JSON: ${res.text.slice(0, 80)}`;
   201	        else diff = cmp(a.v, b.v, rs, [], false);
   202	      } else if (rct.includes('html') && body.includes('grafanaBootData')) {
   203	        const a = bootData(body), b = bootData(res.text);
   204	        if (!b) diff = 'no window.grafanaBootData in the page';
   205	        else diff = cmp(a, b, rs, [], true);
   206	      } else if (/^<\d+ bytes>$/.test(body)) {
   207	        diff = null; // binary in the recording: status only
   208	      } else {
   209	        diff = cmp(body, res.text, rs, [], false);
   210	      }
   211	    }
   212	    const key = routeKey(row.method, row.url);
   213	    const e = results.get(key) || { ok: 0, total: 0, fails: [] };
   214	    e.total++; total++;
   215	    if (!diff) { e.ok++; matched++; } else e.fails.push(`${row.url.slice(0, 90)}: ${diff}`);
   216	    results.set(key, e);
   217	    if (verbose) console.log(`${diff ? 'FAIL' : 'ok  '} ${row.method} ${row.url.slice(0, 100)}${diff ? '\n       ' + diff : ''}`);
   218	  }
   219	}
   220	console.log('');
   221	const keys = [...results.keys()].sort();
   222	for (const k of keys) {
   223	  const e = results.get(k);
   224	  console.log(`${e.ok === e.total ? 'PASS' : 'FAIL'} ${String(e.ok).padStart(3)}/${String(e.total).padEnd(3)} ${k}`);
   225	  if (e.fails.length) console.log(`        ${e.fails[0]}`);
   226	}
   227	console.log(`\n${skipped} websocket rows skipped`);
   228	console.log(`SCORE ${matched}/${total}`);

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "nl -ba tests-data/replay-ignore.json | sed -n '470,780p'; nl -ba tests-data/not-ported.md; wc -l crates/grafana/tests/promlib_*.rs; rg --files upstream-prometheus/pkg/promlib -g '*test.go'" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
   470	        "annotations",
   471	        "grafana.app/updatedBy"
   472	      ],
   473	      "mode": "value",
   474	      "reason": "user:<uid> of the writing user; the user uid is random per install"
   475	    },
   476	    {
   477	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards/*",
   478	      "path": [
   479	        "metadata",
   480	        "annotations",
   481	        "grafana.app/updatedTimestamp"
   482	      ],
   483	      "mode": "value",
   484	      "reason": "write time"
   485	    },
   486	    {
   487	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards/*/dto",
   488	      "path": [
   489	        "metadata",
   490	        "annotations",
   491	        "grafana.app/updatedTimestamp"
   492	      ],
   493	      "mode": "value",
   494	      "reason": "write time"
   495	    },
   496	    {
   497	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
   498	      "path": "metadata.resourceVersion",
   499	      "mode": "value",
   500	      "reason": "write time in microseconds"
   501	    },
   502	    {
   503	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
   504	      "path": "items.*.metadata.uid",
   505	      "mode": "value",
   506	      "reason": "generated Kubernetes object uid"
   507	    },
   508	    {
   509	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
   510	      "path": "items.*.metadata.resourceVersion",
   511	      "mode": "value",
   512	      "reason": "write time in microseconds"
   513	    },
   514	    {
   515	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
   516	      "path": "items.*.metadata.creationTimestamp",
   517	      "mode": "value",
   518	      "reason": "write time"
   519	    },
   520	    {
   521	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
   522	      "path": [
   523	        "items",
   524	        "*",
   525	        "metadata",
   526	        "labels",
   527	        "grafana.app/deprecatedInternalID"
   528	      ],
   529	      "mode": "value",
   530	      "reason": "generated snowflake id (grafana.app/deprecatedInternalID)"
   531	    },
   532	    {
   533	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
   534	      "path": "items.*.metadata.managedFields.*.time",
   535	      "mode": "value",
   536	      "reason": "write time"
   537	    },
   538	    {
   539	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
   540	      "path": [
   541	        "items",
   542	        "*",
   543	        "metadata",
   544	        "annotations",
   545	        "grafana.app/createdBy"
   546	      ],
   547	      "mode": "value",
   548	      "reason": "user:<uid> of the writing user; the user uid is random per install"
   549	    },
   550	    {
   551	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
   552	      "path": [
   553	        "items",
   554	        "*",
   555	        "metadata",
   556	        "annotations",
   557	        "grafana.app/updatedBy"
   558	      ],
   559	      "mode": "value",
   560	      "reason": "user:<uid> of the writing user; the user uid is random per install"
   561	    },
   562	    {
   563	      "route": "GET /apis/dashboard.grafana.app/v1/namespaces/*/dashboards",
   564	      "path": [
   565	        "items",
   566	        "*",
   567	        "metadata",
   568	        "annotations",
   569	        "grafana.app/updatedTimestamp"
   570	      ],
   571	      "mode": "value",
   572	      "reason": "write time"
   573	    },
   574	    {
   575	      "route": "GET /api/search",
   576	      "path": "*.id",
   577	      "mode": "value",
   578	      "reason": "generated snowflake id (grafana.app/deprecatedInternalID)"
   579	    },
   580	    {
   581	      "route": "GET /api/search",
   582	      "path": "*.folderId",
   583	      "mode": "value",
   584	      "reason": "generated snowflake id (grafana.app/deprecatedInternalID)"
   585	    },
   586	    {
   587	      "route": "GET /api/annotations",
   588	      "path": "*.dashboardId",
   589	      "mode": "value",
   590	      "reason": "generated snowflake id (grafana.app/deprecatedInternalID)"
   591	    },
   592	    {
   593	      "route": "GET /api/annotations",
   594	      "path": "*.created",
   595	      "mode": "value",
   596	      "reason": "write time"
   597	    },
   598	    {
   599	      "route": "GET /api/annotations",
   600	      "path": "*.updated",
   601	      "mode": "value",
   602	      "reason": "write time"
   603	    },
   604	    {
   605	      "route": "GET /api/annotations",
   606	      "path": "*.userUID",
   607	      "mode": "value",
   608	      "reason": "uid of the admin user; random per install"
   609	    },
   610	    {
   611	      "route": "GET /api/annotations",
   612	      "path": "*.avatarUrl",
   613	      "mode": "value",
   614	      "reason": "sha256 of the admin email; Grafana ships admin@localhost, the seed may differ"
   615	    },
   616	    {
   617	      "route": "GET /api/query-history",
   618	      "path": "result.queryHistory.*.createdAt",
   619	      "mode": "value",
   620	      "reason": "write time in unix seconds; differs on every run"
   621	    }
   622	  ]
   623	}
     1	# Upstream promlib tests not ported
     2	
     3	Source: `upstream-prometheus/pkg/promlib` (grafana-prometheus-datasource v13.2.6). Ported tests live in
     4	`crates/grafana/tests/promlib_*.rs` (one `#[test]` per upstream case) with the golden files copied to
     5	`tests-data/promlib/`. These upstream cases are not ported:
     6	
     7	| Upstream test | Cases | Reason |
     8	|---|---|---|
     9	| `models/query_test.go` TestParseWithAdhocFilters, TestParseWithScopes, TestParseWithScopesAndAdhocFilters, TestParseWithGroupByKeys, TestParseComplexScenariosWithFilters | 14 | They rewrite PromQL (`ApplyFiltersAndGroupBy`), which needs the Prometheus PromQL parser and printer. grafana-rs has no PromQL parser: a query with scopes, ad hoc filters or group-by keys gets an error response instead of an unfiltered result. |
    10	| `models/scope_test.go` (all) | 7 functions | Same: PromQL rewriting. |
    11	| `resource/range_modifiers_test.go`, `resource/resource_test.go` TestResource_GetSuggestions* | 5 functions | The `suggestions` resource extracts metric names with the PromQL parser. grafana-rs answers `suggestions` with 500 "Failed to call resource". |
    12	| `models/query_test.go` TestQueryTypeDefinitions | 1 | Go schema code generation (`schemabuilder`), nothing to port. |
    13	| `models/lenient_test.go` | 6 functions | Not in the ported set: they check the metrics and logs written for each lenient conversion (instrumentation that grafana-rs does not have). The lenient parsing itself is ported in `promlib::settings` and covered by `promlib_settings.rs`. |
    14	| `resource/search_test.go`, `utils/utils_test.go`, `resource/resource_test.go` (gzip, zstd, framing headers, HTTP boundary), `resource/transport_error_test.go` | 21 functions | They test the Go HTTP transport: streaming NDJSON search bodies, gzip and zstd decoding, context cancellation. grafana-rs asks Prometheus for plain bodies and buffers search answers. |
    15	| `admission_handler_test.go` (Test_ValidateAdmission, Test_MutateAdmission, Test_getBadRequest), `instrumentation/instrumentation_test.go` | 5 functions | Kubernetes admission hooks and Prometheus metrics of the plugin process; grafana-rs has neither. |
    16	| `client/transport_test.go`, `middleware/force_http_get_test.go`, `library_test.go`, `healthcheck_test.go`, `heuristics_test.go` | 5 functions | Plugin SDK wiring (instance manager, middleware objects, admission and conversion handlers, Prometheus metrics). Health check and heuristics behavior is covered by the replay rows `GET /api/datasources/uid/*/health`. |
    17	| `querydata/framing_bench_test.go`, `converter/prom_bench_test.go` | benchmarks | Benchmarks, not checks. |
    18	
    19	Golden files copied but not used by any upstream test: `converter/testdata/prom-exemplars.json`,
    20	`prom-exemplars-frame.jsonc`, `prom-exemplars-a-frame.json`, `prom-exemplars-b-frame.json`,
    21	`loki-streams-structured-metadata.json` and its frame (upstream's `files` list does not name them).
  242 crates/grafana/tests/promlib_client.rs
  258 crates/grafana/tests/promlib_converter.rs
   43 crates/grafana/tests/promlib_exemplar.rs
  282 crates/grafana/tests/promlib_interval.rs
  394 crates/grafana/tests/promlib_models.rs
  254 crates/grafana/tests/promlib_querydata.rs
  205 crates/grafana/tests/promlib_request.rs
  156 crates/grafana/tests/promlib_resource.rs
  171 crates/grafana/tests/promlib_settings.rs
 2005 total
upstream-prometheus/pkg/promlib/utils/utils_test.go
upstream-prometheus/pkg/promlib/resource/transport_error_test.go
upstream-prometheus/pkg/promlib/resource/search_test.go
upstream-prometheus/pkg/promlib/resource/resource_test.go
upstream-prometheus/pkg/promlib/resource/range_modifiers_test.go
upstream-prometheus/pkg/promlib/resource/allowlist_test.go
upstream-prometheus/pkg/promlib/intervalv2/intervalv2_test.go
upstream-prometheus/pkg/promlib/querydata/response_test.go
upstream-prometheus/pkg/promlib/querydata/request_test.go
upstream-prometheus/pkg/promlib/querydata/framing_test.go
upstream-prometheus/pkg/promlib/querydata/framing_bench_test.go
upstream-prometheus/pkg/promlib/instrumentation/instrumentation_test.go
upstream-prometheus/pkg/promlib/heuristics_test.go
upstream-prometheus/pkg/promlib/healthcheck_test.go
upstream-prometheus/pkg/promlib/querydata/exemplar/sampler_test.go
upstream-prometheus/pkg/promlib/querydata/exemplar/sampler_stddev_test.go
upstream-prometheus/pkg/promlib/models/settings_test.go
upstream-prometheus/pkg/promlib/models/scope_test.go
upstream-prometheus/pkg/promlib/models/query_test.go
upstream-prometheus/pkg/promlib/models/prometheus_interval_test.go
upstream-prometheus/pkg/promlib/models/lenient_test.go
upstream-prometheus/pkg/promlib/middleware/force_http_get_test.go
upstream-prometheus/pkg/promlib/middleware/custom_query_params_test.go
upstream-prometheus/pkg/promlib/library_test.go
upstream-prometheus/pkg/promlib/admission_handler_test.go
upstream-prometheus/pkg/promlib/converter/prom_bench_test.go
upstream-prometheus/pkg/promlib/converter/prom_test.go
upstream-prometheus/pkg/promlib/client/transport_test.go
upstream-prometheus/pkg/promlib/client/client_test.go

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc 'nl -ba crates/grafana/tests/promlib_client.rs; nl -ba crates/grafana/tests/promlib_resource.rs; nl -ba crates/grafana/tests/promlib_request.rs' in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
     1	//! Port of promlib `client/client_test.go` (TestClient) and `middleware/custom_query_params_test.go`
     2	//! (TestCustomQueryParametersMiddleware). The Rust client builds the request without sending it, so the request
     3	//! the Go `MockDoer` / round tripper would see is the returned `HttpReq` (or the URL after `apply_middleware`).
     4	use grafana::promlib::client::{HttpReq, Instance, parse_query};
     5	use grafana::promlib::models::{Query, SEC};
     6	use grafana::promlib::settings::PromOptions;
     7	
     8	/// `NewClient(doer, method, "http://localhost:9090", "60s")`.
     9	fn client(method: &str) -> Instance {
    10	    let opts = PromOptions { http_method: method.to_string(), query_timeout: "60s".to_string(), ..Default::default() };
    11	    Instance { url: "http://localhost:9090".to_string(), opts, ..Default::default() }
    12	}
    13	
    14	fn header<'a>(r: &'a HttpReq, name: &str) -> &'a str {
    15	    r.headers.iter().find(|h| h.0.eq_ignore_ascii_case(name)).map(|h| h.1.as_str()).unwrap_or("")
    16	}
    17	
    18	fn raw_query(url: &str) -> &str {
    19	    url.split_once('?').map(|p| p.1).unwrap_or("")
    20	}
    21	
    22	// TestClient / QueryResource
    23	#[test]
    24	fn query_resource_post_configured_client_forwards_post_request_as_post() {
    25	    let c = client("POST");
    26	    let r = c.resource_request("POST", "/api/v1/series", "", b"match%5B%5D=ALERTS&start=1655271408&end=1655293008");
    27	    assert_eq!(r.method, "POST");
    28	    assert_eq!(String::from_utf8(r.body).expect("utf8"), "match%5B%5D=ALERTS&start=1655271408&end=1655293008");
    29	    assert_eq!(r.url, "http://localhost:9090/api/v1/series");
    30	}
    31	
    32	#[test]
    33	fn query_resource_get_configured_client_converts_incoming_post_to_get() {
    34	    let c = client("GET");
    35	    let r = c.resource_request("POST", "/api/v1/query", "", b"query=up&time=1234567890");
    36	    assert_eq!(r.method, "GET");
    37	    assert!(raw_query(&r.url).contains("query=up"), "{}", r.url);
    38	    assert!(raw_query(&r.url).contains("time=1234567890"), "{}", r.url);
    39	}
    40	
    41	#[test]
    42	fn query_resource_get_request_is_always_respected_regardless_of_configured_method() {
    43	    let c = client("POST");
    44	    // CallResourceRequest URL "api/v1/series?match%5B%5D=ALERTS&start=1655272558&end=1655294158"
    45	    let r = c.resource_request("GET", "/api/v1/series", "match%5B%5D=ALERTS&start=1655272558&end=1655294158", &[]);
    46	    assert_eq!(r.method, "GET");
    47	    assert_eq!(r.body, Vec::<u8>::new());
    48	    assert_eq!(r.url, "http://localhost:9090/api/v1/series?match%5B%5D=ALERTS&start=1655272558&end=1655294158");
    49	}
    50	
    51	#[test]
    52	fn query_resource_get_configured_client_preserves_existing_query_params_when_converting_post() {
    53	    let c = client("GET");
    54	    // CallResourceRequest URL "/api/v1/query?timeout=30"
    55	    let r = c.resource_request("POST", "/api/v1/query", "timeout=30", b"query=up");
    56	    assert_eq!(r.method, "GET");
    57	    assert!(raw_query(&r.url).contains("timeout=30"), "{}", r.url);
    58	    assert!(raw_query(&r.url).contains("query=up"), "{}", r.url);
    59	}
    60	
    61	// TestClient / QueryRange
    62	fn range_query() -> Query {
    63	    Query { expr: "rate(ALERTS{job=\"test\" [$__rate_interval]})".to_string(), start: 0, end: 1234 * SEC, range_query: true, step: SEC, ..Default::default() }
    64	}
    65	
    66	#[test]
    67	fn query_range_sends_correct_post_query() {
    68	    let r = client("POST").range_request(&range_query());
    69	    assert_eq!(r.method, "POST");
    70	    assert_eq!(header(&r, "Content-Type"), "application/x-www-form-urlencoded");
    71	    assert_eq!(r.body, b"end=1234&query=rate%28ALERTS%7Bjob%3D%22test%22+%5B%24__rate_interval%5D%7D%29&start=0&step=1&timeout=60s".to_vec());
    72	    assert_eq!(r.url, "http://localhost:9090/api/v1/query_range");
    73	}
    74	
    75	#[test]
    76	fn query_range_sends_correct_get_query() {
    77	    let r = client("GET").range_request(&range_query());
    78	    assert_eq!(r.method, "GET");
    79	    assert_eq!(r.body, Vec::<u8>::new());
    80	    assert_eq!(r.url, "http://localhost:9090/api/v1/query_range?end=1234&query=rate%28ALERTS%7Bjob%3D%22test%22+%5B%24__rate_interval%5D%7D%29&start=0&step=1&timeout=60s");
    81	}
    82	
    83	// TestCustomQueryParametersMiddleware
    84	
    85	/// `CustomQueryParameters(logger, customQueryParams, warnVal, errVal, queryStatsEnabled)` applied to `url`.
    86	fn mw(custom: &str, warn: f64, err: f64, stats: bool, url: &str) -> String {
    87	    let opts = PromOptions {
    88	        custom_query_parameters: custom.to_string(),
    89	        max_samples_processed_warning_threshold: warn,
    90	        max_samples_processed_error_threshold: err,
    91	        query_stats_enabled: stats,
    92	        ..Default::default()
    93	    };
    94	    let inst = Instance { opts, ..Default::default() };
    95	    let mut u = url.to_string();
    96	    inst.apply_middleware(&mut u);
    97	    u
    98	}
    99	
   100	/// Go `req.URL.Query()`.
   101	fn query_of(url: &str) -> Vec<(String, String)> {
   102	    parse_query(raw_query(url)).unwrap_or_default()
   103	}
   104	/// Go `url.Values.Get`: the first value or "".
   105	fn get(q: &[(String, String)], k: &str) -> String {
   106	    q.iter().find(|p| p.0 == k).map(|p| p.1.clone()).unwrap_or_default()
   107	}
   108	fn all(q: &[(String, String)], k: &str) -> Vec<String> {
   109	    q.iter().filter(|p| p.0 == k).map(|p| p.1.clone()).collect()
   110	}
   111	fn escape(s: &str) -> String {
   112	    let mut out = String::new();
   113	    grafana::promlib::client::query_escape(s, &mut out);
   114	    out
   115	}
   116	
   117	#[test]
   118	fn middleware_without_custom_query_parameters_set_should_not_apply_middleware() {
   119	    assert_eq!(mw("", 0.0, 0.0, false, "http://test.com/query?hello=name"), "http://test.com/query?hello=name");
   120	}
   121	
   122	#[test]
   123	fn middleware_with_invalid_custom_query_parameters_should_not_apply_middleware() {
   124	    assert_eq!(mw("custom=%%abc&test=abc", 0.0, 0.0, false, "http://test.com/query?hello=name"), "http://test.com/query?hello=name");
   125	}
   126	
   127	#[test]
   128	fn middleware_with_custom_query_parameters_for_request_url_containing_query_parameters() {
   129	    let u = mw("custom=par/am&second=f oo", 0.0, 0.0, false, "http://test.com/query?hello=name");
   130	    assert!(u.starts_with("http://test.com/query?"), "{u}");
   131	    let q = query_of(&u);
   132	    let mut keys: Vec<&str> = q.iter().map(|p| p.0.as_str()).collect();
   133	    keys.dedup();
   134	    assert_eq!(keys.len(), 3, "{u}");
   135	    assert_eq!(escape(&get(&q, "hello")), "name");
   136	    assert_eq!(escape(&get(&q, "custom")), "par%2Fam");
   137	    assert_eq!(escape(&get(&q, "second")), "f+oo");
   138	}
   139	
   140	#[test]
   141	fn middleware_with_custom_query_parameters_for_request_url_not_containing_query_parameters() {
   142	    assert_eq!(mw("custom=par/am&second=f oo", 0.0, 0.0, false, "http://test.com/query"), "http://test.com/query?custom=par%2Fam&second=f+oo");
   143	}
   144	
   145	#[test]
   146	fn middleware_with_sample_thresholds_only_should_apply_middleware() {
   147	    let q = query_of(&mw("", 500.0, 1000.0, false, "http://test.com/api/v1/query_range?query=up"));
   148	    assert_eq!(get(&q, "max_samples_processed_warning_threshold"), "500");
   149	    assert_eq!(get(&q, "max_samples_processed_error_threshold"), "1000");
   150	    assert_eq!(get(&q, "query"), "up");
   151	}
   152	
   153	#[test]
   154	fn middleware_with_zero_sample_thresholds_should_not_add_threshold_query_params() {
   155	    let q = query_of(&mw("custom=value", 0.0, 0.0, false, "http://test.com/query?hello=name"));
   156	    assert_eq!(get(&q, "hello"), "name");
   157	    assert_eq!(get(&q, "custom"), "value");
   158	    assert_eq!(get(&q, "max_samples_processed_warning_threshold"), "");
   159	    assert_eq!(get(&q, "max_samples_processed_error_threshold"), "");
   160	}
   161	
   162	#[test]
   163	fn middleware_with_custom_query_parameters_and_sample_thresholds_should_merge_query_string() {
   164	    let q = query_of(&mw("timeout=30s", 42.0, 0.0, false, "http://test.com/query"));
   165	    assert_eq!(get(&q, "timeout"), "30s");
   166	    assert_eq!(get(&q, "max_samples_processed_warning_threshold"), "42");
   167	    assert_eq!(get(&q, "max_samples_processed_error_threshold"), "");
   168	}
   169	
   170	#[test]
   171	fn middleware_with_explicit_threshold_fields_and_matching_custom_query_parameters_should_prefer_threshold_fields() {
   172	    let q = query_of(&mw("max_samples_processed_warning_threshold=9&max_samples_processed_error_threshold=17", 42.0, 88.0, false, "http://test.com/query"));
   173	    assert_eq!(get(&q, "max_samples_processed_warning_threshold"), "42");
   174	    assert_eq!(get(&q, "max_samples_processed_error_threshold"), "88");
   175	}
   176	
   177	#[test]
   178	fn middleware_with_query_statistics_disabled_should_not_add_stats() {
   179	    let q = query_of(&mw("", 0.0, 0.0, false, "http://test.com/api/v1/query?query=up"));
   180	    assert_eq!(get(&q, "stats"), "");
   181	}
   182	
   183	/// "With query statistics enabled should add stats for <name>" (the method does not change the URL middleware).
   184	fn stats_added(path: &str) {
   185	    let q = query_of(&mw("", 0.0, 0.0, true, &format!("http://test.com{path}?query=up&existing=value")));
   186	    assert_eq!(get(&q, "stats"), "all");
   187	    assert_eq!(get(&q, "query"), "up");
   188	    assert_eq!(get(&q, "existing"), "value");
   189	}
   190	#[test]
   191	fn middleware_stats_enabled_should_add_stats_for_get_instant_query() {
   192	    stats_added("/api/v1/query");
   193	}
   194	#[test]
   195	fn middleware_stats_enabled_should_add_stats_for_post_instant_query() {
   196	    stats_added("/api/v1/query");
   197	}
   198	#[test]
   199	fn middleware_stats_enabled_should_add_stats_for_get_range_query() {
   200	    stats_added("/api/v1/query_range");
   201	}
   202	#[test]
   203	fn middleware_stats_enabled_should_add_stats_for_post_range_query() {
   204	    stats_added("/api/v1/query_range");
   205	}
   206	#[test]
   207	fn middleware_stats_enabled_should_add_stats_for_query_with_workspace_base_path() {
   208	    stats_added("/workspaces/ws-123/api/v1/query");
   209	}
   210	
   211	/// "With query statistics enabled should not add stats to non-query endpoint <path>".
   212	fn stats_not_added(path: &str) {
   213	    let q = query_of(&mw("", 0.0, 0.0, true, &format!("http://test.com{path}")));
   214	    assert_eq!(get(&q, "stats"), "");
   215	}
   216	#[test]
   217	fn middleware_stats_enabled_should_not_add_stats_to_query_exemplars() {
   218	    stats_not_added("/api/v1/query_exemplars");
   219	}
   220	#[test]
   221	fn middleware_stats_enabled_should_not_add_stats_to_metadata() {
   222	    stats_not_added("/api/v1/metadata");
   223	}
   224	#[test]
   225	fn middleware_stats_enabled_should_not_add_stats_to_labels() {
   226	    stats_not_added("/api/v1/labels");
   227	}
   228	#[test]
   229	fn middleware_stats_enabled_should_not_add_stats_to_series() {
   230	    stats_not_added("/api/v1/series");
   231	}
   232	#[test]
   233	fn middleware_stats_enabled_should_not_add_stats_to_status_buildinfo() {
   234	    stats_not_added("/api/v1/status/buildinfo");
   235	}
   236	
   237	#[test]
   238	fn middleware_explicit_query_statistics_setting_should_override_a_custom_stats_value() {
   239	    let q = query_of(&mw("stats=none&timeout=30s", 0.0, 0.0, true, "http://test.com/api/v1/query_range"));
   240	    assert_eq!(all(&q, "stats"), vec!["all".to_string()]);
   241	    assert_eq!(get(&q, "timeout"), "30s");
   242	}
     1	//! Port of promlib `resource/allowlist_test.go` (TestIsAllowedResourcePath).
     2	use grafana::promlib::is_allowed_resource_path;
     3	
     4	fn allowed(path: &str) {
     5	    assert!(is_allowed_resource_path(path), "expected {path:?} to be allowed");
     6	}
     7	fn rejected(path: &str) {
     8	    assert!(!is_allowed_resource_path(path), "expected {path:?} to be rejected");
     9	}
    10	
    11	// TestIsAllowedResourcePath: allowed
    12	#[test]
    13	fn allowed_api_v1_query() {
    14	    allowed("api/v1/query");
    15	}
    16	#[test]
    17	fn allowed_slash_api_v1_query() {
    18	    allowed("/api/v1/query");
    19	}
    20	#[test]
    21	fn allowed_api_v1_query_range() {
    22	    allowed("api/v1/query_range");
    23	}
    24	#[test]
    25	fn allowed_api_v1_query_exemplars() {
    26	    allowed("api/v1/query_exemplars");
    27	}
    28	#[test]
    29	fn allowed_api_v1_series() {
    30	    allowed("api/v1/series");
    31	}
    32	#[test]
    33	fn allowed_api_v1_labels() {
    34	    allowed("api/v1/labels");
    35	}
    36	#[test]
    37	fn allowed_api_v1_label_instance_values() {
    38	    allowed("api/v1/label/instance/values");
    39	}
    40	#[test]
    41	fn allowed_api_v1_label_name_values() {
    42	    allowed("api/v1/label/__name__/values");
    43	}
    44	#[test]
    45	fn allowed_api_v1_metadata() {
    46	    allowed("api/v1/metadata");
    47	}
    48	#[test]
    49	fn allowed_api_v1_rules() {
    50	    allowed("api/v1/rules");
    51	}
    52	#[test]
    53	fn allowed_api_v1_status_buildinfo() {
    54	    allowed("api/v1/status/buildinfo");
    55	}
    56	#[test]
    57	fn allowed_api_v1_search_metric_names() {
    58	    allowed("api/v1/search/metric_names");
    59	}
    60	#[test]
    61	fn allowed_api_v1_search_label_names() {
    62	    allowed("api/v1/search/label_names");
    63	}
    64	#[test]
    65	fn allowed_api_v1_search_label_values() {
    66	    allowed("api/v1/search/label_values");
    67	}
    68	
    69	// TestIsAllowedResourcePath: rejected
    70	#[test]
    71	fn rejected_empty() {
    72	    rejected("");
    73	}
    74	#[test]
    75	fn rejected_slash() {
    76	    rejected("/");
    77	}
    78	#[test]
    79	fn rejected_reload() {
    80	    rejected("-/reload");
    81	}
    82	#[test]
    83	fn rejected_quit() {
    84	    rejected("-/quit");
    85	}
    86	#[test]
    87	fn rejected_federate() {
    88	    rejected("federate");
    89	}
    90	#[test]
    91	fn rejected_admin_tsdb_delete_series() {
    92	    rejected("api/v1/admin/tsdb/delete_series");
    93	}
    94	#[test]
    95	fn rejected_search_trailing_slash() {
    96	    rejected("api/v1/search/");
    97	}
    98	#[test]
    99	fn rejected_search_unknown_endpoint() {
   100	    rejected("api/v1/search/unknown_endpoint");
   101	}
   102	#[test]
   103	fn rejected_label_empty_name() {
   104	    rejected("api/v1/label//values");
   105	}
   106	#[test]
   107	fn rejected_label_name_with_slash() {
   108	    rejected("api/v1/label/foo/bar/values");
   109	}
   110	#[test]
   111	fn rejected_dot_dot_admin() {
   112	    rejected("api/v1/../admin");
   113	}
   114	#[test]
   115	fn rejected_label_dot_dot_reload() {
   116	    rejected("api/v1/label/../../-/reload/values");
   117	}
   118	#[test]
   119	fn rejected_dot_prefix() {
   120	    rejected("./api/v1/labels");
   121	}
   122	#[test]
   123	fn rejected_encoded_slashes() {
   124	    rejected("api%2Fv1%2Flabels");
   125	}
   126	#[test]
   127	fn rejected_encoded_dot_dot() {
   128	    rejected("api/v1/labels%2F..%2F..%2F-%2Freload");
   129	}
   130	
   131	// resource/resource_test.go: TestNewResource, TestResource_Execute, TestResource_ExecuteRejectsDisallowedPath
   132	fn resource_instance() -> grafana::promlib::client::Instance {
   133	    grafana::promlib::client::Instance { url: "http://localhost:9090".to_string(), opts: grafana::promlib::settings::parse_prom_options("{}").expect("options"), timeout_secs: 30, ..Default::default() }
   134	}
   135	
   136	fn ok_transport(_r: &grafana::promlib::client::HttpReq) -> Result<grafana::promlib::client::HttpResp, grafana::promlib::client::TransportError> {
   137	    Ok(grafana::promlib::client::HttpResp { status: 200, headers: Vec::new(), body: b"{}".to_vec() })
   138	}
   139	
   140	#[test]
   141	fn new_resource() {
   142	    let inst = resource_instance();
   143	    assert_eq!(inst.url, "http://localhost:9090");
   144	}
   145	
   146	#[test]
   147	fn resource_execute() {
   148	    let r = grafana::promlib::call_resource(&resource_instance(), "", "/api/v1/labels", "", &[], None, &ok_transport).expect("response");
   149	    assert_eq!(r.status, 200);
   150	}
   151	
   152	#[test]
   153	fn resource_execute_rejects_disallowed_path() {
   154	    let r = grafana::promlib::call_resource(&resource_instance(), "", "-/reload", "", &[], None, &ok_transport).expect("response");
   155	    assert_eq!(r.status, 403);
   156	}
     1	//! Port of promlib `querydata/request_test.go` (TestPrometheus_parseTimeSeriesResponse,
     2	//! TestPrometheus_executedQueryString). Go's `fakeHttpClientProvider` answers `/api/v1/query_range`,
     3	//! `/api/v1/query` and `/api/v1/query_exemplars` with canned bodies built by `toAPIResponse`; here a transport
     4	//! closure does the same. Go's "timestamps are UTC zoned" checks have no Rust counterpart (times are Unix ns).
     5	use grafana::promlib::client::{HttpReq, HttpResp, Instance, TransportError};
     6	use grafana::promlib::frame::{Field, Frame, Values};
     7	use grafana::promlib::models::{DataQuery, HOUR, MIN, MS, SEC};
     8	use grafana::promlib::querydata::execute;
     9	use grafana::promlib::settings::parse_prom_options;
    10	use json::Json;
    11	
    12	/// `setup()`: a data source at localhost:9090 with `timeInterval: 15s`.
    13	fn instance() -> Instance {
    14	    Instance { url: "http://localhost:9090".to_string(), opts: parse_prom_options(r#"{"timeInterval": "15s"}"#).expect("options"), ..Default::default() }
    15	}
    16	
    17	/// `toAPIResponse(d)`: `{"status":"success","data":<d>}` (`null` for a nil result).
    18	fn api(data: Option<&str>) -> Vec<u8> {
    19	    format!(r#"{{"status":"success","data":{}}}"#, data.unwrap_or("null")).into_bytes()
    20	}
    21	
    22	/// `execute(tctx, query, rqr, iqr, eqr)`: the frames of the query's response.
    23	fn run(query: DataQuery, range: Option<&str>, instant: Option<&str>, exemplar: Option<&str>) -> Vec<Frame> {
    24	    let (rb, ib, eb) = (api(range), api(instant), api(exemplar));
    25	    let t = |r: &HttpReq| -> Result<HttpResp, TransportError> {
    26	        let no_q = r.url.split('?').next().unwrap_or("");
    27	        let path = no_q.strip_prefix("http://localhost:9090").unwrap_or(no_q);
    28	        let body = match path {
    29	            "/api/v1/query_range" => rb.clone(),
    30	            "/api/v1/query" => ib.clone(),
    31	            "/api/v1/query_exemplars" => eb.clone(),
    32	            _ => return Err(TransportError { message: format!("no such path: {path}"), timeout: false }),
    33	        };
    34	        Ok(HttpResp { status: 200, headers: Vec::new(), body })
    35	    };
    36	    let ref_id = query.ref_id.clone();
    37	    let res = execute(&instance(), std::slice::from_ref(&query), &t, false);
    38	    let dr = res.into_iter().find(|r| r.0 == ref_id).expect("response for refId").1;
    39	    dr.frames.unwrap_or_default()
    40	}
    41	
    42	/// Go `data.Labels.String()`: `k=v` sorted by key, joined by ", ".
    43	fn labels_string(f: &Field) -> String {
    44	    f.labels.as_ref().map(|l| l.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", ")).unwrap_or_default()
    45	}
    46	fn labels_len(f: &Field) -> usize {
    47	    f.labels.as_ref().map(|l| l.len()).unwrap_or(0)
    48	}
    49	fn display_name_from_ds(f: &Field) -> String {
    50	    f.config.as_ref().map(|c| c.display_name_from_ds.clone()).unwrap_or_default()
    51	}
    52	fn f64_at(f: &Field, i: usize) -> f64 {
    53	    match &f.values {
    54	        Values::F64(v) => v[i],
    55	        other => panic!("field {} is not float64: {other:?}", f.name),
    56	    }
    57	}
    58	fn time_at(f: &Field, i: usize) -> i64 {
    59	    match &f.values {
    60	        Values::Time(v) => v[i],
    61	        other => panic!("field {} is not time: {other:?}", f.name),
    62	    }
    63	}
    64	
    65	/// A query whose JSON is `json` over [from, to] (seconds).
    66	fn query(json: Json, from: i64, to: i64) -> DataQuery {
    67	    DataQuery { from: from * SEC, to: to * SEC, json, ..Default::default() }
    68	}
    69	
    70	// TestPrometheus_parseTimeSeriesResponse
    71	#[test]
    72	fn parse_ts_exemplars_response_should_be_sampled_and_parsed_normally() {
    73	    let now_ms = grafana::util::now_ms();
    74	    let ts = |ago: i64| -> String { format!("{}", (now_ms - ago * MIN / MS) as f64 / 1000.0) };
    75	    let exemplars = format!(
    76	        r#"[{{"seriesLabels":{{"__name__":"tns_request_duration_seconds_bucket","instance":"app:80","job":"tns/app"}},"exemplars":[{{"labels":{{"traceID":"test1"}},"value":"0.003535405","timestamp":{}}},{{"labels":{{"traceID":"test2"}},"value":"0.005555605","timestamp":{}}},{{"labels":{{"traceID":"test3"}},"value":"0.007545445","timestamp":{}}},{{"labels":{{"traceID":"test4"}},"value":"0.009545445","timestamp":{}}}]}}]"#,
    77	        ts(2),
    78	        ts(4),
    79	        ts(6),
    80	        ts(7)
    81	    );
    82	    let range = r#"{"resultType":"matrix","result":[{"metric":{"app":"Application","tag2":"tag2"},"values":[[1,"1"],[4,"4"],[7,"6"],[1.1,"8"]]}]}"#;
    83	    let qm = Json::obj().with("utcOffsetSec", 0).with("legendFormat", "legend {{app}}").with("exemplar", true);
    84	    let q = DataQuery { ref_id: "A".to_string(), json: qm, ..Default::default() };
    85	    let res = run(q, Some(range), None, Some(&exemplars));
    86	
    87	    assert_eq!(res.len(), 2);
    88	    let exemplar_frame = res.iter().find(|f| f.name == "exemplar").expect("exemplar frame");
    89	    let _range_frame = res.iter().find(|f| f.name != "exemplar").expect("range frame");
    90	    assert_eq!(exemplar_frame.fields[0].name, "Time");
    91	    assert_eq!(exemplar_frame.fields[1].name, "Value");
    92	    assert_eq!(exemplar_frame.fields.len(), 6);
    93	
    94	    assert_eq!(exemplar_frame.fields[1].len(), 4);
    95	    assert_eq!(f64_at(&exemplar_frame.fields[1], 0), 0.009545445);
    96	    assert_eq!(f64_at(&exemplar_frame.fields[1], 3), 0.003535405);
    97	}
    98	
    99	#[test]
   100	fn parse_ts_matrix_response_should_be_parsed_normally() {
   101	    let range = r#"{"resultType":"matrix","result":[{"metric":{"app":"Application","tag2":"tag2"},"values":[[1,"1"],[2,"2"],[3,"3"],[4,"4"],[5,"5"]]}]}"#;
   102	    let qm = Json::obj().with("utcOffsetSec", 0).with("range", true).with("legendFormat", "legend {{app}}");
   103	    let res = run(query(qm, 1, 5), Some(range), None, None);
   104	
   105	    assert_eq!(res.len(), 1);
   106	    assert_eq!(res[0].fields.len(), 2);
   107	    assert_eq!(labels_len(&res[0].fields[0]), 0);
   108	    assert_eq!(res[0].fields[0].name, "Time");
   109	    assert_eq!(labels_len(&res[0].fields[1]), 2);
   110	    assert_eq!(labels_string(&res[0].fields[1]), "app=Application, tag2=tag2");
   111	    assert_eq!(display_name_from_ds(&res[0].fields[1]), "legend Application");
   112	}
   113	
   114	#[test]
   115	fn parse_ts_matrix_response_with_missed_data_points_should_be_parsed_correctly() {
   116	    let range = r#"{"resultType":"matrix","result":[{"metric":{"app":"Application","tag2":"tag2"},"values":[[1,"1"],[4,"4"]]}]}"#;
   117	    let qm = Json::obj().with("utcOffsetSec", 0).with("range", true).with("legendFormat", "");
   118	    let res = run(query(qm, 1, 4), Some(range), None, None);
   119	
   120	    assert_eq!(res.len(), 1);
   121	    assert_eq!(res[0].fields[0].len(), 2);
   122	    assert_eq!(time_at(&res[0].fields[0], 0), SEC);
   123	    assert_eq!(time_at(&res[0].fields[0], 1), 4 * SEC);
   124	    assert_eq!(res[0].fields[1].len(), 2);
   125	    assert_eq!(f64_at(&res[0].fields[1], 0), 1.0);
   126	    assert_eq!(f64_at(&res[0].fields[1], 1), 4.0);
   127	}
   128	
   129	#[test]
   130	fn parse_ts_matrix_response_with_from_alerting_missed_data_points_should_be_parsed_correctly() {
   131	    let range = r#"{"resultType":"matrix","result":[{"metric":{"app":"Application","tag2":"tag2"},"values":[[1,"1"],[4,"4"]]}]}"#;
   132	    let qm = Json::obj().with("utcOffsetSec", 0).with("range", true).with("legendFormat", "");
   133	    let res = run(query(qm, 1, 4), Some(range), None, None);
   134	
   135	    assert_eq!(res.len(), 1);
   136	    assert_eq!(res[0].fields.len(), 2);
   137	    assert_eq!(labels_len(&res[0].fields[0]), 0);
   138	    assert_eq!(res[0].fields[0].name, "Time");
   139	    assert_eq!(labels_len(&res[0].fields[1]), 2);
   140	    assert_eq!(labels_string(&res[0].fields[1]), "app=Application, tag2=tag2");
   141	    assert_eq!(display_name_from_ds(&res[0].fields[1]), r#"{app="Application", tag2="tag2"}"#);
   142	}
   143	
   144	#[test]
   145	fn parse_ts_matrix_response_with_nan_value_should_be_changed_to_null() {
   146	    let range = r#"{"resultType":"matrix","result":[{"metric":{"app":"Application"},"values":[[1,"NaN"]]}]}"#;
   147	    let qm = Json::obj().with("utcOffsetSec", 0).with("range", true).with("legendFormat", "");
   148	    let res = run(query(qm, 1, 4), Some(range), None, None);
   149	
   150	    assert_eq!(display_name_from_ds(&res[0].fields[1]), r#"{app="Application"}"#);
   151	    assert!(f64_at(&res[0].fields[1], 0).is_nan());
   152	}
   153	
   154	#[test]
   155	fn parse_ts_vector_response_should_be_parsed_normally() {
   156	    let instant = r#"{"resultType":"vector","result":[{"metric":{"app":"Application","tag2":"tag2"},"value":[0.123,"1"]}]}"#;
   157	    let qm = Json::obj().with("utcOffsetSec", 0).with("instant", true).with("legendFormat", "legend {{app}}");
   158	    let res = run(DataQuery { json: qm, ..Default::default() }, None, Some(instant), None);
   159	
   160	    assert_eq!(res.len(), 1);
   161	    assert_eq!(res[0].fields.len(), 2);
   162	    assert_eq!(labels_len(&res[0].fields[0]), 0);
   163	    assert_eq!(res[0].fields[0].name, "Time");
   164	    assert_eq!(res[0].fields[0].name, "Time");
   165	    assert_eq!(labels_len(&res[0].fields[1]), 2);
   166	    assert_eq!(labels_string(&res[0].fields[1]), "app=Application, tag2=tag2");
   167	    assert_eq!(display_name_from_ds(&res[0].fields[1]), "legend Application");
   168	    assert_eq!(time_at(&res[0].fields[0], 0) / MS, 123);
   169	}
   170	
   171	#[test]
   172	fn parse_ts_scalar_response_should_be_parsed_normally() {
   173	    let instant = r#"{"resultType":"scalar","result":[0.123,"1"]}"#;
   174	    let qm = Json::obj().with("utcOffsetSec", 0).with("instant", true).with("legendFormat", "");
   175	    let res = run(DataQuery { json: qm, ..Default::default() }, None, Some(instant), None);
   176	
   177	    assert_eq!(res.len(), 1);
   178	    assert_eq!(res[0].fields.len(), 2);
   179	    assert_eq!(labels_len(&res[0].fields[0]), 0);
   180	    assert_eq!(res[0].fields[0].name, "Time");
   181	    assert_eq!(res[0].fields[1].name, "Value");
   182	    assert_eq!(f64_at(&res[0].fields[1], 0), 1.0);
   183	    assert_eq!(time_at(&res[0].fields[0], 0) / MS, 123);
   184	}
   185	
   186	// TestPrometheus_executedQueryString
   187	fn executed_query_string(interval_ms: i64, max_data_points: i64) -> String {
   188	    let range = r#"{"resultType":"matrix","result":[{"metric":{"app":"Application"},"values":[[1,"1"],[2,"2"]]}]}"#;
   189	    let qj = format!(r#"{{"expr": "test_metric", "format": "time_series", "intervalFactor": 1, "interval": "2m", "intervalMs": {interval_ms}, "maxDataPoints": {max_data_points}, "refId": "A", "range": true}}"#);
   190	    let now = grafana::util::now_ms() * MS + 123_456;
   191	    let q = DataQuery { ref_id: "A".to_string(), max_data_points, interval: interval_ms * MS, from: now, to: now + 48 * HOUR, json: Json::parse(&qj).expect("query json"), ..Default::default() };
   192	    let res = run(q, Some(range), None, None);
   193	    assert_eq!(res.len(), 1);
   194	    res[0].meta.as_ref().expect("meta").executed_query_string.clone()
   195	}
   196	
   197	#[test]
   198	fn executed_query_string_with_interval_ms_300_000() {
   199	    assert_eq!(executed_query_string(300_000, 761), "Expr: test_metric\nStep: 5m0s");
   200	}
   201	
   202	#[test]
   203	fn executed_query_string_with_interval_ms_900_000() {
   204	    assert_eq!(executed_query_string(900_000, 175), "Expr: test_metric\nStep: 15m0s");
   205	}

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "rg -n --glob '*.rs' --glob '"'!**/fixtures/**'"' --glob '"'!**/data/**'"' 'bench1|prom1|gsshcd|3300|3310|9390|recording|captur|replay' crates/grafana/src crates/store-sqlite/src crates/server/src; rg -n '"'^func Test|t.Run'"\\(' upstream-prometheus/pkg/promlib/{client/client_test.go,resource/allowlist_test.go,resource/resource_test.go,querydata/request_test.go}" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
crates/grafana/src/api/ofrep.rs:3://! values (`captured/ofrep-flags.json`, from the recording), then the `[feature_toggles]` settings (`enable = a,b` and
crates/grafana/src/api/ofrep.rs:100:    let data = Json::parse(include_str!("captured/ofrep-flags.json")).unwrap_or(Json::Null);
crates/grafana/src/api/discovery.rs:2://! (APIResourceList). The groups, versions and resources are those Grafana 13.3.1 serves, captured from a real
crates/grafana/src/api/discovery.rs:3://! instance into `captured/discovery.json`. The bodies are built once, pretty-printed like the apiserver's answers.
crates/grafana/src/api/discovery.rs:28:        let data = Json::parse(include_str!("captured/discovery.json")).unwrap_or(Json::Null);
crates/grafana/src/api/stubs.rs:110:/// The check types of Grafana 13.3.1, captured from a real instance (`captured/advisor-checktypes.json`). Their uid,
crates/grafana/src/api/stubs.rs:111:/// resourceVersion and times are those of the capture.
crates/grafana/src/api/stubs.rs:115:        let list = Json::parse(include_str!("captured/advisor-checktypes.json")).unwrap_or(Json::Null);
crates/grafana/src/access.rs:4://! The tables are the unscoped action sets Grafana 13.3.1 reports for each basic role (captured from a real
crates/grafana/src/access.rs:80:/// Org role None (and nobody): captured from Grafana 13.3.1.
crates/grafana/src/access.rs:84:/// Org role Viewer, captured from Grafana 13.3.1 (anonymous Viewer).
crates/grafana/src/access.rs:117:/// Org role Editor, captured from Grafana 13.3.1 (anonymous Editor).
crates/grafana/src/access.rs:196:/// Org role Admin, captured from Grafana 13.3.1 (anonymous Admin; equal to the ui-tour-v1 recording).
crates/grafana/src/access.rs:368:/// Added for a Grafana server admin (the api-flows-v1 recording of admin minus ADMIN).
crates/grafana/src/api/boot.rs:408:            Json::parse(r#"{"minInterval":"10s","stateHistory":{"backend":"annotations","prometheusMetricName":"GRAFANA_ALERTS"},"recordingRulesEnabled":true,"notificationHistoryEnabled":false,"alertStateHistoryBackend":"annotations"}"#).unwrap_or(Json::obj()),
crates/grafana/src/api/boot.rs:432:/// Preinstalled plugins of `[plugins] preinstall` (Grafana builds this list from a map; the order is the recording's).
upstream-prometheus/pkg/promlib/resource/allowlist_test.go:5:func TestIsAllowedResourcePath(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/allowlist_test.go:23:		t.Run(path, func(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/allowlist_test.go:51:		t.Run(path, func(t *testing.T) {
upstream-prometheus/pkg/promlib/querydata/request_test.go:28:func TestPrometheus_parseTimeSeriesResponse(t *testing.T) {
upstream-prometheus/pkg/promlib/querydata/request_test.go:29:	t.Run("exemplars response should be sampled and parsed normally", func(t *testing.T) {
upstream-prometheus/pkg/promlib/querydata/request_test.go:121:	t.Run("matrix response should be parsed normally", func(t *testing.T) {
upstream-prometheus/pkg/promlib/querydata/request_test.go:173:	t.Run("matrix response with missed data points should be parsed correctly", func(t *testing.T) {
upstream-prometheus/pkg/promlib/querydata/request_test.go:218:	t.Run("matrix response with from alerting missed data points should be parsed correctly", func(t *testing.T) {
upstream-prometheus/pkg/promlib/querydata/request_test.go:263:	t.Run("matrix response with NaN value should be changed to null", func(t *testing.T) {
upstream-prometheus/pkg/promlib/querydata/request_test.go:302:	t.Run("vector response should be parsed normally", func(t *testing.T) {
upstream-prometheus/pkg/promlib/querydata/request_test.go:345:	t.Run("scalar response should be parsed normally", func(t *testing.T) {
upstream-prometheus/pkg/promlib/querydata/request_test.go:384:func TestPrometheus_executedQueryString(t *testing.T) {
upstream-prometheus/pkg/promlib/querydata/request_test.go:385:	t.Run("executedQueryString should match expected format with intervalMs 300_000", func(t *testing.T) {
upstream-prometheus/pkg/promlib/querydata/request_test.go:432:	t.Run("executedQueryString should match expected format with intervalMs 900_000", func(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/resource_test.go:80:func TestNewResource(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/resource_test.go:87:func TestResource_Execute(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/resource_test.go:103:func TestResource_ExecuteRejectsDisallowedPath(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/resource_test.go:116:func TestResource_GetSuggestionsRejectsLabelNamePathInjection(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/resource_test.go:138:func TestResource_ExecuteDecodesCompressedResponse(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/resource_test.go:173:func TestResource_ExecuteStripsFramingHeadersAcrossEncodings(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/resource_test.go:187:		t.Run(tc.name, func(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/resource_test.go:233:func TestResource_ExecuteReturnsErrorForUnexpectedEncoding(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/resource_test.go:262:func TestResource_ExecutePinsAcceptEncodingToGzip(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/resource_test.go:321:func TestResource_ExecuteResponseSurvivesHTTPBoundary(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/resource_test.go:382:func TestResource_GetSuggestions(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/resource_test.go:392:		t.Run(settingsCase.name, func(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/resource_test.go:400:				t.Run(path, func(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/resource_test.go:451:func TestResource_GetSuggestionsForwardsCacheHeader(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/resource_test.go:490:func TestResource_GetSuggestionsWithEmptyQueriesButFilters(t *testing.T) {
upstream-prometheus/pkg/promlib/client/client_test.go:29:func TestClient(t *testing.T) {
upstream-prometheus/pkg/promlib/client/client_test.go:30:	t.Run("QueryResource", func(t *testing.T) {
upstream-prometheus/pkg/promlib/client/client_test.go:31:		t.Run("POST-configured client forwards POST request as POST", func(t *testing.T) {
upstream-prometheus/pkg/promlib/client/client_test.go:59:		t.Run("GET-configured client converts incoming POST to GET", func(t *testing.T) {
upstream-prometheus/pkg/promlib/client/client_test.go:85:		t.Run("GET request is always respected regardless of configured method", func(t *testing.T) {
upstream-prometheus/pkg/promlib/client/client_test.go:112:		t.Run("GET-configured client preserves existing query params when converting POST", func(t *testing.T) {
upstream-prometheus/pkg/promlib/client/client_test.go:139:	t.Run("QueryRange", func(t *testing.T) {
upstream-prometheus/pkg/promlib/client/client_test.go:142:		t.Run("sends correct POST query", func(t *testing.T) {
upstream-prometheus/pkg/promlib/client/client_test.go:169:		t.Run("sends correct GET query", func(t *testing.T) {

hook: PostToolUse
hook: PostToolUse Completed
codex
The replay code does not compare HTML boot-data values. The OFREP rules also discard each flag’s key and value, so a wrong flag can pass. I’ll check the test pairs and look for runtime code that depends on recorded data.

hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc 'nl -ba crates/grafana/tests/promlib_querydata.rs; nl -ba crates/grafana/tests/promlib_converter.rs' in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
     1	//! Port of promlib `querydata/framing_test.go` (golden files in `testdata/`) and `querydata/response_test.go`.
     2	mod common;
     3	
     4	use common::{check_golden, read_bytes};
     5	use grafana::promlib::client::{HttpReq, HttpResp, Instance, TransportError};
     6	use grafana::promlib::frame::{DataResponse, Field, FieldConfig, Frame, Meta, Values};
     7	use grafana::promlib::models::{DataQuery, Query, SEC};
     8	use grafana::promlib::querydata::{QueryType, execute, parse_query_stats, parse_response, process_exemplars};
     9	use grafana::promlib::settings::parse_prom_options;
    10	use json::Json;
    11	
    12	/// `setup()`: a data source at localhost:9090 with `timeInterval: 15s`.
    13	fn instance() -> Instance {
    14	    Instance { url: "http://localhost:9090".to_string(), opts: parse_prom_options(r#"{"timeInterval": "15s"}"#).expect("options"), timeout_secs: 30, ..Default::default() }
    15	}
    16	
    17	/// `loadStoredQuery` + `runQuery`: the stored query as a request, every Prometheus call answered with `response`.
    18	fn golden_scenario(name: &str) {
    19	    let sq = Json::parse(&String::from_utf8(read_bytes(&format!("querydata/{name}.query.json"))).expect("utf8")).expect("query json");
    20	    let step = sq.i64_of("Step");
    21	    let qm = Json::obj()
    22	        .with("expr", sq.str_of("Expr"))
    23	        .with("range", sq.bool_of("RangeQuery"))
    24	        .with("exemplar", sq.bool_of("ExemplarQuery"))
    25	        .with("legendFormat", sq.str_of("LegendFormat"))
    26	        .with("intervalMs", step * 1000)
    27	        .with("interval", format!("{step}s"));
    28	    let q = DataQuery { ref_id: sq.str_of("RefId").to_string(), from: sq.i64_of("Start") * SEC, to: sq.i64_of("End") * SEC, interval: step * SEC, json: qm, ..Default::default() };
    29	    let body = read_bytes(&format!("querydata/{name}.result.json"));
    30	    let t = |_r: &HttpReq| -> Result<HttpResp, TransportError> { Ok(HttpResp { status: 200, headers: Vec::new(), body: body.clone() }) };
    31	    let res = execute(&instance(), std::slice::from_ref(&q), &t, false);
    32	    assert_eq!(res.len(), 1);
    33	    let (ref_id, dr) = &res[0];
    34	    assert_eq!(ref_id, "A");
    35	    check_golden(&format!("querydata/{name}.result.golden.jsonc"), dr);
    36	}
    37	
    38	// TestRangeResponses
    39	#[test]
    40	fn range_parse_a_simple_matrix_response() {
    41	    golden_scenario("range_simple");
    42	}
    43	#[test]
    44	fn range_parse_a_simple_matrix_response_with_value_missing_steps() {
    45	    golden_scenario("range_missing");
    46	}
    47	#[test]
    48	fn range_parse_a_matrix_response_with_infinity() {
    49	    golden_scenario("range_infinity");
    50	}
    51	#[test]
    52	fn range_parse_a_matrix_response_with_nan() {
    53	    golden_scenario("range_nan");
    54	}
    55	#[test]
    56	fn range_parse_a_response_with_legend_format_auto() {
    57	    golden_scenario("range_auto");
    58	}
    59	
    60	// TestExemplarResponses
    61	#[test]
    62	fn exemplar_parse_an_exemplar_response() {
    63	    golden_scenario("exemplar");
    64	}
    65	
    66	// TestQueryData_parseResponse
    67	const SERIES: &str = r#"[{"metric":{"__name__":"some_name","environment":"some_env","id":"some_id","instance":"some_instance:1234","job":"some_job","name":"another_name","region":"some_region"},"value":[1.1,"2"]}]"#;
    68	
    69	fn parse(body: &str, status: u16) -> DataResponse {
    70	    let res = HttpResp { status, headers: Vec::new(), body: body.as_bytes().to_vec() };
    71	    parse_response(&Query::default(), &res, QueryType::Range)
    72	}
    73	
    74	#[test]
    75	fn parse_response_result_type_before_result() {
    76	    let r = parse(&format!(r#"{{"data":{{"resultType":"vector", "result":{SERIES}}},"status":"success"}}"#), 200);
    77	    assert!(r.error.is_none());
    78	    assert_eq!(r.frame_count(), 1);
    79	}
    80	
    81	#[test]
    82	fn parse_response_result_type_after_result() {
    83	    let r = parse(&format!(r#"{{"data":{{"result":{SERIES},"resultType":"vector"}},"status":"success"}}"#), 200);
    84	    assert!(r.error.is_none());
    85	    assert_eq!(r.frame_count(), 1);
    86	}
    87	
    88	#[test]
    89	fn parse_response_no_result_type() {
    90	    let r = parse(&format!(r#"{{"data":{{"result":{SERIES}}},"status":"success"}}"#), 200);
    91	    assert_eq!(r.error.as_deref(), Some("no resultType found"));
    92	}
    93	
    94	#[test]
    95	fn parse_response_empty_result_type_before_result() {
    96	    let r = parse(&format!(r#"{{"data":{{"resultType":"", "result":{SERIES}}},"status":"success"}}"#), 200);
    97	    assert_eq!(r.error.as_deref(), Some("unknown result type: "));
    98	}
    99	
   100	#[test]
   101	fn parse_response_empty_result_type_after_result() {
   102	    let r = parse(&format!(r#"{{"data":{{"result":{SERIES},"resultType":""}},"status":"success"}}"#), 200);
   103	    assert_eq!(r.error.as_deref(), Some("unknown result type: "));
   104	}
   105	
   106	// TestAddMetadataToMultiFrame
   107	#[test]
   108	fn add_metadata_when_you_have_native_histogram_result() {
   109	    let body = r#"{"status":"success","data":{"resultType":"matrix","result":[{"metric":{"__name__":"rpc_durations_native_histogram_seconds","instance":"nativehisto:8080","job":"prometheus"},"histograms":[[1729529685,{"count":"7243102","sum":"72460202.93145595","buckets":[[0,"1.8340080864093422","2","10"],[0,"2","2.1810154653305154","68"]]}],[1729529700,{"count":"7243490","sum":"72464056.03309634","buckets":[[0,"1.8340080864093422","2","10"],[0,"2","2.1810154653305154","68"]]}],[1729529715,{"count":"7243880","sum":"72467935.35871512","buckets":[[0,"1.8340080864093422","2","10"],[0,"2","2.1810154653305154","68"]]}]]}]}}"#;
   110	    let r = parse(body, 200);
   111	    assert!(r.error.is_none());
   112	    assert_eq!(r.frame_count(), 1);
   113	    assert_eq!(r.frames.as_ref().expect("frames")[0].fields[1].name, "yMin");
   114	}
   115	
   116	// TestParseQueryStats
   117	fn stats(lines: &[&str], qt: QueryType) -> Vec<(String, String, f64)> {
   118	    let headers = lines.iter().map(|l| ("server-timing".to_string(), l.to_string())).collect();
   119	    let res = HttpResp { status: 200, headers, body: Vec::new() };
   120	    parse_query_stats(&res, qt).into_iter().map(|s| (s.display_name, s.unit, s.value)).collect()
   121	}
   122	fn st(d: &str, u: &str, v: f64) -> (String, String, f64) {
   123	    (d.to_string(), u.to_string(), v)
   124	}
   125	
   126	#[test]
   127	fn query_stats_absent_header() {
   128	    assert_eq!(stats(&[], QueryType::Range), vec![]);
   129	}
   130	#[test]
   131	fn query_stats_all_known_mimir_metrics() {
   132	    assert_eq!(
   133	        stats(&["bytes_processed;val=11188007, equivalent_samples_read;val=20000, querier_wall_time;dur=5215.074756, response_time;dur=1061.890603, samples_processed;val=17647"], QueryType::Range),
   134	        vec![
   135	            st("Range: Bytes processed", "decbytes", 11188007.0),
   136	            st("Range: Equivalent samples read", "short", 20000.0),
   137	            st("Range: Querier wall time", "ms", 5215.074756),
   138	            st("Range: Response time", "ms", 1061.890603),
   139	            st("Range: Samples processed", "short", 17647.0),
   140	        ]
   141	    );
   142	}
   143	#[test]
   144	fn query_stats_unknown_metrics_are_skipped() {
   145	    assert_eq!(stats(&["future_metric;val=42, bytes_processed;val=11188007"], QueryType::Range), vec![st("Range: Bytes processed", "decbytes", 11188007.0)]);
   146	}
   147	#[test]
   148	fn query_stats_malformed_entries_are_skipped() {
   149	    assert_eq!(stats(&["querier_wall_time, bytes_processed;val=notanumber, response_time;dur=10"], QueryType::Range), vec![st("Range: Response time", "ms", 10.0)]);
   150	}
   151	#[test]
   152	fn query_stats_instant_query_stats_are_labeled_instant() {
   153	    assert_eq!(stats(&["response_time;dur=10"], QueryType::Instant), vec![st("Instant: Response time", "ms", 10.0)]);
   154	}
   155	#[test]
   156	fn query_stats_exemplar_query_stats_are_labeled_exemplar() {
   157	    assert_eq!(stats(&["response_time;dur=10"], QueryType::Exemplar), vec![st("Exemplar: Response time", "ms", 10.0)]);
   158	}
   159	#[test]
   160	fn query_stats_multiple_header_lines_are_all_read() {
   161	    assert_eq!(stats(&["querier_wall_time;dur=5215.074756", "bytes_processed;val=11188007"], QueryType::Range), vec![st("Range: Querier wall time", "ms", 5215.074756), st("Range: Bytes processed", "decbytes", 11188007.0)]);
   162	}
   163	#[test]
   164	fn query_stats_a_second_entry_param_is_ignored() {
   165	    assert_eq!(stats(&[r#"response_time;dur=10;desc="foo""#], QueryType::Range), vec![st("Range: Response time", "ms", 10.0)]);
   166	}
   167	#[test]
   168	fn query_stats_a_stray_space_before_the_semicolon_still_matches() {
   169	    assert_eq!(stats(&["bytes_processed ;val=5"], QueryType::Range), vec![st("Range: Bytes processed", "decbytes", 5.0)]);
   170	}
   171	#[test]
   172	fn query_stats_a_metric_repeated_within_one_line_keeps_the_first() {
   173	    assert_eq!(stats(&["bytes_processed;val=1, bytes_processed;val=2"], QueryType::Range), vec![st("Range: Bytes processed", "decbytes", 1.0)]);
   174	}
   175	#[test]
   176	fn query_stats_a_metric_repeated_across_lines_keeps_the_first() {
   177	    assert_eq!(stats(&["bytes_processed;val=1", "bytes_processed;val=2"], QueryType::Range), vec![st("Range: Bytes processed", "decbytes", 1.0)]);
   178	}
   179	
   180	// TestParseResponse_QueryStats
   181	const UP: &str = r#"{"data":{"resultType":"vector","result":[{"metric":{"__name__":"up"},"value":[1.1,"2"]}]},"status":"success"}"#;
   182	
   183	fn with_timing(header: Option<&str>, body: &str) -> Vec<(String, String, f64)> {
   184	    let headers = match header {
   185	        Some(h) => vec![("server-timing".to_string(), h.to_string())],
   186	        None => Vec::new(),
   187	    };
   188	    let res = HttpResp { status: 200, headers, body: body.as_bytes().to_vec() };
   189	    let r = parse_response(&Query::default(), &res, QueryType::Range);
   190	    assert!(r.error.is_none());
   191	    let frames = r.frames.expect("frames");
   192	    assert_eq!(frames.len(), 1);
   193	    frames[0].meta.as_ref().expect("meta").stats.iter().map(|s| (s.display_name.clone(), s.unit.clone(), s.value)).collect()
   194	}
   195	
   196	#[test]
   197	fn parse_response_populates_frame_meta_stats_from_server_timing() {
   198	    assert_eq!(with_timing(Some("equivalent_samples_read;val=20000"), UP), vec![st("Range: Equivalent samples read", "short", 20000.0)]);
   199	}
   200	
   201	#[test]
   202	fn parse_response_leaves_stats_nil_when_header_absent() {
   203	    assert_eq!(with_timing(None, UP), vec![]);
   204	}
   205	
   206	#[test]
   207	fn parse_response_keeps_prometheus_stats_alongside_server_timing_stats() {
   208	    let body = r#"{"data":{"resultType":"vector","result":[{"metric":{"__name__":"up"},"value":[1.1,"2"]}],"stats":{"samples":{"totalQueryableSamples":30}}},"status":"success"}"#;
   209	    assert_eq!(with_timing(Some("equivalent_samples_read;val=20000"), body), vec![st("Total queryable samples", "", 30.0), st("Range: Equivalent samples read", "short", 20000.0)]);
   210	}
   211	
   212	// TestParseResponse_ErrorCases
   213	fn error_case(status: u16, body: &str) {
   214	    let r = parse(body, status);
   215	    let e = r.error.as_deref().expect("error");
   216	    assert!(e.contains("unexpected response"), "{e}");
   217	    assert_eq!(r.frame_count(), 1);
   218	}
   219	#[test]
   220	fn parse_response_error_500() {
   221	    error_case(500, r#"{"error":"internal server error"}"#);
   222	}
   223	#[test]
   224	fn parse_response_error_404() {
   225	    error_case(404, r#"{"error":"not found"}"#);
   226	}
   227	#[test]
   228	fn parse_response_error_401() {
   229	    error_case(401, r#"{"error":"unauthorized"}"#);
   230	}
   231	
   232	// TestProcessExemplars
   233	fn new_exemplar_frame(meta: Meta) -> Frame {
   234	    let mut tf = Field::new("Time", Values::Time(vec![SEC]));
   235	    tf.config = Some(FieldConfig { interval: 15000.0, ..Default::default() });
   236	    let mut vf = Field::new("Value", Values::F64(vec![1.0]));
   237	    vf.labels = Some(vec![("__name__".to_string(), "up".to_string())]);
   238	    let mut f = Frame::with_fields("", vec![tf, vf]);
   239	    f.ref_id = "A".to_string();
   240	    f.meta = Some(meta);
   241	    f
   242	}
   243	
   244	#[test]
   245	fn process_exemplars_preserves_the_first_exemplar_frames_meta() {
   246	    let custom = Some(Json::obj().with("resultType", "exemplar"));
   247	    let f1 = new_exemplar_frame(Meta { executed_query_string: "Expr: up\nStep: 15s".to_string(), custom: custom.clone(), ..Default::default() });
   248	    let f2 = new_exemplar_frame(Meta { custom, ..Default::default() });
   249	    let r = process_exemplars(DataResponse { frames: Some(vec![f1, f2]), ..Default::default() });
   250	    assert!(r.error.is_none());
   251	    let frames = r.frames.expect("frames");
   252	    assert_eq!(frames.len(), 1);
   253	    assert_eq!(frames[0].meta.as_ref().map(|m| m.executed_query_string.as_str()), Some("Expr: up\nStep: 15s"));
   254	}
     1	//! Port of promlib `converter/prom_test.go`: one test per upstream case, same inputs, same golden outputs.
     2	mod common;
     3	
     4	use common::{check_golden, read_bytes};
     5	use grafana::promlib::converter::{read_prometheus_style_result, time_from_float, time_from_loki_string};
     6	use grafana::promlib::frame::{DataResponse, Severity, Values};
     7	use json::Json;
     8	
     9	/// `runScenario`.
    10	fn run_scenario(name: &str) {
    11	    let b = read_bytes(&format!("converter/{name}.json"));
    12	    let rsp = read_prometheus_style_result(&b);
    13	    if name.contains("error") {
    14	        assert!(rsp.error.is_some(), "{name}: expected an error");
    15	        return;
    16	    }
    17	    let has = |sev: Severity| rsp.frames.as_ref().map(|fs| fs.iter().any(|f| f.meta.as_ref().map(|m| m.notices.iter().any(|n| n.severity == sev)).unwrap_or(false))).unwrap_or(false);
    18	    if name.contains("warnings") {
    19	        assert!(has(Severity::Warning), "{name}: expected a warning notice");
    20	    }
    21	    if name.contains("infos") {
    22	        assert!(has(Severity::Info), "{name}: expected an info notice");
    23	    }
    24	    assert!(rsp.error.is_none(), "{name}: {:?}", rsp.error);
    25	    check_golden(&format!("converter/{name}-frame.jsonc"), &rsp);
    26	}
    27	
    28	// TestReadPromFrames
    29	#[test]
    30	fn read_prom_frames_prom_labels() {
    31	    run_scenario("prom-labels");
    32	}
    33	#[test]
    34	fn read_prom_frames_prom_matrix() {
    35	    run_scenario("prom-matrix");
    36	}
    37	#[test]
    38	fn read_prom_frames_prom_matrix_with_nans() {
    39	    run_scenario("prom-matrix-with-nans");
    40	}
    41	#[test]
    42	fn read_prom_frames_prom_matrix_histogram_no_labels() {
    43	    run_scenario("prom-matrix-histogram-no-labels");
    44	}
    45	#[test]
    46	fn read_prom_frames_prom_matrix_histogram_partitioned() {
    47	    run_scenario("prom-matrix-histogram-partitioned");
    48	}
    49	#[test]
    50	fn read_prom_frames_prom_vector_histogram_no_labels() {
    51	    run_scenario("prom-vector-histogram-no-labels");
    52	}
    53	#[test]
    54	fn read_prom_frames_prom_vector() {
    55	    run_scenario("prom-vector");
    56	}
    57	#[test]
    58	fn read_prom_frames_prom_string() {
    59	    run_scenario("prom-string");
    60	}
    61	#[test]
    62	fn read_prom_frames_prom_scalar() {
    63	    run_scenario("prom-scalar");
    64	}
    65	#[test]
    66	fn read_prom_frames_prom_series() {
    67	    run_scenario("prom-series");
    68	}
    69	#[test]
    70	fn read_prom_frames_prom_warnings() {
    71	    run_scenario("prom-warnings");
    72	}
    73	#[test]
    74	fn read_prom_frames_prom_warnings_no_data() {
    75	    run_scenario("prom-warnings-no-data");
    76	}
    77	#[test]
    78	fn read_prom_frames_prom_infos() {
    79	    run_scenario("prom-infos");
    80	}
    81	#[test]
    82	fn read_prom_frames_prom_infos_no_data() {
    83	    run_scenario("prom-infos-no-data");
    84	}
    85	#[test]
    86	fn read_prom_frames_prom_error() {
    87	    run_scenario("prom-error");
    88	}
    89	#[test]
    90	fn read_prom_frames_prom_exemplars_a() {
    91	    run_scenario("prom-exemplars-a");
    92	}
    93	#[test]
    94	fn read_prom_frames_prom_exemplars_b() {
    95	    run_scenario("prom-exemplars-b");
    96	}
    97	#[test]
    98	fn read_prom_frames_prom_exemplars_diff_labels() {
    99	    run_scenario("prom-exemplars-diff-labels");
   100	}
   101	#[test]
   102	fn read_prom_frames_prom_query_range() {
   103	    run_scenario("prom-query-range");
   104	}
   105	#[test]
   106	fn read_prom_frames_prom_query_range_big() {
   107	    run_scenario("prom-query-range-big");
   108	}
   109	#[test]
   110	fn read_prom_frames_loki_streams_a() {
   111	    run_scenario("loki-streams-a");
   112	}
   113	#[test]
   114	fn read_prom_frames_loki_streams_b() {
   115	    run_scenario("loki-streams-b");
   116	}
   117	#[test]
   118	fn read_prom_frames_loki_streams_c() {
   119	    run_scenario("loki-streams-c");
   120	}
   121	
   122	// TestReadPrometheusQueryStats
   123	fn read_ok(payload: &str) -> DataResponse {
   124	    let rsp = read_prometheus_style_result(payload.as_bytes());
   125	    assert!(rsp.error.is_none(), "{:?}", rsp.error);
   126	    rsp
   127	}
   128	
   129	const INSTANT_RESULT: &str = r#"[{"metric":{"__name__":"up","instance":"localhost"},"value":[1710000000,"1"]}]"#;
   130	const PER_STEP: &str = "[[1710000000,12],[1710000060,18]]";
   131	
   132	fn check_stats_case(data: &str) {
   133	    let rsp = read_ok(&format!(r#"{{"status":"success","data":{data}}}"#));
   134	    let frames = rsp.frames.as_ref().expect("frames");
   135	    assert_eq!(frames.len(), 1);
   136	    let meta = frames[0].meta.as_ref().expect("meta");
   137	    assert_eq!(meta.stats.len(), 1);
   138	    assert_eq!(meta.stats[0].display_name, "Total queryable samples");
   139	    assert_eq!(meta.stats[0].unit, "");
   140	    assert_eq!(meta.stats[0].value, 30.0);
   141	    let custom = meta.custom.as_ref().expect("custom");
   142	    assert_eq!(custom.str_of("resultType"), "vector");
   143	    let samples = custom.path("stats.samples").expect("samples");
   144	    assert_eq!(samples.get("totalQueryableSamples").and_then(|v| v.as_f64()), Some(30.0));
   145	    assert_eq!(samples.get("totalQueryableSamplesPerStep").and_then(|v| v.as_arr()).map(|a| a.len()), Some(2));
   146	}
   147	
   148	#[test]
   149	fn query_stats_stats_after_result() {
   150	    check_stats_case(&format!(r#"{{"resultType":"vector","result":{INSTANT_RESULT},"stats":{{"samples":{{"totalQueryableSamples":30,"totalQueryableSamplesPerStep":{PER_STEP}}}}}}}"#));
   151	}
   152	
   153	#[test]
   154	fn query_stats_stats_before_result() {
   155	    check_stats_case(&format!(r#"{{"stats":{{"samples":{{"totalQueryableSamples":30,"totalQueryableSamplesPerStep":{PER_STEP}}}}},"resultType":"vector","result":{INSTANT_RESULT}}}"#));
   156	}
   157	
   158	#[test]
   159	fn query_stats_stats_and_result_before_result_type() {
   160	    check_stats_case(&format!(r#"{{"stats":{{"samples":{{"totalQueryableSamples":30,"totalQueryableSamplesPerStep":{PER_STEP}}}}},"result":{INSTANT_RESULT},"resultType":"vector"}}"#));
   161	}
   162	
   163	#[test]
   164	fn query_stats_zero_samples_is_retained_as_a_typed_stat() {
   165	    let rsp = read_ok(r#"{"status":"success","data":{"resultType":"vector","result":[],"stats":{"samples":{"totalQueryableSamples":0}}}}"#);
   166	    let frames = rsp.frames.as_ref().expect("frames");
   167	    assert_eq!(frames.len(), 1);
   168	    let meta = frames[0].meta.as_ref().expect("meta");
   169	    assert_eq!(meta.stats.len(), 1);
   170	    assert_eq!(meta.stats[0].value, 0.0);
   171	}
   172	
   173	#[test]
   174	fn query_stats_range_stats_are_attached_to_the_first_frame_only() {
   175	    let rsp = read_ok(r#"{"status":"success","data":{"resultType":"matrix","result":[{"metric":{"job":"one"},"values":[[1710000000,"1"]]},{"metric":{"job":"two"},"values":[[1710000000,"2"]]}],"stats":{"samples":{"totalQueryableSamples":2}}}}"#);
   176	    let frames = rsp.frames.as_ref().expect("frames");
   177	    assert_eq!(frames.len(), 2);
   178	    assert_eq!(frames[0].meta.as_ref().map(|m| m.stats.len()), Some(1));
   179	    assert!(frames[1].meta.as_ref().map(|m| m.stats.is_empty()).unwrap_or(true));
   180	}
   181	
   182	#[test]
   183	fn query_stats_empty_result_creates_a_metadata_only_frame() {
   184	    let rsp = read_ok(r#"{"status":"success","data":{"resultType":"matrix","result":[],"stats":{"samples":{"totalQueryableSamples":7}}}}"#);
   185	    let frames = rsp.frames.as_ref().expect("frames");
   186	    assert_eq!(frames.len(), 1);
   187	    assert_eq!(frames[0].name, "Query statistics");
   188	    assert!(frames[0].fields.is_empty());
   189	    assert_eq!(frames[0].meta.as_ref().map(|m| m.stats[0].value), Some(7.0));
   190	}
   191	
   192	#[test]
   193	fn query_stats_malformed_stats_do_not_fail_a_valid_query() {
   194	    let rsp = read_ok(&format!(r#"{{"status":"success","data":{{"resultType":"vector","result":{INSTANT_RESULT},"stats":"not-an-object"}}}}"#));
   195	    let frames = rsp.frames.as_ref().expect("frames");
   196	    assert_eq!(frames.len(), 1);
   197	    let meta = frames[0].meta.as_ref().expect("meta");
   198	    assert!(meta.stats.is_empty());
   199	    assert_eq!(meta.custom.as_ref().and_then(|c| c.get("stats")).and_then(|s| s.as_str()), Some("not-an-object"));
   200	}
   201	
   202	#[test]
   203	fn query_stats_malformed_total_omits_only_the_typed_stat() {
   204	    let rsp = read_ok(&format!(r#"{{"status":"success","data":{{"resultType":"vector","result":{INSTANT_RESULT},"stats":{{"samples":{{"totalQueryableSamples":"unknown","totalQueryableSamplesPerStep":{PER_STEP}}}}}}}}}"#));
   205	    let meta = rsp.frames.as_ref().expect("frames")[0].meta.as_ref().expect("meta");
   206	    assert!(meta.stats.is_empty());
   207	    assert!(meta.custom.as_ref().and_then(|c| c.get("stats")).map(|s| !s.is_null()).unwrap_or(false));
   208	}
   209	
   210	#[test]
   211	fn query_stats_missing_stats_leaves_existing_metadata_unchanged() {
   212	    let rsp = read_ok(&format!(r#"{{"status":"success","data":{{"resultType":"vector","result":{INSTANT_RESULT}}}}}"#));
   213	    let meta = rsp.frames.as_ref().expect("frames")[0].meta.as_ref().expect("meta");
   214	    assert!(meta.stats.is_empty());
   215	    assert_eq!(meta.custom, Some(Json::obj().with("resultType", "vector")));
   216	}
   217	
   218	// TestTimeConversions
   219	#[test]
   220	fn time_conversions() {
   221	    // 2020-09-14 15:22:25.479 UTC, millisecond precision
   222	    assert_eq!(time_from_float(1600096945.479), 1600096945479 * 1_000_000);
   223	    // 2022-02-16 16:50:46.277587968 UTC
   224	    assert_eq!(time_from_loki_string("1645030246277587968"), Ok(1645030246277587968));
   225	    // 2033-05-18 03:33:20 UTC
   226	    assert_eq!(time_from_loki_string("2000000000000000000"), Ok(2000000000 * 1_000_000_000));
   227	}
   228	
   229	// TestReadPrometheusNumericStrings
   230	fn assert_numeric_string_values(v: &Values) {
   231	    match v {
   232	        Values::F64(x) => {
   233	            assert_eq!(x.len(), 5);
   234	            assert_eq!(x[0], 1.25);
   235	            assert_eq!(x[1], 1.25e10);
   236	            assert!(x[2].is_nan());
   237	            assert_eq!(x[3], f64::INFINITY);
   238	            assert_eq!(x[4], f64::NEG_INFINITY);
   239	        }
   240	        other => panic!("not float64: {other:?}"),
   241	    }
   242	}
   243	
   244	#[test]
   245	fn numeric_strings_matrix() {
   246	    let rsp = read_ok(r#"{"status":"success","data":{"resultType":"matrix","result":[{"metric":{},"values":[[1,"1.25"],[2,"1.25e10"],[3,"NaN"],[4,"+Inf"],[5,"-Inf"]]}]}}"#);
   247	    let frames = rsp.frames.as_ref().expect("frames");
   248	    assert_eq!(frames.len(), 1);
   249	    assert_numeric_string_values(&frames[0].fields[1].values);
   250	}
   251	
   252	#[test]
   253	fn numeric_strings_exemplars() {
   254	    let rsp = read_ok(r#"{"status":"success","data":[{"seriesLabels":{},"exemplars":[{"value":"1.25","timestamp":1},{"value":"1.25e10","timestamp":2},{"value":"NaN","timestamp":3},{"value":"+Inf","timestamp":4},{"value":"-Inf","timestamp":5}]}]}"#);
   255	    let frames = rsp.frames.as_ref().expect("frames");
   256	    assert_eq!(frames.len(), 1);
   257	    assert_numeric_string_values(&frames[0].fields[1].values);
   258	}

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc 'nl -ba crates/grafana/tests/promlib_models.rs; nl -ba crates/grafana/tests/promlib_exemplar.rs' in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
     1	//! Port of promlib `models/query_test.go` (TestParse, TestRateInterval, TestAlignTimeRange,
     2	//! TestParseNumericFormatError, TestPromQueryFormatUnmarshalJSON). The scope, ad hoc filter and group-by cases
     3	//! are listed in tests-data/not-ported.md.
     4	use grafana::promlib::models::{DataQuery, HOUR, MIN, MS, Query, SEC, align_time_range, parse, parse_format};
     5	use json::Json;
     6	
     7	fn now() -> i64 {
     8	    grafana::util::now_ms() * MS + 123_456 // not aligned to anything, like time.Now()
     9	}
    10	
    11	/// `queryContext(json, timeRange, queryInterval)`.
    12	fn query_context(json: &str, from: i64, to: i64, interval: i64) -> DataQuery {
    13	    DataQuery { ref_id: "A".to_string(), interval, from, to, json: Json::parse(json).expect("query json"), ..Default::default() }
    14	}
    15	
    16	fn parse_ok(q: &DataQuery, scrape: &str) -> Query {
    17	    parse(q, scrape, false).expect("parse")
    18	}
    19	
    20	// TestParse
    21	#[test]
    22	fn parse_parsing_query_from_unified_alerting() {
    23	    let n = now();
    24	    let q = query_context(r#"{"expr": "go_goroutines", "refId": "A", "exemplar": true}"#, n, n + 12 * HOUR, 0);
    25	    let res = parse(&q, "15s", true).expect("parse");
    26	    assert!(!res.exemplar_query);
    27	}
    28	
    29	#[test]
    30	fn parse_parsing_query_model_with_step() {
    31	    let n = now();
    32	    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "refId": "A"}"#, n, n + 12 * HOUR, MIN);
    33	    assert_eq!(parse_ok(&q, "15s").step, 30 * SEC);
    34	}
    35	
    36	#[test]
    37	fn parse_parsing_query_model_without_step_parameter() {
    38	    let n = now();
    39	    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + HOUR, MIN);
    40	    assert_eq!(parse_ok(&q, "15s").step, 15 * SEC);
    41	}
    42	
    43	#[test]
    44	fn parse_parsing_query_model_with_high_interval_factor() {
    45	    let n = now();
    46	    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "intervalFactor": 10, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    47	    assert_eq!(parse_ok(&q, "15s").step, 20 * MIN);
    48	}
    49	
    50	#[test]
    51	fn parse_parsing_query_model_with_low_interval_factor() {
    52	    let n = now();
    53	    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    54	    assert_eq!(parse_ok(&q, "15s").step, 2 * MIN);
    55	}
    56	
    57	#[test]
    58	fn parse_parsing_query_model_specified_scrape_interval_in_the_data_source() {
    59	    let n = now();
    60	    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    61	    assert_eq!(parse_ok(&q, "240s").step, 4 * MIN);
    62	}
    63	
    64	#[test]
    65	fn parse_parsing_query_model_with_interval_variable() {
    66	    let n = now();
    67	    let mut q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__interval]})", "format": "time_series", "intervalFactor": 1, "intervalMs": 60000, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    68	    q.max_data_points = 2110;
    69	    let res = parse_ok(&q, "15s");
    70	    assert_eq!(res.expr, "rate(ALERTS{job=\"test\" [1m]})");
    71	    assert_eq!(res.step, 60 * SEC);
    72	}
    73	
    74	#[test]
    75	fn parse_parsing_query_model_with_interval_ms_variable() {
    76	    let n = now();
    77	    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__interval_ms]})", "format": "time_series", "intervalFactor": 1, "intervalMs": 60000, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    78	    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [120000]})");
    79	}
    80	
    81	#[test]
    82	fn parse_parsing_query_model_with_interval_ms_and_interval_variable() {
    83	    let n = now();
    84	    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__interval_ms]}) + rate(ALERTS{job=\"test\" [$__interval]})", "format": "time_series", "intervalFactor": 1, "intervalMs": 60000, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    85	    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [120000]}) + rate(ALERTS{job=\"test\" [2m]})");
    86	}
    87	
    88	#[test]
    89	fn parse_parsing_query_model_with_braced_interval_ms_and_interval_variable() {
    90	    let n = now();
    91	    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [${__interval_ms}]}) + rate(ALERTS{job=\"test\" [${__interval}]})", "format": "time_series", "intervalFactor": 1, "intervalMs": 60000, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    92	    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [120000]}) + rate(ALERTS{job=\"test\" [2m]})");
    93	}
    94	
    95	#[test]
    96	fn parse_parsing_query_model_with_range_variable() {
    97	    let n = now();
    98	    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__range]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    99	    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [172800s]})");
   100	}
   101	
   102	#[test]
   103	fn parse_parsing_query_model_with_range_s_variable() {
   104	    let n = now();
   105	    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__range_s]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
   106	    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [172800]})");
   107	}
   108	
   109	#[test]
   110	fn parse_parsing_query_model_with_braced_range_s_variable() {
   111	    let n = now();
   112	    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [${__range_s}s]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
   113	    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [172800s]})");
   114	}
   115	
   116	#[test]
   117	fn parse_parsing_query_model_with_range_s_variable_below_half_a_second() {
   118	    let n = now();
   119	    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__range_s]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 40 * MS, MIN);
   120	    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [0]})");
   121	}
   122	
   123	#[test]
   124	fn parse_parsing_query_model_with_range_s_variable_between_half_and_one_second() {
   125	    let n = now();
   126	    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__range_s]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 800 * MS, MIN);
   127	    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [1]})");
   128	}
   129	
   130	#[test]
   131	fn parse_parsing_query_model_with_range_ms_variable() {
   132	    let n = now();
   133	    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__range_ms]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
   134	    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [172800000]})");
   135	}
   136	
   137	#[test]
   138	fn parse_parsing_query_model_with_range_ms_variable_below_1s() {
   139	    let n = now();
   140	    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__range_ms]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 20 * MS, MIN);
   141	    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [20]})");
   142	}
   143	
   144	#[test]
   145	fn parse_parsing_query_model_with_rate_interval_variable() {
   146	    let n = now();
   147	    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__rate_interval]})", "format": "time_series", "intervalFactor": 1, "interval": "5m", "refId": "A"}"#, n, n + 5 * MIN, MIN);
   148	    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [20m0s]})");
   149	}
   150	
   151	#[test]
   152	fn parse_parsing_query_model_with_rate_interval_variable_in_expr_and_interval() {
   153	    let n = now();
   154	    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__rate_interval]})", "format": "time_series", "intervalFactor": 1, "interval": "$__rate_interval", "refId": "A"}"#, n, n + 5 * MIN, MIN);
   155	    let res = parse_ok(&q, "15s");
   156	    assert_eq!(res.expr, "rate(ALERTS{job=\"test\" [1m0s]})");
   157	    assert_eq!(res.step, MIN);
   158	}
   159	
   160	#[test]
   161	fn parse_parsing_query_model_with_rate_interval_ms_variable() {
   162	    let n = now();
   163	    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__rate_interval_ms]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, 2 * MIN);
   164	    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [135000]})");
   165	}
   166	
   167	#[test]
   168	fn parse_parsing_query_model_with_rate_interval_ms_and_rate_interval_variable() {
   169	    let n = now();
   170	    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__rate_interval_ms]}) + rate(ALERTS{job=\"test\" [$__rate_interval]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, 2 * MIN);
   171	    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [135000]}) + rate(ALERTS{job=\"test\" [2m15s]})");
   172	}
   173	
   174	#[test]
   175	fn parse_parsing_query_model_with_legacy_datasource_reference() {
   176	    let n = now();
   177	    let q = query_context(r#"{"datasource": "hello", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, 2 * MIN);
   178	    assert_eq!(parse_ok(&q, "15s").ref_id, "A");
   179	}
   180	
   181	#[test]
   182	fn parse_parsing_query_model_with_braced_rate_interval_ms_and_rate_interval_variable() {
   183	    let n = now();
   184	    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [${__rate_interval_ms}]}) + rate(ALERTS{job=\"test\" [${__rate_interval}]})", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, 2 * MIN);
   185	    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [135000]}) + rate(ALERTS{job=\"test\" [2m15s]})");
   186	}
   187	
   188	#[test]
   189	fn parse_parsing_query_model_of_range_query() {
   190	    let n = now();
   191	    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "intervalFactor": 1, "refId": "A", "range": true}"#, n, n + 48 * HOUR, MIN);
   192	    assert!(parse_ok(&q, "15s").range_query);
   193	}
   194	
   195	#[test]
   196	fn parse_parsing_query_model_of_range_and_instant_query() {
   197	    let n = now();
   198	    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "intervalFactor": 1, "refId": "A", "range": true, "instant": true}"#, n, n + 48 * HOUR, MIN);
   199	    let res = parse_ok(&q, "15s");
   200	    assert!(res.range_query);
   201	    assert!(res.instant_query);
   202	}
   203	
   204	#[test]
   205	fn parse_parsing_query_model_of_with_no_query_type() {
   206	    let n = now();
   207	    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
   208	    assert!(parse_ok(&q, "15s").range_query);
   209	}
   210	
   211	// TestRateInterval
   212	/// `mockQuery`: default time range of one hour.
   213	fn mock_query(expr: &str, interval: &str, interval_ms: i64, range: Option<(i64, i64)>) -> DataQuery {
   214	    let n = now();
   215	    let (from, to) = range.unwrap_or((n, n + HOUR));
   216	    let json = format!(r#"{{"expr": "{expr}", "format": "time_series", "interval": "{interval}", "intervalMs": {interval_ms}, "intervalFactor": 1, "refId": "A"}}"#);
   217	    DataQuery { ref_id: "A".to_string(), interval: interval_ms * MS, from, to, json: Json::parse(&json).expect("json"), ..Default::default() }
   218	}
   219	
   220	fn rate_case(expr: &str, interval: &str, interval_ms: i64, scrape: &str, range: Option<i64>, want_expr: &str, want_step: i64) {
   221	    let n = now();
   222	    let mut q = mock_query(expr, interval, interval_ms, range.map(|d| (n, n + d)));
   223	    q.max_data_points = 12384;
   224	    let res = parse_ok(&q, scrape);
   225	    assert_eq!(res.expr, want_expr);
   226	    assert_eq!(res.step, want_step);
   227	}
   228	
   229	const RATE: &str = "rate(rpc_durations_seconds_count[$__rate_interval])";
   230	
   231	#[test]
   232	fn rate_interval_interval_ms_100s_min_step_override_150s_and_scrape_interval_30s() {
   233	    rate_case(RATE, "150s", 100000, "30s", None, "rate(rpc_durations_seconds_count[10m0s])", 150 * SEC);
   234	}
   235	#[test]
   236	fn rate_interval_interval_ms_120s_min_step_override_150s_and_ds_scrape_interval_30s() {
   237	    rate_case(RATE, "150s", 120000, "30s", None, "rate(rpc_durations_seconds_count[10m0s])", 150 * SEC);
   238	}
   239	#[test]
   240	fn rate_interval_interval_ms_120s_min_step_auto_and_ds_scrape_interval_30s() {
   241	    rate_case(RATE, "120s", 120000, "30s", None, "rate(rpc_durations_seconds_count[8m0s])", 120 * SEC);
   242	}
   243	#[test]
   244	fn rate_interval_interval_and_min_step_automatic_ds_scrape_interval_30s_time_range_1h() {
   245	    rate_case(RATE, "30s", 30000, "30s", Some(HOUR), "rate(rpc_durations_seconds_count[2m0s])", 30 * SEC);
   246	}
   247	#[test]
   248	fn rate_interval_min_step_is_rate_interval_ds_scrape_interval_30s_time_range_1h() {
   249	    rate_case(RATE, "$__rate_interval", 30000, "30s", Some(HOUR), "rate(rpc_durations_seconds_count[2m0s])", 2 * MIN);
   250	}
   251	#[test]
   252	fn rate_interval_min_step_is_rate_interval_ds_scrape_interval_30s_time_range_2d() {
   253	    rate_case(RATE, "$__rate_interval", 120000, "30s", Some(48 * HOUR), "rate(rpc_durations_seconds_count[2m30s])", 150 * SEC);
   254	}
   255	#[test]
   256	fn rate_interval_min_step_is_interval_ds_scrape_interval_15s_time_range_2d() {
   257	    rate_case(RATE, "$__interval", 120000, "15s", Some(48 * HOUR), "rate(rpc_durations_seconds_count[8m0s])", 120 * SEC);
   258	}
   259	
   260	#[test]
   261	fn rate_interval_min_step_is_auto_and_ds_scrape_interval_30s_and_time_range_1_hour() {
   262	    let n = now();
   263	    let json = r#"{
   264	        "datasource":{"type":"prometheus","uid":"zxS5e5W4k"},
   265	        "datasourceId":38,
   266	        "editorMode":"code",
   267	        "exemplar":false,
   268	        "expr":"sum(rate(process_cpu_seconds_total[$__rate_interval]))",
   269	        "instant":false,
   270	        "interval":"",
   271	        "intervalMs":30000,
   272	        "key":"Q-f96b6729-c47a-4ea8-8f71-a79774cf9bd5-0",
   273	        "legendFormat":"__auto",
   274	        "maxDataPoints":1613,
   275	        "range":true,
   276	        "refId":"G",
   277	        "requestId":"1G",
   278	        "utcOffsetSec":3600
   279	    }"#;
   280	    let q = DataQuery { ref_id: "G".to_string(), max_data_points: 1613, interval: 30 * SEC, from: n, to: n + HOUR, json: Json::parse(json).expect("json"), ..Default::default() };
   281	    let res = parse_ok(&q, "30s");
   282	    assert_eq!(res.expr, "sum(rate(process_cpu_seconds_total[2m0s]))");
   283	    assert_eq!(res.step, 30 * SEC);
   284	}
   285	
   286	#[test]
   287	fn rate_interval_min_step_is_auto_and_ds_scrape_interval_15s_and_time_range_5_minutes() {
   288	    let n = now();
   289	    let json = r#"{
   290	        "datasource": {"type": "prometheus", "uid": "2z9d6ElGk"},
   291	        "editorMode": "code",
   292	        "expr": "sum(rate(cache_requests_total[$__rate_interval]))",
   293	        "legendFormat": "__auto",
   294	        "range": true,
   295	        "refId": "A",
   296	        "exemplar": false,
   297	        "requestId": "1A",
   298	        "utcOffsetSec": 0,
   299	        "interval": "",
   300	        "datasourceId": 508,
   301	        "intervalMs": 15000,
   302	        "maxDataPoints": 1055
   303	    }"#;
   304	    let q = DataQuery { ref_id: "A".to_string(), max_data_points: 1055, interval: 15 * SEC, from: n, to: n + 5 * MIN, json: Json::parse(json).expect("json"), ..Default::default() };
   305	    let res = parse_ok(&q, "15s");
   306	    assert_eq!(res.expr, "sum(rate(cache_requests_total[1m0s]))");
   307	    assert_eq!(res.step, 15 * SEC);
   308	}
   309	
   310	// TestAlignTimeRange
   311	#[test]
   312	fn align_time_range_second_step() {
   313	    assert_eq!(align_time_range(1664816826 * SEC, 10 * SEC, 0), 1664816820 * SEC);
   314	}
   315	#[test]
   316	fn align_time_range_millisecond_step() {
   317	    assert_eq!(align_time_range(1664816825 * SEC + 5 * MS, 10 * MS, 0), 1664816825 * SEC);
   318	}
   319	#[test]
   320	fn align_time_range_second_step_with_offset() {
   321	    assert_eq!(align_time_range(1664816825 * SEC + 5 * MS, 2 * SEC, -3), 1664816825 * SEC);
   322	}
   323	#[test]
   324	fn align_time_range_1w_step_with_range_date_of_monday_changes_the_range_to_a_thursday() {
   325	    let monday = 1704672000;
   326	    let thursday = 1704326400;
   327	    assert_eq!(align_time_range(monday * SEC, 604800 * SEC, 0), thursday * SEC);
   328	}
   329	
   330	// TestParseNumericFormatError
   331	fn numeric_format(format: &str) {
   332	    let n = now();
   333	    let q = query_context(&format!(r#"{{"expr": "up", "format": {format}, "refId": "A"}}"#), n, n + 12 * HOUR, 0);
   334	    let res = parse_ok(&q, "15s");
   335	    assert_eq!(res.expr, "up");
   336	}
   337	#[test]
   338	fn numeric_format_0_should_be_handled_gracefully() {
   339	    numeric_format("0");
   340	}
   341	#[test]
   342	fn numeric_format_1_should_default_to_time_series() {
   343	    numeric_format("1");
   344	}
   345	#[test]
   346	fn numeric_format_2_should_map_to_table() {
   347	    numeric_format("2");
   348	}
   349	#[test]
   350	fn numeric_format_3_should_map_to_heatmap() {
   351	    numeric_format("3");
   352	}
   353	#[test]
   354	fn numeric_format_unknown_number_should_default_to_time_series() {
   355	    numeric_format("999");
   356	}
   357	#[test]
   358	fn numeric_format_float_should_be_handled_gracefully() {
   359	    numeric_format("1.5");
   360	}
   361	
   362	// TestPromQueryFormatUnmarshalJSON
   363	fn format_of(json: &str) -> &'static str {
   364	    let v = Json::parse(json).expect("json");
   365	    parse_format(v.get("format"))
   366	}
   367	#[test]
   368	fn query_format_valid_time_series() {
   369	    assert_eq!(format_of(r#"{"format": "time_series"}"#), "time_series");
   370	}
   371	#[test]
   372	fn query_format_valid_table() {
   373	    assert_eq!(format_of(r#"{"format": "table"}"#), "table");
   374	}
   375	#[test]
   376	fn query_format_valid_heatmap() {
   377	    assert_eq!(format_of(r#"{"format": "heatmap"}"#), "heatmap");
   378	}
   379	#[test]
   380	fn query_format_invalid_unknown_format_string() {
   381	    assert_eq!(format_of(r#"{"format": "invalid_format"}"#), "time_series");
   382	}
   383	#[test]
   384	fn query_format_invalid_empty_string() {
   385	    assert_eq!(format_of(r#"{"format": ""}"#), "time_series");
   386	}
   387	#[test]
   388	fn query_format_invalid_random_string() {
   389	    assert_eq!(format_of(r#"{"format": "foobar"}"#), "time_series");
   390	}
   391	#[test]
   392	fn query_format_invalid_typo_in_format() {
   393	    assert_eq!(format_of(r#"{"format": "time_serie"}"#), "time_series");
   394	}
     1	//! Port of promlib `querydata/exemplar/sampler_stddev_test.go` and `sampler_noop_test.go` (golden files).
     2	mod common;
     3	
     4	use common::check_golden;
     5	use grafana::promlib::exemplar::{Exemplar, Framer, NoOpSampler, Sampler, StdDevSampler};
     6	use grafana::promlib::frame::DataResponse;
     7	use grafana::promlib::models::SEC;
     8	
     9	/// `generateTestExemplars`: one exemplar per second from start (inclusive) to end (exclusive), value = Unix seconds.
    10	fn generate(start: i64, end: i64) -> Vec<Exemplar> {
    11	    let mut out = Vec::new();
    12	    let mut t = start;
    13	    while t < end {
    14	        out.push(Exemplar { series_labels: Vec::new(), source: usize::MAX, row_idx: 0, value: (t / SEC) as f64, timestamp: t });
    15	        t += SEC;
    16	    }
    17	    out
    18	}
    19	
    20	/// `CheckGoldenJSONFramer` = `backend.FrameResponse(framer)`.
    21	fn framer_response(sampler: &mut dyn Sampler) -> DataResponse {
    22	    let frames = Framer::new().frames(sampler);
    23	    DataResponse { frames: Some(frames), ..Default::default() }
    24	}
    25	
    26	#[test]
    27	fn std_dev_sampler_standard_deviation_sampler() {
    28	    let mut s = StdDevSampler::default();
    29	    s.set_step(600 * SEC);
    30	    for e in generate(0, 100000 * SEC) {
    31	        s.add(e);
    32	    }
    33	    check_golden("exemplar/stddev_sampler.jsonc", &framer_response(&mut s));
    34	}
    35	
    36	#[test]
    37	fn no_op_sampler_no_op_sampler() {
    38	    let mut s = NoOpSampler::default();
    39	    for e in generate(0, 2000 * SEC) {
    40	        s.add(e);
    41	    }
    42	    check_golden("exemplar/noop_sampler.jsonc", &framer_response(&mut s));
    43	}

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "nl -ba crates/grafana/tests/common/mod.rs; nl -ba upstream-prometheus/pkg/promlib/resource/resource_test.go | sed -n '1,138p'; nl -ba upstream-prometheus/pkg/promlib/client/client_test.go" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
     1	//! Shared helpers for the promlib golden tests: the plugin SDK's golden file format and `assert.JSONEq`.
     2	#![allow(dead_code)]
     3	use json::Json;
     4	use std::path::PathBuf;
     5	
     6	/// `tests-data/promlib/<rel>`.
     7	pub fn data_path(rel: &str) -> PathBuf {
     8	    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests-data/promlib").join(rel)
     9	}
    10	
    11	pub fn read_bytes(rel: &str) -> Vec<u8> {
    12	    std::fs::read(data_path(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
    13	}
    14	
    15	/// The JSON part of a `.jsonc` golden file (`readGoldenJSONFile`: the third chunk split on the machine marker).
    16	pub fn golden(rel: &str) -> Json {
    17	    let raw = String::from_utf8(read_bytes(rel)).expect("utf8");
    18	    let marker = "//  🌟 This was machine generated.  Do not edit. 🌟";
    19	    let chunks: Vec<&str> = raw.split(marker).collect();
    20	    assert!(chunks.len() >= 3, "no golden data found in {rel}");
    21	    Json::parse(chunks[2]).unwrap_or_else(|e| panic!("golden {rel} is not JSON: {e:?}"))
    22	}
    23	
    24	/// testify `assert.JSONEq`: same values, object keys in any order, numbers compared as float64.
    25	pub fn json_eq(a: &Json, b: &Json, path: &str) -> Result<(), String> {
    26	    match (a, b) {
    27	        (Json::Obj(x), Json::Obj(y)) => {
    28	            if x.len() != y.len() {
    29	                let kx: Vec<&str> = x.iter().map(|p| p.0.as_str()).collect();
    30	                let ky: Vec<&str> = y.iter().map(|p| p.0.as_str()).collect();
    31	                return Err(format!("{path}: keys {kx:?} vs {ky:?}"));
    32	            }
    33	            for (k, v) in x.iter() {
    34	                match y.iter().find(|p| &p.0 == k) {
    35	                    Some((_, w)) => json_eq(v, w, &format!("{path}.{k}"))?,
    36	                    None => return Err(format!("{path}: missing key {k}")),
    37	                }
    38	            }
    39	            Ok(())
    40	        }
    41	        (Json::Arr(x), Json::Arr(y)) => {
    42	            if x.len() != y.len() {
    43	                return Err(format!("{path}: length {} vs {}", x.len(), y.len()));
    44	            }
    45	            for (i, (v, w)) in x.iter().zip(y.iter()).enumerate() {
    46	                json_eq(v, w, &format!("{path}[{i}]"))?;
    47	            }
    48	            Ok(())
    49	        }
    50	        (Json::Int(_) | Json::Float(_), Json::Int(_) | Json::Float(_)) => {
    51	            let fa = a.as_f64().unwrap_or(f64::NAN);
    52	            let fb = b.as_f64().unwrap_or(f64::NAN);
    53	            if fa == fb { Ok(()) } else { Err(format!("{path}: {fa} vs {fb}")) }
    54	        }
    55	        _ => {
    56	            if a == b {
    57	                Ok(())
    58	            } else {
    59	                Err(format!("{path}: {} vs {}", a.stringify(), b.stringify()))
    60	            }
    61	        }
    62	    }
    63	}
    64	
    65	/// `experimental.CheckGoldenJSONResponse`: the response JSON equals the golden file.
    66	pub fn check_golden(rel: &str, dr: &grafana::promlib::frame::DataResponse) {
    67	    let expected = golden(rel);
    68	    let text = dr.to_json_string();
    69	    let actual = Json::parse(&text).unwrap_or_else(|e| panic!("our JSON does not parse: {e:?}\n{text}"));
    70	    if let Err(e) = json_eq(&expected, &actual, "$") {
    71	        panic!("golden {rel} differs: {e}");
    72	    }
    73	}
     1	package resource_test
     2	
     3	import (
     4		"bytes"
     5		"compress/flate"
     6		"compress/gzip"
     7		"context"
     8		"encoding/json"
     9		"io"
    10		"net/http"
    11		"net/http/httptest"
    12		"net/url"
    13		"strconv"
    14		"testing"
    15	
    16		"github.com/andybalholm/brotli"
    17		"github.com/grafana/grafana-plugin-sdk-go/backend"
    18		"github.com/grafana/grafana-plugin-sdk-go/backend/log"
    19		scope "github.com/grafana/grafana/apps/scope/pkg/apis/scope/v0alpha1"
    20		"github.com/stretchr/testify/assert"
    21		"github.com/stretchr/testify/require"
    22	
    23		"github.com/grafana/grafana-prometheus-datasource/pkg/promlib/models"
    24		"github.com/grafana/grafana-prometheus-datasource/pkg/promlib/resource"
    25	)
    26	
    27	// newResource parses settings and builds a Resource the way library.go's
    28	// newInstanceSettings does, instead of resource.New parsing settings itself.
    29	func newResource(t *testing.T, httpClient *http.Client, settings backend.DataSourceInstanceSettings, plog log.Logger) (*resource.Resource, error) {
    30		t.Helper()
    31		jsonData, err := models.ParsePromOptions(settings)
    32		if err != nil {
    33			return nil, err
    34		}
    35		return resource.New(httpClient, settings, jsonData.HTTPMethod, plog)
    36	}
    37	
    38	type mockRoundTripper struct {
    39		Response        *http.Response
    40		Err             error
    41		customRoundTrip func(req *http.Request) (*http.Response, error)
    42	}
    43	
    44	func (m *mockRoundTripper) RoundTrip(req *http.Request) (*http.Response, error) {
    45		if m.customRoundTrip != nil {
    46			return m.customRoundTrip(req)
    47		}
    48		return m.Response, m.Err
    49	}
    50	
    51	func setup() (*http.Client, backend.DataSourceInstanceSettings, log.Logger) {
    52		// Mock HTTP Response
    53		mockResponse := &http.Response{
    54			StatusCode: 200,
    55			Body:       io.NopCloser(bytes.NewReader([]byte(`{"message": "success"}`))),
    56			Header:     make(http.Header),
    57		}
    58	
    59		// Create a mock RoundTripper
    60		mockTransport := &mockRoundTripper{
    61			Response: mockResponse,
    62		}
    63	
    64		// Create a mock HTTP client using the mock RoundTripper
    65		mockClient := &http.Client{
    66			Transport: mockTransport,
    67		}
    68	
    69		settings := backend.DataSourceInstanceSettings{
    70			ID:       1,
    71			URL:      "http://mock-server",
    72			JSONData: []byte(`{}`),
    73		}
    74	
    75		logger := log.DefaultLogger
    76	
    77		return mockClient, settings, logger
    78	}
    79	
    80	func TestNewResource(t *testing.T) {
    81		mockClient, settings, logger := setup()
    82		res, err := newResource(t, mockClient, settings, logger)
    83		require.NoError(t, err)
    84		assert.NotNil(t, res)
    85	}
    86	
    87	func TestResource_Execute(t *testing.T) {
    88		mockClient, settings, logger := setup()
    89		res, err := newResource(t, mockClient, settings, logger)
    90		require.NoError(t, err)
    91	
    92		req := &backend.CallResourceRequest{
    93			Path: "/api/v1/labels",
    94			URL:  "/api/v1/labels",
    95		}
    96		ctx := context.Background()
    97	
    98		resp, err := res.Execute(ctx, req)
    99		require.NoError(t, err)
   100		assert.NotNil(t, resp)
   101	}
   102	
   103	func TestResource_ExecuteRejectsDisallowedPath(t *testing.T) {
   104		mockClient, settings, logger := setup()
   105		res, err := newResource(t, mockClient, settings, logger)
   106		require.NoError(t, err)
   107	
   108		resp, err := res.Execute(context.Background(), &backend.CallResourceRequest{
   109			Path: "-/reload",
   110			URL:  "/-/reload",
   111		})
   112		require.NoError(t, err)
   113		require.Equal(t, http.StatusForbidden, resp.Status)
   114	}
   115	
   116	func TestResource_GetSuggestionsRejectsLabelNamePathInjection(t *testing.T) {
   117		mockClient, _, logger := setup()
   118		settings := backend.DataSourceInstanceSettings{
   119			ID:       1,
   120			URL:      "http://localhost:9090",
   121			JSONData: []byte(`{"httpMethod": "GET"}`),
   122		}
   123		res, err := newResource(t, mockClient, settings, logger)
   124		require.NoError(t, err)
   125	
   126		suggestionReq := resource.SuggestionRequest{
   127			LabelName: "../../-/reload",
   128			Queries:   []string{"up"},
   129		}
   130		body, err := json.Marshal(suggestionReq)
   131		require.NoError(t, err)
   132	
   133		resp, err := res.GetSuggestions(context.Background(), &backend.CallResourceRequest{Body: body})
   134		require.NoError(t, err)
   135		require.Equal(t, http.StatusForbidden, resp.Status)
   136	}
   137	
   138	func TestResource_ExecuteDecodesCompressedResponse(t *testing.T) {
     1	package client
     2	
     3	import (
     4		"context"
     5		"fmt"
     6		"io"
     7		"net/http"
     8		"testing"
     9		"time"
    10	
    11		"github.com/grafana/grafana-plugin-sdk-go/backend"
    12		"github.com/stretchr/testify/require"
    13	
    14		"github.com/grafana/grafana-prometheus-datasource/pkg/promlib/models"
    15	)
    16	
    17	type MockDoer struct {
    18		Req *http.Request
    19	}
    20	
    21	func (doer *MockDoer) Do(req *http.Request) (*http.Response, error) {
    22		doer.Req = req
    23		return &http.Response{
    24			StatusCode: http.StatusOK,
    25			Status:     "200 OK",
    26		}, nil
    27	}
    28	
    29	func TestClient(t *testing.T) {
    30		t.Run("QueryResource", func(t *testing.T) {
    31			t.Run("POST-configured client forwards POST request as POST", func(t *testing.T) {
    32				doer := &MockDoer{}
    33				client := NewClient(doer, http.MethodPost, "http://localhost:9090", "60s")
    34	
    35				req := &backend.CallResourceRequest{
    36					PluginContext: backend.PluginContext{},
    37					Path:          "/api/v1/series",
    38					Method:        http.MethodPost,
    39					URL:           "/api/v1/series",
    40					Body:          []byte("match%5B%5D=ALERTS&start=1655271408&end=1655293008"),
    41				}
    42				res, err := client.QueryResource(context.Background(), req)
    43				defer func() {
    44					if res != nil && res.Body != nil {
    45						if err := res.Body.Close(); err != nil {
    46							fmt.Println("Error", "err", err)
    47						}
    48					}
    49				}()
    50				require.NoError(t, err)
    51				require.NotNil(t, doer.Req)
    52				require.Equal(t, http.MethodPost, doer.Req.Method)
    53				body, err := io.ReadAll(doer.Req.Body)
    54				require.NoError(t, err)
    55				require.Equal(t, "match%5B%5D=ALERTS&start=1655271408&end=1655293008", string(body))
    56				require.Equal(t, "http://localhost:9090/api/v1/series", doer.Req.URL.String())
    57			})
    58	
    59			t.Run("GET-configured client converts incoming POST to GET", func(t *testing.T) {
    60				doer := &MockDoer{}
    61				client := NewClient(doer, http.MethodGet, "http://localhost:9090", "60s")
    62	
    63				req := &backend.CallResourceRequest{
    64					PluginContext: backend.PluginContext{},
    65					Path:          "/api/v1/query",
    66					Method:        http.MethodPost,
    67					URL:           "/api/v1/query",
    68					Body:          []byte("query=up&time=1234567890"),
    69				}
    70				res, err := client.QueryResource(context.Background(), req)
    71				defer func() {
    72					if res != nil && res.Body != nil {
    73						if err := res.Body.Close(); err != nil {
    74							fmt.Println("Error", "err", err)
    75						}
    76					}
    77				}()
    78				require.NoError(t, err)
    79				require.NotNil(t, doer.Req)
    80				require.Equal(t, http.MethodGet, doer.Req.Method)
    81				require.Contains(t, doer.Req.URL.RawQuery, "query=up")
    82				require.Contains(t, doer.Req.URL.RawQuery, "time=1234567890")
    83			})
    84	
    85			t.Run("GET request is always respected regardless of configured method", func(t *testing.T) {
    86				doer := &MockDoer{}
    87				client := NewClient(doer, http.MethodPost, "http://localhost:9090", "60s")
    88	
    89				req := &backend.CallResourceRequest{
    90					PluginContext: backend.PluginContext{},
    91					Path:          "/api/v1/series",
    92					Method:        http.MethodGet,
    93					URL:           "api/v1/series?match%5B%5D=ALERTS&start=1655272558&end=1655294158",
    94				}
    95				res, err := client.QueryResource(context.Background(), req)
    96				defer func() {
    97					if res != nil && res.Body != nil {
    98						if err := res.Body.Close(); err != nil {
    99							fmt.Println("Error", "err", err)
   100						}
   101					}
   102				}()
   103				require.NoError(t, err)
   104				require.NotNil(t, doer.Req)
   105				require.Equal(t, http.MethodGet, doer.Req.Method)
   106				body, err := io.ReadAll(doer.Req.Body)
   107				require.NoError(t, err)
   108				require.Equal(t, []byte{}, body)
   109				require.Equal(t, "http://localhost:9090/api/v1/series?match%5B%5D=ALERTS&start=1655272558&end=1655294158", doer.Req.URL.String())
   110			})
   111	
   112			t.Run("GET-configured client preserves existing query params when converting POST", func(t *testing.T) {
   113				doer := &MockDoer{}
   114				client := NewClient(doer, http.MethodGet, "http://localhost:9090", "60s")
   115	
   116				req := &backend.CallResourceRequest{
   117					PluginContext: backend.PluginContext{},
   118					Path:          "/api/v1/query",
   119					Method:        http.MethodPost,
   120					URL:           "/api/v1/query?timeout=30",
   121					Body:          []byte("query=up"),
   122				}
   123				res, err := client.QueryResource(context.Background(), req)
   124				defer func() {
   125					if res != nil && res.Body != nil {
   126						if err := res.Body.Close(); err != nil {
   127							fmt.Println("Error", "err", err)
   128						}
   129					}
   130				}()
   131				require.NoError(t, err)
   132				require.NotNil(t, doer.Req)
   133				require.Equal(t, http.MethodGet, doer.Req.Method)
   134				require.Contains(t, doer.Req.URL.RawQuery, "timeout=30")
   135				require.Contains(t, doer.Req.URL.RawQuery, "query=up")
   136			})
   137		})
   138	
   139		t.Run("QueryRange", func(t *testing.T) {
   140			doer := &MockDoer{}
   141	
   142			t.Run("sends correct POST query", func(t *testing.T) {
   143				client := NewClient(doer, http.MethodPost, "http://localhost:9090", "60s")
   144				req := &models.Query{
   145					Expr:       "rate(ALERTS{job=\"test\" [$__rate_interval]})",
   146					Start:      time.Unix(0, 0),
   147					End:        time.Unix(1234, 0),
   148					RangeQuery: true,
   149					Step:       1 * time.Second,
   150				}
   151				res, err := client.QueryRange(context.Background(), req)
   152				defer func() {
   153					if res != nil && res.Body != nil {
   154						if err := res.Body.Close(); err != nil {
   155							fmt.Println("Error", "err", err)
   156						}
   157					}
   158				}()
   159				require.NoError(t, err)
   160				require.NotNil(t, doer.Req)
   161				require.Equal(t, http.MethodPost, doer.Req.Method)
   162				require.Equal(t, "application/x-www-form-urlencoded", doer.Req.Header.Get("Content-Type"))
   163				body, err := io.ReadAll(doer.Req.Body)
   164				require.NoError(t, err)
   165				require.Equal(t, []byte("end=1234&query=rate%28ALERTS%7Bjob%3D%22test%22+%5B%24__rate_interval%5D%7D%29&start=0&step=1&timeout=60s"), body)
   166				require.Equal(t, "http://localhost:9090/api/v1/query_range", doer.Req.URL.String())
   167			})
   168	
   169			t.Run("sends correct GET query", func(t *testing.T) {
   170				client := NewClient(doer, http.MethodGet, "http://localhost:9090", "60s")
   171				req := &models.Query{
   172					Expr:       "rate(ALERTS{job=\"test\" [$__rate_interval]})",
   173					Start:      time.Unix(0, 0),
   174					End:        time.Unix(1234, 0),
   175					RangeQuery: true,
   176					Step:       1 * time.Second,
   177				}
   178				res, err := client.QueryRange(context.Background(), req)
   179				defer func() {
   180					if res != nil && res.Body != nil {
   181						if err := res.Body.Close(); err != nil {
   182							fmt.Println("Error", "err", err)
   183						}
   184					}
   185				}()
   186				require.NoError(t, err)
   187				require.NotNil(t, doer.Req)
   188				require.Equal(t, http.MethodGet, doer.Req.Method)
   189				body, err := io.ReadAll(doer.Req.Body)
   190				require.NoError(t, err)
   191				require.Equal(t, []byte{}, body)
   192				require.Equal(t, "http://localhost:9090/api/v1/query_range?end=1234&query=rate%28ALERTS%7Bjob%3D%22test%22+%5B%24__rate_interval%5D%7D%29&start=0&step=1&timeout=60s", doer.Req.URL.String())
   193			})
   194		})
   195	}

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "nl -ba upstream-prometheus/pkg/promlib/querydata/request_test.go | sed -n '25,385p'; rg -n '"'^func Test|t.Run|require'"\\.|assert\\.' upstream-prometheus/pkg/promlib/querydata/response_test.go" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
    25		"github.com/grafana/grafana-prometheus-datasource/pkg/promlib/querydata"
    26	)
    27	
    28	func TestPrometheus_parseTimeSeriesResponse(t *testing.T) {
    29		t.Run("exemplars response should be sampled and parsed normally", func(t *testing.T) {
    30			exemplars := []apiv1.ExemplarQueryResult{
    31				{
    32					SeriesLabels: p.LabelSet{
    33						"__name__": "tns_request_duration_seconds_bucket",
    34						"instance": "app:80",
    35						"job":      "tns/app",
    36					},
    37					Exemplars: []apiv1.Exemplar{
    38						{
    39							Labels:    p.LabelSet{"traceID": "test1"},
    40							Value:     0.003535405,
    41							Timestamp: p.TimeFromUnixNano(time.Now().Add(-2 * time.Minute).UnixNano()),
    42						},
    43						{
    44							Labels:    p.LabelSet{"traceID": "test2"},
    45							Value:     0.005555605,
    46							Timestamp: p.TimeFromUnixNano(time.Now().Add(-4 * time.Minute).UnixNano()),
    47						},
    48						{
    49							Labels:    p.LabelSet{"traceID": "test3"},
    50							Value:     0.007545445,
    51							Timestamp: p.TimeFromUnixNano(time.Now().Add(-6 * time.Minute).UnixNano()),
    52						},
    53						{
    54							Labels:    p.LabelSet{"traceID": "test4"},
    55							Value:     0.009545445,
    56							Timestamp: p.TimeFromUnixNano(time.Now().Add(-7 * time.Minute).UnixNano()),
    57						},
    58					},
    59				},
    60			}
    61	
    62			values := []p.SamplePair{
    63				{Value: 1, Timestamp: 1000},
    64				{Value: 4, Timestamp: 4000},
    65				{Value: 6, Timestamp: 7000},
    66				{Value: 8, Timestamp: 1100},
    67			}
    68			rangeResult := queryResult{
    69				Type: p.ValMatrix,
    70				Result: p.Matrix{
    71					&p.SampleStream{
    72						Metric: p.Metric{"app": "Application", "tag2": "tag2"},
    73						Values: values,
    74					},
    75				},
    76			}
    77	
    78			tctx, err := setup()
    79			require.NoError(t, err)
    80	
    81			qm := models.QueryModel{
    82				UtcOffsetSec: 0,
    83				PrometheusQueryProperties: models.PrometheusQueryProperties{
    84					LegendFormat: "legend {{app}}",
    85					Exemplar:     true,
    86				},
    87			}
    88			b, err := json.Marshal(&qm)
    89			require.NoError(t, err)
    90			query := backend.DataQuery{
    91				RefID: "A",
    92				JSON:  b,
    93			}
    94			res, err := execute(tctx, query, rangeResult, nil, exemplars)
    95			require.NoError(t, err)
    96	
    97			// Test fields
    98			require.Len(t, res, 2)
    99			// Find the exemplar frame
   100			var exemplarFrame *data.Frame
   101			var rangeFrame *data.Frame
   102			for _, frame := range res {
   103				if frame.Name == "exemplar" {
   104					exemplarFrame = frame
   105				} else {
   106					rangeFrame = frame
   107				}
   108			}
   109			require.NotNil(t, exemplarFrame)
   110			require.NotNil(t, rangeFrame)
   111			require.Equal(t, "Time", exemplarFrame.Fields[0].Name)
   112			require.Equal(t, "Value", exemplarFrame.Fields[1].Name)
   113			require.Len(t, exemplarFrame.Fields, 6)
   114	
   115			// Test correct values (sampled to 2)
   116			require.Equal(t, 4, exemplarFrame.Fields[1].Len())
   117			require.Equal(t, 0.009545445, exemplarFrame.Fields[1].At(0))
   118			require.Equal(t, 0.003535405, exemplarFrame.Fields[1].At(3))
   119		})
   120	
   121		t.Run("matrix response should be parsed normally", func(t *testing.T) {
   122			values := []p.SamplePair{
   123				{Value: 1, Timestamp: 1000},
   124				{Value: 2, Timestamp: 2000},
   125				{Value: 3, Timestamp: 3000},
   126				{Value: 4, Timestamp: 4000},
   127				{Value: 5, Timestamp: 5000},
   128			}
   129			result := queryResult{
   130				Type: p.ValMatrix,
   131				Result: p.Matrix{
   132					&p.SampleStream{
   133						Metric: p.Metric{"app": "Application", "tag2": "tag2"},
   134						Values: values,
   135					},
   136				},
   137			}
   138	
   139			qm := models.QueryModel{
   140				UtcOffsetSec: 0,
   141				PrometheusQueryProperties: models.PrometheusQueryProperties{
   142					Range:        true,
   143					LegendFormat: "legend {{app}}",
   144				},
   145			}
   146			b, err := json.Marshal(&qm)
   147			require.NoError(t, err)
   148			query := backend.DataQuery{
   149				TimeRange: backend.TimeRange{
   150					From: time.Unix(1, 0).UTC(),
   151					To:   time.Unix(5, 0).UTC(),
   152				},
   153				JSON: b,
   154			}
   155			tctx, err := setup()
   156			require.NoError(t, err)
   157			res, err := execute(tctx, query, result, nil, nil)
   158			require.NoError(t, err)
   159	
   160			require.Len(t, res, 1)
   161			require.Len(t, res[0].Fields, 2)
   162			require.Len(t, res[0].Fields[0].Labels, 0)
   163			require.Equal(t, "Time", res[0].Fields[0].Name)
   164			require.Len(t, res[0].Fields[1].Labels, 2)
   165			require.Equal(t, "app=Application, tag2=tag2", res[0].Fields[1].Labels.String())
   166			require.Equal(t, "legend Application", res[0].Fields[1].Config.DisplayNameFromDS)
   167	
   168			// Ensure the timestamps are UTC zoned
   169			testValue := res[0].Fields[0].At(0)
   170			require.Equal(t, "UTC", testValue.(time.Time).Location().String())
   171		})
   172	
   173		t.Run("matrix response with missed data points should be parsed correctly", func(t *testing.T) {
   174			values := []p.SamplePair{
   175				{Value: 1, Timestamp: 1000},
   176				{Value: 4, Timestamp: 4000},
   177			}
   178			result := queryResult{
   179				Type: p.ValMatrix,
   180				Result: p.Matrix{
   181					&p.SampleStream{
   182						Metric: p.Metric{"app": "Application", "tag2": "tag2"},
   183						Values: values,
   184					},
   185				},
   186			}
   187	
   188			qm := models.QueryModel{
   189				UtcOffsetSec: 0,
   190				PrometheusQueryProperties: models.PrometheusQueryProperties{
   191					Range:        true,
   192					LegendFormat: "",
   193				},
   194			}
   195			b, err := json.Marshal(&qm)
   196			require.NoError(t, err)
   197			query := backend.DataQuery{
   198				TimeRange: backend.TimeRange{
   199					From: time.Unix(1, 0).UTC(),
   200					To:   time.Unix(4, 0).UTC(),
   201				},
   202				JSON: b,
   203			}
   204			tctx, err := setup()
   205			require.NoError(t, err)
   206			res, err := execute(tctx, query, result, nil, nil)
   207	
   208			require.NoError(t, err)
   209			require.Len(t, res, 1)
   210			require.Equal(t, res[0].Fields[0].Len(), 2)
   211			require.Equal(t, time.Unix(1, 0).UTC(), res[0].Fields[0].At(0))
   212			require.Equal(t, time.Unix(4, 0).UTC(), res[0].Fields[0].At(1))
   213			require.Equal(t, res[0].Fields[1].Len(), 2)
   214			require.Equal(t, float64(1), res[0].Fields[1].At(0).(float64))
   215			require.Equal(t, float64(4), res[0].Fields[1].At(1).(float64))
   216		})
   217	
   218		t.Run("matrix response with from alerting missed data points should be parsed correctly", func(t *testing.T) {
   219			values := []p.SamplePair{
   220				{Value: 1, Timestamp: 1000},
   221				{Value: 4, Timestamp: 4000},
   222			}
   223			result := queryResult{
   224				Type: p.ValMatrix,
   225				Result: p.Matrix{
   226					&p.SampleStream{
   227						Metric: p.Metric{"app": "Application", "tag2": "tag2"},
   228						Values: values,
   229					},
   230				},
   231			}
   232	
   233			qm := models.QueryModel{
   234				UtcOffsetSec: 0,
   235				PrometheusQueryProperties: models.PrometheusQueryProperties{
   236					Range:        true,
   237					LegendFormat: "",
   238				},
   239			}
   240			b, err := json.Marshal(&qm)
   241			require.NoError(t, err)
   242			query := backend.DataQuery{
   243				TimeRange: backend.TimeRange{
   244					From: time.Unix(1, 0).UTC(),
   245					To:   time.Unix(4, 0).UTC(),
   246				},
   247				JSON: b,
   248			}
   249			tctx, err := setup()
   250			require.NoError(t, err)
   251			res, err := execute(tctx, query, result, nil, nil)
   252	
   253			require.NoError(t, err)
   254			require.Len(t, res, 1)
   255			require.Len(t, res[0].Fields, 2)
   256			require.Len(t, res[0].Fields[0].Labels, 0)
   257			require.Equal(t, res[0].Fields[0].Name, "Time")
   258			require.Len(t, res[0].Fields[1].Labels, 2)
   259			require.Equal(t, res[0].Fields[1].Labels.String(), "app=Application, tag2=tag2")
   260			require.Equal(t, `{app="Application", tag2="tag2"}`, res[0].Fields[1].Config.DisplayNameFromDS)
   261		})
   262	
   263		t.Run("matrix response with NaN value should be changed to null", func(t *testing.T) {
   264			result := queryResult{
   265				Type: p.ValMatrix,
   266				Result: p.Matrix{
   267					&p.SampleStream{
   268						Metric: p.Metric{"app": "Application"},
   269						Values: []p.SamplePair{
   270							{Value: p.SampleValue(math.NaN()), Timestamp: 1000},
   271						},
   272					},
   273				},
   274			}
   275	
   276			qm := models.QueryModel{
   277				UtcOffsetSec: 0,
   278				PrometheusQueryProperties: models.PrometheusQueryProperties{
   279					Range:        true,
   280					LegendFormat: "",
   281				},
   282			}
   283			b, err := json.Marshal(&qm)
   284			require.NoError(t, err)
   285			query := backend.DataQuery{
   286				TimeRange: backend.TimeRange{
   287					From: time.Unix(1, 0).UTC(),
   288					To:   time.Unix(4, 0).UTC(),
   289				},
   290				JSON: b,
   291			}
   292	
   293			tctx, err := setup()
   294			require.NoError(t, err)
   295			res, err := execute(tctx, query, result, nil, nil)
   296			require.NoError(t, err)
   297	
   298			require.Equal(t, `{app="Application"}`, res[0].Fields[1].Config.DisplayNameFromDS)
   299			require.True(t, math.IsNaN(res[0].Fields[1].At(0).(float64)))
   300		})
   301	
   302		t.Run("vector response should be parsed normally", func(t *testing.T) {
   303			qr := queryResult{
   304				Type: p.ValVector,
   305				Result: p.Vector{
   306					&p.Sample{
   307						Metric:    p.Metric{"app": "Application", "tag2": "tag2"},
   308						Value:     1,
   309						Timestamp: 123,
   310					},
   311				},
   312			}
   313			qm := models.QueryModel{
   314				UtcOffsetSec: 0,
   315				PrometheusQueryProperties: models.PrometheusQueryProperties{
   316					Instant:      true,
   317					LegendFormat: "legend {{app}}",
   318				},
   319			}
   320			b, err := json.Marshal(&qm)
   321			require.NoError(t, err)
   322			query := backend.DataQuery{
   323				JSON: b,
   324			}
   325			tctx, err := setup()
   326			require.NoError(t, err)
   327			res, err := execute(tctx, query, nil, qr, nil)
   328			require.NoError(t, err)
   329	
   330			require.Len(t, res, 1)
   331			require.Len(t, res[0].Fields, 2)
   332			require.Len(t, res[0].Fields[0].Labels, 0)
   333			require.Equal(t, res[0].Fields[0].Name, "Time")
   334			require.Equal(t, res[0].Fields[0].Name, "Time")
   335			require.Len(t, res[0].Fields[1].Labels, 2)
   336			require.Equal(t, res[0].Fields[1].Labels.String(), "app=Application, tag2=tag2")
   337			require.Equal(t, "legend Application", res[0].Fields[1].Config.DisplayNameFromDS)
   338	
   339			// Ensure the timestamps are UTC zoned
   340			testValue := res[0].Fields[0].At(0)
   341			require.Equal(t, "UTC", testValue.(time.Time).Location().String())
   342			require.Equal(t, int64(123), testValue.(time.Time).UnixMilli())
   343		})
   344	
   345		t.Run("scalar response should be parsed normally", func(t *testing.T) {
   346			qr := queryResult{
   347				Type: p.ValScalar,
   348				Result: &p.Scalar{
   349					Value:     1,
   350					Timestamp: 123,
   351				},
   352			}
   353			qm := models.QueryModel{
   354				UtcOffsetSec: 0,
   355				PrometheusQueryProperties: models.PrometheusQueryProperties{
   356					Instant:      true,
   357					LegendFormat: "",
   358				},
   359			}
   360			b, err := json.Marshal(&qm)
   361			require.NoError(t, err)
   362			query := backend.DataQuery{
   363				JSON: b,
   364			}
   365			tctx, err := setup()
   366			require.NoError(t, err)
   367			res, err := execute(tctx, query, nil, qr, nil)
   368			require.NoError(t, err)
   369	
   370			require.Len(t, res, 1)
   371			require.Len(t, res[0].Fields, 2)
   372			require.Len(t, res[0].Fields[0].Labels, 0)
   373			require.Equal(t, res[0].Fields[0].Name, "Time")
   374			require.Equal(t, "Value", res[0].Fields[1].Name)
   375			require.Equal(t, float64(1), res[0].Fields[1].At(0))
   376	
   377			// Ensure the timestamps are UTC zoned
   378			testValue := res[0].Fields[0].At(0)
   379			require.Equal(t, "UTC", testValue.(time.Time).Location().String())
   380			require.Equal(t, int64(123), testValue.(time.Time).UnixMilli())
   381		})
   382	}
   383	
   384	func TestPrometheus_executedQueryString(t *testing.T) {
   385		t.Run("executedQueryString should match expected format with intervalMs 300_000", func(t *testing.T) {
21:func TestQueryData_parseResponse(t *testing.T) {
24:	t.Run("resultType is before result the field must parsed normally", func(t *testing.T) {
28:		assert.Nil(t, result.Error)
29:		assert.Len(t, result.Frames, 1)
32:	t.Run("resultType is after the result field must parsed normally", func(t *testing.T) {
36:		assert.Nil(t, result.Error)
37:		assert.Len(t, result.Frames, 1)
40:	t.Run("no resultType is existed in the data", func(t *testing.T) {
44:		assert.Error(t, result.Error)
45:		assert.Equal(t, result.Error.Error(), "no resultType found")
48:	t.Run("resultType is set as empty string before result", func(t *testing.T) {
52:		assert.Error(t, result.Error)
53:		assert.Equal(t, result.Error.Error(), "unknown result type: ")
56:	t.Run("resultType is set as empty string after result", func(t *testing.T) {
60:		assert.Error(t, result.Error)
61:		assert.Equal(t, result.Error.Error(), "unknown result type: ")
65:func TestAddMetadataToMultiFrame(t *testing.T) {
66:	t.Run("when you have native histogram result", func(t *testing.T) {
71:		assert.Nil(t, result.Error)
72:		assert.Len(t, result.Frames, 1)
73:		assert.Equal(t, "yMin", result.Frames[0].Fields[1].Name)
77:func TestParseQueryStats(t *testing.T) {
178:		t.Run(tt.name, func(t *testing.T) {
183:			assert.Equal(t, tt.want, parseQueryStats(header, tt.queryType))
188:func TestParseResponse_QueryStats(t *testing.T) {
192:	t.Run("populates frame meta stats from Server-Timing header", func(t *testing.T) {
199:		require.Nil(t, result.Error)
200:		require.Len(t, result.Frames, 1)
201:		assert.Equal(t, []data.QueryStat{
206:	t.Run("leaves stats nil when header absent", func(t *testing.T) {
209:		require.Nil(t, result.Error)
210:		require.Len(t, result.Frames, 1)
211:		assert.Nil(t, result.Frames[0].Meta.Stats)
214:	t.Run("keeps Prometheus's own stats alongside Server-Timing stats", func(t *testing.T) {
222:		require.Nil(t, result.Error)
223:		require.Len(t, result.Frames, 1)
224:		assert.Equal(t, []data.QueryStat{
239:func TestParseResponse_ErrorCases(t *testing.T) {
253:		t.Run(tt.name, func(t *testing.T) {
260:			require.Error(t, resp.Error)
261:			assert.Contains(t, resp.Error.Error(), "unexpected response")
262:			assert.Len(t, resp.Frames, 1)
263:			assert.NoError(t, res.Body.Close())
278:func TestProcessExemplars(t *testing.T) {
279:	t.Run("preserves the first exemplar frame's Meta when merging multiple series", func(t *testing.T) {
294:		require.NoError(t, result.Error)
295:		require.Len(t, result.Frames, 1)
296:		assert.Equal(t, "Expr: up\nStep: 15s", result.Frames[0].Meta.ExecutedQueryString)

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "nl -ba crates/grafana/tests/promlib_settings.rs; nl -ba upstream-prometheus/pkg/promlib/models/settings_test.go; nl -ba crates/grafana/tests/promlib_interval.rs | sed -n '1,110p'; nl -ba upstream-prometheus/pkg/promlib/querydata/exemplar/sampler_stddev_test.go; nl -ba upstream-prometheus/pkg/promlib/querydata/exemplar/sampler_test.go" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
     1	//! Port of promlib `models/settings_test.go`.
     2	use grafana::promlib::settings::{PromOptions, parse_prom_options};
     3	
     4	fn with_method(m: &str) -> Result<PromOptions, String> {
     5	    parse_prom_options(&json::Json::obj().with("httpMethod", m).stringify())
     6	}
     7	
     8	fn method_ok(input: &str, want: &str) {
     9	    assert_eq!(with_method(input).expect("options").http_method, want);
    10	}
    11	
    12	fn method_err(input: &str, want: &str) {
    13	    let e = with_method(input).expect_err("error");
    14	    assert!(e.contains(want), "{e}");
    15	}
    16	
    17	// TestParsePromOptions_HTTPMethod
    18	#[test]
    19	fn http_method_empty_defaults_to_post() {
    20	    method_ok("", "POST");
    21	}
    22	#[test]
    23	fn http_method_lowercase_get_normalised_to_get() {
    24	    method_ok("get", "GET");
    25	}
    26	#[test]
    27	fn http_method_mixed_case_post_normalised_to_post() {
    28	    method_ok("Post", "POST");
    29	}
    30	#[test]
    31	fn http_method_uppercase_post_accepted() {
    32	    method_ok("POST", "POST");
    33	}
    34	#[test]
    35	fn http_method_uppercase_get_accepted() {
    36	    method_ok("GET", "GET");
    37	}
    38	#[test]
    39	fn http_method_whitespace_padded_get_normalised() {
    40	    method_ok("  get  ", "GET");
    41	}
    42	#[test]
    43	fn http_method_put_rejected() {
    44	    method_err("PUT", r#"invalid httpMethod "PUT""#);
    45	}
    46	#[test]
    47	fn http_method_delete_rejected() {
    48	    method_err("DELETE", r#"invalid httpMethod "DELETE""#);
    49	}
    50	
    51	// TestParsePromOptions_MalformedJSON
    52	#[test]
    53	fn malformed_json() {
    54	    let e = parse_prom_options("{not valid json").expect_err("error");
    55	    assert!(e.contains("error unmarshalling JSONData"), "{e}");
    56	}
    57	
    58	// TestParsePromOptions_EmptyJSONData
    59	#[test]
    60	fn empty_json_data_nil() {
    61	    assert_eq!(parse_prom_options("").expect("options").http_method, "POST");
    62	}
    63	#[test]
    64	fn empty_json_data_empty_slice() {
    65	    assert_eq!(parse_prom_options("").expect("options").http_method, "POST");
    66	}
    67	#[test]
    68	fn empty_json_data_empty_json_object() {
    69	    assert_eq!(parse_prom_options("{}").expect("options").http_method, "POST");
    70	}
    71	
    72	// TestParsePromOptions_QueryStatsEnabled
    73	#[test]
    74	fn query_stats_enabled_true() {
    75	    assert!(parse_prom_options(r#"{"queryStatsEnabled":true}"#).expect("options").query_stats_enabled);
    76	}
    77	#[test]
    78	fn query_stats_enabled_false() {
    79	    assert!(!parse_prom_options(r#"{"queryStatsEnabled":false}"#).expect("options").query_stats_enabled);
    80	}
    81	#[test]
    82	fn query_stats_enabled_missing_defaults_to_false() {
    83	    assert!(!parse_prom_options("{}").expect("options").query_stats_enabled);
    84	}
    85	
    86	// TestPromOptions_ApplyDefaults
    87	fn defaults(input: &str, want: &str) {
    88	    let mut o = PromOptions { http_method: input.to_string(), ..Default::default() };
    89	    o.apply_defaults();
    90	    assert_eq!(o.http_method, want);
    91	}
    92	#[test]
    93	fn apply_defaults_empty_http_method_defaults_to_post() {
    94	    defaults("", "POST");
    95	}
    96	#[test]
    97	fn apply_defaults_whitespace_only_http_method_defaults_to_post() {
    98	    defaults("   ", "POST");
    99	}
   100	#[test]
   101	fn apply_defaults_lowercase_get_is_normalised_to_get() {
   102	    defaults("get", "GET");
   103	}
   104	#[test]
   105	fn apply_defaults_mixed_case_post_is_normalised_to_post() {
   106	    defaults("Post", "POST");
   107	}
   108	#[test]
   109	fn apply_defaults_whitespace_padded_value_is_trimmed_and_uppercased() {
   110	    defaults("  get  ", "GET");
   111	}
   112	#[test]
   113	fn apply_defaults_already_uppercase_value_is_preserved() {
   114	    defaults("GET", "GET");
   115	}
   116	
   117	// TestPromOptions_ApplyDefaults_DoesNotMutateUnrelatedFields
   118	#[test]
   119	fn apply_defaults_does_not_mutate_unrelated_fields() {
   120	    let mut o = PromOptions { time_interval: "30s".to_string(), query_timeout: "60s".to_string(), prometheus_type: "Prometheus".to_string(), series_limit: Some(42.0), ..Default::default() };
   121	    o.apply_defaults();
   122	    assert_eq!(o.time_interval, "30s");
   123	    assert_eq!(o.query_timeout, "60s");
   124	    assert_eq!(o.prometheus_type, "Prometheus");
   125	    assert_eq!(o.series_limit, Some(42.0));
   126	}
   127	
   128	// TestPromOptions_Validate
   129	fn validate(method: &str) -> Result<(), String> {
   130	    PromOptions { http_method: method.to_string(), ..Default::default() }.validate()
   131	}
   132	fn validate_err(method: &str, want: &str) {
   133	    let e = validate(method).expect_err("error");
   134	    assert!(e.contains(want), "{e}");
   135	}
   136	#[test]
   137	fn validate_empty_is_valid() {
   138	    assert!(validate("").is_ok());
   139	}
   140	#[test]
   141	fn validate_get_is_valid() {
   142	    assert!(validate("GET").is_ok());
   143	}
   144	#[test]
   145	fn validate_post_is_valid() {
   146	    assert!(validate("POST").is_ok());
   147	}
   148	#[test]
   149	fn validate_lowercase_get_is_valid() {
   150	    assert!(validate("get").is_ok());
   151	}
   152	#[test]
   153	fn validate_lowercase_post_is_valid() {
   154	    assert!(validate("post").is_ok());
   155	}
   156	#[test]
   157	fn validate_put_is_rejected() {
   158	    validate_err("PUT", r#"invalid httpMethod "PUT""#);
   159	}
   160	#[test]
   161	fn validate_delete_is_rejected() {
   162	    validate_err("DELETE", r#"invalid httpMethod "DELETE""#);
   163	}
   164	#[test]
   165	fn validate_patch_is_rejected() {
   166	    validate_err("PATCH", r#"invalid httpMethod "PATCH""#);
   167	}
   168	#[test]
   169	fn validate_arbitrary_string_is_rejected() {
   170	    validate_err("foo", r#"invalid httpMethod "foo""#);
   171	}
     1	package models_test
     2	
     3	import (
     4		"encoding/json"
     5		"net/http"
     6		"testing"
     7	
     8		"github.com/grafana/grafana-plugin-sdk-go/backend"
     9		"github.com/stretchr/testify/require"
    10	
    11		"github.com/grafana/grafana-prometheus-datasource/pkg/promlib/models"
    12	)
    13	
    14	func settingsWithJSON(t *testing.T, v any) backend.DataSourceInstanceSettings {
    15		t.Helper()
    16		b, err := json.Marshal(v)
    17		require.NoError(t, err)
    18		return backend.DataSourceInstanceSettings{JSONData: b}
    19	}
    20	
    21	func TestParsePromOptions_HTTPMethod(t *testing.T) {
    22		cases := []struct {
    23			name       string
    24			input      string
    25			wantMethod string
    26			wantErrStr string
    27		}{
    28			{
    29				name:       "empty defaults to POST",
    30				input:      "",
    31				wantMethod: http.MethodPost,
    32			},
    33			{
    34				name:       "lowercase get normalised to GET",
    35				input:      "get",
    36				wantMethod: http.MethodGet,
    37			},
    38			{
    39				name:       "mixed-case post normalised to POST",
    40				input:      "Post",
    41				wantMethod: http.MethodPost,
    42			},
    43			{
    44				name:       "uppercase POST accepted",
    45				input:      http.MethodPost,
    46				wantMethod: http.MethodPost,
    47			},
    48			{
    49				name:       "uppercase GET accepted",
    50				input:      http.MethodGet,
    51				wantMethod: http.MethodGet,
    52			},
    53			{
    54				name:       "whitespace-padded get normalised",
    55				input:      "  get  ",
    56				wantMethod: http.MethodGet,
    57			},
    58			{
    59				name:       "PUT rejected",
    60				input:      "PUT",
    61				wantErrStr: `invalid httpMethod "PUT"`,
    62			},
    63			{
    64				name:       "DELETE rejected",
    65				input:      "DELETE",
    66				wantErrStr: `invalid httpMethod "DELETE"`,
    67			},
    68		}
    69	
    70		for _, tc := range cases {
    71			t.Run(tc.name, func(t *testing.T) {
    72				opts, err := models.ParsePromOptions(settingsWithJSON(t, map[string]any{
    73					"httpMethod": tc.input,
    74				}))
    75	
    76				if tc.wantErrStr != "" {
    77					require.ErrorContains(t, err, tc.wantErrStr)
    78					return
    79				}
    80				require.NoError(t, err)
    81				require.Equal(t, tc.wantMethod, opts.HTTPMethod)
    82			})
    83		}
    84	}
    85	
    86	func TestParsePromOptions_MalformedJSON(t *testing.T) {
    87		settings := backend.DataSourceInstanceSettings{JSONData: []byte(`{not valid json`)}
    88		_, err := models.ParsePromOptions(settings)
    89		require.ErrorContains(t, err, "error unmarshalling JSONData")
    90	}
    91	
    92	func TestParsePromOptions_EmptyJSONData(t *testing.T) {
    93		cases := []struct {
    94			name     string
    95			jsonData []byte
    96		}{
    97			{name: "nil JSONData", jsonData: nil},
    98			{name: "empty slice", jsonData: []byte{}},
    99			{name: "empty JSON object", jsonData: []byte(`{}`)},
   100		}
   101		for _, tc := range cases {
   102			t.Run(tc.name, func(t *testing.T) {
   103				settings := backend.DataSourceInstanceSettings{JSONData: tc.jsonData}
   104				opts, err := models.ParsePromOptions(settings)
   105				require.NoError(t, err)
   106				require.Equal(t, http.MethodPost, opts.HTTPMethod)
   107			})
   108		}
   109	}
   110	
   111	func TestParsePromOptions_QueryStatsEnabled(t *testing.T) {
   112		tests := []struct {
   113			name string
   114			json map[string]any
   115			want bool
   116		}{
   117			{name: "true", json: map[string]any{"queryStatsEnabled": true}, want: true},
   118			{name: "false", json: map[string]any{"queryStatsEnabled": false}, want: false},
   119			{name: "missing defaults to false", json: map[string]any{}, want: false},
   120		}
   121	
   122		for _, tc := range tests {
   123			t.Run(tc.name, func(t *testing.T) {
   124				opts, err := models.ParsePromOptions(settingsWithJSON(t, tc.json))
   125				require.NoError(t, err)
   126				require.Equal(t, tc.want, bool(opts.QueryStatsEnabled))
   127			})
   128		}
   129	}
   130	
   131	func TestPromOptions_ApplyDefaults(t *testing.T) {
   132		cases := []struct {
   133			name       string
   134			input      models.PromOptions
   135			wantMethod string
   136		}{
   137			{
   138				name:       "empty HTTPMethod defaults to POST",
   139				input:      models.PromOptions{},
   140				wantMethod: http.MethodPost,
   141			},
   142			{
   143				name:       "whitespace-only HTTPMethod defaults to POST",
   144				input:      models.PromOptions{HTTPMethod: "   "},
   145				wantMethod: http.MethodPost,
   146			},
   147			{
   148				name:       "lowercase get is normalised to GET",
   149				input:      models.PromOptions{HTTPMethod: "get"},
   150				wantMethod: http.MethodGet,
   151			},
   152			{
   153				name:       "mixed-case Post is normalised to POST",
   154				input:      models.PromOptions{HTTPMethod: "Post"},
   155				wantMethod: http.MethodPost,
   156			},
   157			{
   158				name:       "whitespace-padded value is trimmed and uppercased",
   159				input:      models.PromOptions{HTTPMethod: "  get  "},
   160				wantMethod: http.MethodGet,
   161			},
   162			{
   163				name:       "already-uppercase value is preserved",
   164				input:      models.PromOptions{HTTPMethod: http.MethodGet},
   165				wantMethod: http.MethodGet,
   166			},
   167		}
   168		for _, tc := range cases {
   169			t.Run(tc.name, func(t *testing.T) {
   170				opts := tc.input
   171				opts.ApplyDefaults()
   172				require.Equal(t, tc.wantMethod, opts.HTTPMethod)
   173			})
   174		}
   175	}
   176	
   177	func TestPromOptions_ApplyDefaults_DoesNotMutateUnrelatedFields(t *testing.T) {
   178		seriesLimit := models.LenientFloat64(42)
   179		opts := models.PromOptions{
   180			TimeInterval:   "30s",
   181			QueryTimeout:   "60s",
   182			PrometheusType: models.LenientString(models.PromApplicationPrometheus),
   183			SeriesLimit:    &seriesLimit,
   184		}
   185		opts.ApplyDefaults()
   186	
   187		require.Equal(t, "30s", opts.TimeInterval)
   188		require.Equal(t, "60s", opts.QueryTimeout)
   189		require.Equal(t, "Prometheus", string(opts.PrometheusType))
   190		require.NotNil(t, opts.SeriesLimit)
   191		require.Equal(t, 42.0, float64(*opts.SeriesLimit))
   192	}
   193	
   194	func TestPromOptions_Validate(t *testing.T) {
   195		cases := []struct {
   196			name       string
   197			method     string
   198			wantErrStr string
   199		}{
   200			{name: "empty is valid", method: ""},
   201			{name: "GET is valid", method: http.MethodGet},
   202			{name: "POST is valid", method: http.MethodPost},
   203			{name: "lowercase get is valid (Validate uppercases before comparing)", method: "get"},
   204			{name: "lowercase post is valid", method: "post"},
   205			{name: "PUT is rejected", method: http.MethodPut, wantErrStr: `invalid httpMethod "PUT"`},
   206			{name: "DELETE is rejected", method: http.MethodDelete, wantErrStr: `invalid httpMethod "DELETE"`},
   207			{name: "PATCH is rejected", method: http.MethodPatch, wantErrStr: `invalid httpMethod "PATCH"`},
   208			{name: "arbitrary string is rejected", method: "foo", wantErrStr: `invalid httpMethod "foo"`},
   209		}
   210		for _, tc := range cases {
   211			t.Run(tc.name, func(t *testing.T) {
   212				opts := models.PromOptions{HTTPMethod: tc.method}
   213				err := opts.Validate()
   214				if tc.wantErrStr != "" {
   215					require.ErrorContains(t, err, tc.wantErrStr)
   216					return
   217				}
   218				require.NoError(t, err)
   219			})
   220		}
   221	}
     1	//! Port of promlib `models/prometheus_interval_test.go` (TestCalculatePrometheusInterval,
     2	//! TestCalculateRateInterval) and `intervalv2/intervalv2_test.go` (TestIntervalCalculator_Calculate,
     3	//! TestIntervalCalculator_CalculateSafeInterval). Go's intervalv2 returns an `Interval` with a `Text`; the Rust
     4	//! functions return the duration, so the text is `format_interval(value)`.
     5	use grafana::promlib::models::{DataQuery, DAY, HOUR, MIN, MS, SEC, calculate_interval, calculate_prometheus_interval, calculate_rate_interval, calculate_safe_interval, format_interval};
     6	
     7	fn now() -> i64 {
     8	    grafana::util::now_ms() * MS + 123_456 // not aligned to anything, like time.Now()
     9	}
    10	
    11	/// One TestCalculatePrometheusInterval case: (queryInterval, dsScrapeInterval, intervalMs, intervalFactor,
    12	/// time range length, query Interval, MaxDataPoints) -> want (None: wantErr).
    13	#[allow(clippy::too_many_arguments)]
    14	fn prom_interval(query_interval: &str, ds_scrape_interval: &str, interval_ms: i64, interval_factor: i64, range: i64, interval: i64, max_data_points: i64, want: Option<i64>) {
    15	    let n = now();
    16	    let q = DataQuery { from: n, to: n + range, interval, max_data_points, ..Default::default() };
    17	    let got = calculate_prometheus_interval(query_interval, ds_scrape_interval, interval_ms, interval_factor, &q);
    18	    match want {
    19	        None => assert!(got.is_err(), "want error, got {got:?}"),
    20	        Some(w) => assert_eq!(got.expect("no error"), w),
    21	    }
    22	}
    23	
    24	// TestCalculatePrometheusInterval
    25	#[test]
    26	fn prom_interval_use_min_step_value_when_its_bigger_than_the_calculated_value() {
    27	    prom_interval("10m", "", 300000, 1, 48 * HOUR, 5 * MIN, 761, Some(10 * MIN));
    28	}
    29	#[test]
    30	fn prom_interval_use_calculated_value_when_min_step_value_is_smaller() {
    31	    prom_interval("2m", "", 300000, 1, 48 * HOUR, 5 * MIN, 761, Some(5 * MIN));
    32	}
    33	#[test]
    34	fn prom_interval_min_step_2m_with_900000_interval_ms() {
    35	    prom_interval("2m", "", 900000, 1, 48 * HOUR, 15 * MIN, 175, Some(15 * MIN));
    36	}
    37	#[test]
    38	fn prom_interval_with_step_parameter() {
    39	    prom_interval("", "15s", 0, 1, 12 * HOUR, MIN, 0, Some(30 * SEC));
    40	}
    41	#[test]
    42	fn prom_interval_without_step_parameter() {
    43	    prom_interval("", "15s", 0, 1, HOUR, MIN, 0, Some(15 * SEC));
    44	}
    45	#[test]
    46	fn prom_interval_with_high_interval_factor() {
    47	    prom_interval("", "15s", 0, 10, 48 * HOUR, MIN, 0, Some(20 * MIN));
    48	}
    49	#[test]
    50	fn prom_interval_with_low_interval_factor() {
    51	    prom_interval("", "15s", 0, 1, 48 * HOUR, MIN, 0, Some(2 * MIN));
    52	}
    53	#[test]
    54	fn prom_interval_with_specified_scrape_interval_in_data_source() {
    55	    prom_interval("", "240s", 0, 1, 48 * HOUR, MIN, 0, Some(4 * MIN));
    56	}
    57	#[test]
    58	fn prom_interval_with_zero_interval_factor_defaults_to_1() {
    59	    prom_interval("", "15s", 0, 0, HOUR, MIN, 0, Some(15 * SEC));
    60	}
    61	#[test]
    62	fn prom_interval_with_interval_variable() {
    63	    prom_interval("$__interval", "15s", 60000, 1, 48 * HOUR, MIN, 0, Some(120 * SEC));
    64	}
    65	#[test]
    66	fn prom_interval_with_braced_interval_variable() {
    67	    prom_interval("${__interval}", "15s", 60000, 1, 48 * HOUR, MIN, 0, Some(120 * SEC));
    68	}
    69	#[test]
    70	fn prom_interval_with_braced_interval_variable_and_explicit_interval() {
    71	    prom_interval("1m", "15s", 60000, 1, 48 * HOUR, MIN, 0, Some(2 * MIN));
    72	}
    73	#[test]
    74	fn prom_interval_with_rate_interval_variable() {
    75	    prom_interval("$__rate_interval", "30s", 100000, 1, 2 * DAY, 100 * SEC, 12384, Some(130 * SEC));
    76	}
    77	#[test]
    78	fn prom_interval_with_braced_rate_interval_variable() {
    79	    prom_interval("${__rate_interval}", "30s", 100000, 1, 2 * DAY, 100 * SEC, 12384, Some(130 * SEC));
    80	}
    81	#[test]
    82	fn prom_interval_interval_ms_100s_min_step_override_150s_and_scrape_interval_30s() {
    83	    prom_interval("150s", "30s", 100000, 1, 2 * DAY, 100 * SEC, 12384, Some(150 * SEC));
    84	}
    85	#[test]
    86	fn prom_interval_interval_ms_120s_min_step_override_150s_and_ds_scrape_interval_30s() {
    87	    prom_interval("150s", "30s", 120000, 1, 2 * DAY, 120 * SEC, 12384, Some(150 * SEC));
    88	}
    89	#[test]
    90	fn prom_interval_interval_ms_120s_min_step_auto_and_ds_scrape_interval_30s() {
    91	    prom_interval("120s", "30s", 120000, 1, 2 * DAY, 120 * SEC, 12384, Some(120 * SEC));
    92	}
    93	#[test]
    94	fn prom_interval_interval_and_min_step_automatic_ds_scrape_30s_range_1_hour() {
    95	    prom_interval("30s", "30s", 30000, 1, HOUR, 30 * SEC, 12384, Some(30 * SEC));
    96	}
    97	#[test]
    98	fn prom_interval_min_step_rate_interval_ds_scrape_30s_range_1_hour() {
    99	    prom_interval("$__rate_interval", "30s", 30000, 1, HOUR, 30 * SEC, 12384, Some(2 * MIN));
   100	}
   101	#[test]
   102	fn prom_interval_min_step_rate_interval_ds_scrape_30s_range_2_days() {
   103	    prom_interval("$__rate_interval", "30s", 120000, 1, 2 * DAY, 120 * SEC, 12384, Some(150 * SEC));
   104	}
   105	#[test]
   106	fn prom_interval_min_step_interval_ds_scrape_15s_range_2_days() {
   107	    prom_interval("$__interval", "15s", 120000, 1, 2 * DAY, 120 * SEC, 12384, Some(120 * SEC));
   108	}
   109	#[test]
   110	fn prom_interval_with_empty_ds_scrape_interval_defaults_to_15s() {
     1	package exemplar_test
     2	
     3	import (
     4		"testing"
     5		"time"
     6	
     7		"github.com/grafana/grafana-plugin-sdk-go/experimental"
     8	
     9		"github.com/grafana/grafana-prometheus-datasource/pkg/promlib/models"
    10		"github.com/grafana/grafana-prometheus-datasource/pkg/promlib/querydata/exemplar"
    11	)
    12	
    13	func TestStdDevSampler(t *testing.T) {
    14		sampler := exemplar.NewStandardDeviationSampler().(*exemplar.StandardDeviationSampler)
    15		t.Run("standard deviation sampler", func(t *testing.T) {
    16			tr := models.TimeRange{
    17				Start: time.Unix(0, 0),
    18				End:   time.Unix(100000, 0),
    19			}
    20			ex := generateTestExemplars(tr)
    21			sampler.SetStep(600 * time.Second)
    22			for i := 0; i < len(ex); i++ {
    23				sampler.Add(ex[i])
    24			}
    25			framer := exemplar.NewFramer(sampler, exemplar.NewLabelTracker())
    26			experimental.CheckGoldenJSONFramer(t, "testdata", "stddev_sampler", framer, update)
    27		})
    28	}
     1	package exemplar_test
     2	
     3	import (
     4		"testing"
     5		"time"
     6	
     7		"github.com/grafana/grafana-plugin-sdk-go/experimental"
     8	
     9		"github.com/grafana/grafana-prometheus-datasource/pkg/promlib/models"
    10		"github.com/grafana/grafana-prometheus-datasource/pkg/promlib/querydata/exemplar"
    11	)
    12	
    13	const update = true
    14	
    15	func TestNoOpSampler(t *testing.T) {
    16		sampler := exemplar.NewNoOpSampler().(*exemplar.NoOpSampler)
    17		t.Run("no-op sampler", func(t *testing.T) {
    18			tr := models.TimeRange{
    19				Start: time.Unix(0, 0),
    20				End:   time.Unix(2000, 0),
    21			}
    22			ex := generateTestExemplars(tr)
    23			for i := 0; i < len(ex); i++ {
    24				sampler.Add(ex[i])
    25			}
    26			framer := exemplar.NewFramer(sampler, exemplar.NewLabelTracker())
    27			experimental.CheckGoldenJSONFramer(t, "testdata", "noop_sampler", framer, update)
    28		})
    29	}
    30	
    31	func generateTestExemplars(tr models.TimeRange) []models.Exemplar {
    32		exemplars := []models.Exemplar{}
    33		next := tr.Start.UTC()
    34		for !next.Equal(tr.End) && !next.After(tr.End) {
    35			exemplars = append(exemplars, models.Exemplar{
    36				Timestamp: next,
    37				Value:     float64(next.Unix()),
    38			})
    39			next = next.Add(time.Second).UTC()
    40		}
    41		return exemplars
    42	}

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "nl -ba crates/grafana/tests/promlib_converter.rs | sed -n '122,270p'; nl -ba upstream-prometheus/pkg/promlib/converter/prom_test.go | sed -n '1,200p'; nl -ba upstream-prometheus/pkg/promlib/querydata/framing_test.go | sed -n '1,240p'" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
   122	// TestReadPrometheusQueryStats
   123	fn read_ok(payload: &str) -> DataResponse {
   124	    let rsp = read_prometheus_style_result(payload.as_bytes());
   125	    assert!(rsp.error.is_none(), "{:?}", rsp.error);
   126	    rsp
   127	}
   128	
   129	const INSTANT_RESULT: &str = r#"[{"metric":{"__name__":"up","instance":"localhost"},"value":[1710000000,"1"]}]"#;
   130	const PER_STEP: &str = "[[1710000000,12],[1710000060,18]]";
   131	
   132	fn check_stats_case(data: &str) {
   133	    let rsp = read_ok(&format!(r#"{{"status":"success","data":{data}}}"#));
   134	    let frames = rsp.frames.as_ref().expect("frames");
   135	    assert_eq!(frames.len(), 1);
   136	    let meta = frames[0].meta.as_ref().expect("meta");
   137	    assert_eq!(meta.stats.len(), 1);
   138	    assert_eq!(meta.stats[0].display_name, "Total queryable samples");
   139	    assert_eq!(meta.stats[0].unit, "");
   140	    assert_eq!(meta.stats[0].value, 30.0);
   141	    let custom = meta.custom.as_ref().expect("custom");
   142	    assert_eq!(custom.str_of("resultType"), "vector");
   143	    let samples = custom.path("stats.samples").expect("samples");
   144	    assert_eq!(samples.get("totalQueryableSamples").and_then(|v| v.as_f64()), Some(30.0));
   145	    assert_eq!(samples.get("totalQueryableSamplesPerStep").and_then(|v| v.as_arr()).map(|a| a.len()), Some(2));
   146	}
   147	
   148	#[test]
   149	fn query_stats_stats_after_result() {
   150	    check_stats_case(&format!(r#"{{"resultType":"vector","result":{INSTANT_RESULT},"stats":{{"samples":{{"totalQueryableSamples":30,"totalQueryableSamplesPerStep":{PER_STEP}}}}}}}"#));
   151	}
   152	
   153	#[test]
   154	fn query_stats_stats_before_result() {
   155	    check_stats_case(&format!(r#"{{"stats":{{"samples":{{"totalQueryableSamples":30,"totalQueryableSamplesPerStep":{PER_STEP}}}}},"resultType":"vector","result":{INSTANT_RESULT}}}"#));
   156	}
   157	
   158	#[test]
   159	fn query_stats_stats_and_result_before_result_type() {
   160	    check_stats_case(&format!(r#"{{"stats":{{"samples":{{"totalQueryableSamples":30,"totalQueryableSamplesPerStep":{PER_STEP}}}}},"result":{INSTANT_RESULT},"resultType":"vector"}}"#));
   161	}
   162	
   163	#[test]
   164	fn query_stats_zero_samples_is_retained_as_a_typed_stat() {
   165	    let rsp = read_ok(r#"{"status":"success","data":{"resultType":"vector","result":[],"stats":{"samples":{"totalQueryableSamples":0}}}}"#);
   166	    let frames = rsp.frames.as_ref().expect("frames");
   167	    assert_eq!(frames.len(), 1);
   168	    let meta = frames[0].meta.as_ref().expect("meta");
   169	    assert_eq!(meta.stats.len(), 1);
   170	    assert_eq!(meta.stats[0].value, 0.0);
   171	}
   172	
   173	#[test]
   174	fn query_stats_range_stats_are_attached_to_the_first_frame_only() {
   175	    let rsp = read_ok(r#"{"status":"success","data":{"resultType":"matrix","result":[{"metric":{"job":"one"},"values":[[1710000000,"1"]]},{"metric":{"job":"two"},"values":[[1710000000,"2"]]}],"stats":{"samples":{"totalQueryableSamples":2}}}}"#);
   176	    let frames = rsp.frames.as_ref().expect("frames");
   177	    assert_eq!(frames.len(), 2);
   178	    assert_eq!(frames[0].meta.as_ref().map(|m| m.stats.len()), Some(1));
   179	    assert!(frames[1].meta.as_ref().map(|m| m.stats.is_empty()).unwrap_or(true));
   180	}
   181	
   182	#[test]
   183	fn query_stats_empty_result_creates_a_metadata_only_frame() {
   184	    let rsp = read_ok(r#"{"status":"success","data":{"resultType":"matrix","result":[],"stats":{"samples":{"totalQueryableSamples":7}}}}"#);
   185	    let frames = rsp.frames.as_ref().expect("frames");
   186	    assert_eq!(frames.len(), 1);
   187	    assert_eq!(frames[0].name, "Query statistics");
   188	    assert!(frames[0].fields.is_empty());
   189	    assert_eq!(frames[0].meta.as_ref().map(|m| m.stats[0].value), Some(7.0));
   190	}
   191	
   192	#[test]
   193	fn query_stats_malformed_stats_do_not_fail_a_valid_query() {
   194	    let rsp = read_ok(&format!(r#"{{"status":"success","data":{{"resultType":"vector","result":{INSTANT_RESULT},"stats":"not-an-object"}}}}"#));
   195	    let frames = rsp.frames.as_ref().expect("frames");
   196	    assert_eq!(frames.len(), 1);
   197	    let meta = frames[0].meta.as_ref().expect("meta");
   198	    assert!(meta.stats.is_empty());
   199	    assert_eq!(meta.custom.as_ref().and_then(|c| c.get("stats")).and_then(|s| s.as_str()), Some("not-an-object"));
   200	}
   201	
   202	#[test]
   203	fn query_stats_malformed_total_omits_only_the_typed_stat() {
   204	    let rsp = read_ok(&format!(r#"{{"status":"success","data":{{"resultType":"vector","result":{INSTANT_RESULT},"stats":{{"samples":{{"totalQueryableSamples":"unknown","totalQueryableSamplesPerStep":{PER_STEP}}}}}}}}}"#));
   205	    let meta = rsp.frames.as_ref().expect("frames")[0].meta.as_ref().expect("meta");
   206	    assert!(meta.stats.is_empty());
   207	    assert!(meta.custom.as_ref().and_then(|c| c.get("stats")).map(|s| !s.is_null()).unwrap_or(false));
   208	}
   209	
   210	#[test]
   211	fn query_stats_missing_stats_leaves_existing_metadata_unchanged() {
   212	    let rsp = read_ok(&format!(r#"{{"status":"success","data":{{"resultType":"vector","result":{INSTANT_RESULT}}}}}"#));
   213	    let meta = rsp.frames.as_ref().expect("frames")[0].meta.as_ref().expect("meta");
   214	    assert!(meta.stats.is_empty());
   215	    assert_eq!(meta.custom, Some(Json::obj().with("resultType", "vector")));
   216	}
   217	
   218	// TestTimeConversions
   219	#[test]
   220	fn time_conversions() {
   221	    // 2020-09-14 15:22:25.479 UTC, millisecond precision
   222	    assert_eq!(time_from_float(1600096945.479), 1600096945479 * 1_000_000);
   223	    // 2022-02-16 16:50:46.277587968 UTC
   224	    assert_eq!(time_from_loki_string("1645030246277587968"), Ok(1645030246277587968));
   225	    // 2033-05-18 03:33:20 UTC
   226	    assert_eq!(time_from_loki_string("2000000000000000000"), Ok(2000000000 * 1_000_000_000));
   227	}
   228	
   229	// TestReadPrometheusNumericStrings
   230	fn assert_numeric_string_values(v: &Values) {
   231	    match v {
   232	        Values::F64(x) => {
   233	            assert_eq!(x.len(), 5);
   234	            assert_eq!(x[0], 1.25);
   235	            assert_eq!(x[1], 1.25e10);
   236	            assert!(x[2].is_nan());
   237	            assert_eq!(x[3], f64::INFINITY);
   238	            assert_eq!(x[4], f64::NEG_INFINITY);
   239	        }
   240	        other => panic!("not float64: {other:?}"),
   241	    }
   242	}
   243	
   244	#[test]
   245	fn numeric_strings_matrix() {
   246	    let rsp = read_ok(r#"{"status":"success","data":{"resultType":"matrix","result":[{"metric":{},"values":[[1,"1.25"],[2,"1.25e10"],[3,"NaN"],[4,"+Inf"],[5,"-Inf"]]}]}}"#);
   247	    let frames = rsp.frames.as_ref().expect("frames");
   248	    assert_eq!(frames.len(), 1);
   249	    assert_numeric_string_values(&frames[0].fields[1].values);
   250	}
   251	
   252	#[test]
   253	fn numeric_strings_exemplars() {
   254	    let rsp = read_ok(r#"{"status":"success","data":[{"seriesLabels":{},"exemplars":[{"value":"1.25","timestamp":1},{"value":"1.25e10","timestamp":2},{"value":"NaN","timestamp":3},{"value":"+Inf","timestamp":4},{"value":"-Inf","timestamp":5}]}]}"#);
   255	    let frames = rsp.frames.as_ref().expect("frames");
   256	    assert_eq!(frames.len(), 1);
   257	    assert_numeric_string_values(&frames[0].fields[1].values);
   258	}
     1	package converter
     2	
     3	import (
     4		"math"
     5		"os"
     6		"path"
     7		"strings"
     8		"testing"
     9		"time"
    10	
    11		"github.com/grafana/grafana-plugin-sdk-go/backend"
    12		"github.com/grafana/grafana-plugin-sdk-go/data"
    13		sdkjsoniter "github.com/grafana/grafana-plugin-sdk-go/data/utils/jsoniter"
    14		"github.com/grafana/grafana-plugin-sdk-go/experimental"
    15		jsoniter "github.com/json-iterator/go"
    16		"github.com/stretchr/testify/assert"
    17		"github.com/stretchr/testify/require"
    18	)
    19	
    20	const update = false
    21	
    22	var files = []string{
    23		"prom-labels",
    24		"prom-matrix",
    25		"prom-matrix-with-nans",
    26		"prom-matrix-histogram-no-labels",
    27		"prom-matrix-histogram-partitioned",
    28		"prom-vector-histogram-no-labels",
    29		"prom-vector",
    30		"prom-string",
    31		"prom-scalar",
    32		"prom-series",
    33		"prom-warnings",
    34		"prom-warnings-no-data",
    35		"prom-infos",
    36		"prom-infos-no-data",
    37		"prom-error",
    38		"prom-exemplars-a",
    39		"prom-exemplars-b",
    40		"prom-exemplars-diff-labels",
    41		"prom-query-range",
    42		"prom-query-range-big",
    43		"loki-streams-a",
    44		"loki-streams-b",
    45		"loki-streams-c",
    46	}
    47	
    48	func TestReadPromFrames(t *testing.T) {
    49		for _, name := range files {
    50			t.Run(name, runScenario(name, Options{}))
    51		}
    52	}
    53	
    54	func TestReadPrometheusQueryStats(t *testing.T) {
    55		read := func(t *testing.T, payload string) *backend.DataResponse {
    56			t.Helper()
    57			iter := jsoniter.ParseBytes(sdkjsoniter.ConfigDefault, []byte(payload))
    58			rsp := ReadPrometheusStyleResult(iter, Options{})
    59			require.NoError(t, rsp.Error)
    60			return &rsp
    61		}
    62	
    63		instantResult := `[{"metric":{"__name__":"up","instance":"localhost"},"value":[1710000000,"1"]}]`
    64		perStep := `[[1710000000,12],[1710000060,18]]`
    65	
    66		for _, tc := range []struct {
    67			name string
    68			data string
    69		}{
    70			{
    71				name: "stats after result",
    72				data: `{"resultType":"vector","result":` + instantResult + `,"stats":{"samples":{"totalQueryableSamples":30,"totalQueryableSamplesPerStep":` + perStep + `}}}`,
    73			},
    74			{
    75				name: "stats before result",
    76				data: `{"stats":{"samples":{"totalQueryableSamples":30,"totalQueryableSamplesPerStep":` + perStep + `}},"resultType":"vector","result":` + instantResult + `}`,
    77			},
    78			{
    79				name: "stats and result before result type",
    80				data: `{"stats":{"samples":{"totalQueryableSamples":30,"totalQueryableSamplesPerStep":` + perStep + `}},"result":` + instantResult + `,"resultType":"vector"}`,
    81			},
    82		} {
    83			t.Run(tc.name, func(t *testing.T) {
    84				rsp := read(t, `{"status":"success","data":`+tc.data+`}`)
    85				require.Len(t, rsp.Frames, 1)
    86				require.NotNil(t, rsp.Frames[0].Meta)
    87				require.Equal(t, []data.QueryStat{{
    88					FieldConfig: data.FieldConfig{DisplayName: "Total queryable samples"},
    89					Value:       30,
    90				}}, rsp.Frames[0].Meta.Stats)
    91	
    92				custom, ok := rsp.Frames[0].Meta.Custom.(map[string]any)
    93				require.True(t, ok)
    94				require.Equal(t, "vector", custom["resultType"])
    95				rawStats, ok := custom["stats"].(map[string]any)
    96				require.True(t, ok)
    97				samples, ok := rawStats["samples"].(map[string]any)
    98				require.True(t, ok)
    99				require.Equal(t, float64(30), samples["totalQueryableSamples"])
   100				require.Len(t, samples["totalQueryableSamplesPerStep"], 2)
   101			})
   102		}
   103	
   104		t.Run("zero samples is retained as a typed stat", func(t *testing.T) {
   105			rsp := read(t, `{"status":"success","data":{"resultType":"vector","result":[],"stats":{"samples":{"totalQueryableSamples":0}}}}`)
   106			require.Len(t, rsp.Frames, 1)
   107			require.Len(t, rsp.Frames[0].Meta.Stats, 1)
   108			require.Equal(t, float64(0), rsp.Frames[0].Meta.Stats[0].Value)
   109		})
   110	
   111		t.Run("range stats are attached to the first frame only", func(t *testing.T) {
   112			rsp := read(t, `{"status":"success","data":{"resultType":"matrix","result":[{"metric":{"job":"one"},"values":[[1710000000,"1"]]},{"metric":{"job":"two"},"values":[[1710000000,"2"]]}],"stats":{"samples":{"totalQueryableSamples":2}}}}`)
   113			require.Len(t, rsp.Frames, 2)
   114			require.Len(t, rsp.Frames[0].Meta.Stats, 1)
   115			require.Empty(t, rsp.Frames[1].Meta.Stats)
   116		})
   117	
   118		t.Run("empty result creates a metadata-only frame", func(t *testing.T) {
   119			rsp := read(t, `{"status":"success","data":{"resultType":"matrix","result":[],"stats":{"samples":{"totalQueryableSamples":7}}}}`)
   120			require.Len(t, rsp.Frames, 1)
   121			require.Equal(t, "Query statistics", rsp.Frames[0].Name)
   122			require.Empty(t, rsp.Frames[0].Fields)
   123			require.Equal(t, float64(7), rsp.Frames[0].Meta.Stats[0].Value)
   124		})
   125	
   126		t.Run("malformed stats do not fail a valid query", func(t *testing.T) {
   127			rsp := read(t, `{"status":"success","data":{"resultType":"vector","result":`+instantResult+`,"stats":"not-an-object"}}`)
   128			require.Len(t, rsp.Frames, 1)
   129			require.Empty(t, rsp.Frames[0].Meta.Stats)
   130			custom := rsp.Frames[0].Meta.Custom.(map[string]any)
   131			require.Equal(t, "not-an-object", custom["stats"])
   132		})
   133	
   134		t.Run("malformed total omits only the typed stat", func(t *testing.T) {
   135			rsp := read(t, `{"status":"success","data":{"resultType":"vector","result":`+instantResult+`,"stats":{"samples":{"totalQueryableSamples":"unknown","totalQueryableSamplesPerStep":`+perStep+`}}}}`)
   136			require.Empty(t, rsp.Frames[0].Meta.Stats)
   137			custom := rsp.Frames[0].Meta.Custom.(map[string]any)
   138			require.NotNil(t, custom["stats"])
   139		})
   140	
   141		t.Run("missing stats leaves existing metadata unchanged", func(t *testing.T) {
   142			rsp := read(t, `{"status":"success","data":{"resultType":"vector","result":`+instantResult+`}}`)
   143			require.Empty(t, rsp.Frames[0].Meta.Stats)
   144			require.Equal(t, map[string]any{"resultType": "vector"}, rsp.Frames[0].Meta.Custom)
   145		})
   146	}
   147	
   148	func runScenario(name string, opts Options) func(t *testing.T) {
   149		return func(t *testing.T) {
   150			// Safe to disable, this is a test.
   151			// nolint:gosec
   152			f, err := os.Open(path.Join("testdata", name+".json"))
   153			require.NoError(t, err)
   154	
   155			iter := jsoniter.Parse(sdkjsoniter.ConfigDefault, f, 1024)
   156			rsp := ReadPrometheusStyleResult(iter, opts)
   157	
   158			if strings.Contains(name, "error") {
   159				require.Error(t, rsp.Error)
   160				return
   161			}
   162	
   163			if strings.Contains(name, "warnings") {
   164				hasWarning := false
   165				for _, frame := range rsp.Frames {
   166					for _, notice := range frame.Meta.Notices {
   167						if notice.Severity == data.NoticeSeverityWarning {
   168							hasWarning = true
   169							break
   170						}
   171					}
   172					if hasWarning {
   173						break
   174					}
   175				}
   176	
   177				require.True(t, hasWarning)
   178			}
   179	
   180			if strings.Contains(name, "infos") {
   181				hasInfo := false
   182				for _, frame := range rsp.Frames {
   183					for _, notice := range frame.Meta.Notices {
   184						if notice.Severity == data.NoticeSeverityInfo {
   185							hasInfo = true
   186							break
   187						}
   188					}
   189					if hasInfo {
   190						break
   191					}
   192				}
   193	
   194				require.True(t, hasInfo)
   195			}
   196	
   197			require.NoError(t, rsp.Error)
   198	
   199			fname := name + "-frame"
   200			experimental.CheckGoldenJSONResponse(t, "testdata", fname, &rsp, update)
     1	package querydata_test
     2	
     3	import (
     4		"bytes"
     5		"context"
     6		"encoding/json"
     7		"fmt"
     8		"io"
     9		"net/http"
    10		"net/url"
    11		"os"
    12		"path/filepath"
    13		"testing"
    14		"time"
    15	
    16		"github.com/stretchr/testify/require"
    17	
    18		"github.com/grafana/grafana-plugin-sdk-go/backend"
    19		"github.com/grafana/grafana-plugin-sdk-go/experimental"
    20		sdkapi "github.com/grafana/grafana-plugin-sdk-go/experimental/apis/datasource/v0alpha1"
    21	
    22		"github.com/grafana/grafana-prometheus-datasource/pkg/promlib/models"
    23	)
    24	
    25	var update = false
    26	
    27	func TestRangeResponses(t *testing.T) {
    28		tt := []struct {
    29			name     string
    30			filepath string
    31		}{
    32			{name: "parse a simple matrix response", filepath: "range_simple"},
    33			{name: "parse a simple matrix response with value missing steps", filepath: "range_missing"},
    34			{name: "parse a matrix response with Infinity", filepath: "range_infinity"},
    35			{name: "parse a matrix response with NaN", filepath: "range_nan"},
    36			{name: "parse a response with legendFormat __auto", filepath: "range_auto"},
    37		}
    38	
    39		for _, test := range tt {
    40			queryFileName := filepath.Join("../testdata", test.filepath+".query.json")
    41			responseFileName := filepath.Join("../testdata", test.filepath+".result.json")
    42			goldenFileName := test.filepath + ".result.golden"
    43			t.Run(test.name, goldenScenario(test.name, queryFileName, responseFileName, goldenFileName))
    44		}
    45	}
    46	
    47	func TestExemplarResponses(t *testing.T) {
    48		tt := []struct {
    49			name     string
    50			filepath string
    51		}{
    52			{name: "parse an exemplar response", filepath: "exemplar"},
    53		}
    54	
    55		for _, test := range tt {
    56			queryFileName := filepath.Join("../testdata", test.filepath+".query.json")
    57			responseFileName := filepath.Join("../testdata", test.filepath+".result.json")
    58			goldenFileName := test.filepath + ".result.golden"
    59			t.Run(test.name, goldenScenario(test.name, queryFileName, responseFileName, goldenFileName))
    60		}
    61	}
    62	
    63	func goldenScenario(name, queryFileName, responseFileName, goldenFileName string) func(t *testing.T) {
    64		return func(t *testing.T) {
    65			query, err := loadStoredQuery(queryFileName)
    66			require.NoError(t, err)
    67	
    68			//nolint:gosec
    69			responseBytes, err := os.ReadFile(responseFileName)
    70			require.NoError(t, err)
    71	
    72			result, err := runQuery(responseBytes, query)
    73			require.NoError(t, err)
    74			require.Len(t, result.Responses, 1)
    75	
    76			dr, found := result.Responses["A"]
    77			require.True(t, found)
    78	
    79			experimental.CheckGoldenJSONResponse(t, "../testdata", goldenFileName, &dr, update)
    80		}
    81	}
    82	
    83	// we store the prometheus query data in a json file, here is some minimal code
    84	// to be able to read it back. unfortunately we cannot use the models.Query
    85	// struct here, because it has `time.time` and `time.duration` fields that
    86	// cannot be unmarshalled from JSON automatically.
    87	type storedPrometheusQuery struct {
    88		RefId         string
    89		RangeQuery    bool
    90		ExemplarQuery bool
    91		Start         int64
    92		End           int64
    93		Step          int64
    94		Expr          string
    95		LegendFormat  string
    96	}
    97	
    98	func loadStoredQuery(fileName string) (*backend.QueryDataRequest, error) {
    99		//nolint:gosec
   100		bytes, err := os.ReadFile(fileName)
   101		if err != nil {
   102			return nil, err
   103		}
   104	
   105		var sq storedPrometheusQuery
   106	
   107		err = json.Unmarshal(bytes, &sq)
   108		if err != nil {
   109			return nil, err
   110		}
   111	
   112		qm := models.QueryModel{
   113			PrometheusQueryProperties: models.PrometheusQueryProperties{
   114				Range:        sq.RangeQuery,
   115				Exemplar:     sq.ExemplarQuery,
   116				Expr:         sq.Expr,
   117				LegendFormat: sq.LegendFormat,
   118			},
   119			CommonQueryProperties: sdkapi.CommonQueryProperties{
   120				IntervalMS: float64(sq.Step * 1000),
   121			},
   122			Interval: fmt.Sprintf("%ds", sq.Step),
   123		}
   124	
   125		data, err := json.Marshal(&qm)
   126		if err != nil {
   127			return nil, err
   128		}
   129	
   130		return &backend.QueryDataRequest{
   131			Queries: []backend.DataQuery{
   132				{
   133					TimeRange: backend.TimeRange{
   134						From: time.Unix(sq.Start, 0),
   135						To:   time.Unix(sq.End, 0),
   136					},
   137					RefID:    sq.RefId,
   138					Interval: time.Second * time.Duration(sq.Step),
   139					JSON:     json.RawMessage(data),
   140				},
   141			},
   142		}, nil
   143	}
   144	
   145	func runQuery(response []byte, q *backend.QueryDataRequest) (*backend.QueryDataResponse, error) {
   146		tCtx, err := setup()
   147		if err != nil {
   148			return nil, err
   149		}
   150	
   151		// Create initial response
   152		res := &http.Response{
   153			StatusCode: 200,
   154			Body:       io.NopCloser(bytes.NewReader(response)),
   155			Request: &http.Request{
   156				URL: &url.URL{
   157					Path: "api/v1/query_range",
   158				},
   159			},
   160		}
   161	
   162		// Create a proper clone for the exemplar response with a different path
   163		exemplarRes := &http.Response{
   164			StatusCode: 200,
   165			Body:       io.NopCloser(bytes.NewReader(response)),
   166			Request: &http.Request{
   167				URL: &url.URL{
   168					Path: "api/v1/query_exemplars",
   169				},
   170			},
   171		}
   172	
   173		tCtx.httpProvider.setResponse(res, nil, exemplarRes)
   174	
   175		// Add GrafanaConfig to the context to prevent nil pointer dereference
   176		ctx := backend.WithGrafanaConfig(context.Background(), backend.NewGrafanaCfg(map[string]string{
   177			"concurrent_query_count": "10",
   178		}))
   179	
   180		// Add a PluginContext with GrafanaConfig to the request
   181		q.PluginContext = backend.PluginContext{
   182			GrafanaConfig: backend.NewGrafanaCfg(map[string]string{
   183				"concurrent_query_count": "10",
   184			}),
   185		}
   186	
   187		return tCtx.queryData.Execute(ctx, q)
   188	}

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "nl -ba upstream-prometheus/pkg/promlib/querydata/exemplar/sampler_stddev_test.go; nl -ba upstream-prometheus/pkg/promlib/querydata/exemplar/sampler_test.go; nl -ba upstream-prometheus/pkg/promlib/healthcheck_test.go; nl -ba upstream-prometheus/pkg/promlib/heuristics_test.go; nl -ba upstream-prometheus/pkg/promlib/models/lenient_test.go | sed -n '1,185p'" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
     1	package exemplar_test
     2	
     3	import (
     4		"testing"
     5		"time"
     6	
     7		"github.com/grafana/grafana-plugin-sdk-go/experimental"
     8	
     9		"github.com/grafana/grafana-prometheus-datasource/pkg/promlib/models"
    10		"github.com/grafana/grafana-prometheus-datasource/pkg/promlib/querydata/exemplar"
    11	)
    12	
    13	func TestStdDevSampler(t *testing.T) {
    14		sampler := exemplar.NewStandardDeviationSampler().(*exemplar.StandardDeviationSampler)
    15		t.Run("standard deviation sampler", func(t *testing.T) {
    16			tr := models.TimeRange{
    17				Start: time.Unix(0, 0),
    18				End:   time.Unix(100000, 0),
    19			}
    20			ex := generateTestExemplars(tr)
    21			sampler.SetStep(600 * time.Second)
    22			for i := 0; i < len(ex); i++ {
    23				sampler.Add(ex[i])
    24			}
    25			framer := exemplar.NewFramer(sampler, exemplar.NewLabelTracker())
    26			experimental.CheckGoldenJSONFramer(t, "testdata", "stddev_sampler", framer, update)
    27		})
    28	}
     1	package exemplar_test
     2	
     3	import (
     4		"testing"
     5		"time"
     6	
     7		"github.com/grafana/grafana-plugin-sdk-go/experimental"
     8	
     9		"github.com/grafana/grafana-prometheus-datasource/pkg/promlib/models"
    10		"github.com/grafana/grafana-prometheus-datasource/pkg/promlib/querydata/exemplar"
    11	)
    12	
    13	const update = true
    14	
    15	func TestNoOpSampler(t *testing.T) {
    16		sampler := exemplar.NewNoOpSampler().(*exemplar.NoOpSampler)
    17		t.Run("no-op sampler", func(t *testing.T) {
    18			tr := models.TimeRange{
    19				Start: time.Unix(0, 0),
    20				End:   time.Unix(2000, 0),
    21			}
    22			ex := generateTestExemplars(tr)
    23			for i := 0; i < len(ex); i++ {
    24				sampler.Add(ex[i])
    25			}
    26			framer := exemplar.NewFramer(sampler, exemplar.NewLabelTracker())
    27			experimental.CheckGoldenJSONFramer(t, "testdata", "noop_sampler", framer, update)
    28		})
    29	}
    30	
    31	func generateTestExemplars(tr models.TimeRange) []models.Exemplar {
    32		exemplars := []models.Exemplar{}
    33		next := tr.Start.UTC()
    34		for !next.Equal(tr.End) && !next.After(tr.End) {
    35			exemplars = append(exemplars, models.Exemplar{
    36				Timestamp: next,
    37				Value:     float64(next.Unix()),
    38			})
    39			next = next.Add(time.Second).UTC()
    40		}
    41		return exemplars
    42	}
     1	package promlib
     2	
     3	import (
     4		"context"
     5		"io"
     6		"net/http"
     7		"strings"
     8		"testing"
     9		"time"
    10	
    11		"github.com/grafana/grafana-plugin-sdk-go/backend"
    12		"github.com/grafana/grafana-plugin-sdk-go/backend/datasource"
    13		sdkhttpclient "github.com/grafana/grafana-plugin-sdk-go/backend/httpclient"
    14		"github.com/stretchr/testify/assert"
    15	)
    16	
    17	type healthCheckProvider[T http.RoundTripper] struct {
    18		sdkhttpclient.Provider
    19		RoundTripper *T
    20	}
    21	
    22	type healthCheckSuccessRoundTripper struct {
    23	}
    24	
    25	type healthCheckFailRoundTripper struct {
    26	}
    27	
    28	func (rt *healthCheckSuccessRoundTripper) RoundTrip(req *http.Request) (*http.Response, error) {
    29		return &http.Response{
    30			Status:     "200",
    31			StatusCode: 200,
    32			Header:     nil,
    33			Body: io.NopCloser(strings.NewReader(`{
    34				"status": "success",
    35				"data": {
    36					"resultType": "scalar",
    37					"result": [
    38						1692969348.331,
    39						"3"
    40					]
    41				}
    42			}`)),
    43			ContentLength: 0,
    44			Request:       req,
    45		}, nil
    46	}
    47	
    48	func (rt *healthCheckFailRoundTripper) RoundTrip(req *http.Request) (*http.Response, error) {
    49		return &http.Response{
    50			Status:        "400",
    51			StatusCode:    400,
    52			Header:        nil,
    53			Body:          nil,
    54			ContentLength: 0,
    55			Request:       req,
    56		}, nil
    57	}
    58	
    59	func (provider *healthCheckProvider[T]) New(opts ...sdkhttpclient.Options) (*http.Client, error) {
    60		client := &http.Client{}
    61		provider.RoundTripper = new(T)
    62		client.Transport = *provider.RoundTripper
    63		return client, nil
    64	}
    65	
    66	func (provider *healthCheckProvider[T]) GetTransport(opts ...sdkhttpclient.Options) (http.RoundTripper, error) {
    67		return *new(T), nil
    68	}
    69	
    70	func getMockProvider[T http.RoundTripper]() *sdkhttpclient.Provider {
    71		p := &healthCheckProvider[T]{
    72			RoundTripper: new(T),
    73		}
    74		anotherFN := func(o sdkhttpclient.Options, next http.RoundTripper) http.RoundTripper {
    75			return *p.RoundTripper
    76		}
    77		fn := sdkhttpclient.MiddlewareFunc(anotherFN)
    78		mid := sdkhttpclient.NamedMiddlewareFunc("mock", fn)
    79		return sdkhttpclient.NewProvider(sdkhttpclient.ProviderOptions{Middlewares: []sdkhttpclient.Middleware{mid}})
    80	}
    81	
    82	func Test_healthcheck(t *testing.T) {
    83		t.Run("should do a successful health check", func(t *testing.T) {
    84			httpProvider := getMockProvider[*healthCheckSuccessRoundTripper]()
    85			logger := backend.NewLoggerWith("logger", "test")
    86			s := &Service{
    87				im:     datasource.NewInstanceManager(newInstanceSettings(httpProvider, logger, mockExtendClientOpts)),
    88				logger: logger,
    89			}
    90	
    91			req := &backend.CheckHealthRequest{
    92				PluginContext: getPluginContext(),
    93				Headers:       nil,
    94			}
    95	
    96			res, err := s.CheckHealth(context.Background(), req)
    97			assert.NoError(t, err)
    98			assert.Equal(t, backend.HealthStatusOk, res.Status)
    99		})
   100	
   101		t.Run("should return an error for an unsuccessful health check", func(t *testing.T) {
   102			httpProvider := getMockProvider[*healthCheckFailRoundTripper]()
   103			logger := backend.NewLoggerWith("logger", "test")
   104			s := &Service{
   105				im:     datasource.NewInstanceManager(newInstanceSettings(httpProvider, logger, mockExtendClientOpts)),
   106				logger: logger,
   107			}
   108	
   109			req := &backend.CheckHealthRequest{
   110				PluginContext: getPluginContext(),
   111				Headers:       nil,
   112			}
   113	
   114			res, err := s.CheckHealth(context.Background(), req)
   115			assert.NoError(t, err)
   116			assert.Equal(t, backend.HealthStatusError, res.Status)
   117		})
   118	}
   119	
   120	func getPluginContext() backend.PluginContext {
   121		return backend.PluginContext{
   122			OrgID:                      0,
   123			PluginID:                   "prometheus",
   124			User:                       nil,
   125			AppInstanceSettings:        nil,
   126			DataSourceInstanceSettings: getPromInstanceSettings(),
   127			GrafanaConfig:              backend.NewGrafanaCfg(map[string]string{"concurrent_query_count": "10"}),
   128		}
   129	}
   130	
   131	func getPromInstanceSettings() *backend.DataSourceInstanceSettings {
   132		return &backend.DataSourceInstanceSettings{
   133			ID:                      0,
   134			UID:                     "",
   135			Type:                    "prometheus",
   136			Name:                    "test-prometheus",
   137			URL:                     "http://promurl:9090",
   138			User:                    "",
   139			Database:                "",
   140			BasicAuthEnabled:        true,
   141			BasicAuthUser:           "admin",
   142			JSONData:                []byte("{}"),
   143			DecryptedSecureJSONData: map[string]string{},
   144			Updated:                 time.Time{},
   145		}
   146	}
     1	package promlib
     2	
     3	import (
     4		"context"
     5		"io"
     6		"net/http"
     7		"strconv"
     8		"strings"
     9		"testing"
    10	
    11		"github.com/stretchr/testify/assert"
    12		"github.com/stretchr/testify/require"
    13	
    14		"github.com/grafana/grafana-plugin-sdk-go/backend"
    15		"github.com/grafana/grafana-plugin-sdk-go/backend/datasource"
    16		sdkhttpclient "github.com/grafana/grafana-plugin-sdk-go/backend/httpclient"
    17		"github.com/grafana/grafana-plugin-sdk-go/backend/log"
    18	)
    19	
    20	type heuristicsSuccessRoundTripper struct {
    21		res    io.ReadCloser
    22		status int
    23	}
    24	
    25	func (rt *heuristicsSuccessRoundTripper) RoundTrip(req *http.Request) (*http.Response, error) {
    26		return &http.Response{
    27			Status:        strconv.Itoa(rt.status),
    28			StatusCode:    rt.status,
    29			Header:        nil,
    30			Body:          rt.res,
    31			ContentLength: 0,
    32			Request:       req,
    33		}, nil
    34	}
    35	
    36	func newHeuristicsSDKProvider(hrt heuristicsSuccessRoundTripper) *sdkhttpclient.Provider {
    37		anotherFN := func(o sdkhttpclient.Options, next http.RoundTripper) http.RoundTripper {
    38			return &hrt
    39		}
    40		fn := sdkhttpclient.MiddlewareFunc(anotherFN)
    41		mid := sdkhttpclient.NamedMiddlewareFunc("mock", fn)
    42		return sdkhttpclient.NewProvider(sdkhttpclient.ProviderOptions{Middlewares: []sdkhttpclient.Middleware{mid}})
    43	}
    44	
    45	func mockExtendClientOpts(ctx context.Context, settings backend.DataSourceInstanceSettings, clientOpts *sdkhttpclient.Options, log log.Logger) error {
    46		return nil
    47	}
    48	
    49	// capturingRoundTripper records the outgoing request so tests can assert on the method/URL.
    50	type capturingRoundTripper struct {
    51		gotReq *http.Request
    52		res    io.ReadCloser
    53		status int
    54	}
    55	
    56	func (rt *capturingRoundTripper) RoundTrip(req *http.Request) (*http.Response, error) {
    57		rt.gotReq = req
    58		return &http.Response{
    59			Status:     strconv.Itoa(rt.status),
    60			StatusCode: rt.status,
    61			Body:       rt.res,
    62			Request:    req,
    63		}, nil
    64	}
    65	
    66	func newCapturingSDKProvider(rt *capturingRoundTripper) *sdkhttpclient.Provider {
    67		fn := sdkhttpclient.MiddlewareFunc(func(o sdkhttpclient.Options, next http.RoundTripper) http.RoundTripper {
    68			return rt
    69		})
    70		mid := sdkhttpclient.NamedMiddlewareFunc("mock", fn)
    71		return sdkhttpclient.NewProvider(sdkhttpclient.ProviderOptions{Middlewares: []sdkhttpclient.Middleware{mid}})
    72	}
    73	
    74	func Test_GetHeuristics(t *testing.T) {
    75		t.Run("should return Prometheus", func(t *testing.T) {
    76			rt := heuristicsSuccessRoundTripper{
    77				res:    io.NopCloser(strings.NewReader("{\"status\":\"success\",\"data\":{\"version\":\"1.0\"}}")),
    78				status: http.StatusOK,
    79			}
    80			httpProvider := newHeuristicsSDKProvider(rt)
    81			logger := backend.NewLoggerWith("logger", "test")
    82			s := &Service{
    83				im:     datasource.NewInstanceManager(newInstanceSettings(httpProvider, logger, mockExtendClientOpts)),
    84				logger: logger,
    85			}
    86	
    87			req := HeuristicsRequest{
    88				PluginContext: getPluginContext(),
    89			}
    90			res, err := s.GetHeuristics(context.Background(), req)
    91			assert.NoError(t, err)
    92			require.NotNil(t, res)
    93			assert.Equal(t, KindPrometheus, res.Application)
    94			assert.Equal(t, Features{RulerApiEnabled: false}, res.Features)
    95		})
    96	
    97		t.Run("should return Mimir", func(t *testing.T) {
    98			rt := heuristicsSuccessRoundTripper{
    99				res:    io.NopCloser(strings.NewReader("{\"status\":\"success\",\"data\":{\"features\":{\"foo\":\"bar\"},\"version\":\"1.0\"}}")),
   100				status: http.StatusOK,
   101			}
   102			httpProvider := newHeuristicsSDKProvider(rt)
   103			logger := backend.NewLoggerWith("logger", "test")
   104			s := &Service{
   105				im:     datasource.NewInstanceManager(newInstanceSettings(httpProvider, logger, mockExtendClientOpts)),
   106				logger: logger,
   107			}
   108	
   109			req := HeuristicsRequest{
   110				PluginContext: getPluginContext(),
   111			}
   112			res, err := s.GetHeuristics(context.Background(), req)
   113			assert.NoError(t, err)
   114			require.NotNil(t, res)
   115			assert.Equal(t, KindMimir, res.Application)
   116			assert.Equal(t, Features{RulerApiEnabled: true}, res.Features)
   117		})
   118	
   119		t.Run("buildinfo request is always sent as GET even for POST-configured datasources", func(t *testing.T) {
   120			// Regression test: /api/v1/status/buildinfo is GET-only. getBuildInfo must force GET so the
   121			// request does not inherit the datasource's configured POST method and get a 405 back.
   122			rt := &capturingRoundTripper{
   123				res:    io.NopCloser(strings.NewReader(`{"status":"success","data":{"version":"1.0"}}`)),
   124				status: http.StatusOK,
   125			}
   126			httpProvider := newCapturingSDKProvider(rt)
   127			logger := backend.NewLoggerWith("logger", "test")
   128			s := &Service{
   129				im:     datasource.NewInstanceManager(newInstanceSettings(httpProvider, logger, mockExtendClientOpts)),
   130				logger: logger,
   131			}
   132	
   133			// getPluginContext configures the datasource with httpMethod POST (see helper).
   134			res, err := s.GetHeuristics(context.Background(), HeuristicsRequest{PluginContext: getPluginContext()})
   135			require.NoError(t, err)
   136			require.NotNil(t, res)
   137			require.NotNil(t, rt.gotReq)
   138			assert.Equal(t, http.MethodGet, rt.gotReq.Method)
   139			assert.Equal(t, "/api/v1/status/buildinfo", rt.gotReq.URL.Path)
   140		})
   141	}
     1	package models_test
     2	
     3	import (
     4		"encoding/json"
     5		"fmt"
     6		"maps"
     7		"net/http"
     8		"reflect"
     9		"slices"
    10		"strings"
    11		"testing"
    12	
    13		"github.com/grafana/grafana-plugin-sdk-go/backend"
    14		"github.com/grafana/grafana-plugin-sdk-go/backend/log"
    15		"github.com/stretchr/testify/require"
    16	
    17		"github.com/grafana/grafana-prometheus-datasource/pkg/promlib/models"
    18	)
    19	
    20	// Mistyping any property #220 declared was harmless before it, so it must not fail the
    21	// datasource now.
    22	func TestParsePromOptions_LooselyTypedJSONData(t *testing.T) {
    23		cases := []struct {
    24			name     string
    25			jsonData string
    26			assert   func(t *testing.T, opts *models.PromOptions)
    27		}{
    28			{
    29				name:     "booleans stored as quoted strings",
    30				jsonData: `{"seriesEndpoint":"true","disableRecordingRules":"false","oauthPassThru":"1"}`,
    31				assert: func(t *testing.T, opts *models.PromOptions) {
    32					require.True(t, bool(opts.SeriesEndpoint))
    33					require.False(t, bool(opts.DisableRecordingRules))
    34					require.True(t, bool(opts.OauthPassThru))
    35				},
    36			},
    37			{
    38				name:     "capitalised booleans are not silently inverted",
    39				jsonData: `{"seriesEndpoint":"True","disableMetricsLookup":"TRUE","incrementalQuerying":"False"}`,
    40				assert: func(t *testing.T, opts *models.PromOptions) {
    41					require.True(t, bool(opts.SeriesEndpoint))
    42					require.True(t, bool(opts.DisableMetricsLookup))
    43					require.False(t, bool(opts.IncrementalQuerying))
    44				},
    45			},
    46			{
    47				name:     "an unrecognised boolean spelling falls back to false",
    48				jsonData: `{"seriesEndpoint":"yes","oauthPassThru":"maybe"}`,
    49				assert: func(t *testing.T, opts *models.PromOptions) {
    50					require.False(t, bool(opts.SeriesEndpoint))
    51					require.False(t, bool(opts.OauthPassThru))
    52				},
    53			},
    54			{
    55				name:     "booleans stored as 0/1",
    56				jsonData: `{"queryStatsEnabled":1,"disableMetricsLookup":0}`,
    57				assert: func(t *testing.T, opts *models.PromOptions) {
    58					require.True(t, bool(opts.QueryStatsEnabled))
    59					require.False(t, bool(opts.DisableMetricsLookup))
    60				},
    61			},
    62			{
    63				name:     "strings stored as bare numbers keep their text",
    64				jsonData: `{"incrementalQueryOverlapWindow":10,"prometheusVersion":2.4,"customQueryParameters":123}`,
    65				assert: func(t *testing.T, opts *models.PromOptions) {
    66					require.Equal(t, "10", string(opts.IncrementalQueryOverlapWindow))
    67					require.Equal(t, "2.4", string(opts.PrometheusVersion))
    68					require.Equal(t, "123", string(opts.CustomQueryParameters))
    69				},
    70			},
    71			{
    72				name:     "thresholds stored as quoted numbers",
    73				jsonData: `{"maxSamplesProcessedWarningThreshold":"100000","maxSamplesProcessedErrorThreshold":"200000"}`,
    74				assert: func(t *testing.T, opts *models.PromOptions) {
    75					require.Equal(t, 100000.0, float64(opts.MaxSamplesProcessedWarningThreshold))
    76					require.Equal(t, 200000.0, float64(opts.MaxSamplesProcessedErrorThreshold))
    77				},
    78			},
    79			{
    80				name:     "seriesLimit stored as a quoted number",
    81				jsonData: `{"seriesLimit":"1000"}`,
    82				assert: func(t *testing.T, opts *models.PromOptions) {
    83					require.NotNil(t, opts.SeriesLimit)
    84					require.Equal(t, 1000.0, float64(*opts.SeriesLimit))
    85				},
    86			},
    87			{
    88				// 1000.0 is schema-valid but encoding/json rejects it for an integer field.
    89				name:     "seriesLimit stored as a fractional literal",
    90				jsonData: `{"seriesLimit":1000.0}`,
    91				assert: func(t *testing.T, opts *models.PromOptions) {
    92					require.NotNil(t, opts.SeriesLimit)
    93					require.Equal(t, 1000.0, float64(*opts.SeriesLimit))
    94				},
    95			},
    96			{
    97				// Not salvaged into a one-element list: the backend never reads this, so a
    98				// guessed value would only disagree with what the frontend reads.
    99				name:     "an exemplar value that is not a list is ignored",
   100				jsonData: `{"exemplarTraceIdDestinations":{"name":"traceID"},"queryStatsEnabled":"true"}`,
   101				assert: func(t *testing.T, opts *models.PromOptions) {
   102					require.Empty(t, opts.ExemplarTraceIDDestinations)
   103					require.True(t, bool(opts.QueryStatsEnabled))
   104				},
   105			},
   106			{
   107				name:     "a well-formed exemplar list still decodes",
   108				jsonData: `{"exemplarTraceIdDestinations":[{"name":"traceID","datasourceUid":"abc"}]}`,
   109				assert: func(t *testing.T, opts *models.PromOptions) {
   110					require.Len(t, opts.ExemplarTraceIDDestinations, 1)
   111					require.Equal(t, "traceID", opts.ExemplarTraceIDDestinations[0].Name)
   112					require.Equal(t, "abc", opts.ExemplarTraceIDDestinations[0].DatasourceUID)
   113				},
   114			},
   115			{
   116				name:     "a value that cannot be read falls back to the zero value",
   117				jsonData: `{"seriesEndpoint":{"a":1},"prometheusVersion":["x"],"queryStatsEnabled":"true"}`,
   118				assert: func(t *testing.T, opts *models.PromOptions) {
   119					require.False(t, bool(opts.SeriesEndpoint))
   120					require.Empty(t, opts.PrometheusVersion)
   121					require.True(t, bool(opts.QueryStatsEnabled))
   122				},
   123			},
   124			{
   125				name:     "a mistyped field does not discard the fields around it",
   126				jsonData: `{"httpMethod":"GET","timeInterval":"30s","seriesLimit":"5","queryStatsEnabled":"true"}`,
   127				assert: func(t *testing.T, opts *models.PromOptions) {
   128					require.Equal(t, http.MethodGet, opts.HTTPMethod)
   129					require.Equal(t, "30s", opts.TimeInterval)
   130					require.NotNil(t, opts.SeriesLimit)
   131					require.Equal(t, 5.0, float64(*opts.SeriesLimit))
   132					require.True(t, bool(opts.QueryStatsEnabled))
   133				},
   134			},
   135		}
   136	
   137		for _, tc := range cases {
   138			t.Run(tc.name, func(t *testing.T) {
   139				opts, err := models.ParsePromOptions(backend.DataSourceInstanceSettings{
   140					JSONData: []byte(tc.jsonData),
   141				})
   142				require.NoError(t, err)
   143				tc.assert(t, opts)
   144			})
   145		}
   146	}
   147	
   148	// Already strict before #220, so they stay strict.
   149	func TestParsePromOptions_PreexistingStrictFieldsStayStrict(t *testing.T) {
   150		for _, jsonData := range []string{
   151			`{"httpMethod":30}`,
   152			`{"timeInterval":30}`,
   153			`{"queryTimeout":60}`,
   154		} {
   155			t.Run(jsonData, func(t *testing.T) {
   156				_, err := models.ParsePromOptions(backend.DataSourceInstanceSettings{
   157					JSONData: []byte(jsonData),
   158				})
   159				require.ErrorContains(t, err, "error unmarshalling JSONData")
   160			})
   161		}
   162	}
   163	
   164	// Every lenient property must tolerate any stored type. The key list comes from the struct on
   165	// purpose: the original gap was a property nobody remembered to account for.
   166	func TestParsePromOptions_LenientFieldsCannotFailTheDatasource(t *testing.T) {
   167		values := []string{
   168			`"true"`, `"false"`, `"True"`, `"nonsense"`, `true`, `false`, `1`, `0`,
   169			`30.5`, `30.0`, `60`, `"60"`, `"30s"`, `"abc"`, `""`, `null`,
   170			`[]`, `["a"]`, `[1]`, `{}`, `{"a":1}`,
   171		}
   172	
   173		for _, key := range jsonDataKeys(t) {
   174			if strictJSONDataFields[key] {
   175				continue
   176			}
   177			for _, value := range values {
   178				jsonData := fmt.Sprintf(`{%q:%s}`, key, value)
   179				t.Run(key+"="+value, func(t *testing.T) {
   180					opts, err := models.ParsePromOptions(backend.DataSourceInstanceSettings{
   181						JSONData: []byte(jsonData),
   182					})
   183					require.NoError(t, err, "jsonData %s must not fail the datasource", jsonData)
   184					require.NotNil(t, opts)
   185				})

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "git status --short; rg -n --glob '*.rs' --glob '"'!**/captured/**'"' 'bench|prom1|gsshcd|9390|3300|3310|3400|localhost|127.0.0.1|include_str"'!|include_bytes!'"' crates/grafana/src crates/store-sqlite/src crates/server/src | cut -c 1-230; nl -ba upstream-prometheus/pkg/promlib/models/query_test.go | sed -n '1,135p'; rg -n 'NumericFormat|format 2|numeric|TimeSeries|table|heatmap' upstream-prometheus/pkg/promlib/models/query_test.go | tail -30" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
?? bench/grafana-rs.sh
?? bench/results/grafana-rs-startup.txt
crates/server/src/main.rs:4://! GF_SERVER_HTTP_PORT (default 3400 here), GF_SERVER_HTTP_ADDR, GF_AUTH_ANONYMOUS_ENABLED,
crates/server/src/main.rs:57:    let cfg = Settings::load(home, &[("server", "http_port", "3400"), ("paths", "data", DEFAULT_DATA)]);
crates/store-sqlite/src/lib.rs:25:const SCHEMA: &str = include_str!("schema.sql");
crates/grafana/src/api/discovery.rs:28:        let data = Json::parse(include_str!("captured/discovery.json")).unwrap_or(Json::Null);
crates/grafana/src/api/stubs.rs:115:        let list = Json::parse(include_str!("captured/advisor-checktypes.json")).unwrap_or(Json::Null);
crates/grafana/src/api/ofrep.rs:100:    let data = Json::parse(include_str!("captured/ofrep-flags.json")).unwrap_or(Json::Null);
crates/grafana/src/seed.rs:24:        email: if cfg.admin_email.is_empty() { "admin@localhost".to_string() } else { cfg.admin_email.clone() },
crates/grafana/src/api/boot.rs:878:const W_DRILLDOWN: i64 = -3400;
     1	package models_test
     2	
     3	import (
     4		"context"
     5		"encoding/json"
     6		"fmt"
     7		"reflect"
     8		"testing"
     9		"time"
    10	
    11		"github.com/stretchr/testify/require"
    12		"go.opentelemetry.io/otel"
    13	
    14		"github.com/grafana/grafana-plugin-sdk-go/backend"
    15		"github.com/grafana/grafana-plugin-sdk-go/backend/log"
    16		sdkapi "github.com/grafana/grafana-plugin-sdk-go/experimental/apis/datasource/v0alpha1"
    17		"github.com/grafana/grafana-plugin-sdk-go/experimental/schemabuilder"
    18	
    19		"github.com/grafana/grafana-prometheus-datasource/pkg/promlib/intervalv2"
    20		"github.com/grafana/grafana-prometheus-datasource/pkg/promlib/models"
    21	)
    22	
    23	var (
    24		now                = time.Now()
    25		intervalCalculator = intervalv2.NewCalculator()
    26		tracer             = otel.Tracer("instrumentation/package/name")
    27	)
    28	
    29	func TestParse(t *testing.T) {
    30		_, span := tracer.Start(context.Background(), "operation")
    31		defer span.End()
    32		t.Run("parsing query from unified alerting", func(t *testing.T) {
    33			timeRange := backend.TimeRange{
    34				From: now,
    35				To:   now.Add(12 * time.Hour),
    36			}
    37	
    38			queryJson := `{
    39				"expr": "go_goroutines",
    40				"refId": "A",
    41				"exemplar": true
    42			}`
    43	
    44			q := backend.DataQuery{
    45				JSON:      []byte(queryJson),
    46				TimeRange: timeRange,
    47				RefID:     "A",
    48			}
    49	
    50			res, err := models.Parse(context.Background(), log.New(), span, q, "15s", intervalCalculator, true)
    51			require.NoError(t, err)
    52			require.Equal(t, false, res.ExemplarQuery)
    53		})
    54	
    55		t.Run("parsing query model with step", func(t *testing.T) {
    56			timeRange := backend.TimeRange{
    57				From: now,
    58				To:   now.Add(12 * time.Hour),
    59			}
    60	
    61			q := queryContext(`{
    62				"expr": "go_goroutines",
    63				"format": "time_series",
    64				"refId": "A"
    65			}`, timeRange, time.Duration(1)*time.Minute)
    66	
    67			res, err := models.Parse(context.Background(), log.New(), span, q, "15s", intervalCalculator, false)
    68			require.NoError(t, err)
    69			require.Equal(t, time.Second*30, res.Step)
    70		})
    71	
    72		t.Run("parsing query model without step parameter", func(t *testing.T) {
    73			timeRange := backend.TimeRange{
    74				From: now,
    75				To:   now.Add(1 * time.Hour),
    76			}
    77	
    78			q := queryContext(`{
    79				"expr": "go_goroutines",
    80				"format": "time_series",
    81				"intervalFactor": 1,
    82				"refId": "A"
    83			}`, timeRange, time.Duration(1)*time.Minute)
    84	
    85			res, err := models.Parse(context.Background(), log.New(), span, q, "15s", intervalCalculator, false)
    86			require.NoError(t, err)
    87			require.Equal(t, time.Second*15, res.Step)
    88		})
    89	
    90		t.Run("parsing query model with high intervalFactor", func(t *testing.T) {
    91			timeRange := backend.TimeRange{
    92				From: now,
    93				To:   now.Add(48 * time.Hour),
    94			}
    95	
    96			q := queryContext(`{
    97				"expr": "go_goroutines",
    98				"format": "time_series",
    99				"intervalFactor": 10,
   100				"refId": "A"
   101			}`, timeRange, time.Duration(1)*time.Minute)
   102	
   103			res, err := models.Parse(context.Background(), log.New(), span, q, "15s", intervalCalculator, false)
   104			require.NoError(t, err)
   105			require.Equal(t, time.Minute*20, res.Step)
   106		})
   107	
   108		t.Run("parsing query model with low intervalFactor", func(t *testing.T) {
   109			timeRange := backend.TimeRange{
   110				From: now,
   111				To:   now.Add(48 * time.Hour),
   112			}
   113	
   114			q := queryContext(`{
   115				"expr": "go_goroutines",
   116				"format": "time_series",
   117				"intervalFactor": 1,
   118				"refId": "A"
   119			}`, timeRange, time.Duration(1)*time.Minute)
   120	
   121			res, err := models.Parse(context.Background(), log.New(), span, q, "15s", intervalCalculator, false)
   122			require.NoError(t, err)
   123			require.Equal(t, time.Minute*2, res.Step)
   124		})
   125	
   126		t.Run("parsing query model specified scrape-interval in the data source", func(t *testing.T) {
   127			timeRange := backend.TimeRange{
   128				From: now,
   129				To:   now.Add(48 * time.Hour),
   130			}
   131	
   132			q := queryContext(`{
   133				"expr": "go_goroutines",
   134				"format": "time_series",
   135				"intervalFactor": 1,
1293:func TestParseNumericFormatError(t *testing.T) {
1339:	t.Run("format as number 2 should map to table", func(t *testing.T) {
1357:	t.Run("format as number 3 should map to heatmap", func(t *testing.T) {
1422:				expected: models.PromQueryFormatTimeSeries,
1425:				name:     "table format",
1426:				json:     `{"format": "table"}`,
1430:				name:     "heatmap format",
1431:				json:     `{"format": "heatmap"}`,
1474:				require.Equal(t, models.PromQueryFormatTimeSeries, props.Format, "invalid format should default to time_series")
1489:				reflect.TypeOf(models.PromQueryFormatTimeSeries), // pick an example value (not the root)

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "nl -ba crates/grafana/src/promlib/settings.rs | sed -n '1,260p'; nl -ba crates/grafana/src/promlib/mod.rs | sed -n '1,270p'; nl -ba crates/grafana/src/api/prometheus.rs | sed -n '1,260p'" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
     1	//! Data source settings for the plugin: promlib `models/settings.go` (`ParsePromOptions`, lenient members) and
     2	//! the HTTP client options of the plugin SDK that this plugin uses (basic auth, custom headers, timeout).
     3	use json::Json;
     4	
     5	/// `models.PromOptions`, the members the backend reads.
     6	#[derive(Debug, Clone, Default, PartialEq)]
     7	pub struct PromOptions {
     8	    pub http_method: String,
     9	    pub time_interval: String,
    10	    pub query_timeout: String,
    11	    pub custom_query_parameters: String,
    12	    pub max_samples_processed_warning_threshold: f64,
    13	    pub max_samples_processed_error_threshold: f64,
    14	    pub query_stats_enabled: bool,
    15	    pub prometheus_type: String,
    16	    pub series_limit: Option<f64>,
    17	    /// `timeout` (seconds) of the HTTP client options; 0 for the default.
    18	    pub timeout: f64,
    19	}
    20	
    21	fn type_name(v: &Json) -> &'static str {
    22	    match v {
    23	        Json::Null => "null",
    24	        Json::Bool(_) => "bool",
    25	        Json::Int(_) | Json::Float(_) => "number",
    26	        Json::Str(_) => "string",
    27	        Json::Arr(_) => "array",
    28	        Json::Obj(_) => "object",
    29	    }
    30	}
    31	
    32	fn strict_string(o: &Json, k: &str) -> Result<String, String> {
    33	    match super::models::get_ci(o, k) {
    34	        None | Some(Json::Null) => Ok(String::new()),
    35	        Some(Json::Str(s)) => Ok(s.clone()),
    36	        Some(v) => Err(format!("error unmarshalling JSONData: json: cannot unmarshal {} into Go struct field PromOptions.{k} of type string", type_name(v))),
    37	    }
    38	}
    39	
    40	/// `LenientString`: strings, or the JSON text of a number or bool.
    41	fn lenient_string(o: &Json, k: &str) -> String {
    42	    match super::models::get_ci(o, k) {
    43	        Some(Json::Str(s)) => s.clone(),
    44	        Some(v @ (Json::Int(_) | Json::Float(_) | Json::Bool(_))) => v.stringify(),
    45	        _ => String::new(),
    46	    }
    47	}
    48	
    49	/// `readFloat64`: a number or a number string.
    50	fn lenient_f64(v: Option<&Json>) -> Option<f64> {
    51	    match v {
    52	        Some(Json::Int(i)) => Some(*i as f64),
    53	        Some(Json::Float(f)) => Some(*f),
    54	        Some(Json::Str(s)) => s.trim().parse::<f64>().ok(),
    55	        _ => None,
    56	    }
    57	}
    58	
    59	/// `LenientBool`: bools, `strconv.ParseBool` strings, numbers (non-zero is true).
    60	fn lenient_bool(v: Option<&Json>) -> bool {
    61	    match v {
    62	        Some(Json::Bool(b)) => *b,
    63	        Some(Json::Str(s)) => matches!(s.trim(), "1" | "t" | "T" | "TRUE" | "true" | "True"),
    64	        Some(Json::Int(i)) => *i != 0,
    65	        Some(Json::Float(f)) => *f != 0.0,
    66	        _ => false,
    67	    }
    68	}
    69	
    70	impl PromOptions {
    71	    /// `ApplyDefaults`: trims and uppercases the method; empty is POST.
    72	    pub fn apply_defaults(&mut self) {
    73	        self.http_method = self.http_method.trim().to_ascii_uppercase();
    74	        if self.http_method.is_empty() {
    75	            self.http_method = "POST".to_string();
    76	        }
    77	    }
    78	    /// `Validate`.
    79	    pub fn validate(&self) -> Result<(), String> {
    80	        let m = self.http_method.to_ascii_uppercase();
    81	        if !m.is_empty() && m != "GET" && m != "POST" {
    82	            return Err(format!("invalid httpMethod {}: must be GET or POST", json::quote(&self.http_method)));
    83	        }
    84	        Ok(())
    85	    }
    86	}
    87	
    88	/// `ParsePromOptions` from the `jsonData` text.
    89	pub fn parse_prom_options(json_data: &str) -> Result<PromOptions, String> {
    90	    let text = if json_data.trim().is_empty() { "{}" } else { json_data };
    91	    let o = match Json::parse(text) {
    92	        Ok(v) => v,
    93	        Err(e) => return Err(format!("error unmarshalling JSONData: {e:?}")),
    94	    };
    95	    if !matches!(o, Json::Obj(_) | Json::Null) {
    96	        return Err(format!("error unmarshalling JSONData: json: cannot unmarshal {} into Go value of type models.PromOptions", type_name(&o)));
    97	    }
    98	    let g = |k: &str| super::models::get_ci(&o, k);
    99	    let mut p = PromOptions {
   100	        http_method: strict_string(&o, "httpMethod")?,
   101	        time_interval: strict_string(&o, "timeInterval")?,
   102	        query_timeout: strict_string(&o, "queryTimeout")?,
   103	        custom_query_parameters: lenient_string(&o, "customQueryParameters"),
   104	        max_samples_processed_warning_threshold: lenient_f64(g("maxSamplesProcessedWarningThreshold")).unwrap_or(0.0),
   105	        max_samples_processed_error_threshold: lenient_f64(g("maxSamplesProcessedErrorThreshold")).unwrap_or(0.0),
   106	        query_stats_enabled: lenient_bool(g("queryStatsEnabled")),
   107	        prometheus_type: lenient_string(&o, "prometheusType"),
   108	        series_limit: lenient_f64(g("seriesLimit")),
   109	        timeout: lenient_f64(g("timeout")).unwrap_or(0.0),
   110	    };
   111	    p.apply_defaults();
   112	    p.validate()?;
   113	    Ok(p)
   114	}
     1	//! The Prometheus data source plugin backend in Rust: a port of `pkg/promlib` from grafana-prometheus-datasource
     2	//! v13.2.6 (`upstream-prometheus/`), the plugin Grafana 13.3.1 installs. It answers `/api/ds/query`, the health
     3	//! check and the resource calls in process (no plugin process, no gRPC/Arrow hop); the JSON it writes is what
     4	//! Grafana writes after that hop.
     5	pub mod client;
     6	pub mod converter;
     7	pub mod exemplar;
     8	pub mod fast;
     9	pub mod frame;
    10	pub mod jsonpull;
    11	pub mod models;
    12	pub mod querydata;
    13	pub mod settings;
    14	
    15	use client::{HttpReq, HttpResp, Instance, Transport};
    16	use json::Json;
    17	
    18	/// `healthcheck` + `getHeuristics`: (ok, message, details). Upstream `CheckHealth`.
    19	pub fn check_health(inst: &Instance, t: Transport<'_>) -> (bool, String, Option<Json>) {
    20	    let q = models::DataQuery {
    21	        ref_id: "__healthcheck__".to_string(),
    22	        from: models::SEC,
    23	        to: 4 * models::SEC,
    24	        json: Json::obj().with("refId", "__healthcheck__").with("expr", "1+1").with("instant", true),
    25	        ..Default::default()
    26	    };
    27	    let res = querydata::execute(inst, std::slice::from_ref(&q), t, false);
    28	    let err = res.into_iter().find(|r| r.0 == "__healthcheck__").and_then(|r| r.1.error);
    29	    let (ok, message) = match err {
    30	        None => (true, "Successfully queried the Prometheus API.".to_string()),
    31	        Some(e) => (false, format!("{e} - There was an error returned querying the Prometheus API.")),
    32	    };
    33	    let details = heuristics(inst, t);
    34	    (ok, message, details)
    35	}
    36	
    37	/// `getHeuristics`: Prometheus when buildinfo has no `features`, else Mimir with the ruler API.
    38	pub fn heuristics(inst: &Instance, t: Transport<'_>) -> Option<Json> {
    39	    let req = inst.resource_request("GET", "api/v1/status/buildinfo", "", &[]);
    40	    let res = t(&req).ok()?;
    41	    if res.status != 200 {
    42	        return None;
    43	    }
    44	    let v = Json::parse_bytes(&res.body).ok()?;
    45	    let features = match v.path("data.features") {
    46	        Some(Json::Obj(o)) => o.len(),
    47	        _ => 0,
    48	    };
    49	    let (app, ruler) = if features == 0 { ("Prometheus", false) } else { ("Mimir", true) };
    50	    Some(Json::obj().with("application", app).with("features", Json::obj().with("rulerApiEnabled", ruler)))
    51	}
    52	
    53	/// Resource paths the plugin forwards (`resource/allowlist.go`).
    54	pub fn is_allowed_resource_path(path: &str) -> bool {
    55	    let p = path.strip_prefix('/').unwrap_or(path);
    56	    if p.is_empty() || p.contains('%') {
    57	        return false;
    58	    }
    59	    if p.split('/').any(|s| s == "." || s == "..") {
    60	        return false;
    61	    }
    62	    match p {
    63	        "api/v1/query" | "api/v1/query_range" | "api/v1/query_exemplars" | "api/v1/series" | "api/v1/labels" | "api/v1/metadata" | "api/v1/rules" | "api/v1/status/buildinfo" => true,
    64	        "api/v1/search/metric_names" | "api/v1/search/label_names" | "api/v1/search/label_values" => true,
    65	        _ => match p.strip_prefix("api/v1/label/").and_then(|r| r.strip_suffix("/values")) {
    66	            Some(name) => !name.is_empty() && !name.contains('/'),
    67	            None => false,
    68	        },
    69	    }
    70	}
    71	
    72	/// A resource response (`Resource.Execute`): status, headers to forward, body.
    73	pub struct ResourceResponse {
    74	    pub status: u16,
    75	    pub headers: Vec<(String, String)>,
    76	    pub body: Vec<u8>,
    77	}
    78	
    79	/// `Resource.Execute` for a path that is not `suggestions`. `cache` is the caller's `X-Grafana-Cache` header:
    80	/// when set, the answer carries `X-Grafana-Cache: y` and `Cache-Control: <cache>`.
    81	pub fn call_resource(inst: &Instance, method: &str, path: &str, raw_query: &str, body: &[u8], cache: Option<&str>, t: Transport<'_>) -> Result<ResourceResponse, String> {
    82	    if !is_allowed_resource_path(path) {
    83	        return Ok(ResourceResponse {
    84	            status: 403,
    85	            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
    86	            body: b"{\"message\":\"the requested resource path is not supported by this datasource\"}".to_vec(),
    87	        });
    88	    }
    89	    let req: HttpReq = inst.resource_request(method, path, raw_query, body);
    90	    let res: HttpResp = match t(&req) {
    91	        Ok(r) => r,
    92	        Err(e) => return Err(format!("error querying resource: {}", e.message)),
    93	    };
    94	    let mut headers = Vec::new();
    95	    for (k, v) in res.headers {
    96	        match k.as_str() {
    97	            "content-encoding" | "content-length" | "transfer-encoding" | "connection" | "keep-alive" | "date" => {}
    98	            "x-grafana-cache" | "cache-control" if cache.map(|c| !c.is_empty()).unwrap_or(false) => {}
    99	            _ => headers.push((k, v)),
   100	        }
   101	    }
   102	    if let Some(c) = cache {
   103	        if !c.is_empty() {
   104	            headers.push(("X-Grafana-Cache".to_string(), "y".to_string()));
   105	            headers.push(("Cache-Control".to_string(), c.to_string()));
   106	        }
   107	    }
   108	    Ok(ResourceResponse { status: res.status, headers, body: res.body })
   109	}
     1	//! The Prometheus data-source plugin backend (agent C). Upstream: upstream-prometheus/pkg/promlib.
     2	use crate::ctx::Routes;
     3	
     4	pub fn routes(_r: &mut Routes) {}

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "rg -n '"'^func Test|t.Run'"\\(' upstream-prometheus/pkg/promlib/{library_test.go,client/transport_test.go,middleware/force_http_get_test.go,resource/search_test.go,resource/transport_error_test.go,utils/utils_test.go}; nl -ba upstream-prometheus/pkg/promlib/models/query_test.go | sed -n '1293,1415p'; rg -n 'uid|id|port|include|static|once' crates/grafana/src/api/dashboards.rs | head -65; rg -n 'fn .*uid|snowflake|internal_id|fn id|id: 1|id: 2|id = [123]|== [123]' crates/store-sqlite/src/{dashboards,folders,datasources,users}.rs" in /home/kamrot/code/grafana-rs
 exited 2 in 0ms:
upstream-prometheus/pkg/promlib/utils/utils_test.go:14:func TestDecode(t *testing.T) {
upstream-prometheus/pkg/promlib/utils/utils_test.go:45:		t.Run(tc.name, func(t *testing.T) {
upstream-prometheus/pkg/promlib/utils/utils_test.go:54:func TestDecodeReturnsErrorForInvalidGzip(t *testing.T) {
upstream-prometheus/pkg/promlib/utils/utils_test.go:63:func TestDecodeReturnsErrorForZstd(t *testing.T) {
upstream-prometheus/pkg/promlib/utils/utils_test.go:69:func TestDecodeReturnsErrorUnknownEncoding(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/transport_error_test.go:19:func TestResourceTransportErrorsKeepTheirSource(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/transport_error_test.go:31:		t.Run(tc.name, func(t *testing.T) {
upstream-prometheus/pkg/promlib/middleware/force_http_get_test.go:12:func TestEnsureHttpMethodMiddleware(t *testing.T) {
upstream-prometheus/pkg/promlib/middleware/force_http_get_test.go:13:	t.Run("Name should be correct", func(t *testing.T) {
upstream-prometheus/pkg/promlib/middleware/force_http_get_test.go:25:	t.Run("Should force GET method", func(t *testing.T) {
upstream-prometheus/pkg/promlib/client/transport_test.go:14:func TestCreateTransportOptions(t *testing.T) {
upstream-prometheus/pkg/promlib/client/transport_test.go:15:	t.Run("creates correct options object", func(t *testing.T) {
upstream-prometheus/pkg/promlib/client/transport_test.go:41:	t.Run("enables ForwardHTTPHeaders so Grafana headers (FromAlert, X-Rule-*, X-Dashboard-*, X-Panel-*, X-Grafana-*) are forwarded to the datasource", func(t *testing.T) {
upstream-prometheus/pkg/promlib/library_test.go:71:func TestService(t *testing.T) {
upstream-prometheus/pkg/promlib/library_test.go:72:	t.Run("Service", func(t *testing.T) {
upstream-prometheus/pkg/promlib/library_test.go:73:		t.Run("CallResource", func(t *testing.T) {
upstream-prometheus/pkg/promlib/library_test.go:74:			t.Run("creates correct request", func(t *testing.T) {
upstream-prometheus/pkg/promlib/library_test.go:100:	t.Run("no extendOptions function provided", func(t *testing.T) {
upstream-prometheus/pkg/promlib/library_test.go:108:	t.Run("extendOptions function provided", func(t *testing.T) {
upstream-prometheus/pkg/promlib/library_test.go:125:	t.Run("suggest resource", func(t *testing.T) {
upstream-prometheus/pkg/promlib/library_test.go:138:	t.Run("search resource uses streaming handler", func(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/search_test.go:23:func TestResourceExecuteSearchStreamsResponse(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/search_test.go:72:func TestResourceExecuteSearchRejectsDisallowedEndpoint(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/search_test.go:89:func TestResourceExecuteSearchDecodesGzipResponse(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/search_test.go:129:func TestResourceExecuteSearchPassesThroughErrorResponse(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/search_test.go:156:func TestResourceExecuteSearchStripsFramingHeadersFromErrorResponse(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/search_test.go:191:func TestResourceExecuteSearchDecodesGzipErrorResponse(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/search_test.go:226:func TestResourceExecuteSearchLimitsErrorBodySize(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/search_test.go:252:func TestResourceExecuteSearchStopsOnCancellation(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/search_test.go:279:func TestResourceExecuteSearchReturnsSenderError(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/search_test.go:298:func TestResourceExecuteSearchHandlesNilResponseHeader(t *testing.T) {
upstream-prometheus/pkg/promlib/resource/search_test.go:333:func TestResourceExecuteSearchLogsNonEOFReadError(t *testing.T) {
  1293	func TestParseNumericFormatError(t *testing.T) {
  1294		_, span := tracer.Start(context.Background(), "operation")
  1295		defer span.End()
  1296	
  1297		timeRange := backend.TimeRange{
  1298			From: now,
  1299			To:   now.Add(12 * time.Hour),
  1300		}
  1301	
  1302		t.Run("format as number 0 should be handled gracefully", func(t *testing.T) {
  1303			queryJson := `{
  1304				"expr": "up",
  1305				"format": 0,
  1306				"refId": "A"
  1307			}`
  1308	
  1309			q := backend.DataQuery{
  1310				JSON:      []byte(queryJson),
  1311				TimeRange: timeRange,
  1312				RefID:     "A",
  1313			}
  1314	
  1315			res, err := models.Parse(context.Background(), log.New(), span, q, "15s", intervalCalculator, false)
  1316			require.NoError(t, err)
  1317			require.NotNil(t, res)
  1318			require.Equal(t, "up", res.Expr)
  1319		})
  1320	
  1321		t.Run("format as number 1 should default to time_series", func(t *testing.T) {
  1322			queryJson := `{
  1323				"expr": "up",
  1324				"format": 1,
  1325				"refId": "A"
  1326			}`
  1327	
  1328			q := backend.DataQuery{
  1329				JSON:      []byte(queryJson),
  1330				TimeRange: timeRange,
  1331				RefID:     "A",
  1332			}
  1333	
  1334			res, err := models.Parse(context.Background(), log.New(), span, q, "15s", intervalCalculator, false)
  1335			require.NoError(t, err)
  1336			require.NotNil(t, res)
  1337		})
  1338	
  1339		t.Run("format as number 2 should map to table", func(t *testing.T) {
  1340			queryJson := `{
  1341				"expr": "up",
  1342				"format": 2,
  1343				"refId": "A"
  1344			}`
  1345	
  1346			q := backend.DataQuery{
  1347				JSON:      []byte(queryJson),
  1348				TimeRange: timeRange,
  1349				RefID:     "A",
  1350			}
  1351	
  1352			res, err := models.Parse(context.Background(), log.New(), span, q, "15s", intervalCalculator, false)
  1353			require.NoError(t, err)
  1354			require.NotNil(t, res)
  1355		})
  1356	
  1357		t.Run("format as number 3 should map to heatmap", func(t *testing.T) {
  1358			queryJson := `{
  1359				"expr": "up",
  1360				"format": 3,
  1361				"refId": "A"
  1362			}`
  1363	
  1364			q := backend.DataQuery{
  1365				JSON:      []byte(queryJson),
  1366				TimeRange: timeRange,
  1367				RefID:     "A",
  1368			}
  1369	
  1370			res, err := models.Parse(context.Background(), log.New(), span, q, "15s", intervalCalculator, false)
  1371			require.NoError(t, err)
  1372			require.NotNil(t, res)
  1373		})
  1374	
  1375		t.Run("format as unknown number should default to time_series", func(t *testing.T) {
  1376			queryJson := `{
  1377				"expr": "up",
  1378				"format": 999,
  1379				"refId": "A"
  1380			}`
  1381	
  1382			q := backend.DataQuery{
  1383				JSON:      []byte(queryJson),
  1384				TimeRange: timeRange,
  1385				RefID:     "A",
  1386			}
  1387	
  1388			res, err := models.Parse(context.Background(), log.New(), span, q, "15s", intervalCalculator, false)
  1389			require.NoError(t, err)
  1390			require.NotNil(t, res)
  1391		})
  1392	
  1393		t.Run("format as float should be handled gracefully", func(t *testing.T) {
  1394			queryJson := `{
  1395				"expr": "up",
  1396				"format": 1.5,
  1397				"refId": "A"
  1398			}`
  1399	
  1400			q := backend.DataQuery{
  1401				JSON:      []byte(queryJson),
  1402				TimeRange: timeRange,
  1403				RefID:     "A",
  1404			}
  1405	
  1406			res, err := models.Parse(context.Background(), log.New(), span, q, "15s", intervalCalculator, false)
  1407			require.NoError(t, err)
  1408			require.NotNil(t, res)
  1409		})
  1410	}
  1411	
  1412	func TestPromQueryFormatUnmarshalJSON(t *testing.T) {
  1413		t.Run("valid string format values", func(t *testing.T) {
  1414			tests := []struct {
  1415				name     string
5://! `id`, `uid` and `version`, with sorted keys, as stored at `api_version` ("v0alpha1" for /api/dashboards/db
7://! columns do not hold (k8s uid, resourceVersion, annotations, managedFields). Legacy reads return the stored spec;
14:use crate::util::{local_rfc3339, rfc3339, slugify, valid_uid};
22:    r.get("/api/dashboards/uid/:uid", Access::SignedIn, get_dashboard);
23:    r.delete("/api/dashboards/uid/:uid", Access::SignedIn, delete_dashboard);
25:    r.get("/api/dashboards/uid/:uid/versions", Access::SignedIn, get_versions);
26:    r.get("/api/dashboards/uid/:uid/versions/:id", Access::SignedIn, get_version);
27:    r.post("/api/dashboards/uid/:uid/restore", Access::SignedIn, restore_version);
30:    r.get("/api/dashboards/ids/:ids", Access::SignedIn, get_uids_by_ids);
45:/// org role decides (Admin: everything, Editor: everything but permissions, Viewer: read).
73:/// Login of a user id for createdBy/updatedBy ("Anonymous" for 0 or unknown; a service account's name).
74:pub fn login_of(app: &App, user_id: i64) -> String {
75:    if user_id <= 0 {
78:    match app.store.user_by_id(user_id) {
90:/// `<sub>/d/<uid>/<slug>`.
91:pub fn dashboard_url(app: &App, uid: &str, slug: &str) -> String {
92:    format!("{}/d/{}/{}", app.cfg.app_sub_url, uid, slug)
95:/// `<sub>/dashboards/f/<uid>/<slug>`.
96:pub fn folder_url(app: &App, uid: &str, title: &str) -> String {
97:    format!("{}/dashboards/f/{}/{}", app.cfg.app_sub_url, uid, slugify(title))
134:fn storage_version(v: &str) -> &'static str {
143:    org_id: i64,
144:    uid: String,
145:    /// The spec as written (without id, uid, version), keys sorted.
147:    folder_uid: String,
158:    internal_id: i64,
165:    let ident = k8s::identity_of(c);
171:    let (id, version, created, created_by, k8s_uid, mut managed, created_ident) = match old {
175:            let cident = m.path("annotations").and_then(|a| a.get(k8s::ANNO_CREATED_BY)).and_then(Json::as_str).unwrap_or("").to_string();
176:            (o.id, o.version + 1, o.created, o.created_by, m.str_of("uid").to_string(), managed, cident)
179:            let id = if w.internal_id > 0 { w.internal_id } else { k8s::snowflake_id() };
180:            (id, 1, now, c.user.user_id, k8s::uuid_v4(), Json::arr(), ident.clone())
183:    annos.push((k8s::ANNO_CREATED_BY.to_string(), Json::from(created_ident.as_str())));
184:    annos.push((k8s::ANNO_UPDATED_BY.to_string(), Json::from(ident.as_str())));
188:    if !w.folder_uid.is_empty() || w.folder_anno {
189:        annos.push((k8s::ANNO_FOLDER.to_string(), Json::from(w.folder_uid.as_str())));
200:    let meta = Json::obj().with("uid", k8s_uid).with("resourceVersion", k8s::next_rv()).with("annotations", annotations).with("managedFields", managed);
202:        id,
203:        uid: w.uid.clone(),
204:        org_id: w.org_id,
209:        folder_uid: w.folder_uid.clone(),
213:        updated_by: c.user.user_id,
214:        gnet_id: w.spec.get("gnetId").and_then(Json::as_i64).unwrap_or(0),
215:        plugin_id: String::new(),
223:        Err(StoreError::Conflict(_)) => Err(ApiError::new(400, "A dashboard with the same uid already exists")),
228:/// Checks that a folder uid names an existing folder ("" and "general" are the root).
229:fn check_folder(c: &Ctx<'_>, uid: &str) -> Result<String, ApiError> {
230:    if uid.is_empty() || uid == "general" {
233:    match c.app.store.folder_by_uid(c.user.org_id, uid) {
234:        Ok(_) => Ok(uid.to_string()),
263:    let d = match c.app.store.dashboard_by_uid(c.user.org_id, c.param("uid")) {
269:    set_sorted(&mut dash, "id", Json::Int(d.id));
270:    set_sorted(&mut dash, "uid", Json::from(d.uid.as_str()));
283:        .with("url", dashboard_url(c.app, &d.uid, &d.slug))
293:    let (mut fid, mut fuid, mut ftitle, mut furl) = (0i64, String::new(), "General".to_string(), String::new());
294:    if !d.folder_uid.is_empty() {
295:        match c.app.store.folder_by_uid(c.user.org_id, &d.folder_uid) {
297:                furl = folder_url(c.app, &f.uid, &f.title);
298:                fid = f.id;
299:                fuid = f.uid;
306:    meta.set("folderId", fid);
307:    meta.set("folderUid", fuid);
323:    let d = match c.app.store.dashboard_by_uid(c.user.org_id, c.param("uid")) {
329:        return Err(ApiError::forbidden());
331:    c.app.store.dashboard_delete(c.user.org_id, &d.uid)?;
rg: crates/store-sqlite/src/dashboards.rs: No such file or directory (os error 2)
rg: crates/store-sqlite/src/folders.rs: No such file or directory (os error 2)
rg: crates/store-sqlite/src/datasources.rs: No such file or directory (os error 2)
rg: crates/store-sqlite/src/users.rs: No such file or directory (os error 2)

hook: PostToolUse
hook: PostToolUse Completed
codex
Several sampled tests keep the Go assertions. I found two clear gaps: the Rust “new resource” test only checks a URL, and the omitted lenient-input tests include parsing checks, although the omission note calls them instrumentation tests. I found no `bench1`, `prom1`, or `gsshcd` branch in the server source checked so far.

hook: PreToolUse
hook: PreToolUse Completed
hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "nl -ba crates/grafana/src/api/boot.rs | sed -n '1,180p'; nl -ba crates/grafana/src/api/boot.rs | sed -n '420,458p'; nl -ba crates/grafana/src/api/ofrep.rs | sed -n '1,160p'; nl -ba crates/grafana/src/api/stubs.rs | sed -n '104,145p'; rg -n -i --glob '*.rs' --glob '"'!**/captured/**'"' 'fixture|hard.?code|capture|recorded|recording|seed|special.case' crates | cut -c 1-240" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
     1	//! The index page (`public/views/index.html` with `window.grafanaBootData`), `/bootdata`, `/api/frontend/settings`,
     2	//! `/api/frontend/assets`, `/api/access-control/user/actions`, and the frontend page routes (agent A).
     3	//! Upstream: pkg/api/index.go (setIndexViewData), pkg/api/bootdata.go (getFrontendSettings, getFSDataSources),
     4	//! pkg/api/frontendsettings/frontendsettings.go (GetBaseFrontendSettings), pkg/services/navtree/navtreeimpl,
     5	//! pkg/api/api.go (page routes).
     6	//!
     7	//! Cost: the static part of the settings (everything but data sources, the default data source, allowOrgCreate,
     8	//! the license URL, the namespace and the hidden version) is serialized once into text segments; a request writes
     9	//! the segments and the few dynamic values. The permissions map is prebuilt per basic role (`access`). The nav
    10	//! tree is small and built per request.
    11	use crate::access;
    12	use crate::app::App;
    13	use crate::ctx::{Access, Ctx, Role, Routes, SignedInUser};
    14	use crate::error::ApiError;
    15	use crate::settings::Settings;
    16	use http::Response;
    17	use json::Json;
    18	use std::sync::OnceLock;
    19	
    20	pub fn routes(r: &mut Routes) {
    21	    // Page routes of upstream pkg/api/api.go that render the index. Fine-grained `authorize(...)` page checks
    22	    // redirect home when they fail (accesscontrol deny for a page).
    23	    r.get("/", Access::SignedIn, index);
    24	    r.get("/invite/:code", Access::Optional, index_public);
    25	    r.get("/profile", Access::NoAnonymous, index);
    26	    r.get("/profile/password", Access::NoAnonymous, index);
    27	    r.get("/profile/notifications", Access::NoAnonymous, index);
    28	    r.get("/org", Access::SignedIn, page_org_prefs);
    29	    r.get("/org/new", Access::GrafanaAdmin, index);
    30	    r.get("/org/users", Access::SignedIn, page_org_users);
    31	    r.get("/org/users/new", Access::Role(Role::Admin), index);
    32	    r.get("/org/users/invite", Access::SignedIn, page_org_users);
    33	    r.get("/org/teams", Access::SignedIn, page_teams);
    34	    r.get("/org/teams/*", Access::SignedIn, page_teams);
    35	    r.get("/org/serviceaccounts", Access::SignedIn, page_service_accounts);
    36	    r.get("/org/serviceaccounts/*", Access::SignedIn, page_service_accounts);
    37	    r.get("/datasources", Access::SignedIn, page_ds_config);
    38	    r.get("/datasources/new", Access::SignedIn, page_ds_new);
    39	    r.get("/datasources/edit/*", Access::SignedIn, page_ds_edit);
    40	    r.get("/datasources/correlations", Access::SignedIn, page_explore);
    41	    r.get("/connections", Access::SignedIn, index);
    42	    r.get("/connections/*", Access::SignedIn, index);
    43	    r.get("/connections/datasources", Access::SignedIn, page_ds_config);
    44	    r.get("/connections/datasources/new", Access::SignedIn, page_ds_new);
    45	    r.get("/connections/datasources/edit/*", Access::SignedIn, page_ds_edit);
    46	    r.get("/connections/add-new-connection", Access::SignedIn, page_ds_config);
    47	    r.get("/admin", Access::Role(Role::Admin), index);
    48	    r.get("/admin/*", Access::SignedIn, page_admin);
    49	    r.get("/plugins", Access::SignedIn, page_plugins);
    50	    r.get("/plugins/*", Access::SignedIn, page_plugins);
    51	    r.get("/a/:id", Access::SignedIn, index);
    52	    r.get("/a/:id/*", Access::SignedIn, index);
    53	    r.get("/d/:uid", Access::SignedIn, index);
    54	    r.get("/d/:uid/:slug", Access::SignedIn, index);
    55	    r.get("/d-solo/:uid", Access::SignedIn, index);
    56	    r.get("/d-solo/:uid/:slug", Access::SignedIn, index);
    57	    r.get("/dashboard/new", Access::SignedIn, index);
    58	    r.get("/dashboard/import", Access::SignedIn, index);
    59	    r.get("/dashboard/*", Access::SignedIn, index);
    60	    r.get("/dashboard-solo/*", Access::SignedIn, index);
    61	    r.get("/import/dashboard", Access::SignedIn, index);
    62	    r.get("/dashboards", Access::SignedIn, index);
    63	    r.get("/dashboards/*", Access::SignedIn, index);
    64	    r.get("/explore", Access::SignedIn, page_explore);
    65	    r.get("/drilldown", Access::SignedIn, page_explore);
    66	    r.get("/playlists", Access::SignedIn, index);
    67	    r.get("/playlists/*", Access::SignedIn, index);
    68	    r.get("/alerting", Access::SignedIn, index);
    69	    r.get("/alerting/*", Access::SignedIn, index);
    70	    r.get("/library-panels", Access::SignedIn, index);
    71	    r.get("/bookmarks", Access::SignedIn, index);
    72	    r.get("/styleguide", Access::SignedIn, index);
    73	    r.get("/support-bundles", Access::SignedIn, index);
    74	    r.get("/support-bundles/*", Access::SignedIn, index);
    75	    r.get("/verify", Access::Optional, index_public);
    76	    r.get("/signup", Access::Optional, index_public);
    77	    r.get("/user/password/send-reset-email", Access::Optional, index_public);
    78	    r.get("/user/password/reset", Access::Optional, index_public);
    79	
    80	    r.get("/bootdata", Access::Optional, get_bootdata);
    81	    r.get("/api/frontend/settings", Access::Optional, get_frontend_settings);
    82	    r.get("/api/frontend/assets", Access::Optional, get_frontend_assets);
    83	    r.get("/api/access-control/user/actions", Access::SignedIn, get_user_actions);
    84	}
    85	
    86	// ---------------------------------------------------------------- page checks (accesscontrol authorize on pages)
    87	
    88	fn page_if(c: &mut Ctx<'_>, ok: bool) -> Result<Response, ApiError> {
    89	    if ok { index(c) } else { Ok(Response::redirect(302, &format!("{}/", c.app.cfg.app_sub_url))) }
    90	}
    91	fn page_org_prefs(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    92	    let u = &c.user;
    93	    let ok = (access::has(u, "orgs:read") && access::has(u, "orgs:write")) || (access::has(u, "orgs.preferences:read") && access::has(u, "orgs.preferences:write"));
    94	    page_if(c, ok)
    95	}
    96	fn page_org_users(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    97	    let ok = access::has(&c.user, "org.users:read");
    98	    page_if(c, ok)
    99	}
   100	fn page_teams(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
   101	    let ok = teams_access(&c.user);
   102	    page_if(c, ok)
   103	}
   104	fn page_service_accounts(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
   105	    let ok = access::has(&c.user, "serviceaccounts:read");
   106	    page_if(c, ok)
   107	}
   108	fn page_ds_config(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
   109	    let ok = ds_config_access(&c.user);
   110	    page_if(c, ok)
   111	}
   112	fn page_ds_new(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
   113	    let ok = access::has(&c.user, "datasources:read") && access::has(&c.user, "datasources:create");
   114	    page_if(c, ok)
   115	}
   116	fn page_ds_edit(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
   117	    let ok = access::has(&c.user, "datasources:read") && access::has(&c.user, "datasources:write");
   118	    page_if(c, ok)
   119	}
   120	fn page_explore(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
   121	    let ok = access::has(&c.user, "datasources:explore");
   122	    page_if(c, ok)
   123	}
   124	fn page_plugins(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
   125	    let ok = can_admin_plugins(c.app, &c.user);
   126	    page_if(c, ok)
   127	}
   128	fn page_admin(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
   129	    let ok = c.user.has_role(Role::Admin) || c.user.is_grafana_admin || access::has(&c.user, "org.users:read");
   130	    page_if(c, ok)
   131	}
   132	/// Pages without an auth requirement (invite, sign up, password reset).
   133	fn index_public(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
   134	    index(c)
   135	}
   136	
   137	fn teams_access(u: &SignedInUser) -> bool {
   138	    access::has(u, "teams:create") || (access::has(u, "teams:read") && (access::has(u, "teams:write") || access::has(u, "teams.permissions:write")))
   139	}
   140	fn ds_config_access(u: &SignedInUser) -> bool {
   141	    access::has(u, "datasources:create") || (access::has(u, "datasources:read") && (access::has(u, "datasources:delete") || access::has(u, "datasources:write")))
   142	}
   143	fn can_admin_plugins(app: &App, u: &SignedInUser) -> bool {
   144	    (app.cfg.get_bool("plugins", "plugin_admin_enabled") && u.is_grafana_admin) || access::has_any(u, &["plugins:write", "plugins:install"])
   145	}
   146	
   147	// ---------------------------------------------------------------- assets
   148	
   149	/// `assets` of the boot data from `public/build/assets-manifest.json` (pkg/api/webassets). Loaded once by `App::new`.
   150	pub fn load_assets(app_home: &std::path::Path) -> Json {
   151	    let text = std::fs::read_to_string(app_home.join("public/build/assets-manifest.json")).unwrap_or_default();
   152	    let m = Json::parse(&text).unwrap_or(Json::Null);
   153	    let integrity = |src: &str| -> String {
   154	        if let Some(o) = m.as_obj() {
   155	            for (_, v) in o.iter() {
   156	                if v.str_of("src") == src {
   157	                    return v.str_of("integrity").to_string();
   158	                }
   159	            }
   160	        }
   161	        String::new()
   162	    };
   163	    let files = |path: &str| -> Json {
   164	        let mut a = Json::arr();
   165	        if let Some(list) = m.path(path).and_then(|v| v.as_arr()) {
   166	            for f in list.iter() {
   167	                let src = f.as_str().unwrap_or("");
   168	                a.push(Json::obj().with("filePath", src).with("integrity", integrity(src)));
   169	            }
   170	        }
   171	        a
   172	    };
   173	    let first = |path: &str| -> String { m.path(path).and_then(|v| v.at(0)).and_then(|v| v.as_str()).unwrap_or("").to_string() };
   174	    Json::obj()
   175	        .with("jsFiles", files("entrypoints.app.assets.js"))
   176	        .with("cssFiles", files("entrypoints.app.assets.css"))
   177	        .with("dark", first("entrypoints.dark.assets.css"))
   178	        .with("light", first("entrypoints.light.assets.css"))
   179	}
   180	
   420	        .with("cloudMigrationIsTarget", false)
   421	        .with("cloudMigrationPollIntervalMs", 2000)
   422	        .with("dateFormats", date_formats)
   423	        .with("namespace", marker(4))
   424	        .with("sqlConnectionLimits", Json::obj().with("maxOpenConns", 100).with("maxIdleConns", 100).with("connMaxLifetime", 14400))
   425	        .with("localFileSystemAvailable", true)
   426	        .with("provisioningEnabled", true)
   427	        .with("listScopesEndpoint", "")
   428	        .with("listDashboardScopesEndpoint", "")
   429	        .with("openFeatureContext", Json::obj().with("grafana_version", cfg.build_version.as_str()).with("namespace", "default"))
   430	}
   431	
   432	/// Preinstalled plugins of `[plugins] preinstall` (Grafana builds this list from a map; the order is the recording's).
   433	const PREINSTALLED: &str = r#"[{"id":"influxdb","version":""},{"id":"zipkin","version":""},{"id":"loki","version":""},{"id":"mssql","version":""},{"id":"mysql","version":""},{"id":"cloudwatch","version":""},{"id":"grafana-postgresql-datasource","version":""},{"id":"tempo","version":""},{"id":"grafana-pyroscope-app","version":""},{"id":"opentsdb","version":""},{"id":"jaeger","version":""},{"id":"grafana-pyroscope-datasource","version":""},{"id":"grafana-metricsdrilldown-app","version":""},{"id":"grafana-advisor-app","version":""},{"id":"grafana-lokiexplore-app","version":""},{"id":"prometheus","version":""},{"id":"stackdriver","version":""},{"id":"grafana-exploretraces-app","version":""},{"id":"elasticsearch","version":""}]"#;
   434	
   435	fn settings_tpl(app: &App) -> &'static SettingsTpl {
   436	    static T: OnceLock<SettingsTpl> = OnceLock::new();
   437	    T.get_or_init(|| {
   438	        let mut text = build_static_settings(app).stringify();
   439	        // static plugin sections go in as text
   440	        text = text.replace(&json::quote("\u{1}panels"), &app.plugins.panels_json);
   441	        text = text.replace(&json::quote("\u{1}apps"), &app.plugins.apps_json);
   442	        let quoted: Vec<String> = MARKERS.iter().map(|m| json::quote(m)).collect();
   443	        let mut parts = Vec::new();
   444	        let mut rest = text.as_str();
   445	        loop {
   446	            let mut best: Option<(usize, usize)> = None;
   447	            for (i, q) in quoted.iter().enumerate() {
   448	                if let Some(p) = rest.find(q.as_str()) {
   449	                    if best.map_or(true, |b| p < b.0) {
   450	                        best = Some((p, i));
   451	                    }
   452	                }
   453	            }
   454	            match best {
   455	                Some((p, i)) => {
   456	                    parts.push(Part::Text(rest[..p].to_string()));
   457	                    parts.push(match i {
   458	                        0 => Part::DefaultDs,
     1	//! OpenFeature remote evaluation (OFREP) with Grafana's static provider (upstream pkg/services/ofrep/static.go,
     2	//! pkg/services/featuremgmt/static_evaluator.go). The flags are Grafana 13.3.1's standard flags with their default
     3	//! values (`captured/ofrep-flags.json`, from the recording), then the `[feature_toggles]` settings (`enable = a,b` and
     4	//! `<flag> = <value>`), as upstream `buildStaticFlagsMapFromCfg`. `dashboardNewLayouts` is always false: grafana-rs
     5	//! serves only the v1 dashboard API. Both answers are built once.
     6	use crate::ctx::{Access, Ctx, Routes};
     7	use crate::error::ApiError;
     8	use crate::settings::Settings;
     9	use http::Response;
    10	use json::Json;
    11	use std::sync::OnceLock;
    12	
    13	pub fn routes(r: &mut Routes) {
    14	    // No auth middleware upstream: the static provider answers everyone the same.
    15	    r.post("/ofrep/v1/evaluate/flags", Access::Public, all_flags);
    16	    r.post("/ofrep/v1/evaluate/flags/:flagKey", Access::Public, one_flag);
    17	    // Deprecated path that upstream keeps for older clients.
    18	    r.post("/apis/features.grafana.app/v0alpha1/namespaces/:namespace/ofrep/v1/evaluate/flags", Access::Public, all_flags);
    19	    r.post("/apis/features.grafana.app/v0alpha1/namespaces/:namespace/ofrep/v1/evaluate/flags/:flagKey", Access::Public, one_flag);
    20	}
    21	
    22	const REASON: &str = "static provider evaluation result";
    23	
    24	struct Flags {
    25	    all: String,
    26	    /// (key, single-flag body)
    27	    one: Vec<(String, String)>,
    28	    /// Keys of the flags whose value is `true` (boot data `featureToggles`).
    29	    enabled: Vec<String>,
    30	}
    31	
    32	/// The flags that evaluate to `true`, in flag order (boot data `settings.featureToggles`).
    33	pub fn enabled_flags(cfg: &Settings) -> &'static [String] {
    34	    &flags(cfg).enabled
    35	}
    36	
    37	fn flags(cfg: &Settings) -> &'static Flags {
    38	    static F: OnceLock<Flags> = OnceLock::new();
    39	    F.get_or_init(|| build(cfg))
    40	}
    41	
    42	/// Grafana's `setting.ParseFlag`: int, then float, then JSON object, then bool, else the string.
    43	fn parse_flag(v: &str) -> Json {
    44	    if let Ok(i) = v.parse::<i64>() {
    45	        return Json::Int(i);
    46	    }
    47	    if let Ok(f) = v.parse::<f64>() {
    48	        if f.is_finite() {
    49	            return Json::Float(f);
    50	        }
    51	    }
    52	    if v.starts_with('{') {
    53	        if let Ok(j) = Json::parse(v) {
    54	            if let Json::Obj(_) = j {
    55	                return j;
    56	            }
    57	        }
    58	    }
    59	    match v {
    60	        "1" | "t" | "T" | "TRUE" | "true" | "True" => Json::Bool(true),
    61	        "0" | "f" | "F" | "FALSE" | "false" | "False" => Json::Bool(false),
    62	        _ => Json::Str(v.to_string()),
    63	    }
    64	}
    65	
    66	/// Grafana's `util.SplitString`: a JSON list, or names separated by commas or spaces.
    67	fn split_list(s: &str) -> Vec<String> {
    68	    let t = s.trim();
    69	    let mut out = Vec::new();
    70	    if t.starts_with('[') {
    71	        if let Ok(Json::Arr(a)) = Json::parse(t) {
    72	            for v in a.iter() {
    73	                if let Some(x) = v.as_str() {
    74	                    out.push(x.to_string());
    75	                }
    76	            }
    77	        }
    78	        return out;
    79	    }
    80	    for part in t.split(|c: char| c == ',' || c.is_whitespace()) {
    81	        let p = part.trim_matches('"');
    82	        if !p.is_empty() {
    83	            out.push(p.to_string());
    84	        }
    85	    }
    86	    out
    87	}
    88	
    89	fn set_flag(list: &mut Vec<(String, Json)>, key: &str, value: Json) {
    90	    for f in list.iter_mut() {
    91	        if f.0 == key {
    92	            f.1 = value;
    93	            return;
    94	        }
    95	    }
    96	    list.push((key.to_string(), value));
    97	}
    98	
    99	fn build(cfg: &Settings) -> Flags {
   100	    let data = Json::parse(include_str!("captured/ofrep-flags.json")).unwrap_or(Json::Null);
   101	    let mut list: Vec<(String, Json)> = Vec::new();
   102	    if let Some(fs) = data.get("flags").and_then(|v| v.as_arr()) {
   103	        for f in fs.iter() {
   104	            list.push((f.str_of("key").to_string(), f.get("value").cloned().unwrap_or(Json::Bool(false))));
   105	        }
   106	    }
   107	    for name in split_list(cfg.get("feature_toggles", "enable")) {
   108	        set_flag(&mut list, &name, Json::Bool(true));
   109	    }
   110	    for (k, v) in cfg.section("feature_toggles") {
   111	        if k != "enable" {
   112	            set_flag(&mut list, &k, parse_flag(&v));
   113	        }
   114	    }
   115	    set_flag(&mut list, "dashboardNewLayouts", Json::Bool(false));
   116	
   117	    let mut all = Json::arr();
   118	    let mut one = Vec::with_capacity(list.len());
   119	    let mut enabled = Vec::new();
   120	    for (k, v) in list {
   121	        if v == Json::Bool(true) {
   122	            enabled.push(k.clone());
   123	        }
   124	        let f = Json::obj().with("key", k.as_str()).with("value", v).with("reason", REASON).with("variant", "default");
   125	        let mut s = f.stringify();
   126	        s.push('\n');
   127	        one.push((k, s));
   128	        all.push(f);
   129	    }
   130	    let mut all_s = Json::obj().with("flags", all).stringify();
   131	    all_s.push('\n');
   132	    Flags { all: all_s, one, enabled }
   133	}
   134	
   135	fn all_flags(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
   136	    Ok(Response::json_text(200, flags(&c.app.cfg).all.clone()))
   137	}
   138	
   139	/// An unknown flag: upstream `http.Error(w, "failed to evaluate flag", 500)`.
   140	fn one_flag(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
   141	    let key = c.param("flagKey");
   142	    for (k, body) in flags(&c.app.cfg).one.iter() {
   143	        if k == key {
   144	            return Ok(Response::json_text(200, body.clone()));
   145	        }
   146	    }
   147	    Ok(Response::text(500, "text/plain; charset=utf-8", &b"failed to evaluate flag\n"[..]))
   148	}
   104	    json_static(
   105	        200,
   106	        "{\n  \"kind\": \"CheckList\",\n  \"apiVersion\": \"advisor.grafana.app/v0alpha1\",\n  \"metadata\": {\n    \"resourceVersion\": \"1\"\n  },\n  \"items\": []\n}",
   107	    )
   108	}
   109	
   110	/// The check types of Grafana 13.3.1, captured from a real instance (`captured/advisor-checktypes.json`). Their uid,
   111	/// resourceVersion and times are those of the capture.
   112	fn checktypes() -> &'static (String, Vec<(String, String)>) {
   113	    static CT: OnceLock<(String, Vec<(String, String)>)> = OnceLock::new();
   114	    CT.get_or_init(|| {
   115	        let list = Json::parse(include_str!("captured/advisor-checktypes.json")).unwrap_or(Json::Null);
   116	        let mut one = Vec::new();
   117	        if let Some(items) = list.get("items").and_then(|v| v.as_arr()) {
   118	            for it in items.iter() {
   119	                let name = it.get("metadata").map(|m| m.str_of("name")).unwrap_or("");
   120	                one.push((name.to_string(), it.stringify_pretty("  ")));
   121	            }
   122	        }
   123	        (list.stringify_pretty("  "), one)
   124	    })
   125	}
   126	
   127	fn advisor_checktypes(_c: &mut Ctx<'_>) -> Result<Response, ApiError> {
   128	    Ok(Response::json_text(200, checktypes().0.clone()))
   129	}
   130	
   131	fn advisor_checktype(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
   132	    let name = c.param("name");
   133	    for (n, body) in checktypes().1.iter() {
   134	        if n == name {
   135	            return Ok(Response::json_text(200, body.clone()));
   136	        }
   137	    }
   138	    let status = Json::obj()
   139	        .with("kind", "Status")
   140	        .with("apiVersion", "v1")
   141	        .with("metadata", Json::obj())
   142	        .with("status", "Failure")
   143	        .with("message", format!("checktypes.advisor.grafana.app \"{name}\" not found"))
   144	        .with("reason", "NotFound")
   145	        .with("details", Json::obj().with("name", name).with("group", "advisor.grafana.app").with("kind", "checktypes"))
crates/server/src/main.rs:66:    if let Err(e) = grafana::seed::seed(&*store, &cfg) {
crates/server/src/main.rs:67:        eprintln!("grafana-rs: cannot seed database: {e}");
crates/http/src/router.rs:3://! - `:name`: one segment, captured (percent-decoded)
crates/http/src/router.rs:4://! - `*` or `*name`: the rest of the path (may be empty), captured under "*" or `name`; must be last
crates/http/src/router.rs:29:    /// The route value and the captured params.
crates/http/src/router.rs:109:                Found::Ok(&e.value, capture(&e.segs, parts))
crates/http/src/router.rs:138:fn capture(segs: &[Seg], parts: &[&str]) -> Vec<(String, String)> {
crates/grafana/src/access.rs:4://! The tables are the unscoped action sets Grafana 13.3.1 reports for each basic role (captured from a real
crates/grafana/src/access.rs:80:/// Org role None (and nobody): captured from Grafana 13.3.1.
crates/grafana/src/access.rs:84:/// Org role Viewer, captured from Grafana 13.3.1 (anonymous Viewer).
crates/grafana/src/access.rs:117:/// Org role Editor, captured from Grafana 13.3.1 (anonymous Editor).
crates/grafana/src/access.rs:196:/// Org role Admin, captured from Grafana 13.3.1 (anonymous Admin; equal to the ui-tour-v1 recording).
crates/grafana/src/access.rs:368:/// Added for a Grafana server admin (the api-flows-v1 recording of admin minus ADMIN).
crates/grafana/src/seed.rs:10:pub fn seed(store: &dyn Store, cfg: &Settings) -> SResult<()> {
crates/grafana/src/app.rs:85:                        // errutil body with "extra" (recorded in api-flows-v1: GET /api/user after logout).
crates/grafana/src/lib.rs:13:pub mod seed;
crates/grafana/src/api/ofrep.rs:3://! values (`captured/ofrep-flags.json`, from the recording), then the `[feature_toggles]` settings (`enable = a,b` and
crates/grafana/src/api/ofrep.rs:100:    let data = Json::parse(include_str!("captured/ofrep-flags.json")).unwrap_or(Json::Null);
crates/grafana/src/api/discovery.rs:2://! (APIResourceList). The groups, versions and resources are those Grafana 13.3.1 serves, captured from a real
crates/grafana/src/api/discovery.rs:3://! instance into `captured/discovery.json`. The bodies are built once, pretty-printed like the apiserver's answers.
crates/grafana/src/api/discovery.rs:28:        let data = Json::parse(include_str!("captured/discovery.json")).unwrap_or(Json::Null);
crates/grafana/src/api/stubs.rs:1://! Out-of-scope routes answered with an empty or disabled result of the recorded shape (alerting, live, advisor,
crates/grafana/src/api/stubs.rs:74:/// The metrics are not recorded here; the body is only checked to be `{"events": [...]}` JSON as Grafana binds it.
crates/grafana/src/api/stubs.rs:110:/// The check types of Grafana 13.3.1, captured from a real instance (`captured/advisor-checktypes.json`). Their uid,
crates/grafana/src/api/stubs.rs:111:/// resourceVersion and times are those of the capture.
crates/grafana/src/api/stubs.rs:115:        let list = Json::parse(include_str!("captured/advisor-checktypes.json")).unwrap_or(Json::Null);
crates/grafana/src/api/boot.rs:394:        .with("defaultAllowRecordingRulesTargetAlertsUiToggle", true)
crates/grafana/src/api/boot.rs:402:        .with("recordedQueries", Json::obj().with("enabled", cfg_bool(cfg, "recorded_queries", "enabled", true)))
crates/grafana/src/api/boot.rs:408:            Json::parse(r#"{"minInterval":"10s","stateHistory":{"backend":"annotations","prometheusMetricName":"GRAFANA_ALERTS"},"recordingRulesEnabled":true,"notificationHistoryEnabled":false,"alertStateH
crates/grafana/src/api/boot.rs:432:/// Preinstalled plugins of `[plugins] preinstall` (Grafana builds this list from a map; the order is the recording's).
crates/grafana/src/api/boot.rs:1006:    // Alerting (unified alerting enabled, navigation V2 as the recorded feature flags say)

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "python -c 'from pathlib import Path; import json
for a,b in [(Path(\"upstream-prometheus/pkg/promlib/converter/testdata\"),Path(\"tests-data/promlib/converter\")),(Path(\"upstream-prometheus/pkg/promlib/testdata\"),Path(\"tests-data/promlib/querydata\")),(Path(\"upstream-prometheus/pkg/promlib/querydata/exemplar/testdata\"),Path(\"tests-data/promlib/exemplar\"))]:
 files=[p for p in b.iterdir() if p.is_file()]; print(b,\"files\",len(files),\"different\",[p.name for p in files if (a/p.name).exists() and p.read_bytes()"'!=(a/p.name).read_bytes()],"missing",[p.name for p in files if not (a/p.name).exists()])
s=Path("tests-data/replay-ignore.json").read_text(); pos=0
for n,r in enumerate(json.loads(s)["rules"],1):
 pos=s.find("'"\\\"route\\\": \"+json.dumps(r[\"route\"]),pos); line=s[:pos].count(\"\\n\")+1; pos+=1; print(n,\"L\"+str(line),r[\"mode\"],r[\"path\"])'" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
tests-data/promlib/converter files 52 different [] missing []
tests-data/promlib/querydata files 18 different [] missing []
tests-data/promlib/exemplar files 2 different [] missing []
1 L4 map metadata.name
2 L10 map metadata.uid
3 L16 value metadata.resourceVersion
4 L22 value metadata.creationTimestamp
5 L28 value ['metadata', 'labels', 'grafana.app/deprecatedInternalID']
6 L38 value metadata.managedFields.*.time
7 L44 map metadata.uid
8 L50 value metadata.resourceVersion
9 L56 value metadata.creationTimestamp
10 L62 value ['metadata', 'labels', 'grafana.app/deprecatedInternalID']
11 L72 value metadata.managedFields.*.time
12 L78 value ['metadata', 'annotations', 'grafana.app/createdBy']
13 L88 value ['metadata', 'annotations', 'grafana.app/updatedBy']
14 L98 map result.uid
15 L104 value result.createdAt
16 L110 value metadata.uid
17 L116 value metadata.resourceVersion
18 L122 value metadata.creationTimestamp
19 L128 value metadata.managedFields.*.time
20 L134 value flags.*.key
21 L140 any flags.*.value
22 L146 any ['settings', 'featureToggles', 'dashboardNewLayouts']
23 L156 any ['featureToggles', 'dashboardNewLayouts']
24 L165 value liveEnabled
25 L171 value appUrl
26 L177 value pluginCatalogPreinstalledPlugins.*.id
27 L183 map uid
28 L189 value updatedAt
29 L195 value createdAt
30 L201 map *.uid
31 L207 value *.lastSeenAt
32 L213 value *.created
33 L219 map users.*.uid
34 L225 value users.*.lastSeenAt
35 L231 value users.*.created
36 L237 value hits.*.score
37 L243 value maxScore
38 L249 value queryCost
39 L255 value metadata.managedFields.*.manager
40 L261 value metadata.managedFields.*.manager
41 L267 value id
42 L273 value created
43 L279 value updated
44 L285 value id
45 L291 value created
46 L297 value updated
47 L303 value *.id
48 L309 value metadata.uid
49 L315 value metadata.resourceVersion
50 L321 value metadata.creationTimestamp
51 L327 value ['metadata', 'labels', 'grafana.app/deprecatedInternalID']
52 L337 value metadata.managedFields.*.time
53 L343 value ['metadata', 'annotations', 'grafana.app/createdBy']
54 L353 value ['metadata', 'annotations', 'grafana.app/updatedBy']
55 L363 value id
56 L369 map uid
57 L375 value dashboard.id
58 L381 value meta.folderId
59 L387 value meta.created
60 L393 value meta.updated
61 L399 value versions.*.dashboardId
62 L405 value versions.*.created
63 L411 value dashboardId
64 L417 value created
65 L423 value metadata.uid
66 L429 value metadata.resourceVersion
67 L435 value metadata.creationTimestamp
68 L441 value ['metadata', 'labels', 'grafana.app/deprecatedInternalID']
69 L451 value metadata.managedFields.*.time
70 L457 value ['metadata', 'annotations', 'grafana.app/createdBy']
71 L467 value ['metadata', 'annotations', 'grafana.app/updatedBy']
72 L477 value ['metadata', 'annotations', 'grafana.app/updatedTimestamp']
73 L487 value ['metadata', 'annotations', 'grafana.app/updatedTimestamp']
74 L497 value metadata.resourceVersion
75 L503 value items.*.metadata.uid
76 L509 value items.*.metadata.resourceVersion
77 L515 value items.*.metadata.creationTimestamp
78 L521 value ['items', '*', 'metadata', 'labels', 'grafana.app/deprecatedInternalID']
79 L533 value items.*.metadata.managedFields.*.time
80 L539 value ['items', '*', 'metadata', 'annotations', 'grafana.app/createdBy']
81 L551 value ['items', '*', 'metadata', 'annotations', 'grafana.app/updatedBy']
82 L563 value ['items', '*', 'metadata', 'annotations', 'grafana.app/updatedTimestamp']
83 L575 value *.id
84 L581 value *.folderId
85 L587 value *.dashboardId
86 L593 value *.created
87 L599 value *.updated
88 L605 value *.userUID
89 L611 value *.avatarUrl
90 L617 value result.queryHistory.*.createdAt

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "nl -ba upstream-prometheus/pkg/promlib/models/prometheus_interval_test.go | sed -n '1,150p'; nl -ba crates/grafana/tests/promlib_models.rs | sed -n '1,78p'; nl -ba upstream-prometheus/pkg/promlib/middleware/custom_query_params_test.go | sed -n '230,360p'; rg -n '"'^func Test|t.Run'"' upstream-prometheus/pkg/promlib/models/lenient_test.go; rg -n 'health|heuristic' tests-data/recordings/*.jsonl | cut -c 1-220" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
     1	package models
     2	
     3	import (
     4		"context"
     5		"testing"
     6		"time"
     7	
     8		"github.com/grafana/grafana-plugin-sdk-go/backend"
     9		"github.com/stretchr/testify/require"
    10		"go.opentelemetry.io/otel"
    11	
    12		"github.com/grafana/grafana-prometheus-datasource/pkg/promlib/intervalv2"
    13	)
    14	
    15	var (
    16		testNow                = time.Now()
    17		testIntervalCalculator = intervalv2.NewCalculator()
    18		testTracer             = otel.Tracer("test/interval")
    19	)
    20	
    21	func TestCalculatePrometheusInterval(t *testing.T) {
    22		_, span := testTracer.Start(context.Background(), "test")
    23		defer span.End()
    24	
    25		tests := []struct {
    26			name             string
    27			queryInterval    string
    28			dsScrapeInterval string
    29			intervalMs       int64
    30			intervalFactor   int64
    31			query            backend.DataQuery
    32			want             time.Duration
    33			wantErr          bool
    34		}{
    35			{
    36				name:             "use min step value when it's bigger than the calculated value",
    37				queryInterval:    "10m",
    38				dsScrapeInterval: "",
    39				intervalMs:       300000,
    40				intervalFactor:   1,
    41				query: backend.DataQuery{
    42					TimeRange: backend.TimeRange{
    43						From: testNow,
    44						To:   testNow.Add(48 * time.Hour),
    45					},
    46					Interval:      5 * time.Minute,
    47					MaxDataPoints: 761,
    48				},
    49				want:    10 * time.Minute,
    50				wantErr: false,
    51			},
    52			{
    53				name:             "use calculated value when min step value is smaller",
    54				queryInterval:    "2m",
    55				dsScrapeInterval: "",
    56				intervalMs:       300000,
    57				intervalFactor:   1,
    58				query: backend.DataQuery{
    59					TimeRange: backend.TimeRange{
    60						From: testNow,
    61						To:   testNow.Add(48 * time.Hour),
    62					},
    63					Interval:      5 * time.Minute,
    64					MaxDataPoints: 761,
    65				},
    66				want:    5 * time.Minute,
    67				wantErr: false,
    68			},
    69			{
    70				name:             "min step 2m with 900000 intervalMs",
    71				queryInterval:    "2m",
    72				dsScrapeInterval: "",
    73				intervalMs:       900000,
    74				intervalFactor:   1,
    75				query: backend.DataQuery{
    76					TimeRange: backend.TimeRange{
    77						From: testNow,
    78						To:   testNow.Add(48 * time.Hour),
    79					},
    80					Interval:      15 * time.Minute,
    81					MaxDataPoints: 175,
    82				},
    83				want:    15 * time.Minute,
    84				wantErr: false,
    85			},
    86			{
    87				name:             "with step parameter",
    88				queryInterval:    "",
    89				dsScrapeInterval: "15s",
    90				intervalMs:       0,
    91				intervalFactor:   1,
    92				query: backend.DataQuery{
    93					TimeRange: backend.TimeRange{
    94						From: testNow,
    95						To:   testNow.Add(12 * time.Hour),
    96					},
    97					Interval: 1 * time.Minute,
    98				},
    99				want:    30 * time.Second,
   100				wantErr: false,
   101			},
   102			{
   103				name:             "without step parameter",
   104				queryInterval:    "",
   105				dsScrapeInterval: "15s",
   106				intervalMs:       0,
   107				intervalFactor:   1,
   108				query: backend.DataQuery{
   109					TimeRange: backend.TimeRange{
   110						From: testNow,
   111						To:   testNow.Add(1 * time.Hour),
   112					},
   113					Interval: 1 * time.Minute,
   114				},
   115				want:    15 * time.Second,
   116				wantErr: false,
   117			},
   118			{
   119				name:             "with high intervalFactor",
   120				queryInterval:    "",
   121				dsScrapeInterval: "15s",
   122				intervalMs:       0,
   123				intervalFactor:   10,
   124				query: backend.DataQuery{
   125					TimeRange: backend.TimeRange{
   126						From: testNow,
   127						To:   testNow.Add(48 * time.Hour),
   128					},
   129					Interval: 1 * time.Minute,
   130				},
   131				want:    20 * time.Minute,
   132				wantErr: false,
   133			},
   134			{
   135				name:             "with low intervalFactor",
   136				queryInterval:    "",
   137				dsScrapeInterval: "15s",
   138				intervalMs:       0,
   139				intervalFactor:   1,
   140				query: backend.DataQuery{
   141					TimeRange: backend.TimeRange{
   142						From: testNow,
   143						To:   testNow.Add(48 * time.Hour),
   144					},
   145					Interval: 1 * time.Minute,
   146				},
   147				want:    2 * time.Minute,
   148				wantErr: false,
   149			},
   150			{
     1	//! Port of promlib `models/query_test.go` (TestParse, TestRateInterval, TestAlignTimeRange,
     2	//! TestParseNumericFormatError, TestPromQueryFormatUnmarshalJSON). The scope, ad hoc filter and group-by cases
     3	//! are listed in tests-data/not-ported.md.
     4	use grafana::promlib::models::{DataQuery, HOUR, MIN, MS, Query, SEC, align_time_range, parse, parse_format};
     5	use json::Json;
     6	
     7	fn now() -> i64 {
     8	    grafana::util::now_ms() * MS + 123_456 // not aligned to anything, like time.Now()
     9	}
    10	
    11	/// `queryContext(json, timeRange, queryInterval)`.
    12	fn query_context(json: &str, from: i64, to: i64, interval: i64) -> DataQuery {
    13	    DataQuery { ref_id: "A".to_string(), interval, from, to, json: Json::parse(json).expect("query json"), ..Default::default() }
    14	}
    15	
    16	fn parse_ok(q: &DataQuery, scrape: &str) -> Query {
    17	    parse(q, scrape, false).expect("parse")
    18	}
    19	
    20	// TestParse
    21	#[test]
    22	fn parse_parsing_query_from_unified_alerting() {
    23	    let n = now();
    24	    let q = query_context(r#"{"expr": "go_goroutines", "refId": "A", "exemplar": true}"#, n, n + 12 * HOUR, 0);
    25	    let res = parse(&q, "15s", true).expect("parse");
    26	    assert!(!res.exemplar_query);
    27	}
    28	
    29	#[test]
    30	fn parse_parsing_query_model_with_step() {
    31	    let n = now();
    32	    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "refId": "A"}"#, n, n + 12 * HOUR, MIN);
    33	    assert_eq!(parse_ok(&q, "15s").step, 30 * SEC);
    34	}
    35	
    36	#[test]
    37	fn parse_parsing_query_model_without_step_parameter() {
    38	    let n = now();
    39	    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + HOUR, MIN);
    40	    assert_eq!(parse_ok(&q, "15s").step, 15 * SEC);
    41	}
    42	
    43	#[test]
    44	fn parse_parsing_query_model_with_high_interval_factor() {
    45	    let n = now();
    46	    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "intervalFactor": 10, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    47	    assert_eq!(parse_ok(&q, "15s").step, 20 * MIN);
    48	}
    49	
    50	#[test]
    51	fn parse_parsing_query_model_with_low_interval_factor() {
    52	    let n = now();
    53	    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    54	    assert_eq!(parse_ok(&q, "15s").step, 2 * MIN);
    55	}
    56	
    57	#[test]
    58	fn parse_parsing_query_model_specified_scrape_interval_in_the_data_source() {
    59	    let n = now();
    60	    let q = query_context(r#"{"expr": "go_goroutines", "format": "time_series", "intervalFactor": 1, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    61	    assert_eq!(parse_ok(&q, "240s").step, 4 * MIN);
    62	}
    63	
    64	#[test]
    65	fn parse_parsing_query_model_with_interval_variable() {
    66	    let n = now();
    67	    let mut q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__interval]})", "format": "time_series", "intervalFactor": 1, "intervalMs": 60000, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    68	    q.max_data_points = 2110;
    69	    let res = parse_ok(&q, "15s");
    70	    assert_eq!(res.expr, "rate(ALERTS{job=\"test\" [1m]})");
    71	    assert_eq!(res.step, 60 * SEC);
    72	}
    73	
    74	#[test]
    75	fn parse_parsing_query_model_with_interval_ms_variable() {
    76	    let n = now();
    77	    let q = query_context(r#"{"expr": "rate(ALERTS{job=\"test\" [$__interval_ms]})", "format": "time_series", "intervalFactor": 1, "intervalMs": 60000, "refId": "A"}"#, n, n + 48 * HOUR, MIN);
    78	    assert_eq!(parse_ok(&q, "15s").expr, "rate(ALERTS{job=\"test\" [120000]})");
   230				require.Empty(t, req.URL.Query().Get(queryStatsKey))
   231			})
   232		}
   233	
   234		t.Run("Explicit query statistics setting should override a custom stats value", func(t *testing.T) {
   235			mw := CustomQueryParameters(backend.NewLoggerWith("logger", "test"), "stats=none&timeout=30s", 0, 0, true)
   236			rt := mw.CreateMiddleware(httpclient.Options{}, finalRoundTripper)
   237			req, err := http.NewRequest(http.MethodPost, "http://test.com/api/v1/query_range", nil)
   238			require.NoError(t, err)
   239	
   240			_, err = rt.RoundTrip(req)
   241			require.NoError(t, err)
   242			require.Equal(t, []string{queryStatsValue}, req.URL.Query()[queryStatsKey])
   243			require.Equal(t, "30s", req.URL.Query().Get("timeout"))
   244		})
   245	}
22:func TestParsePromOptions_LooselyTypedJSONData(t *testing.T) {
138:		t.Run(tc.name, func(t *testing.T) {
149:func TestParsePromOptions_PreexistingStrictFieldsStayStrict(t *testing.T) {
155:		t.Run(jsonData, func(t *testing.T) {
166:func TestParsePromOptions_LenientFieldsCannotFailTheDatasource(t *testing.T) {
179:			t.Run(key+"="+value, func(t *testing.T) {
215:func TestLenientTypes_LogOnlyWhenLenient(t *testing.T) {
294:		t.Run(tc.name, func(t *testing.T) {
342:func TestParsePromOptions_DroppedPointerIsLeftUnset(t *testing.T) {
364:		t.Run(tc.name, func(t *testing.T) {
384:func TestPointerPropertiesAreAccountedFor(t *testing.T) {
tests-data/recordings/api-flows-v1.jsonl:23:{"t":1794842297454,"method":"GET","url":"/api/datasources/uid/prom2/health","reqHeaders":{"cookie":"grafana_session_expiry=1794842892; grafana_session=c6283e98ee8ffa6d66b4a0b40
tests-data/recordings/api-flows-v1.jsonl:30:{"t":1794842297590,"method":"GET","url":"/api/datasources/uid/prom1/health","reqHeaders":{"cookie":"grafana_session_expiry=1794842892; grafana_session=c6283e98ee8ffa6d66b4a0b40
tests-data/recordings/api-flows-v1.jsonl:38:{"t":1794842297689,"method":"POST","url":"/api/ds/query?ds_type=prometheus","reqHeaders":{"content-type":"application/json","cookie":"grafana_session_expiry=1794842892; grafana
tests-data/recordings/ui-tour-v1.jsonl:39:{"t":1794841816228,"method":"POST","url":"/api/ds/query?ds_type=prometheus&requestId=SQR100","reqHeaders":{"content-type":"application/json","x-grafana-org-id":"1","x-datasource-
tests-data/recordings/ui-tour-v1.jsonl:87:{"t":1794841829956,"method":"GET","url":"/apis/advisor.grafana.app/v0alpha1/namespaces/default/checktypes/datasource","reqHeaders":{"x-grafana-org-id":"1"},"status":200,"resHeade
tests-data/recordings/ui-tour-v1.jsonl:103:{"t":1794841833111,"method":"GET","url":"/apis/advisor.grafana.app/v0alpha1/namespaces/default/checktypes/datasource","reqHeaders":{"x-grafana-org-id":"1"},"status":200,"resHead
tests-data/recordings/ui-tour-v1.jsonl:104:{"t":1794841833813,"method":"GET","url":"/api/gnet/plugins?includeDeprecated=true","reqHeaders":{"x-grafana-org-id":"1"},"status":200,"resHeaders":{"content-type":"application/j
tests-data/recordings/ui-tour-v1.jsonl:105:{"t":1794841833887,"method":"GET","url":"/api/gnet/plugins/prometheus","reqHeaders":{"x-grafana-org-id":"1"},"status":200,"resHeaders":{"content-type":"application/json; charset
tests-data/recordings/ui-tour-v1.jsonl:108:{"t":1794841833982,"method":"GET","url":"/api/plugins/prometheus/markdown/CHANGELOG","reqHeaders":{"x-grafana-org-id":"1"},"status":200,"resHeaders":{"content-type":"text/plain;
tests-data/recordings/ui-tour-v1.jsonl:123:{"t":1794841837897,"method":"POST","url":"/api/ds/query?ds_type=prometheus&requestId=SQR100","reqHeaders":{"content-type":"application/json","x-grafana-org-id":"1","x-datasource
tests-data/recordings/ui-tour.jsonl:33:{"t":1794841559032,"method":"POST","url":"/api/ds/query?ds_type=prometheus&requestId=SQR100","reqHeaders":{"content-type":"application/json","x-grafana-org-id":"1","x-datasource-uid
tests-data/recordings/ui-tour.jsonl:89:{"t":1794841572236,"method":"GET","url":"/apis/advisor.grafana.app/v0alpha1/namespaces/default/checktypes/datasource","reqHeaders":{"x-grafana-org-id":"1"},"status":200,"resHeaders"
tests-data/recordings/ui-tour.jsonl:107:{"t":1794841576004,"method":"GET","url":"/apis/advisor.grafana.app/v0alpha1/namespaces/default/checktypes/datasource","reqHeaders":{"x-grafana-org-id":"1"},"status":200,"resHeaders
tests-data/recordings/ui-tour.jsonl:109:{"t":1794841576710,"method":"GET","url":"/api/gnet/plugins/prometheus","reqHeaders":{"x-grafana-org-id":"1"},"status":200,"resHeaders":{"content-type":"application/json; charset=ut
tests-data/recordings/ui-tour.jsonl:112:{"t":1794841576733,"method":"GET","url":"/api/plugins/prometheus/markdown/CHANGELOG","reqHeaders":{"x-grafana-org-id":"1"},"status":200,"resHeaders":{"content-type":"text/plain; ch
tests-data/recordings/ui-tour.jsonl:114:{"t":1794841577605,"method":"GET","url":"/api/gnet/plugins?includeDeprecated=true","reqHeaders":{"x-grafana-org-id":"1"},"status":200,"resHeaders":{"content-type":"application/json
tests-data/recordings/ui-tour.jsonl:133:{"t":1794841589640,"method":"POST","url":"/api/ds/query?ds_type=prometheus&requestId=SQR100","reqHeaders":{"content-type":"application/json","x-grafana-org-id":"1","x-datasource-ui

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "node -e 'const fs=require(\"node:fs\");const s=fs.readFileSync(\"scripts/replay.mjs\",\"utf8\");const c=s.slice(s.indexOf(\"const idMap\"),s.indexOf(\"// ---- cookies\"))+s.slice(s.indexOf(\"const typeOf\"),s.indexOf(\"async function call\"));const {cmp}=new Function(c+\";return {cmp}\")();const check=(n,a,b,r=[],struct=false)=>console.log(n,cmp(a,b,r,[],struct));check(\"large integers\",JSON.parse('\\''{\"id\":9007199254740992}'\\''),JSON.parse('\\''{\"id\":9007199254740993}'\\''));check(\"integer key order\",JSON.parse('\\''{\"2\":\"b\",\"1\":\"a\"}'\\''),JSON.parse('\\''{\"1\":\"a\",\"2\":\"b\"}'\\''));check(\"HTML values\",{user:{isGrafanaAdmin:false},settings:{appUrl:\"http://a/\"}},{user:{isGrafanaAdmin:true},settings:{appUrl:\"http://evil/\"}},[],true);check(\"null vs missing\",{x:null},{});check(\"array order\",[1,2],[2,1]);check(\"OFREP wrong flags\",{flags:[{key:\"featureA\",value:true}]},{flags:[{key:\"wrongFeature\",value:\"wrong type\"}]},[{_path:[\"flags\",\"*\",\"key\"],mode:\"value\"},{_path:[\"flags\",\"*\",\"value\"],mode:\"any\"}]);check(\"array value length\",[1],[1,2],[{_path:[],mode:\"value\"}]);'" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
large integers null
integer key order null
HTML values null
null vs missing $: missing keys x
array order $.0: 2, want 1
OFREP wrong flags null
array value length null

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "python -c 'import json; from pathlib import Path
for fn in [\"ui-tour-v1.jsonl\",\"api-flows-v1.jsonl\"]:
 rows=[json.loads(x) for x in (Path(\"tests-data/recordings\")/fn).read_text().splitlines() if x]; print(fn,\"rows\",len(rows),\"upgrade\",sum(r[\"method\"]==\"UPGRADE\" for r in rows))
 for r in rows:
  if r[\"url\"].endswith(\"/health\") or \"heuristic\" in r[\"url\"]: print(r[\"method\"],r[\"url\"],r[\"status\"],r.get(\"resBody\"))'" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
ui-tour-v1.jsonl rows 190 upgrade 25
api-flows-v1.jsonl rows 77 upgrade 0
GET /api/datasources/uid/prom2/health 200 {"details":{"application":"Prometheus","features":{"rulerApiEnabled":false}},"message":"Successfully queried the Prometheus API.","status":"OK"}
GET /api/datasources/uid/prom1/health 200 {"details":{"application":"Prometheus","features":{"rulerApiEnabled":false}},"message":"Successfully queried the Prometheus API.","status":"OK"}

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "rg -n --glob '*.rs' --glob '"'!**/captured/**'"' '(1794[0-9]{4,}|[0-9]{10,}|\"[A-Za-z0-9_-]{6,}\"\\s*=>|uid\\s*==|\\.uid\\s*==|uid\\s*"'!=|'"\\.uid\\s*"'!=|'"\\.id\\s*==|port\\s*==)' crates/grafana/src crates/store-sqlite/src crates/server/src | cut -c 1-240; nl -ba crates/grafana/src/api/search.rs | sed -n '165,285p'; nl -ba crates/grafana/src/api/k8s.rs | sed -n '1,170p'; nl -ba crates/grafana/src/api/annotations.rs | sed -n '100,210p'" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
crates/store-sqlite/src/dashboard.rs:151:                if i < list.len() && list[i].id == id {
crates/grafana/src/promlib/querydata.rs:239:                "querier_wall_time" => ("Querier wall time", "ms"),
crates/grafana/src/promlib/querydata.rs:240:                "response_time" => ("Response time", "ms"),
crates/grafana/src/promlib/querydata.rs:241:                "bytes_processed" => ("Bytes processed", "decbytes"),
crates/grafana/src/promlib/querydata.rs:242:                "samples_processed" => ("Samples processed", "short"),
crates/grafana/src/promlib/querydata.rs:243:                "equivalent_samples_read" => ("Equivalent samples read", "short"),
crates/store-sqlite/src/annotation.rs:95:            "annotation" => sql.push_str(" AND alert_id = 0"),
crates/grafana/src/promlib/models.rs:55:            "time_series" => "time_series",
crates/grafana/src/promlib/models.rs:57:            "heatmap" => "heatmap",
crates/grafana/src/promlib/models.rs:320:        (1814400000, 604800000),
crates/grafana/src/promlib/models.rs:331:    if i < ms(63072000000) { ms(2592000000) } else { ms(31536000000) }
crates/grafana/src/promlib/converter.rs:99:            "status" => match it.read_string() {
crates/grafana/src/promlib/converter.rs:113:            "errorType" => match it.read_string() {
crates/grafana/src/promlib/converter.rs:117:            "warnings" => match read_annotations(&mut it, Severity::Warning) {
crates/grafana/src/promlib/converter.rs:184:            "encodingFlags" => loop {
crates/grafana/src/promlib/converter.rs:194:            "resultType" => {
crates/grafana/src/promlib/converter.rs:207:            "result" => {
crates/grafana/src/promlib/converter.rs:256:        "matrix" | "vector" => read_matrix_or_vector_multi(it, result_type),
crates/grafana/src/promlib/converter.rs:257:        "streams" => {
crates/grafana/src/promlib/converter.rs:264:        "string" => read_string_result(it),
crates/grafana/src/promlib/converter.rs:265:        "scalar" => read_scalar(it),
crates/grafana/src/promlib/converter.rs:361:            "seriesLabels" => {
crates/grafana/src/promlib/converter.rs:367:            "exemplars" => {
crates/grafana/src/promlib/converter.rs:377:                            "timestamp" => times.push(time_from_float(it.read_f64()?)),
crates/grafana/src/promlib/converter.rs:378:                            "labels" => {
crates/grafana/src/promlib/converter.rs:510:                "metric" => {
crates/grafana/src/promlib/converter.rs:520:                "values" => {
crates/grafana/src/promlib/converter.rs:527:                "histogram" => {
crates/grafana/src/promlib/converter.rs:535:                "histograms" => {
crates/grafana/src/promlib/converter.rs:579:            "buckets" => {
crates/grafana/src/promlib/converter.rs:629:                    "stream" => {
crates/grafana/src/promlib/converter.rs:634:                    "values" => {
crates/grafana/src/promlib/converter.rs:674:                    "stream" => {
crates/grafana/src/promlib/converter.rs:678:                    "values" => {
crates/grafana/src/promlib/converter.rs:689:                                    "parsed" => it.read_labels(&mut parsed)?,
crates/grafana/src/promlib/converter.rs:690:                                    "structuredMetadata" => it.read_labels(&mut structured)?,
crates/grafana/src/auth.rs:433:        "strict" => "Strict".to_string(),
crates/grafana/src/promlib/fast.rs:222:/// The common sample form `[1700000000,"1.5"]` (whole seconds, a value in Go's JSON float form, no spaces),
crates/grafana/src/plugins.rs:126:            if out.list.iter().any(|p| p.id == id) {
crates/grafana/src/plugins.rs:168:                "unsigned" => ("signatureMissing", "Plugin signature is missing"),
crates/grafana/src/plugins.rs:169:                "modified" => ("signatureModified", "Plugin signature does not match"),
crates/grafana/src/plugins.rs:183:        self.list.iter().find(|p| p.id == id)
crates/grafana/src/plugins.rs:188:            Some(p) if p.kind == "datasource" => Some(&p.ds_meta),
crates/grafana/src/plugins.rs:609:                    "component" => 1,
crates/grafana/src/plugins.rs:675:        "timeseries" => 1,
crates/grafana/src/plugins.rs:676:        "barchart" => 2,
crates/grafana/src/plugins.rs:679:        "bargauge" => 5,
crates/grafana/src/plugins.rs:681:        "singlestat" => 7,
crates/grafana/src/plugins.rs:682:        "piechart" => 8,
crates/grafana/src/plugins.rs:683:        "state-timeline" => 9,
crates/grafana/src/plugins.rs:684:        "heatmap" => 10,
crates/grafana/src/plugins.rs:685:        "status-history" => 11,
crates/grafana/src/plugins.rs:686:        "histogram" => 12,
crates/grafana/src/plugins.rs:689:        "alertlist" => 15,
crates/grafana/src/plugins.rs:690:        "dashlist" => 16,
crates/grafana/src/promlib/client.rs:60:    const HEX: &[u8; 16] = b"0123456789ABCDEF";
crates/grafana/src/ctx.rs:19:            "Viewer" => Some(Role::Viewer),
crates/grafana/src/ctx.rs:20:            "Editor" => Some(Role::Editor),
crates/grafana/src/util.rs:78:    const D: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
crates/grafana/src/util.rs:96:    let ms = (now_ms() - 1288834974657).max(0) as u64;
crates/grafana/src/k8s.rs:93:/// A Grafana internal id: a snowflake (bwmarrin epoch 1288834974657, random 10-bit node, 12-bit step) masked to
crates/grafana/src/k8s.rs:98:    let ms = (crate::util::now_ms() - 1288834974657).max(0);
crates/grafana/src/api/plugins.rs:30:            "accesscontrol" => accesscontrol = matches!(v.as_str(), "1" | "t" | "T" | "TRUE" | "true" | "True"),
crates/grafana/src/dashboard_migrate.rs:579:        "constant" => {
crates/grafana/src/dashboard_migrate.rs:582:        "datasource" => v.set("options", Json::arr()),
crates/grafana/src/dashboard_migrate.rs:589:        "graph" | "graphite" => {
crates/grafana/src/dashboard_migrate.rs:592:                "series" => {
crates/grafana/src/dashboard_migrate.rs:599:                "histogram" => "histogram",
crates/grafana/src/dashboard_migrate.rs:603:        "table-old" => Some("table"),
crates/grafana/src/dashboard_migrate.rs:604:        "singlestat" | "grafana-singlestat-panel" => Some("stat"),
crates/grafana/src/dashboard_migrate.rs:605:        "grafana-piechart-panel" => Some("piechart"),
crates/grafana/src/dashboard_migrate.rs:606:        "grafana-worldmap-panel" => Some("geomap"),
crates/grafana/src/dashboard_migrate.rs:607:        "natel-discrete-panel" => Some("state-timeline"),
crates/grafana/src/api/datasources.rs:374:    if existing.iter().any(|d| d.uid == cmd.uid) {
crates/grafana/src/api/datasources.rs:417:    if !cmd.uid.is_empty() && cmd.uid != url_uid {
crates/grafana/src/api/datasources.rs:515:            if e.org_id == ds.org_id && e.uid == ds.uid && e.version == ds.version && e.updated == ds.updated {
crates/grafana/src/api/datasources.rs:541:        v.retain(|e| !(e.org_id == ds.org_id && e.uid == ds.uid));
crates/grafana/src/api/datasources.rs:686:                    "content-type" | "content-disposition" | "cache-control" | "etag" | "last-modified" => resp.set_header(k, v),
crates/grafana/src/api/datasources.rs:744:        let ds = match groups.iter().position(|g| g.0.uid == uid && !uid.is_empty()) {
crates/grafana/src/api/datasources.rs:747:                if uid == "__expr__" || uid == "-100" || uid == "grafana" || uid == "-- Grafana --" {
crates/grafana/src/api/datasources.rs:783:        match groups.iter().position(|g| g.0.id == ds.id) {
crates/grafana/src/api/search.rs:314:        "dash-db" => (true, false),
crates/grafana/src/api/search.rs:315:        "dash-folder" => (false, true),
crates/grafana/src/api/search.rs:331:            } else if let Some(f) = folders.iter().find(|f| f.id == *id) {
crates/grafana/src/api/search.rs:390:            if let Some(f) = folders.iter().find(|f| f.uid == h.folder) {
crates/grafana/src/api/folders.rs:54:        if out.iter().any(|x| x.uid == parent) {
crates/grafana/src/api/folders.rs:122:    for f in all.iter().filter(|f| f.parent_uid == parent).skip((page - 1) * limit).take(limit) {
crates/grafana/src/api/folders.rs:203:    if !self_uid.is_empty() && chain.iter().any(|f| f.uid == self_uid) {
crates/grafana/src/api/folders.rs:253:    } else if !valid_uid(&uid) || uid == "general" {
crates/grafana/src/api/folders.rs:327:            if f.parent_uid == cur && !set.contains(&f.uid) {
crates/grafana/src/api/folders.rs:399:            if f.parent_uid != p {
crates/grafana/src/api/folders.rs:663:    for f in c.app.store.folders(c.user.org_id)?.iter().filter(|f| f.parent_uid == parent) {
crates/grafana/src/api/dashboards.rs:230:    if uid.is_empty() || uid == "general" {
crates/grafana/src/api/boot.rs:578:        if p.id == "grafana" {
crates/grafana/src/api/boot.rs:664:const BUILD_STAMP: i64 = 1794326153;
crates/grafana/src/api/boot.rs:890:        "grafana-metricsdrilldown-app" => Some(("drilldown", 1, "Metrics", "")),
crates/grafana/src/api/boot.rs:891:        "grafana-lokiexplore-app" => Some(("drilldown", 2, "Logs", "")),
crates/grafana/src/api/boot.rs:892:        "grafana-exploretraces-app" => Some(("drilldown", 3, "Traces", "")),
crates/grafana/src/api/boot.rs:893:        "grafana-pyroscope-app" => Some(("drilldown", 4, "Profiles", "")),
crates/grafana/src/api/boot.rs:894:        "grafana-sqldrilldown-app" => Some(("drilldown", 5, "SQL", "")),
crates/grafana/src/api/boot.rs:895:        "grafana-advisor-app" => Some(("cfg", 0, "Advisor", "Run checks and get suggestions to fix issues")),
crates/grafana/src/api/boot.rs:904:        if n.id == id || find_path(&n.children, id, path) {
crates/grafana/src/api/users.rs:120:    let id = crypto::random_string(10, b"0123456789");
crates/grafana/src/api/users.rs:370:    u.salt = crypto::random_string(10, b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789");
crates/grafana/src/api/users.rs:382:                .with("isActive", t.id == current_session)
crates/grafana/src/api/users.rs:408:    if t.id == c.user.session_id && user_id == c.user.user_id {
crates/grafana/src/api/users.rs:1426:        rands: crypto::random_string(10, b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"),
crates/grafana/src/api/users.rs:1456:    if !admin && u.id == c.user.user_id {
crates/grafana/src/api/users.rs:1501:    if u.id == c.user.user_id {
   165	        }
   166	    }
   167	}
   168	
   169	fn by_title(a: &Item, b: &Item) -> std::cmp::Ordering {
   170	    a.title.to_lowercase().cmp(&b.title.to_lowercase()).then_with(|| a.uid.cmp(&b.uid))
   171	}
   172	
   173	fn qint(c: &Ctx<'_>, k: &str) -> Option<i64> {
   174	    c.req.query_get(k).and_then(|v| v.parse::<i64>().ok())
   175	}
   176	
   177	fn k8s_search(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
   178	    k8s::check_namespace(c)?;
   179	    let types = c.req.query_all("type");
   180	    let has_dash = types.is_empty() || types.iter().any(|t| t == "dashboard");
   181	    let has_folder = types.is_empty() || types.iter().any(|t| t == "folder");
   182	    let (want_dash, want_folder) = if !has_dash && !has_folder { (true, true) } else { (has_dash, has_folder) };
   183	    let limit = qint(c, "limit").unwrap_or(50);
   184	    let limit = if limit <= 0 { 50 } else { limit } as usize;
   185	    let mut offset = qint(c, "offset").unwrap_or(0).max(0) as usize;
   186	    if c.req.query_get("offset").is_none() {
   187	        if let Some(p) = qint(c, "page") {
   188	            if p > 1 {
   189	                offset = (p as usize - 1) * limit;
   190	            }
   191	        }
   192	    }
   193	    let permission = c.req.query_get("permission").unwrap_or_default().to_lowercase();
   194	    if permission == "create" && (has_dash || !has_folder) {
   195	        return Err(k8s::status_error(400, "BadRequest", "permission=create is only supported with type=folder", None));
   196	    }
   197	    let empty = || Response::json_text(200, "{\"totalHits\":0,\"hits\":[]}\n".to_string());
   198	    if (permission == "edit" || permission == "create" || permission == "admin") && !c.user.has_role(if permission == "admin" { Role::Admin } else { Role::Editor }) {
   199	        return Ok(empty());
   200	    }
   201	    let tags = c.req.query_all("tag");
   202	    let names = c.req.query_all("name");
   203	    let folder = c.req.query_get("folder").map(|f| if f == "general" { String::new() } else { f });
   204	    let q = c.req.query_get("query").unwrap_or_default();
   205	    let text = text_query(&q);
   206	    let scored = matches!(text, Text::Match { .. });
   207	    let mut hits: Vec<Item> = Vec::new();
   208	    for mut it in items(c, want_dash, want_folder)? {
   209	        if !tags.is_empty() && !tags.iter().all(|t| it.tags.contains(t)) {
   210	            continue;
   211	        }
   212	        if !names.is_empty() && !names.contains(&it.uid) {
   213	            continue;
   214	        }
   215	        if let Some(f) = folder.as_ref() {
   216	            if it.folder != *f {
   217	                continue;
   218	            }
   219	        }
   220	        match score(&text, &it.title) {
   221	            Some(s) => it.score = s,
   222	            None => continue,
   223	        }
   224	        hits.push(it);
   225	    }
   226	    let mut sort_desc = None;
   227	    for s in c.req.query_all("sort") {
   228	        let (f, desc) = match s.strip_prefix('-') {
   229	            Some(f) => (f.to_string(), true),
   230	            None => (s.clone(), false),
   231	        };
   232	        if f == "title" || f == "name_sort" || f == "alpha" {
   233	            sort_desc = Some(desc);
   234	        }
   235	    }
   236	    match sort_desc {
   237	        Some(desc) => hits.sort_by(|a, b| if desc { by_title(b, a) } else { by_title(a, b) }),
   238	        None if scored => hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal).then_with(|| by_title(a, b))),
   239	        None => hits.sort_by(by_title),
   240	    }
   241	    let total = hits.len();
   242	    if total == 0 {
   243	        return Ok(empty());
   244	    }
   245	    let max_score = if scored { hits.iter().map(|h| h.score).fold(0.0, f64::max) } else { 1.0 };
   246	    let mut out = String::with_capacity(64 + hits.len() * 96);
   247	    out.push_str("{\"totalHits\":");
   248	    json::write_i64(total as i64, &mut out);
   249	    out.push_str(",\"hits\":[");
   250	    for (i, h) in hits.iter().skip(offset).take(limit).enumerate() {
   251	        if i > 0 {
   252	            out.push(',');
   253	        }
   254	        out.push_str(if h.is_folder { "{\"resource\":\"folders\",\"name\":" } else { "{\"resource\":\"dashboards\",\"name\":" });
   255	        json::write_str(&h.uid, &mut out);
   256	        out.push_str(",\"title\":");
   257	        json::write_str(&h.title, &mut out);
   258	        if !h.tags.is_empty() {
   259	            out.push_str(",\"tags\":[");
   260	            for (j, t) in h.tags.iter().enumerate() {
   261	                if j > 0 {
   262	                    out.push(',');
   263	                }
   264	                json::write_str(t, &mut out);
   265	            }
   266	            out.push(']');
   267	        }
   268	        if !h.folder.is_empty() {
   269	            out.push_str(",\"folder\":");
   270	            json::write_str(&h.folder, &mut out);
   271	        }
   272	        if scored && h.score != 0.0 {
   273	            out.push_str(",\"score\":");
   274	            json::write_f64(h.score, &mut out);
   275	        }
   276	        out.push('}');
   277	    }
   278	    out.push_str("],\"queryCost\":");
   279	    json::write_f64((total * 100) as f64, &mut out);
   280	    out.push_str(",\"maxScore\":");
   281	    json::write_f64(max_score, &mut out);
   282	    out.push_str("}\n");
   283	    Ok(Response::json_text(200, out))
   284	}
   285	
nl: crates/grafana/src/api/k8s.rs: No such file or directory
   100	            o.set("email", u.2.as_str());
   101	            o.set("avatarUrl", avatar_url(c, &u.2));
   102	        }
   103	    }
   104	    let data = match Json::parse(&a.data) {
   105	        Ok(v) => v,
   106	        Err(_) => Json::obj(),
   107	    };
   108	    o.set("data", data);
   109	    o
   110	}
   111	
   112	fn get_annotations(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
   113	    let mut q = AnnotationQuery {
   114	        org_id: c.user.org_id,
   115	        from_ms: qi64(c, "from"),
   116	        to_ms: qi64(c, "to"),
   117	        dashboard_uid: c.req.query_get("dashboardUID").unwrap_or_default(),
   118	        dashboard_id: qi64(c, "dashboardId"),
   119	        panel_id: qi64(c, "panelId"),
   120	        user_id: qi64(c, "userId"),
   121	        alert_id: qi64(c, "alertId"),
   122	        type_: c.req.query_get("type").unwrap_or_default(),
   123	        tags: c.req.query_all("tags"),
   124	        match_any: c.req.query_get("matchAny").as_deref() == Some("true"),
   125	        limit: qi64(c, "limit"),
   126	    };
   127	    if q.limit == 0 {
   128	        q.limit = DEFAULT_LIMIT;
   129	    }
   130	    if q.dashboard_id != 0 && q.dashboard_uid.is_empty() {
   131	        match c.app.store.dashboard_by_id(c.user.org_id, q.dashboard_id) {
   132	            Ok(d) => q.dashboard_uid = d.uid,
   133	            Err(_) => return Err(ApiError::with_trace(400, "Invalid dashboard ID in annotation request")),
   134	        }
   135	    }
   136	    if !q.dashboard_uid.is_empty() {
   137	        // the dashboard uid is the filter; the deprecated id is not stored for every row
   138	        q.dashboard_id = 0;
   139	    }
   140	    let list = c.app.store.annotations_find(&q).map_err(|_| ApiError::with_trace(500, "Failed to get annotations"))?;
   141	    let mut cache = UserCache::new();
   142	    let mut out = Json::arr();
   143	    for a in list.iter() {
   144	        out.push(item_dto(c, &mut cache, a));
   145	    }
   146	    Ok(Response::json(200, &out))
   147	}
   148	
   149	fn tags_of(v: Option<&Json>) -> Option<Vec<String>> {
   150	    match v {
   151	        Some(Json::Arr(a)) => {
   152	            let mut out = Vec::new();
   153	            for t in a.iter() {
   154	                if let Some(s) = t.as_str() {
   155	                    let s = s.trim();
   156	                    if !s.is_empty() && !out.iter().any(|x: &String| x == s) {
   157	                        out.push(s.to_string());
   158	                    }
   159	                }
   160	            }
   161	            Some(out)
   162	        }
   163	        _ => None,
   164	    }
   165	}
   166	
   167	fn data_text(v: Option<&Json>) -> Option<String> {
   168	    match v {
   169	        Some(Json::Null) | None => None,
   170	        Some(d) => Some(d.stringify()),
   171	    }
   172	}
   173	
   174	/// validateTimeRange: end defaults to start; swapped when reversed. Err when both are missing.
   175	fn time_range(epoch: i64, end: i64) -> Result<(i64, i64), ()> {
   176	    let (mut s, mut e) = (epoch, end);
   177	    if e == 0 {
   178	        if s == 0 {
   179	            return Err(());
   180	        }
   181	        e = s;
   182	    }
   183	    if s == 0 {
   184	        s = e;
   185	    }
   186	    if e < s {
   187	        std::mem::swap(&mut s, &mut e);
   188	    }
   189	    Ok((s, e))
   190	}
   191	
   192	fn post_annotation(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
   193	    let cmd = c.body_json()?;
   194	    let mut dashboard_uid = cmd.str_of("dashboardUID").to_string();
   195	    let mut dashboard_id = cmd.i64_of("dashboardId");
   196	    if !dashboard_uid.is_empty() {
   197	        if let Ok(d) = c.app.store.dashboard_by_uid(c.user.org_id, &dashboard_uid) {
   198	            dashboard_id = d.id;
   199	        }
   200	    }
   201	    if dashboard_id != 0 && dashboard_uid.is_empty() {
   202	        match c.app.store.dashboard_by_id(c.user.org_id, dashboard_id) {
   203	            Ok(d) => dashboard_uid = d.uid,
   204	            Err(_) => return Err(ApiError::with_trace(400, "Invalid dashboard ID in annotation request")),
   205	        }
   206	    }
   207	    if !c.user.has_role(Role::Editor) {
   208	        return Err(ApiError::new(403, "Access denied to save the annotation"));
   209	    }
   210	    let text = cmd.str_of("text").to_string();

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "nl -ba upstream-prometheus/pkg/promlib/client/transport_test.go; nl -ba upstream-prometheus/pkg/promlib/resource/transport_error_test.go; nl -ba crates/grafana/src/api/datasources.rs | sed -n '1,150p'; rg -n 'timeout|source|TransportError|forward|header' crates/grafana/src/promlib/client.rs | head -45; nl -ba upstream-prometheus/pkg/promlib/models/query_test.go | sed -n '149,190p'" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
     1	package client
     2	
     3	import (
     4		"context"
     5		"net/http"
     6		"testing"
     7	
     8		"github.com/grafana/grafana-plugin-sdk-go/backend"
     9		"github.com/stretchr/testify/require"
    10	
    11		"github.com/grafana/grafana-prometheus-datasource/pkg/promlib/models"
    12	)
    13	
    14	func TestCreateTransportOptions(t *testing.T) {
    15		t.Run("creates correct options object", func(t *testing.T) {
    16			settings := backend.DataSourceInstanceSettings{
    17				BasicAuthEnabled: false,
    18				BasicAuthUser:    "",
    19				JSONData:         []byte(`{"httpHeaderName1": "foo"}`),
    20				DecryptedSecureJSONData: map[string]string{
    21					"httpHeaderValue1": "bar",
    22				},
    23			}
    24			jsonData, err := models.ParsePromOptions(settings)
    25			require.NoError(t, err)
    26			opts, err := CreateTransportOptions(
    27				context.Background(),
    28				settings,
    29				jsonData.HTTPMethod,
    30				string(jsonData.CustomQueryParameters),
    31				float64(jsonData.MaxSamplesProcessedWarningThreshold),
    32				float64(jsonData.MaxSamplesProcessedErrorThreshold),
    33				bool(jsonData.QueryStatsEnabled),
    34				backend.NewLoggerWith("logger", "test"),
    35			)
    36			require.NoError(t, err)
    37			require.Equal(t, http.Header{"Foo": []string{"bar"}}, opts.Header)
    38			require.Equal(t, 1, len(opts.Middlewares))
    39		})
    40	
    41		t.Run("enables ForwardHTTPHeaders so Grafana headers (FromAlert, X-Rule-*, X-Dashboard-*, X-Panel-*, X-Grafana-*) are forwarded to the datasource", func(t *testing.T) {
    42			settings := backend.DataSourceInstanceSettings{}
    43			jsonData, err := models.ParsePromOptions(settings)
    44			require.NoError(t, err)
    45			opts, err := CreateTransportOptions(
    46				context.Background(),
    47				settings,
    48				jsonData.HTTPMethod,
    49				string(jsonData.CustomQueryParameters),
    50				float64(jsonData.MaxSamplesProcessedWarningThreshold),
    51				float64(jsonData.MaxSamplesProcessedErrorThreshold),
    52				bool(jsonData.QueryStatsEnabled),
    53				backend.NewLoggerWith("logger", "test"),
    54			)
    55			require.NoError(t, err)
    56			require.True(t, opts.ForwardHTTPHeaders)
    57		})
    58	}
     1	package resource_test
     2	
     3	import (
     4		"context"
     5		"errors"
     6		"net/http"
     7		"testing"
     8	
     9		"github.com/grafana/grafana-plugin-sdk-go/backend"
    10		"github.com/stretchr/testify/require"
    11	)
    12	
    13	type timeoutError struct{}
    14	
    15	func (timeoutError) Error() string   { return "dial tcp: i/o timeout" }
    16	func (timeoutError) Timeout() bool   { return true }
    17	func (timeoutError) Temporary() bool { return false }
    18	
    19	func TestResourceTransportErrorsKeepTheirSource(t *testing.T) {
    20		tests := []struct {
    21			name           string
    22			err            error
    23			wantDownstream bool
    24		}{
    25			{name: "error marked downstream", err: backend.DownstreamError(errors.New("too many concurrent requests to host")), wantDownstream: true},
    26			{name: "timeout", err: timeoutError{}, wantDownstream: true},
    27			{name: "cancellation", err: context.Canceled, wantDownstream: true},
    28			{name: "unmarked error", err: errors.New("unexpected failure"), wantDownstream: false},
    29		}
    30		for _, tc := range tests {
    31			t.Run(tc.name, func(t *testing.T) {
    32				_, settings, logger := setup()
    33				client := &http.Client{Transport: &mockRoundTripper{Err: tc.err}}
    34				res, err := newResource(t, client, settings, logger)
    35				require.NoError(t, err)
    36				req := &backend.CallResourceRequest{Method: http.MethodGet, Path: "api/v1/labels", URL: "/api/v1/labels"}
    37	
    38				_, executeErr := res.Execute(context.Background(), req)
    39				searchErr := res.ExecuteSearch(context.Background(), req, backend.CallResourceResponseSenderFunc(func(*backend.CallResourceResponse) error {
    40					return nil
    41				}))
    42	
    43				for _, got := range []error{executeErr, searchErr} {
    44					require.ErrorIs(t, got, tc.err)
    45					require.Equal(t, tc.wantDownstream, backend.IsDownstreamError(got))
    46				}
    47			})
    48		}
    49	}
     1	//! Data sources CRUD, health, resources, the data source proxy, and `/api/ds/query` dispatch (agent C).
     2	//! Upstream: pkg/api/datasources.go, pkg/api/ds_query.go, pkg/services/query/query.go,
     3	//! pkg/services/datasources/service, pkg/api/plugin_resource.go.
     4	//!
     5	//! RBAC as OSS defaults: read, query and id-read are Viewer; create, write and delete are Admin.
     6	//! The numeric-id routes (`/api/datasources/:id...`) are not registered: Grafana 13.3.1 only adds them with the
     7	//! `datasourceLegacyIdApi` feature flag, which is off by default.
     8	use crate::app::App;
     9	use crate::ctx::{Access, Ctx, Role, Routes};
    10	use crate::error::ApiError;
    11	use crate::promlib::client::{HttpReq, HttpResp, Instance, TransportError};
    12	use crate::promlib::frame::{DataResponse, write_sorted};
    13	use crate::promlib::models::{DataQuery, MS};
    14	use crate::promlib::querydata::Rendered;
    15	use crate::promlib::{self, settings};
    16	use crate::store::{DataSource, StoreError};
    17	use http::Response;
    18	use json::Json;
    19	use std::sync::{Arc, Mutex};
    20	use std::time::Duration;
    21	
    22	pub fn routes(r: &mut Routes) {
    23	    let v = Access::Role(Role::Viewer);
    24	    let a = Access::Role(Role::Admin);
    25	    r.get("/api/datasources", v, list);
    26	    r.post("/api/datasources", a, add);
    27	    r.get("/api/datasources/uid/:uid", v, get_by_uid);
    28	    r.put("/api/datasources/uid/:uid", a, update_by_uid);
    29	    r.delete("/api/datasources/uid/:uid", a, delete_by_uid);
    30	    r.any("/api/datasources/uid/:uid/health", v, health_by_uid);
    31	    r.any("/api/datasources/uid/:uid/resources", v, resource_by_uid);
    32	    r.any("/api/datasources/uid/:uid/resources/*rest", v, resource_by_uid);
    33	    r.any("/api/datasources/proxy/uid/:uid", v, proxy_by_uid);
    34	    r.any("/api/datasources/proxy/uid/:uid/*rest", v, proxy_by_uid);
    35	    r.get("/api/datasources/name/:name", v, get_by_name);
    36	    r.delete("/api/datasources/name/:name", a, delete_by_name);
    37	    r.get("/api/datasources/id/:name", v, id_by_name);
    38	    r.post("/api/ds/query", v, ds_query);
    39	}
    40	
    41	// ---------------------------------------------------------------- DTOs
    42	
    43	/// (type name, logo) of a data source plugin type, as the plugin store gives them.
    44	fn plugin_meta(app: &App, typ: &str) -> (String, String) {
    45	    if typ == "prometheus" {
    46	        return ("Prometheus".to_string(), "public/plugins/prometheus/img/prometheus_logo.svg".to_string());
    47	    }
    48	    // other installed or core plugins: plugin.json name and small logo
    49	    let candidates = [app.cfg.plugins_dir().join(typ).join("plugin.json"), app.cfg.home.join("public/app/plugins/datasource").join(typ).join("plugin.json")];
    50	    for (i, p) in candidates.iter().enumerate() {
    51	        if typ.contains('/') || typ.contains("..") || typ.is_empty() {
    52	            break;
    53	        }
    54	        if let Ok(t) = std::fs::read_to_string(p) {
    55	            if let Ok(j) = Json::parse(&t) {
    56	                if j.str_of("type") == "datasource" {
    57	                    let logo = j.path("info.logos.small").and_then(|v| v.as_str()).unwrap_or("");
    58	                    let base = if i == 0 { format!("public/plugins/{typ}/") } else { format!("public/app/plugins/datasource/{typ}/") };
    59	                    return (j.str_of("name").to_string(), if logo.is_empty() { "public/img/icn-datasource.svg".to_string() } else { format!("{base}{logo}") });
    60	                }
    61	            }
    62	        }
    63	    }
    64	    (typ.to_string(), "public/img/icn-datasource.svg".to_string())
    65	}
    66	
    67	fn json_data_value(ds: &DataSource) -> Json {
    68	    if ds.json_data.trim().is_empty() {
    69	        return Json::obj();
    70	    }
    71	    Json::parse(&ds.json_data).unwrap_or_else(|_| Json::obj())
    72	}
    73	
    74	/// jsonData as Go's simplejson writes it (map keys sorted, recursively).
    75	fn sorted(v: &Json) -> Json {
    76	    match v {
    77	        Json::Obj(o) => {
    78	            let mut items: Vec<(String, Json)> = o.iter().map(|(k, x)| (k.clone(), sorted(x))).collect();
    79	            items.sort_by(|a, b| a.0.cmp(&b.0));
    80	            Json::Obj(items)
    81	        }
    82	        Json::Arr(a) => Json::Arr(a.iter().map(sorted).collect()),
    83	        _ => v.clone(),
    84	    }
    85	}
    86	
    87	/// `convertModelToDtos` (dtos.DataSource).
    88	fn dto(c: &Ctx<'_>, ds: &DataSource, access_control: Option<Json>) -> Json {
    89	    let (_, logo) = plugin_meta(c.app, &ds.type_);
    90	    let secure = crate::secrets::decrypt_map(&c.app.cfg.secret_key, &ds.secure_json_data);
    91	    let mut fields = Json::obj();
    92	    let mut keys: Vec<String> = Vec::new();
    93	    if let Some(o) = secure.as_obj() {
    94	        for (k, v) in o.iter() {
    95	            if !v.as_str().unwrap_or("").is_empty() {
    96	                keys.push(k.clone());
    97	            }
    98	        }
    99	    }
   100	    keys.sort();
   101	    for k in keys {
   102	        fields.set(&k, true);
   103	    }
   104	    let mut d = Json::obj()
   105	        .with("id", ds.id)
   106	        .with("uid", ds.uid.as_str())
   107	        .with("orgId", ds.org_id)
   108	        .with("name", ds.name.as_str())
   109	        .with("type", ds.type_.as_str())
   110	        .with("typeLogoUrl", logo)
   111	        .with("access", ds.access.as_str())
   112	        .with("url", ds.url.as_str())
   113	        .with("user", ds.user.as_str())
   114	        .with("database", ds.database.as_str())
   115	        .with("basicAuth", ds.basic_auth)
   116	        .with("basicAuthUser", ds.basic_auth_user.as_str())
   117	        .with("withCredentials", ds.with_credentials)
   118	        .with("isDefault", ds.is_default)
   119	        .with("jsonData", sorted(&json_data_value(ds)))
   120	        .with("secureJsonFields", fields)
   121	        .with("version", ds.version)
   122	        .with("readOnly", ds.read_only);
   123	    if let Some(ac) = access_control {
   124	        d.set("accessControl", ac);
   125	    }
   126	    d.set("apiVersion", ds.api_version.as_str());
   127	    d
   128	}
   129	
   130	/// `getAccessControlMetadata` for a data source, OSS role grants (keys sorted as Go writes the map).
   131	fn access_control(c: &Ctx<'_>) -> Json {
   132	    let r = c.user.org_role;
   133	    let mut m = Json::obj();
   134	    let viewer = r.includes(Role::Viewer);
   135	    let editor = r.includes(Role::Editor);
   136	    let admin = r.includes(Role::Admin);
   137	    let items: [(&str, bool); 11] = [
   138	        ("alert.instances.external:read", viewer),
   139	        ("alert.instances.external:write", editor),
   140	        ("alert.notifications.external:read", viewer),
   141	        ("alert.notifications.external:write", editor),
   142	        ("alert.rules.external:read", viewer),
   143	        ("alert.rules.external:write", editor),
   144	        ("datasources.id:read", viewer),
   145	        ("datasources:delete", admin),
   146	        ("datasources:query", viewer),
   147	        ("datasources:read", viewer),
   148	        ("datasources:write", admin),
   149	    ];
   150	    for (k, ok) in items.iter() {
1://! Requests to Prometheus: promlib `client/client.go` (query, query_range, query_exemplars, resources),
6:/// One HTTP request to the data source.
11:    pub headers: Vec<(String, String)>,
15:/// One HTTP response from the data source (header names in lowercase).
19:    pub headers: Vec<(String, String)>,
23:    pub fn header(&self, name: &str) -> Option<&str> {
24:        for (k, v) in self.headers.iter() {
31:    pub fn header_values(&self, name: &str) -> Vec<&str> {
32:        self.headers.iter().filter(|h| h.0.eq_ignore_ascii_case(name)).map(|h| h.1.as_str()).collect()
38:pub struct TransportError {
40:    pub timeout: bool,
44:pub type Transport<'a> = &'a dyn Fn(&HttpReq) -> Result<HttpResp, TransportError>;
46:/// A data source instance: URL, options, and the headers every request carries (basic auth, custom headers).
53:    pub headers: Vec<(String, String)>,
55:    pub timeout_secs: u64,
268:        let mut headers = self.headers.clone();
270:            headers.push(("Content-Type".to_string(), "application/x-www-form-urlencoded".to_string()));
274:        HttpReq { method: method.to_string(), url, headers, body }
291:        if !self.opts.query_timeout.is_empty() {
292:            qv.push(("timeout".to_string(), self.opts.query_timeout.clone()));
300:        if !self.opts.query_timeout.is_empty() {
301:            qv.push(("timeout".to_string(), self.opts.query_timeout.clone()));
313:    /// `QueryResource`: `path` is the resource path, `raw_query` the caller's query string. GET calls go as GET
315:    pub fn resource_request(&self, method: &str, path: &str, raw_query: &str, body: &[u8]) -> HttpReq {
   149	
   150			q := queryContext(`{
   151				"expr": "rate(ALERTS{job=\"test\" [$__interval]})",
   152				"format": "time_series",
   153				"intervalFactor": 1,
   154				"intervalMs": 60000,
   155				"refId": "A"
   156			}`, timeRange, time.Duration(1)*time.Minute)
   157			q.MaxDataPoints = 2110
   158	
   159			res, err := models.Parse(context.Background(), log.New(), span, q, "15s", intervalCalculator, false)
   160			require.NoError(t, err)
   161			require.Equal(t, "rate(ALERTS{job=\"test\" [1m]})", res.Expr)
   162			require.Equal(t, 60*time.Second, res.Step)
   163		})
   164	
   165		t.Run("parsing query model with $__interval_ms variable", func(t *testing.T) {
   166			timeRange := backend.TimeRange{
   167				From: now,
   168				To:   now.Add(48 * time.Hour),
   169			}
   170	
   171			q := queryContext(`{
   172				"expr": "rate(ALERTS{job=\"test\" [$__interval_ms]})",
   173				"format": "time_series",
   174				"intervalFactor": 1,
   175				"intervalMs": 60000,
   176				"refId": "A"
   177			}`, timeRange, time.Duration(1)*time.Minute)
   178	
   179			res, err := models.Parse(context.Background(), log.New(), span, q, "15s", intervalCalculator, false)
   180			require.NoError(t, err)
   181			require.Equal(t, "rate(ALERTS{job=\"test\" [120000]})", res.Expr)
   182		})
   183	
   184		t.Run("parsing query model with $__interval_ms and $__interval variable", func(t *testing.T) {
   185			timeRange := backend.TimeRange{
   186				From: now,
   187				To:   now.Add(48 * time.Hour),
   188			}
   189	
   190			q := queryContext(`{

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "nl -ba crates/grafana/src/api/datasources.rs | sed -n '550,695p'; nl -ba crates/grafana/src/k8s.rs | sed -n '75,116p'; nl -ba crates/grafana/src/api/search.rs | sed -n '90,170p'; nl -ba upstream-prometheus/pkg/promlib/querydata/request_test.go | sed -n '480,565p'; nl -ba crates/grafana/tests/promlib_converter.rs | sed -n '130,192p'" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
   550	fn title_method(m: &str) -> String {
   551	    let lower = m.to_ascii_lowercase();
   552	    match lower.get(..1) {
   553	        Some(f) => f.to_ascii_uppercase() + &lower[1..],
   554	        None => String::new(),
   555	    }
   556	}
   557	
   558	fn host_port(url: &str) -> String {
   559	    let rest = url.split_once("://").map(|p| p.1).unwrap_or(url);
   560	    let hp = rest.split(['/', '?']).next().unwrap_or("");
   561	    if hp.contains(':') { hp.to_string() } else { format!("{hp}:80") }
   562	}
   563	
   564	/// Sends a plugin request with the shared pooled client; errors read as Go's `*url.Error` text.
   565	pub fn send(app: &App, r: &HttpReq, timeout_secs: u64) -> Result<HttpResp, TransportError> {
   566	    let hdrs: Vec<(&str, &str)> = r.headers.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
   567	    match app.client.request(&r.method, &r.url, &hdrs, &r.body, Duration::from_secs(timeout_secs)) {
   568	        Ok(res) => Ok(HttpResp { status: res.status, headers: res.headers, body: res.body }),
   569	        Err(e) => {
   570	            let prefix = format!("{} {}", title_method(&r.method), json::quote(&r.url));
   571	            let (msg, timeout) = match e {
   572	                http::ClientError::Timeout => (format!("{prefix}: context deadline exceeded (Client.Timeout exceeded while awaiting headers)"), true),
   573	                http::ClientError::Connect(m) => {
   574	                    let what = if m.contains("refused") { "connect: connection refused".to_string() } else { m };
   575	                    (format!("{prefix}: dial tcp {}: {what}", host_port(&r.url)), false)
   576	                }
   577	                http::ClientError::BadUrl(_) => (format!("{prefix}: unsupported protocol scheme"), false),
   578	                other => (format!("{prefix}: {other}"), false),
   579	            };
   580	            Err(TransportError { message: msg, timeout })
   581	        }
   582	    }
   583	}
   584	
   585	// ---------------------------------------------------------------- health, resources, proxy
   586	
   587	fn ds_for_plugin_call(c: &Ctx<'_>, meta_err: &str) -> Result<DataSource, ApiError> {
   588	    let uid = c.param("uid");
   589	    if !crate::util::valid_uid(uid) {
   590	        return Err(ApiError::bad_request("UID is invalid"));
   591	    }
   592	    match c.app.store.datasource_by_uid(c.user.org_id, uid) {
   593	        Ok(d) => Ok(d),
   594	        Err(_) => Err(ApiError::new(500, meta_err)),
   595	    }
   596	}
   597	
   598	fn health_by_uid(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
   599	    let ds = ds_for_plugin_call(c, "Unable to load datasource metadata")?;
   600	    if ds.type_ != "prometheus" {
   601	        return Err(ApiError::new(404, "Plugin not found"));
   602	    }
   603	    let inst = instance(c.app, &ds);
   604	    let (ok, message, details) = match inst {
   605	        Ok(inst) => {
   606	            let app = c.app;
   607	            let to = inst.timeout_secs;
   608	            let t = |r: &HttpReq| send(app, r, to);
   609	            promlib::check_health(&inst, &t)
   610	        }
   611	        Err(e) => (false, format!("{e} - error getting datasource info"), None),
   612	    };
   613	    let mut payload = Json::obj();
   614	    if let Some(d) = details {
   615	        payload.set("details", d);
   616	    }
   617	    payload.set("message", message);
   618	    payload.set("status", if ok { "OK" } else { "ERROR" });
   619	    Ok(Response::json(if ok { 200 } else { 400 }, &payload))
   620	}
   621	
   622	fn resource_by_uid(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
   623	    let ds = ds_for_plugin_call(c, "Unable to load datasource meta data")?;
   624	    if ds.type_ != "prometheus" {
   625	        return Err(ApiError::new(500, "Unable to find datasource plugin"));
   626	    }
   627	    let inst = instance(c.app, &ds).map_err(|_| ApiError::new(500, "Failed to get plugin settings"))?;
   628	    let path = c.param("rest").to_string();
   629	    if path.eq_ignore_ascii_case("suggestions") {
   630	        return Err(ApiError::new(500, "Failed to call resource"));
   631	    }
   632	    let app = c.app;
   633	    let to = inst.timeout_secs;
   634	    let t = |r: &HttpReq| send(app, r, to);
   635	    let body = std::mem::take(&mut c.req.body);
   636	    let cache = c.req.header("x-grafana-cache").map(|v| v.to_string());
   637	    match promlib::call_resource(&inst, &c.req.method, &path, &c.req.query, &body, cache.as_deref(), &t) {
   638	        Ok(r) => {
   639	            let mut resp = Response::new(r.status);
   640	            resp.body = http::Body::Bytes(r.body);
   641	            for (k, v) in r.headers.iter() {
   642	                resp.set_header(k, v);
   643	            }
   644	            Ok(resp)
   645	        }
   646	        Err(_) => Err(ApiError::new(500, "Failed to call resource")),
   647	    }
   648	}
   649	
   650	/// The data source proxy (`ProxyDataSourceRequestWithUID`), as a plain forward to the data source URL with its
   651	/// authentication headers.
   652	fn proxy_by_uid(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
   653	    let uid = c.param("uid").to_string();
   654	    let ds = match c.app.store.datasource_by_uid(c.user.org_id, &uid) {
   655	        Ok(d) => d,
   656	        Err(StoreError::NotFound) => return Err(ApiError::new(404, "Unable to find datasource")),
   657	        Err(_) => return Err(ApiError::new(500, "Unable to load datasource meta data")),
   658	    };
   659	    if ds.access != "proxy" {
   660	        return Err(ApiError::bad_request("Unable to proxy request to datasource, access mode is not proxy"));
   661	    }
   662	    let inst = instance(c.app, &ds).map_err(|_| ApiError::new(500, "Unable to load datasource meta data"))?;
   663	    let rest = c.param("rest").to_string();
   664	    if rest.split('/').any(|s| s == "..") {
   665	        return Err(ApiError::bad_request("Invalid proxy path"));
   666	    }
   667	    let mut url = ds.url.trim_end_matches('/').to_string();
   668	    url.push('/');
   669	    url.push_str(&rest);
   670	    if !c.req.query.is_empty() {
   671	        url.push('?');
   672	        url.push_str(&c.req.query);
   673	    }
   674	    let mut headers = inst.headers.clone();
   675	    for name in ["content-type", "accept"] {
   676	        if let Some(v) = c.req.header(name) {
   677	            headers.push((name.to_string(), v.to_string()));
   678	        }
   679	    }
   680	    let req = HttpReq { method: c.req.method.clone(), url, headers, body: std::mem::take(&mut c.req.body) };
   681	    match send(c.app, &req, inst.timeout_secs) {
   682	        Ok(r) => {
   683	            let mut resp = Response::new(r.status);
   684	            for (k, v) in r.headers.iter() {
   685	                match k.as_str() {
   686	                    "content-type" | "content-disposition" | "cache-control" | "etag" | "last-modified" => resp.set_header(k, v),
   687	                    _ => {}
   688	                }
   689	            }
   690	            resp.body = http::Body::Bytes(r.body);
   691	            Ok(resp)
   692	        }
   693	        Err(_) => Err(ApiError::new(502, "Bad Gateway")),
   694	    }
   695	}
    75	/// A resource version: write time in microseconds, strictly increasing.
    76	pub fn next_rv() -> i64 {
    77	    let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
    78	        Ok(d) => d.as_micros() as i64,
    79	        Err(_) => 0,
    80	    };
    81	    let mut prev = LAST_RV.load(Ordering::Relaxed);
    82	    loop {
    83	        let next = if now > prev { now } else { prev + 1 };
    84	        match LAST_RV.compare_exchange(prev, next, Ordering::Relaxed, Ordering::Relaxed) {
    85	            Ok(_) => return next,
    86	            Err(p) => prev = p,
    87	        }
    88	    }
    89	}
    90	
    91	static SNOW_SEQ: AtomicI64 = AtomicI64::new(0);
    92	
    93	/// A Grafana internal id: a snowflake (bwmarrin epoch 1288834974657, random 10-bit node, 12-bit step) masked to
    94	/// 52 bits, as `apistore` assigns `grafana.app/deprecatedInternalID`.
    95	pub fn snowflake_id() -> i64 {
    96	    let mut r = [0u8; 2];
    97	    crypto::random_bytes(&mut r);
    98	    let ms = (crate::util::now_ms() - 1288834974657).max(0);
    99	    let node = ((r[0] as i64) << 8 | r[1] as i64) & 0x3ff;
   100	    let seq = SNOW_SEQ.fetch_add(1, Ordering::Relaxed) & 0xfff;
   101	    ((ms << 22) | (node << 12) | seq) & ((1i64 << 52) - 1)
   102	}
   103	
   104	/// A random RFC 4122 version 4 UUID (Kubernetes `metadata.uid`).
   105	pub fn uuid_v4() -> String {
   106	    let mut b = [0u8; 16];
   107	    crypto::random_bytes(&mut b);
   108	    b[6] = (b[6] & 0x0f) | 0x40;
   109	    b[8] = (b[8] & 0x3f) | 0x80;
   110	    let h = crypto::hex(&b);
   111	    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
   112	}
   113	
   114	/// `generateName` suffix: 5 characters of the apiserver's alphabet (k8s.io/apimachinery/pkg/util/rand.String).
   115	pub fn generate_name(prefix: &str) -> String {
   116	    const A: &[u8] = b"bcdfghjklmnpqrstvwxz2456789";
    90	    Match { phrase: String, terms: Vec<String> },
    91	}
    92	
    93	fn text_query(q: &str) -> Text {
    94	    let mut q = q.to_string();
    95	    if q == "*" {
    96	        return Text::All;
    97	    }
    98	    if !q.is_empty() && q.len() < 3 && !q.contains('*') {
    99	        q.push('*');
   100	    }
   101	    if q.len() <= 1 {
   102	        return Text::All;
   103	    }
   104	    if q.contains('*') {
   105	        return Text::Wildcard(q.to_lowercase().chars().collect());
   106	    }
   107	    let mut terms = Vec::new();
   108	    for w in q.split_whitespace() {
   109	        if w.len() >= 3 {
   110	            for t in words(w) {
   111	                terms.push(t);
   112	            }
   113	        }
   114	    }
   115	    Text::Match { phrase: q.trim_end_matches(' ').to_lowercase(), terms }
   116	}
   117	
   118	/// Some(score) when the title matches.
   119	fn score(t: &Text, title: &str) -> Option<f64> {
   120	    match t {
   121	        Text::All => Some(0.0),
   122	        Text::Wildcard(p) => {
   123	            let lt = title.to_lowercase();
   124	            let whole: Vec<char> = lt.chars().collect();
   125	            if glob(p, &whole) {
   126	                return Some(0.0);
   127	            }
   128	            for w in words(title) {
   129	                let wc: Vec<char> = w.chars().collect();
   130	                if glob(p, &wc) {
   131	                    return Some(0.0);
   132	                }
   133	            }
   134	            None
   135	        }
   136	        Text::Match { phrase, terms } => {
   137	            let lt = title.to_lowercase();
   138	            let tw = words(title);
   139	            let mut s = 0.0;
   140	            if lt == *phrase {
   141	                s += 10.0;
   142	            }
   143	            if !terms.is_empty() {
   144	                let mut all_words = true;
   145	                let mut all_prefix = true;
   146	                let mut part = 0.0;
   147	                for t in terms.iter() {
   148	                    let exact = tw.iter().any(|w| w == t);
   149	                    let pre: String = t.chars().take(10).collect();
   150	                    let prefix = tw.iter().find(|w| w.starts_with(pre.as_str()));
   151	                    all_words &= exact;
   152	                    match prefix {
   153	                        Some(w) => part += pre.chars().count() as f64 / w.chars().count().max(1) as f64,
   154	                        None => all_prefix = false,
   155	                    }
   156	                }
   157	                if all_words {
   158	                    s += 2.0 * terms.len() as f64 / tw.len().max(1) as f64;
   159	                }
   160	                if all_prefix {
   161	                    s += part / tw.len().max(1) as f64;
   162	                }
   163	            }
   164	            if s > 0.0 { Some(s / 10.0) } else { None }
   165	        }
   166	    }
   167	}
   168	
   169	fn by_title(a: &Item, b: &Item) -> std::cmp::Ordering {
   170	    a.title.to_lowercase().cmp(&b.title.to_lowercase()).then_with(|| a.uid.cmp(&b.uid))
   480	type queryResult struct {
   481		Type   p.ValueType `json:"resultType"`
   482		Result any         `json:"result"`
   483	}
   484	
   485	func executeWithHeaders(tctx *testContext, query backend.DataQuery, rqr, iqr, eqr any, headers map[string]string) (data.Frames, error) {
   486		req := backend.QueryDataRequest{
   487			Queries: []backend.DataQuery{query},
   488			Headers: headers,
   489			PluginContext: backend.PluginContext{
   490				GrafanaConfig: backend.NewGrafanaCfg(map[string]string{
   491					"concurrent_query_count": "10",
   492				}),
   493			},
   494		}
   495	
   496		rangeRes, err := toAPIResponse(rqr)
   497		if err != nil {
   498			return nil, err
   499		}
   500		instantRes, err := toAPIResponse(iqr)
   501		if err != nil {
   502			return nil, err
   503		}
   504		exemplarRes, err := toAPIResponse(eqr)
   505		if err != nil {
   506			return nil, err
   507		}
   508		defer func() {
   509			if err := rangeRes.Body.Close(); err != nil {
   510				fmt.Println(fmt.Errorf("rangeRes body close error: %v", err))
   511			}
   512			if err := instantRes.Body.Close(); err != nil {
   513				fmt.Println(fmt.Errorf("instantRes body close error: %v", err))
   514			}
   515			if err := exemplarRes.Body.Close(); err != nil {
   516				fmt.Println(fmt.Errorf("exemplarRes body close error: %v", err))
   517			}
   518		}()
   519		tctx.httpProvider.setResponse(rangeRes, instantRes, exemplarRes)
   520	
   521		// Create context with GrafanaConfig
   522		ctx := backend.WithGrafanaConfig(context.Background(), backend.NewGrafanaCfg(map[string]string{
   523			"concurrent_query_count": "10",
   524		}))
   525	
   526		res, err := tctx.queryData.Execute(ctx, &req)
   527		if err != nil {
   528			return nil, err
   529		}
   530	
   531		return res.Responses[req.Queries[0].RefID].Frames, nil
   532	}
   533	
   534	func execute(tctx *testContext, query backend.DataQuery, rqr, iqr, eqr any) (data.Frames, error) {
   535		return executeWithHeaders(tctx, query, rqr, iqr, eqr, map[string]string{})
   536	}
   537	
   538	type apiResponse struct {
   539		Status string          `json:"status"`
   540		Data   json.RawMessage `json:"data"`
   541	}
   542	
   543	func toAPIResponse(d any) (*http.Response, error) {
   544		b, err := json.Marshal(d)
   545		if err != nil {
   546			return nil, err
   547		}
   548	
   549		res := apiResponse{
   550			Status: "success",
   551			Data:   json.RawMessage(b),
   552		}
   553	
   554		raw, err := json.Marshal(&res)
   555		if err != nil {
   556			return nil, err
   557		}
   558	
   559		return &http.Response{
   560			StatusCode: 200,
   561			Body:       io.NopCloser(bytes.NewReader(raw)),
   562		}, nil
   563	}
   564	
   565	type testContext struct {
   130	const PER_STEP: &str = "[[1710000000,12],[1710000060,18]]";
   131	
   132	fn check_stats_case(data: &str) {
   133	    let rsp = read_ok(&format!(r#"{{"status":"success","data":{data}}}"#));
   134	    let frames = rsp.frames.as_ref().expect("frames");
   135	    assert_eq!(frames.len(), 1);
   136	    let meta = frames[0].meta.as_ref().expect("meta");
   137	    assert_eq!(meta.stats.len(), 1);
   138	    assert_eq!(meta.stats[0].display_name, "Total queryable samples");
   139	    assert_eq!(meta.stats[0].unit, "");
   140	    assert_eq!(meta.stats[0].value, 30.0);
   141	    let custom = meta.custom.as_ref().expect("custom");
   142	    assert_eq!(custom.str_of("resultType"), "vector");
   143	    let samples = custom.path("stats.samples").expect("samples");
   144	    assert_eq!(samples.get("totalQueryableSamples").and_then(|v| v.as_f64()), Some(30.0));
   145	    assert_eq!(samples.get("totalQueryableSamplesPerStep").and_then(|v| v.as_arr()).map(|a| a.len()), Some(2));
   146	}
   147	
   148	#[test]
   149	fn query_stats_stats_after_result() {
   150	    check_stats_case(&format!(r#"{{"resultType":"vector","result":{INSTANT_RESULT},"stats":{{"samples":{{"totalQueryableSamples":30,"totalQueryableSamplesPerStep":{PER_STEP}}}}}}}"#));
   151	}
   152	
   153	#[test]
   154	fn query_stats_stats_before_result() {
   155	    check_stats_case(&format!(r#"{{"stats":{{"samples":{{"totalQueryableSamples":30,"totalQueryableSamplesPerStep":{PER_STEP}}}}},"resultType":"vector","result":{INSTANT_RESULT}}}"#));
   156	}
   157	
   158	#[test]
   159	fn query_stats_stats_and_result_before_result_type() {
   160	    check_stats_case(&format!(r#"{{"stats":{{"samples":{{"totalQueryableSamples":30,"totalQueryableSamplesPerStep":{PER_STEP}}}}},"result":{INSTANT_RESULT},"resultType":"vector"}}"#));
   161	}
   162	
   163	#[test]
   164	fn query_stats_zero_samples_is_retained_as_a_typed_stat() {
   165	    let rsp = read_ok(r#"{"status":"success","data":{"resultType":"vector","result":[],"stats":{"samples":{"totalQueryableSamples":0}}}}"#);
   166	    let frames = rsp.frames.as_ref().expect("frames");
   167	    assert_eq!(frames.len(), 1);
   168	    let meta = frames[0].meta.as_ref().expect("meta");
   169	    assert_eq!(meta.stats.len(), 1);
   170	    assert_eq!(meta.stats[0].value, 0.0);
   171	}
   172	
   173	#[test]
   174	fn query_stats_range_stats_are_attached_to_the_first_frame_only() {
   175	    let rsp = read_ok(r#"{"status":"success","data":{"resultType":"matrix","result":[{"metric":{"job":"one"},"values":[[1710000000,"1"]]},{"metric":{"job":"two"},"values":[[1710000000,"2"]]}],"stats":{"samples":{"totalQueryableSamples":2}}}}"#);
   176	    let frames = rsp.frames.as_ref().expect("frames");
   177	    assert_eq!(frames.len(), 2);
   178	    assert_eq!(frames[0].meta.as_ref().map(|m| m.stats.len()), Some(1));
   179	    assert!(frames[1].meta.as_ref().map(|m| m.stats.is_empty()).unwrap_or(true));
   180	}
   181	
   182	#[test]
   183	fn query_stats_empty_result_creates_a_metadata_only_frame() {
   184	    let rsp = read_ok(r#"{"status":"success","data":{"resultType":"matrix","result":[],"stats":{"samples":{"totalQueryableSamples":7}}}}"#);
   185	    let frames = rsp.frames.as_ref().expect("frames");
   186	    assert_eq!(frames.len(), 1);
   187	    assert_eq!(frames[0].name, "Query statistics");
   188	    assert!(frames[0].fields.is_empty());
   189	    assert_eq!(frames[0].meta.as_ref().map(|m| m.stats[0].value), Some(7.0));
   190	}
   191	
   192	#[test]

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "rg -n 'created|uid|manager|appUrl|avatarUrl' crates/grafana/src/api/{annotations,users}.rs | head -50; nl -ba crates/grafana/src/api/discovery.rs | sed -n '1,60p'; nl -ba PORTING.md | sed -n '64,80p'; ps -eo pid,ppid,comm,args | rg 'node -e|python -c|grafana-rs|cargo|lust' | cut -c 1-170; ss -ltnp | head -12" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
crates/grafana/src/api/users.rs:26:    r.post("/api/user/stars/dashboard/uid/:uid", Access::NoAnonymous, star_dashboard);
crates/grafana/src/api/users.rs:27:    r.delete("/api/user/stars/dashboard/uid/:uid", Access::NoAnonymous, unstar_dashboard);
crates/grafana/src/api/users.rs:170:/// The user by `:id` (a number or a uid, as upstream's userUIDResolver allows).
crates/grafana/src/api/users.rs:175:        Err(_) => c.app.store.user_by_uid(raw),
crates/grafana/src/api/users.rs:185:    let mut j = Json::obj().with("id", u.id).with("uid", u.uid.as_str()).with("email", u.email.as_str()).with("name", u.name.as_str()).with("login", u.login.as_str()).with("theme", u.theme.as_str());
crates/grafana/src/api/users.rs:196:        .with("createdAt", local_rfc3339(u.created))
crates/grafana/src/api/users.rs:197:        .with("avatarUrl", gravatar_url(&app.cfg, &u.email))
crates/grafana/src/api/users.rs:310:        .with("uid", t.uid.as_str())
crates/grafana/src/api/users.rs:316:        .with("avatarUrl", format!("/avatar/{}", crypto::hex(&crypto::md5(t.email.to_lowercase().as_bytes()))))
crates/grafana/src/api/users.rs:389:                .with("createdAt", local_rfc3339(t.created_at))
crates/grafana/src/api/users.rs:390:                .with("seenAt", local_rfc3339(if t.seen_at > 0 { t.seen_at } else { t.created_at }))
crates/grafana/src/api/users.rs:450:    let uid = c.param("uid").to_string();
crates/grafana/src/api/users.rs:451:    if uid.is_empty() {
crates/grafana/src/api/users.rs:454:    c.app.store.star_add(c.user.org_id, c.user.user_id, &uid).map_err(|_| ApiError::with_trace(500, "Failed to star dashboard"))?;
crates/grafana/src/api/users.rs:459:    let uid = c.param("uid").to_string();
crates/grafana/src/api/users.rs:460:    if uid.is_empty() {
crates/grafana/src/api/users.rs:463:    c.app.store.star_remove(c.user.org_id, c.user.user_id, &uid).map_err(|_| ApiError::with_trace(500, "Failed to unstar dashboard"))?;
crates/grafana/src/api/users.rs:482:                .with("name", format!("user-{}", c.user.uid))
crates/grafana/src/api/users.rs:486:                .with("annotations", Json::obj().with("grafana.app/createdBy", format!("user:{}", c.user.uid))),
crates/grafana/src/api/users.rs:524:    if name != format!("user-{}", c.user.uid) || c.user.uid.is_empty() {
crates/grafana/src/api/users.rs:534:/// `PUT|DELETE .../stars/user-<uid>/update/<group>/<kind>/<name>` (starsREST.Connect).
crates/grafana/src/api/users.rs:537:    if c.user.uid.is_empty() || name != format!("user-{}", c.user.uid) {
crates/grafana/src/api/users.rs:564:    pub home_dashboard_uid: String,
crates/grafana/src/api/users.rs:571:    /// Item names that contributed ("user-<uid>", "team-<uid>", "namespace").
crates/grafana/src/api/users.rs:589:                if !p.home_dashboard_uid.is_empty() {
crates/grafana/src/api/users.rs:590:                    s.set("homeDashboardUID", p.home_dashboard_uid.as_str());
crates/grafana/src/api/users.rs:611:    if !p.home_dashboard_uid.is_empty() {
crates/grafana/src/api/users.rs:612:        s.set("homeDashboardUID", p.home_dashboard_uid.as_str());
crates/grafana/src/api/users.rs:667:                items.push((format!("user-{}", u.uid), row));
crates/grafana/src/api/users.rs:672:                        items.push((format!("team-{}", t.uid), row));
crates/grafana/src/api/users.rs:682:            merge_str(&mut p.home_dashboard_uid, &spec, "homeDashboardUID");
crates/grafana/src/api/users.rs:700:    if p.home_dashboard_uid.is_empty() {
crates/grafana/src/api/users.rs:724:    if !p.home_dashboard_uid.is_empty() {
crates/grafana/src/api/users.rs:725:        spec.set("homeDashboardUID", p.home_dashboard_uid.as_str());
crates/grafana/src/api/users.rs:795:    let created = match st.prefs_get(c.user.org_id, user_id, team_id) {
crates/grafana/src/api/users.rs:796:        Ok(r) => r.created,
crates/grafana/src/api/users.rs:806:        home_dashboard_uid: spec.str_of("homeDashboardUID").to_string(),
crates/grafana/src/api/users.rs:811:        created,
crates/grafana/src/api/users.rs:818:/// The home dashboard uid of an update: `homeDashboardUID`, else the uid of `homeDashboardId`.
crates/grafana/src/api/users.rs:819:fn home_uid(c: &Ctx<'_>, body: &Json) -> Result<Option<String>, ApiError> {
crates/grafana/src/api/users.rs:830:        Ok(d) => Ok(Some(d.uid)),
crates/grafana/src/api/users.rs:848:    if let Some(h) = home_uid(c, body)? {
crates/grafana/src/api/users.rs:875:    if let Some(h) = home_uid(c, body)? {
crates/grafana/src/api/users.rs:899:    let name = format!("user-{}", c.user.uid);
crates/grafana/src/api/users.rs:906:    let name = format!("user-{}", c.user.uid);
crates/grafana/src/api/users.rs:940:        .with("uid", u.uid.as_str())
crates/grafana/src/api/users.rs:945:        .with("avatarUrl", gravatar_url(&app.cfg, &u.email))
crates/grafana/src/api/users.rs:952:        .with("created", local_rfc3339(u.created))
crates/grafana/src/api/users.rs:1135:    Ok(Response::json(200, &Json::obj().with("message", "Organization created").with("orgId", id)))
crates/grafana/src/api/users.rs:1181:fn org_user_json(app: &App, m: &crate::store::OrgMember, uid: &str, disabled: bool, provisioned: bool) -> Json {
     1	//! Kubernetes API discovery: `GET /apis` (APIGroupList), `/apis/<group>` (APIGroup) and `/apis/<group>/<version>`
     2	//! (APIResourceList). The groups, versions and resources are those Grafana 13.3.1 serves, captured from a real
     3	//! instance into `captured/discovery.json`. The bodies are built once, pretty-printed like the apiserver's answers.
     4	use crate::ctx::{Access, Ctx, Routes};
     5	use crate::error::ApiError;
     6	use http::Response;
     7	use json::Json;
     8	use std::sync::OnceLock;
     9	
    10	pub fn routes(r: &mut Routes) {
    11	    r.get("/apis", Access::SignedIn, group_list);
    12	    r.get("/apis/:group", Access::SignedIn, group);
    13	    r.get("/apis/:group/:version", Access::SignedIn, resource_list);
    14	}
    15	
    16	struct Discovery {
    17	    list: String,
    18	    /// (group name, APIGroup body)
    19	    groups: Vec<(String, String)>,
    20	    /// ("group/version", APIResourceList body)
    21	    resources: Vec<(String, String)>,
    22	    version_not_found: String,
    23	}
    24	
    25	fn discovery(port: u16) -> &'static Discovery {
    26	    static D: OnceLock<Discovery> = OnceLock::new();
    27	    D.get_or_init(|| {
    28	        let data = Json::parse(include_str!("captured/discovery.json")).unwrap_or(Json::Null);
    29	        let mut list_groups = Json::arr();
    30	        let mut groups = Vec::new();
    31	        let addr = format!("0.0.0.0:{port}");
    32	        if let Some(gs) = data.get("groups").and_then(|v| v.as_arr()) {
    33	            for g in gs.iter() {
    34	                let name = g.str_of("name");
    35	                let versions = g.get("versions").cloned().unwrap_or(Json::arr());
    36	                let preferred = g.get("preferredVersion").cloned().unwrap_or(Json::obj());
    37	                let one = Json::obj()
    38	                    .with("kind", "APIGroup")
    39	                    .with("apiVersion", "v1")
    40	                    .with("name", name)
    41	                    .with("versions", versions.clone())
    42	                    .with("preferredVersion", preferred.clone());
    43	                groups.push((name.to_string(), one.stringify_pretty("  ")));
    44	                let cidr = Json::obj().with("clientCIDR", "0.0.0.0/0").with("serverAddress", addr.as_str());
    45	                list_groups.push(
    46	                    Json::obj()
    47	                        .with("name", name)
    48	                        .with("versions", versions)
    49	                        .with("preferredVersion", preferred)
    50	                        .with("serverAddressByClientCIDRs", Json::arr().push_item(cidr)),
    51	                );
    52	            }
    53	        }
    54	        let list = Json::obj().with("kind", "APIGroupList").with("groups", list_groups).stringify_pretty("  ");
    55	        let mut resources = Vec::new();
    56	        if let Some(rs) = data.get("resources").and_then(|v| v.as_obj()) {
    57	            for (gv, res) in rs.iter() {
    58	                let body = Json::obj()
    59	                    .with("kind", "APIResourceList")
    60	                    .with("apiVersion", "v1")
    64	- Memory and CPU are the goal: no per-request `format!` on hot paths, no needless clones, stream large responses, keep idle work at zero (no timers that wake when nothing happens).
    65	- No new crates without a real need. Write no automated tests other than the replay ledger.
    66	- Keep each change focused. Do not run `cargo fmt` over files you do not own.
    67	
    68	## Foundations already in place. Read these first
    69	
    70	Built in phase 2. Keep these interfaces stable; small additions are fine, say so in your report.
    71	
    72	### Run it
    73	
    74	```sh
    75	cargo build --release -p server                       # target/release/grafana-rs
    76	GF_AUTH_ANONYMOUS_ENABLED=true GF_AUTH_ANONYMOUS_ORG_ROLE=Admin target/release/grafana-rs   # mode of ui-tour-v1
    77	target/release/grafana-rs                             # anonymous off: mode of api-flows-v1 (login-v1)
    78	```
    79	
    80	- Defaults: port 3400, Grafana home `upstream-bin/grafana-13.3.1` (override: `GF_PATHS_HOME`), data dir `./data`
      1       0 codex           codex-linux-sandbox --sandbox-policy-cwd /home/kamrot/code/grafana-rs --command-cwd /home/kamrot/code/grafana-rs --permission-profile {"ty
      2       1 zsh             /usr/bin/zsh -c rg -n 'created|uid|manager|appUrl|avatarUrl' crates/grafana/src/api/{annotations,users}.rs | head -50; nl -ba crates/grafa
     22       2 rg              rg node -e|python -c|grafana-rs|cargo|lust
Cannot open netlink socket: Operation not permitted
State Recv-Q Send-Q Local Address:Port Peer Address:PortProcess

hook: PostToolUse
hook: PostToolUse Completed
codex
I confirmed false matches with the replay function: it accepts different large integer IDs, reversed numeric object-key order, changed HTML boot-data values, and a wrong OFREP flag with the wrong value type. It rejects ordinary null-versus-missing and reordered arrays. All 72 copied Prometheus fixtures match their upstream files byte for byte.

hook: PreToolUse
hook: PreToolUse Completed
exec
/usr/bin/zsh -lc "python -c 'import json;from pathlib import Path
for f in [\"ui-tour-v1.jsonl\",\"api-flows-v1.jsonl\"]:
 rows=[json.loads(x) for x in (Path(\"tests-data/recordings\")/f).read_text().splitlines() if x]; print(f,\"authorization headers\",sum(\"authorization\" in r.get(\"reqHeaders\",{}) for r in rows),\"user-agent headers\",sum(\"user-agent\" in r.get(\"reqHeaders\",{}) for r in rows))'
nl -ba crates/grafana/src/api/annotations.rs | sed -n '32,64p'
nl -ba PORTING.md | sed -n '58,65p'
nl -ba upstream-prometheus/pkg/promlib/resource/allowlist_test.go | sed -n '1,65p'
nl -ba /home/kamrot/code/baileys-rs/PORTING-PLAYBOOK.md | sed -n '/Phase 6/,+5p'" in /home/kamrot/code/grafana-rs
 succeeded in 0ms:
ui-tour-v1.jsonl authorization headers 0 user-agent headers 0
api-flows-v1.jsonl authorization headers 0 user-agent headers 77
    32	    }
    33	    if let Some(u) = cache.iter().find(|u| u.0 == id) {
    34	        return Some((u.1.clone(), u.2.clone(), u.3.clone()));
    35	    }
    36	    match c.app.store.user_by_id(id) {
    37	        Ok(u) => {
    38	            cache.push((id, u.uid.clone(), u.login.clone(), u.email.clone()));
    39	            Some((u.uid, u.login, u.email))
    40	        }
    41	        Err(_) => None,
    42	    }
    43	}
    44	
    45	/// `<sub>/avatar/<sha256 of the lower-case email>` (dtos.GetGravatarUrl).
    46	fn avatar_url(c: &Ctx<'_>, email: &str) -> String {
    47	    if c.app.cfg.get_bool("security", "disable_gravatar") {
    48	        return format!("{}/public/img/user_profile.png", c.app.cfg.app_sub_url);
    49	    }
    50	    let e = email.trim().to_lowercase();
    51	    if e.is_empty() {
    52	        return String::new();
    53	    }
    54	    format!("{}/avatar/{}", c.app.cfg.app_sub_url, crypto::hex(&crypto::sha256(e.as_bytes())))
    55	}
    56	
    57	/// The ItemDTO of an annotation (omitempty fields left out).
    58	fn item_dto(c: &Ctx<'_>, cache: &mut UserCache, a: &Annotation) -> Json {
    59	    let mut o = Json::obj().with("id", a.id);
    60	    if a.alert_id != 0 {
    61	        o.set("alertId", a.alert_id);
    62	    }
    63	    if a.dashboard_id != 0 {
    64	        o.set("dashboardId", a.dashboard_id);
    58	
    59	## Rules
    60	
    61	- **The recording is the contract.** For each route, match the status code, the JSON keys and their order, value types, and the values the UI reads. Read the Go handler in `upstream/pkg/...` for behavior the recording does not show (errors, permissions, edge cases), and port that behavior too.
    62	- Keep Grafana's error messages, status codes, and JSON field names. The UI shows them.
    63	- Never weaken a check in `scripts/replay.mjs` to get a match. If a field cannot match (for example a timestamp or a random uid), add it to the named ignore list in `tests-data/replay-ignore.json` with the reason.
    64	- Memory and CPU are the goal: no per-request `format!` on hot paths, no needless clones, stream large responses, keep idle work at zero (no timers that wake when nothing happens).
    65	- No new crates without a real need. Write no automated tests other than the replay ledger.
     1	package resource
     2	
     3	import "testing"
     4	
     5	func TestIsAllowedResourcePath(t *testing.T) {
     6		allowed := []string{
     7			"api/v1/query",
     8			"/api/v1/query",
     9			"api/v1/query_range",
    10			"api/v1/query_exemplars",
    11			"api/v1/series",
    12			"api/v1/labels",
    13			"api/v1/label/instance/values",
    14			"api/v1/label/__name__/values",
    15			"api/v1/metadata",
    16			"api/v1/rules",
    17			"api/v1/status/buildinfo",
    18			"api/v1/search/metric_names",
    19			"api/v1/search/label_names",
    20			"api/v1/search/label_values",
    21		}
    22		for _, path := range allowed {
    23			t.Run(path, func(t *testing.T) {
    24				if !isAllowedResourcePath(path) {
    25					t.Errorf("expected %q to be allowed", path)
    26				}
    27			})
    28		}
    29	
    30		rejected := []string{
    31			"",
    32			"/",
    33			"-/reload",
    34			"-/quit",
    35			"federate",
    36			"api/v1/admin/tsdb/delete_series",
    37			"api/v1/search/",
    38			"api/v1/search/unknown_endpoint",
    39			"api/v1/label//values",
    40			"api/v1/label/foo/bar/values",
    41			// dot-segments that path.Join would otherwise resolve, potentially
    42			// escaping a base URL sub-path used for isolation.
    43			"api/v1/../admin",
    44			"api/v1/label/../../-/reload/values",
    45			"./api/v1/labels",
    46			// double-encoded escapes.
    47			"api%2Fv1%2Flabels",
    48			"api/v1/labels%2F..%2F..%2F-%2Freload",
    49		}
    50		for _, path := range rejected {
    51			t.Run(path, func(t *testing.T) {
    52				if isAllowedResourcePath(path) {
    53					t.Errorf("expected %q to be rejected", path)
    54				}
    55			})
    56		}
    57	}
    78	## Phase 6: independent review
    79	
    80	Get a review that is not anchored on your own reading: a second agent or a different model (here GPT Sol through `codex exec`, read-only). Ask it to compare each Rust test with its upstream test and find weakened or changed tests. It found tests that had become sequential or only checked setup, and those went into the report as caveats instead of being hidden.
    81	
    82	## Phase 7: the real world
    83	

hook: PostToolUse
hook: PostToolUse Completed
codex
**The replay score does not prove parity. I would not accept it as merge evidence.** Confidence: high for the confirmed comparison gaps; medium for implementation coverage. This was read-only. No server or build was started.

Below, rule numbers are the 1-based positions in [replay-ignore.json](/home/kamrot/code/grafana-rs/tests-data/replay-ignore.json:2). Equivalent rules are grouped; all 90 are covered.

- **20–21, OFREP:** Ignoring `flags.*.key` and accepting any `value` hides missing flags, wrong flag names, wrong types, and reversed values. **Yes, sort both arrays by `key` in `replay.mjs`**, reject duplicate keys, then compare complete entries. Keep only the exact, declared `dashboardNewLayouts=false` exception. [Source:134](/home/kamrot/code/grafana-rs/tests-data/replay-ignore.json:134).
- **22–23:** `any` accepts missing, true, null, or arbitrary layout-toggle values. Require actual `false`; limit rule 22’s `GET /**` to boot-data responses. **24:** Require `liveEnabled=false`, rather than accepting either Boolean. [Source:146](/home/kamrot/code/grafana-rs/tests-data/replay-ignore.json:146).
- **25:** Ignoring all `appUrl` text hides a wrong host, scheme, or subpath. Replace only the known recording origin. **26:** Ignoring plugin IDs hides wrong or duplicate plugins. Sort complete entries by ID and compare them. [Source:171](/home/kamrot/code/grafana-rs/tests-data/replay-ignore.json:171).
- **36–37:** Ignoring scores hides the replacement relevance formula. Compare upstream scores, or explicitly approve and check defined bounds and ranking. **38:** Ignoring `queryCost` hides the invented `total*100` result; compare upstream behavior or define an approved cost contract. [Implementation:279](/home/kamrot/code/grafana-rs/crates/grafana/src/api/search.rs:279).
- **39–40:** Ignoring manager names hides incorrect write attribution. Replay the recorded User-Agent and compare its expected manager. The scorer deliberately removes that header. [replay:186](/home/kamrot/code/grafana-rs/scripts/replay.mjs:186).
- **89:** Ignoring `avatarUrl` hides a broken hash, path, or user association. Calculate the expected URL from the target user’s email and configuration. [Implementation:45](/home/kamrot/code/grafana-rs/crates/grafana/src/api/annotations.rs:45).
- **1,2,7,14,16,27,30,33,48,56,65,75:** UID mapping or unrestricted values can accept empty, malformed, duplicate, or changing identities. Validate each UID format; use separate, one-to-one maps for users, dashboards, queries, and Kubernetes objects. Preserve explicit request UIDs; map dashboard names only when generated. [replay:101](/home/kamrot/code/grafana-rs/scripts/replay.mjs:101).
- **5,10,41,44,47,51,55,57,58,61,63,68,78,83,84,85:** Ignoring IDs hides wrong dashboard/folder associations and nonzero root-folder IDs. Map generated positive IDs consistently across responses; compare zero sentinels exactly. [Example:381](/home/kamrot/code/grafana-rs/tests-data/replay-ignore.json:381).
- **12,13,53,54,70,71,80,81,88:** Ignoring author identities hides attribution to another user. Compare the mapped user UID, preserving the `user:` prefix where required. [Example:78](/home/kamrot/code/grafana-rs/tests-data/replay-ignore.json:78).
- **3,8,17,49,66,74,76:** Ignoring resource versions hides invalid or unstable versions. Require the expected numeric format, stable reads, and increases after writes. [Example:16](/home/kamrot/code/grafana-rs/tests-data/replay-ignore.json:16).
- **4,6,9,11,15,18,19,28,29,31,32,34,35,42,43,45,46,50,52,59,60,62,64,67,69,72,73,77,79,82,86,87,90:** These time rules accept any value of the same type, including invalid dates or wrong units. Check format, units, plausible write time, creation-time stability, and update order. [Example:22](/home/kamrot/code/grafana-rs/tests-data/replay-ignore.json:22).

The scorer has further gaps against [PORTING.md:61](/home/kamrot/code/grafana-rs/PORTING.md:61):

- HTML comparison discards **all scalar values**, and checks only four boot-data sections. Wrong permissions, URLs, assets, and navigation targets can pass. [replay:202](/home/kamrot/code/grafana-rs/scripts/replay.mjs:202).
- `JSON.parse` rounds large integers and loses duplicate keys; `Object.keys` normalizes numeric-key order. I confirmed false matches for distinct integers and reversed numeric keys. Ordinary types, null versus missing, and array order were correctly rejected. [replay:116](/home/kamrot/code/grafana-rs/scripts/replay.mjs:116).
- `value` also permits different array lengths. ID replacement changes arbitrary substrings globally. [replay:58](/home/kamrot/code/grafana-rs/scripts/replay.mjs:58), [110](/home/kamrot/code/grafana-rs/scripts/replay.mjs:110).
- Binary placeholders check status only; response Content-Type and redirect Location are unchecked. Twenty-five websocket rows are skipped; `--only` exclusions are absent from the skipped count. [replay:181](/home/kamrot/code/grafana-rs/scripts/replay.mjs:181), [206](/home/kamrot/code/grafana-rs/scripts/replay.mjs:206).

I sampled **48 paired cases across all nine Rust test files**: client 6, request 7, response 5, resource 3, settings 8, interval 4, models 4, framing 6, converter statistics 3, samplers 2. All **72 copied fixtures were byte-identical** to upstream.

- `new_resource` only checks its fixture URL; Go calls the resource constructor. [Rust:141](/home/kamrot/code/grafana-rs/crates/grafana/tests/promlib_resource.rs:141), [Go:80](/home/kamrot/code/grafana-rs/upstream-prometheus/pkg/promlib/resource/resource_test.go:80).
- Six client cases stop at request construction; Go invokes its mock transport. The GET case also supplies a leading slash that Go omits. [Rust:45](/home/kamrot/code/grafana-rs/crates/grafana/tests/promlib_client.rs:45), [Go:93](/home/kamrot/code/grafana-rs/upstream-prometheus/pkg/promlib/client/client_test.go:93).
- The lenient-test omission reason is false: omitted tests check conversions and strict-field rejection, not only instrumentation. The claimed settings coverage lacks these inputs. [Ledger:13](/home/kamrot/code/grafana-rs/tests-data/not-ported.md:13), [Go:22](/home/kamrot/code/grafana-rs/upstream-prometheus/pkg/promlib/models/lenient_test.go:22).
- “SDK wiring” also understates omitted health-failure, Mimir-detection, forced-GET, and header-forwarding checks. The two recorded health calls cover successful Prometheus responses only. [Ledger:16](/home/kamrot/code/grafana-rs/tests-data/not-ported.md:16), [Go:97](/home/kamrot/code/grafana-rs/upstream-prometheus/pkg/promlib/heuristics_test.go:97).

I found **no server branches for `bench1`, `prom1`, or `gsshcd`**, or recording-port comparisons. `gsshcd` is replay seed data. Recorded constants remain in OFREP defaults, plugin order, discovery, permissions, and Advisor metadata; the latter serves captured UIDs and times. These are not evidence of hidden UID-specific branches. [seed:170](/home/kamrot/code/grafana-rs/scripts/replay.mjs:170), [OFREP:100](/home/kamrot/code/grafana-rs/crates/grafana/src/api/ofrep.rs:100), [boot:433](/home/kamrot/code/grafana-rs/crates/grafana/src/api/boot.rs:433), [Advisor:110](/home/kamrot/code/grafana-rs/crates/grafana/src/api/stubs.rs:110).
hook: Stop
hook: Stop Completed
tokens used
183,524
**The replay score does not prove parity. I would not accept it as merge evidence.** Confidence: high for the confirmed comparison gaps; medium for implementation coverage. This was read-only. No server or build was started.

Below, rule numbers are the 1-based positions in [replay-ignore.json](/home/kamrot/code/grafana-rs/tests-data/replay-ignore.json:2). Equivalent rules are grouped; all 90 are covered.

- **20–21, OFREP:** Ignoring `flags.*.key` and accepting any `value` hides missing flags, wrong flag names, wrong types, and reversed values. **Yes, sort both arrays by `key` in `replay.mjs`**, reject duplicate keys, then compare complete entries. Keep only the exact, declared `dashboardNewLayouts=false` exception. [Source:134](/home/kamrot/code/grafana-rs/tests-data/replay-ignore.json:134).
- **22–23:** `any` accepts missing, true, null, or arbitrary layout-toggle values. Require actual `false`; limit rule 22’s `GET /**` to boot-data responses. **24:** Require `liveEnabled=false`, rather than accepting either Boolean. [Source:146](/home/kamrot/code/grafana-rs/tests-data/replay-ignore.json:146).
- **25:** Ignoring all `appUrl` text hides a wrong host, scheme, or subpath. Replace only the known recording origin. **26:** Ignoring plugin IDs hides wrong or duplicate plugins. Sort complete entries by ID and compare them. [Source:171](/home/kamrot/code/grafana-rs/tests-data/replay-ignore.json:171).
- **36–37:** Ignoring scores hides the replacement relevance formula. Compare upstream scores, or explicitly approve and check defined bounds and ranking. **38:** Ignoring `queryCost` hides the invented `total*100` result; compare upstream behavior or define an approved cost contract. [Implementation:279](/home/kamrot/code/grafana-rs/crates/grafana/src/api/search.rs:279).
- **39–40:** Ignoring manager names hides incorrect write attribution. Replay the recorded User-Agent and compare its expected manager. The scorer deliberately removes that header. [replay:186](/home/kamrot/code/grafana-rs/scripts/replay.mjs:186).
- **89:** Ignoring `avatarUrl` hides a broken hash, path, or user association. Calculate the expected URL from the target user’s email and configuration. [Implementation:45](/home/kamrot/code/grafana-rs/crates/grafana/src/api/annotations.rs:45).
- **1,2,7,14,16,27,30,33,48,56,65,75:** UID mapping or unrestricted values can accept empty, malformed, duplicate, or changing identities. Validate each UID format; use separate, one-to-one maps for users, dashboards, queries, and Kubernetes objects. Preserve explicit request UIDs; map dashboard names only when generated. [replay:101](/home/kamrot/code/grafana-rs/scripts/replay.mjs:101).
- **5,10,41,44,47,51,55,57,58,61,63,68,78,83,84,85:** Ignoring IDs hides wrong dashboard/folder associations and nonzero root-folder IDs. Map generated positive IDs consistently across responses; compare zero sentinels exactly. [Example:381](/home/kamrot/code/grafana-rs/tests-data/replay-ignore.json:381).
- **12,13,53,54,70,71,80,81,88:** Ignoring author identities hides attribution to another user. Compare the mapped user UID, preserving the `user:` prefix where required. [Example:78](/home/kamrot/code/grafana-rs/tests-data/replay-ignore.json:78).
- **3,8,17,49,66,74,76:** Ignoring resource versions hides invalid or unstable versions. Require the expected numeric format, stable reads, and increases after writes. [Example:16](/home/kamrot/code/grafana-rs/tests-data/replay-ignore.json:16).
- **4,6,9,11,15,18,19,28,29,31,32,34,35,42,43,45,46,50,52,59,60,62,64,67,69,72,73,77,79,82,86,87,90:** These time rules accept any value of the same type, including invalid dates or wrong units. Check format, units, plausible write time, creation-time stability, and update order. [Example:22](/home/kamrot/code/grafana-rs/tests-data/replay-ignore.json:22).

The scorer has further gaps against [PORTING.md:61](/home/kamrot/code/grafana-rs/PORTING.md:61):

- HTML comparison discards **all scalar values**, and checks only four boot-data sections. Wrong permissions, URLs, assets, and navigation targets can pass. [replay:202](/home/kamrot/code/grafana-rs/scripts/replay.mjs:202).
- `JSON.parse` rounds large integers and loses duplicate keys; `Object.keys` normalizes numeric-key order. I confirmed false matches for distinct integers and reversed numeric keys. Ordinary types, null versus missing, and array order were correctly rejected. [replay:116](/home/kamrot/code/grafana-rs/scripts/replay.mjs:116).
- `value` also permits different array lengths. ID replacement changes arbitrary substrings globally. [replay:58](/home/kamrot/code/grafana-rs/scripts/replay.mjs:58), [110](/home/kamrot/code/grafana-rs/scripts/replay.mjs:110).
- Binary placeholders check status only; response Content-Type and redirect Location are unchecked. Twenty-five websocket rows are skipped; `--only` exclusions are absent from the skipped count. [replay:181](/home/kamrot/code/grafana-rs/scripts/replay.mjs:181), [206](/home/kamrot/code/grafana-rs/scripts/replay.mjs:206).

I sampled **48 paired cases across all nine Rust test files**: client 6, request 7, response 5, resource 3, settings 8, interval 4, models 4, framing 6, converter statistics 3, samplers 2. All **72 copied fixtures were byte-identical** to upstream.

- `new_resource` only checks its fixture URL; Go calls the resource constructor. [Rust:141](/home/kamrot/code/grafana-rs/crates/grafana/tests/promlib_resource.rs:141), [Go:80](/home/kamrot/code/grafana-rs/upstream-prometheus/pkg/promlib/resource/resource_test.go:80).
- Six client cases stop at request construction; Go invokes its mock transport. The GET case also supplies a leading slash that Go omits. [Rust:45](/home/kamrot/code/grafana-rs/crates/grafana/tests/promlib_client.rs:45), [Go:93](/home/kamrot/code/grafana-rs/upstream-prometheus/pkg/promlib/client/client_test.go:93).
- The lenient-test omission reason is false: omitted tests check conversions and strict-field rejection, not only instrumentation. The claimed settings coverage lacks these inputs. [Ledger:13](/home/kamrot/code/grafana-rs/tests-data/not-ported.md:13), [Go:22](/home/kamrot/code/grafana-rs/upstream-prometheus/pkg/promlib/models/lenient_test.go:22).
- “SDK wiring” also understates omitted health-failure, Mimir-detection, forced-GET, and header-forwarding checks. The two recorded health calls cover successful Prometheus responses only. [Ledger:16](/home/kamrot/code/grafana-rs/tests-data/not-ported.md:16), [Go:97](/home/kamrot/code/grafana-rs/upstream-prometheus/pkg/promlib/heuristics_test.go:97).

I found **no server branches for `bench1`, `prom1`, or `gsshcd`**, or recording-port comparisons. `gsshcd` is replay seed data. Recorded constants remain in OFREP defaults, plugin order, discovery, permissions, and Advisor metadata; the latter serves captured UIDs and times. These are not evidence of hidden UID-specific branches. [seed:170](/home/kamrot/code/grafana-rs/scripts/replay.mjs:170), [OFREP:100](/home/kamrot/code/grafana-rs/crates/grafana/src/api/ofrep.rs:100), [boot:433](/home/kamrot/code/grafana-rs/crates/grafana/src/api/boot.rs:433), [Advisor:110](/home/kamrot/code/grafana-rs/crates/grafana/src/api/stubs.rs:110).
