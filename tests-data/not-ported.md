# Upstream promlib tests not ported

Source: `upstream-prometheus/pkg/promlib` (grafana-prometheus-datasource v13.2.6). Ported tests live in
`crates/grafana/tests/promlib_*.rs` (one `#[test]` per upstream case) with the golden files copied to
`tests-data/promlib/`. These upstream cases are not ported:

| Upstream test | Cases | Reason |
|---|---|---|
| `models/query_test.go` TestParseWithAdhocFilters, TestParseWithScopes, TestParseWithScopesAndAdhocFilters, TestParseWithGroupByKeys, TestParseComplexScenariosWithFilters | 14 | They rewrite PromQL (`ApplyFiltersAndGroupBy`), which needs the Prometheus PromQL parser and printer. grafana-rs has no PromQL parser: a query with scopes, ad hoc filters or group-by keys gets an error response instead of an unfiltered result. |
| `models/scope_test.go` (all) | 7 functions | Same: PromQL rewriting. |
| `resource/range_modifiers_test.go`, `resource/resource_test.go` TestResource_GetSuggestions* | 5 functions | The `suggestions` resource extracts metric names with the PromQL parser. grafana-rs answers `suggestions` with 500 "Failed to call resource". |
| `models/query_test.go` TestQueryTypeDefinitions | 1 | Go schema code generation (`schemabuilder`), nothing to port. |
| `models/lenient_test.go` TestPointerPropertiesAreAccountedFor | 1 | It walks the Go struct by reflection to find pointer members that `clearDroppedPointers` must handle. In Rust the type system lists the members; `series_limit` is the only `Option`. The other five functions are ported in `promlib_lenient.rs`. |
| `resource/search_test.go`, `utils/utils_test.go`, `resource/resource_test.go` (gzip, zstd, framing headers, HTTP boundary), `resource/transport_error_test.go` | 21 functions | They test the Go HTTP transport: streaming NDJSON search bodies, gzip and zstd decoding, context cancellation. grafana-rs asks Prometheus for plain bodies and buffers search answers. |
| `admission_handler_test.go` (Test_ValidateAdmission, Test_MutateAdmission, Test_getBadRequest), `instrumentation/instrumentation_test.go` | 5 functions | Kubernetes admission hooks and Prometheus metrics of the plugin process; grafana-rs has neither. |
| `library_test.go` TestService ("creates correct request", "no extendOptions function provided", "extendOptions function provided", "suggest resource", "search resource uses streaming handler") | 5 | "creates correct request" requires the outgoing headers to be exactly `Accept-Encoding: gzip` and `Content-Type`; grafana-rs does not ask Prometheus for gzip (it reads plain bodies), so this header set differs by design. The method, body and URL of that call are covered by `promlib_client.rs` (query_resource cases). The extendOptions cases test the SDK instance manager hook, which grafana-rs does not have. "suggest resource" needs the PromQL parser (see above). The search case tests the NDJSON streaming handler (see above). |
| `client/transport_test.go` "creates correct options object": the assertion `len(opts.Middlewares) == 1` | 1 assertion | grafana-rs has no SDK middleware objects: the custom query parameters run in `Instance::apply_middleware`, the force-GET middleware in `Instance::round_trip`. The header part of this case is ported in `promlib_health.rs`, as is the ForwardHTTPHeaders case (as the list of Grafana headers that grafana-rs forwards). |
| `middleware/force_http_get_test.go` "Name should be correct" | 1 | It checks the SDK middleware name `force-http-get`; there is no middleware object to name. "Should force GET method" is ported in `promlib_health.rs`. |
| `querydata/framing_bench_test.go`, `converter/prom_bench_test.go` | benchmarks | Benchmarks, not checks. |

Golden files copied but not used by any upstream test: `converter/testdata/prom-exemplars.json`,
`prom-exemplars-frame.jsonc`, `prom-exemplars-a-frame.json`, `prom-exemplars-b-frame.json`,
`loki-streams-structured-metadata.json` and its frame (upstream's `files` list does not name them).
