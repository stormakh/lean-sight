//! Port of promlib `models/prometheus_interval_test.go` (TestCalculatePrometheusInterval,
//! TestCalculateRateInterval) and `intervalv2/intervalv2_test.go` (TestIntervalCalculator_Calculate,
//! TestIntervalCalculator_CalculateSafeInterval). Go's intervalv2 returns an `Interval` with a `Text`; the Rust
//! functions return the duration, so the text is `format_interval(value)`.
use grafana::promlib::models::{DataQuery, DAY, HOUR, MIN, MS, SEC, calculate_interval, calculate_prometheus_interval, calculate_rate_interval, calculate_safe_interval, format_interval};

fn now() -> i64 {
    grafana::util::now_ms() * MS + 123_456 // not aligned to anything, like time.Now()
}

/// One TestCalculatePrometheusInterval case: (queryInterval, dsScrapeInterval, intervalMs, intervalFactor,
/// time range length, query Interval, MaxDataPoints) -> want (None: wantErr).
#[allow(clippy::too_many_arguments)]
fn prom_interval(query_interval: &str, ds_scrape_interval: &str, interval_ms: i64, interval_factor: i64, range: i64, interval: i64, max_data_points: i64, want: Option<i64>) {
    let n = now();
    let q = DataQuery { from: n, to: n + range, interval, max_data_points, ..Default::default() };
    let got = calculate_prometheus_interval(query_interval, ds_scrape_interval, interval_ms, interval_factor, &q);
    match want {
        None => assert!(got.is_err(), "want error, got {got:?}"),
        Some(w) => assert_eq!(got.expect("no error"), w),
    }
}

