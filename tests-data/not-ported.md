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
| `models/lenient_test.go` | 6 functions | Not in the ported set: they check the metrics and logs written for each lenient conversion (instrumentation that grafana-rs does not have). The lenient parsing itself is ported in `promlib::settings` and covered by `promlib_settings.rs`. |
| `resource/search_test.go`, `utils/utils_test.go`, `resource/resource_test.go` (gzip, zstd, framing headers, HTTP boundary), `resource/transport_error_test.go` | 21 functions | They test the Go HTTP transport: streaming NDJSON search bodies, gzip and zstd decoding, context cancellation. grafana-rs asks Prometheus for plain bodies and buffers search answers. |
| `admission_handler_test.go` (Test_ValidateAdmission, Test_MutateAdmission, Test_getBadRequest), `instrumentation/instrumentation_test.go` | 5 functions | Kubernetes admission hooks and Prometheus metrics of the plugin process; grafana-rs has neither. |
| `client/transport_test.go`, `middleware/force_http_get_test.go`, `library_test.go`, `healthcheck_test.go`, `heuristics_test.go` | 5 functions | Plugin SDK wiring (instance manager, middleware objects, admission and conversion handlers, Prometheus metrics). Health check and heuristics behavior is covered by the replay rows `GET /api/datasources/uid/*/health`. |
| `querydata/framing_bench_test.go`, `converter/prom_bench_test.go` | benchmarks | Benchmarks, not checks. |

Golden files copied but not used by any upstream test: `converter/testdata/prom-exemplars.json`,
`prom-exemplars-frame.jsonc`, `prom-exemplars-a-frame.json`, `prom-exemplars-b-frame.json`,
`loki-streams-structured-metadata.json` and its frame (upstream's `files` list does not name them).
