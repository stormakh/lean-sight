//! `/api/health`, `/healthz`, `/metrics`, and `/api/rs/whoami` (grafana-rs only: shows the request identity).
use crate::ctx::{Access, Ctx, Routes};
use crate::error::ApiError;
use http::Response;
use json::Json;
use std::sync::atomic::{AtomicI64, Ordering};

pub fn routes(r: &mut Routes) {
    r.get("/api/health", Access::Public, health);
    r.get("/healthz", Access::Public, healthz);
    r.get("/metrics", Access::Public, metrics);
    r.get("/api/rs/whoami", Access::SignedIn, whoami);
}

/// Monotonic ms (since app start) until which the cached database health is valid, and the cached result.
static DB_HEALTH_UNTIL_MS: AtomicI64 = AtomicI64::new(-1);
static DB_HEALTH_OK: AtomicI64 = AtomicI64::new(0);

/// upstream databaseHealthy: the result of `SELECT 1`, cached for 5 seconds.
fn database_healthy(c: &Ctx<'_>) -> bool {
    let now = c.app.started.elapsed().as_millis() as i64;
    if now < DB_HEALTH_UNTIL_MS.load(Ordering::Acquire) {
        return DB_HEALTH_OK.load(Ordering::Relaxed) != 0;
    }
    let ok = c.app.store.ping();
    DB_HEALTH_OK.store(ok as i64, Ordering::Relaxed);
    DB_HEALTH_UNTIL_MS.store(now + 5000, Ordering::Release);
    ok
}

/// upstream apiHealthHandler: indented JSON, 503 when the database fails.
fn health(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let ok = database_healthy(c);
    let mut d = Json::obj().with("database", if ok { "ok" } else { "failing" });
    if !c.app.cfg.anonymous_hide_version {
        d.set("version", c.app.cfg.build_version.as_str());
        d.set("commit", c.app.cfg.build_commit.as_str());
    }
    Ok(Response::text(if ok { 200 } else { 503 }, "application/json; charset=UTF-8", d.stringify_pretty("  ")))
}

fn healthz(_c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    Ok(Response::text(200, "text/plain; charset=utf-8", "Ok"))
}

fn whoami(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let u = &c.user;
    let j = Json::obj()
        .with("id", u.user_id)
        .with("login", u.login.as_str())
        .with("orgId", u.org_id)
        .with("orgName", u.org_name.as_str())
        .with("orgRole", u.org_role.as_str())
        .with("isGrafanaAdmin", u.is_grafana_admin)
        .with("isAnonymous", u.is_anonymous)
        .with("authMethod", format!("{:?}", u.auth));
    Ok(Response::json(200, &j))
}

/// Process stats from /proc/self, in Prometheus text format.
fn metrics(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let mut s = String::with_capacity(2048);
    let ticks = 100.0f64; // USER_HZ on Linux
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    // fields after the ")" of the command name
    let rest = match stat.rfind(')') {
        Some(i) => &stat[i + 2..],
        None => "",
    };
    let f: Vec<&str> = rest.split(' ').collect();
    let num = |i: usize| -> f64 { f.get(i).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) };
    // rest[0] is field 3 (state): utime=14 -> index 11, stime=15 -> 12, threads=20 -> 17, vsize=23 -> 20, rss=24 -> 21
    let cpu = (num(11) + num(12)) / ticks;
    let threads = num(17);
    let vsize = num(20);
    let rss = num(21) * 4096.0;
    let fds = match std::fs::read_dir("/proc/self/fd") {
        Ok(d) => d.count() as f64,
        Err(_) => 0.0,
    };
    let mut line = |name: &str, help: &str, kind: &str, v: f64| {
        s.push_str("# HELP ");
        s.push_str(name);
        s.push(' ');
        s.push_str(help);
        s.push_str("\n# TYPE ");
        s.push_str(name);
        s.push(' ');
        s.push_str(kind);
        s.push('\n');
        s.push_str(name);
        s.push(' ');
        json::write_f64(v, &mut s);
        s.push('\n');
    };
    line("process_cpu_seconds_total", "Total user and system CPU time spent in seconds.", "counter", cpu);
    line("process_resident_memory_bytes", "Resident memory size in bytes.", "gauge", rss);
    line("process_virtual_memory_bytes", "Virtual memory size in bytes.", "gauge", vsize);
    line("process_open_fds", "Number of open file descriptors.", "gauge", fds);
    line("process_threads", "Number of OS threads.", "gauge", threads);
    line("process_start_time_seconds", "Start time of the process since unix epoch in seconds.", "gauge", c.app.started_unix as f64);
    s.push_str("# HELP grafana_build_info A metric with a constant '1' value labeled by version, revision, branch, and goversion from which Grafana was built\n# TYPE grafana_build_info gauge\n");
    s.push_str("grafana_build_info{branch=\"HEAD\",edition=\"oss\",revision=\"");
    s.push_str(&c.app.cfg.build_commit);
    s.push_str("\",version=\"");
    s.push_str(&c.app.cfg.build_version);
    s.push_str("\"} 1\n");
    s.push_str("# HELP grafana_rs_http_requests_total Requests served by status class.\n# TYPE grafana_rs_http_requests_total counter\n");
    for i in 1..6 {
        s.push_str("grafana_rs_http_requests_total{code=\"");
        s.push_str(&i.to_string());
        s.push_str("xx\"} ");
        s.push_str(&c.app.requests[i].load(Ordering::Relaxed).to_string());
        s.push('\n');
    }
    Ok(Response::text(200, "text/plain; version=0.0.4; charset=utf-8", s))
}