// TestCalculatePrometheusInterval
#[test]
fn prom_interval_use_min_step_value_when_its_bigger_than_the_calculated_value() {
    prom_interval("10m", "", 300000, 1, 48 * HOUR, 5 * MIN, 761, Some(10 * MIN));
}
#[test]
fn prom_interval_use_calculated_value_when_min_step_value_is_smaller() {
    prom_interval("2m", "", 300000, 1, 48 * HOUR, 5 * MIN, 761, Some(5 * MIN));
}
#[test]
fn prom_interval_min_step_2m_with_900000_interval_ms() {
    prom_interval("2m", "", 900000, 1, 48 * HOUR, 15 * MIN, 175, Some(15 * MIN));
}
#[test]
fn prom_interval_with_step_parameter() {
    prom_interval("", "15s", 0, 1, 12 * HOUR, MIN, 0, Some(30 * SEC));
}
#[test]
fn prom_interval_without_step_parameter() {
    prom_interval("", "15s", 0, 1, HOUR, MIN, 0, Some(15 * SEC));
}
#[test]
fn prom_interval_with_high_interval_factor() {
    prom_interval("", "15s", 0, 10, 48 * HOUR, MIN, 0, Some(20 * MIN));
}
#[test]
fn prom_interval_with_low_interval_factor() {
    prom_interval("", "15s", 0, 1, 48 * HOUR, MIN, 0, Some(2 * MIN));
}
#[test]
fn prom_interval_with_specified_scrape_interval_in_data_source() {
    prom_interval("", "240s", 0, 1, 48 * HOUR, MIN, 0, Some(4 * MIN));
}
#[test]
fn prom_interval_with_zero_interval_factor_defaults_to_1() {
    prom_interval("", "15s", 0, 0, HOUR, MIN, 0, Some(15 * SEC));
}
#[test]
fn prom_interval_with_interval_variable() {
    prom_interval("$__interval", "15s", 60000, 1, 48 * HOUR, MIN, 0, Some(120 * SEC));
}
#[test]
fn prom_interval_with_braced_interval_variable() {
    prom_interval("${__interval}", "15s", 60000, 1, 48 * HOUR, MIN, 0, Some(120 * SEC));
}
#[test]
fn prom_interval_with_braced_interval_variable_and_explicit_interval() {
    prom_interval("1m", "15s", 60000, 1, 48 * HOUR, MIN, 0, Some(2 * MIN));
}
#[test]
fn prom_interval_with_rate_interval_variable() {
    prom_interval("$__rate_interval", "30s", 100000, 1, 2 * DAY, 100 * SEC, 12384, Some(130 * SEC));
}
#[test]
fn prom_interval_with_braced_rate_interval_variable() {
    prom_interval("${__rate_interval}", "30s", 100000, 1, 2 * DAY, 100 * SEC, 12384, Some(130 * SEC));
}
#[test]
fn prom_interval_interval_ms_100s_min_step_override_150s_and_scrape_interval_30s() {
    prom_interval("150s", "30s", 100000, 1, 2 * DAY, 100 * SEC, 12384, Some(150 * SEC));
}
#[test]
fn prom_interval_interval_ms_120s_min_step_override_150s_and_ds_scrape_interval_30s() {
    prom_interval("150s", "30s", 120000, 1, 2 * DAY, 120 * SEC, 12384, Some(150 * SEC));
}
#[test]
fn prom_interval_interval_ms_120s_min_step_auto_and_ds_scrape_interval_30s() {
    prom_interval("120s", "30s", 120000, 1, 2 * DAY, 120 * SEC, 12384, Some(120 * SEC));
}
#[test]
fn prom_interval_interval_and_min_step_automatic_ds_scrape_30s_range_1_hour() {
    prom_interval("30s", "30s", 30000, 1, HOUR, 30 * SEC, 12384, Some(30 * SEC));
}
#[test]
fn prom_interval_min_step_rate_interval_ds_scrape_30s_range_1_hour() {
    prom_interval("$__rate_interval", "30s", 30000, 1, HOUR, 30 * SEC, 12384, Some(2 * MIN));
}
#[test]
fn prom_interval_min_step_rate_interval_ds_scrape_30s_range_2_days() {
    prom_interval("$__rate_interval", "30s", 120000, 1, 2 * DAY, 120 * SEC, 12384, Some(150 * SEC));
}
#[test]
fn prom_interval_min_step_interval_ds_scrape_15s_range_2_days() {
    prom_interval("$__interval", "15s", 120000, 1, 2 * DAY, 120 * SEC, 12384, Some(120 * SEC));
}
#[test]
fn prom_interval_with_empty_ds_scrape_interval_defaults_to_15s() {
    prom_interval("", "", 0, 1, HOUR, MIN, 0, Some(15 * SEC));
}
#[test]
fn prom_interval_with_very_short_time_range() {
    prom_interval("", "15s", 0, 1, MIN, MIN, 0, Some(15 * SEC));
}
#[test]
fn prom_interval_with_very_long_time_range() {
    prom_interval("", "15s", 0, 1, 30 * DAY, MIN, 0, Some(30 * MIN));
}
#[test]
fn prom_interval_with_manual_interval_override() {
    prom_interval("5m", "15s", 0, 1, 48 * HOUR, MIN, 0, Some(5 * MIN));
}
#[test]
fn prom_interval_min_step_auto_ds_scrape_30s_range_1_hour() {
    prom_interval("", "30s", 30000, 1, HOUR, 30 * SEC, 1613, Some(30 * SEC));
}
#[test]
fn prom_interval_min_step_auto_ds_scrape_15s_range_5_minutes() {
    prom_interval("", "15s", 15000, 1, 5 * MIN, 15 * SEC, 1055, Some(15 * SEC));
}
#[test]
fn prom_interval_with_interval_ms_variable() {
    prom_interval("$__interval_ms", "15s", 60000, 1, 48 * HOUR, MIN, 0, Some(120 * SEC));
}
#[test]
fn prom_interval_with_braced_interval_ms_variable() {
    prom_interval("${__interval_ms}", "15s", 60000, 1, 48 * HOUR, MIN, 0, Some(120 * SEC));
}
#[test]
fn prom_interval_with_max_data_points_zero() {
    prom_interval("", "15s", 0, 1, HOUR, MIN, 0, Some(15 * SEC));
}
#[test]
fn prom_interval_with_negative_interval_factor() {
    prom_interval("", "15s", 0, -5, 48 * HOUR, MIN, 0, Some(-10 * MIN));
}
#[test]
fn prom_interval_with_invalid_interval_string_that_fails_parsing() {
    prom_interval("invalid-interval", "15s", 0, 1, 48 * HOUR, MIN, 0, None);
}
#[test]
fn prom_interval_with_very_small_max_data_points() {
    prom_interval("", "15s", 0, 1, HOUR, MIN, 10, Some(5 * MIN));
}
#[test]
fn prom_interval_when_safe_interval_is_larger_than_calculated_interval() {
    prom_interval("", "15s", 0, 1, HOUR, MIN, 10000, Some(15 * SEC));
}

// TestCalculateRateInterval
#[test]
fn rate_interval_30s_scrape_100s_query() {
    assert_eq!(calculate_rate_interval(100 * SEC, "30s"), 130 * SEC);
}
#[test]
fn rate_interval_30s_scrape_60s_query() {
    assert_eq!(calculate_rate_interval(60 * SEC, "30s"), 120 * SEC);
}
#[test]
fn rate_interval_15s_scrape_30s_query() {
    assert_eq!(calculate_rate_interval(30 * SEC, "15s"), 60 * SEC);
}
#[test]
fn rate_interval_empty_scrape_interval_defaults_to_15s() {
    assert_eq!(calculate_rate_interval(30 * SEC, ""), 60 * SEC);
}
#[test]
fn rate_interval_1m_scrape_5m_query() {
    assert_eq!(calculate_rate_interval(5 * MIN, "1m"), 6 * MIN);
}
#[test]
fn rate_interval_1m_scrape_2m_query() {
    assert_eq!(calculate_rate_interval(2 * MIN, "1m"), 4 * MIN);
}
#[test]
fn rate_interval_zero_query_30s_scrape() {
    assert_eq!(calculate_rate_interval(0, "30s"), 120 * SEC);
}
#[test]
fn rate_interval_very_small_query_10s_30s_scrape() {
    assert_eq!(calculate_rate_interval(10 * SEC, "30s"), 120 * SEC);
}
#[test]
fn rate_interval_large_query_10m_30s_scrape() {
    assert_eq!(calculate_rate_interval(10 * MIN, "30s"), 630 * SEC);
}
#[test]
fn rate_interval_5s_scrape_10s_query() {
    assert_eq!(calculate_rate_interval(10 * SEC, "5s"), 20 * SEC);
}
#[test]
fn rate_interval_invalid_scrape_interval_returns_zero() {
    assert_eq!(calculate_rate_interval(100 * SEC, "invalid-interval"), 0);
}
#[test]
fn rate_interval_1h_scrape_2h_query() {
    assert_eq!(calculate_rate_interval(2 * HOUR, "1h"), 4 * HOUR);
}
#[test]
fn rate_interval_edge_case_query_interval_equals_min_rate_interval_boundary() {
    assert_eq!(calculate_rate_interval(45 * SEC, "15s"), 60 * SEC);
}
#[test]
fn rate_interval_10s_scrape_50s_query() {
    assert_eq!(calculate_rate_interval(50 * SEC, "10s"), 60 * SEC);
}
#[test]
fn rate_interval_2m_scrape_1m_query() {
    assert_eq!(calculate_rate_interval(MIN, "2m"), 8 * MIN);
}

// TestIntervalCalculator_Calculate (min interval 1ms)
fn calc(range: i64, resolution: i64, expected: &str) {
    let n = now();
    assert_eq!(format_interval(calculate_interval(n, n + range, MS, resolution)), expected);
}
#[test]
fn calculate_from_5m_to_now_and_default_resolution() {
    calc(5 * MIN, 0, "200ms");
}
#[test]
fn calculate_from_5m_to_now_and_500_resolution() {
    calc(5 * MIN, 500, "500ms");
}
#[test]
fn calculate_from_15m_to_now_and_default_resolution() {
    calc(15 * MIN, 0, "500ms");
}
#[test]
fn calculate_from_15m_to_now_and_100_resolution() {
    calc(15 * MIN, 100, "10s");
}
#[test]
fn calculate_from_30m_to_now_and_default_resolution() {
    calc(30 * MIN, 0, "1s");
}
#[test]
fn calculate_from_30m_to_now_and_3000_resolution() {
    calc(30 * MIN, 3000, "500ms");
}
#[test]
fn calculate_from_1h_to_now_and_default_resolution() {
    calc(HOUR, 0, "2s");
}
#[test]
fn calculate_from_1h_to_now_and_1000_resolution() {
    calc(HOUR, 1000, "5s");
}

// TestIntervalCalculator_CalculateSafeInterval
fn safe(range: i64, safe_res: i64, expected: &str) {
    let n = now();
    assert_eq!(format_interval(calculate_safe_interval(n, n + range, safe_res)), expected);
}
#[test]
fn safe_interval_from_5m_to_now() {
    safe(5 * MIN, 11000, "20ms");
}
#[test]
fn safe_interval_from_15m_to_now() {
    safe(15 * MIN, 11000, "100ms");
}
#[test]
fn safe_interval_from_30m_to_now() {
    safe(30 * MIN, 11000, "200ms");
}
#[test]
fn safe_interval_from_24h_to_now() {
    safe(1440 * MIN, 11000, "10s");
}
