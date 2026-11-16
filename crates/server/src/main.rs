//! `grafana-rs`: the Grafana 13.3.1 server in Rust, serving the real Grafana frontend.
//!
//! Environment (as Grafana): GF_PATHS_HOME (Grafana home with conf/ and public/), GF_PATHS_DATA,
//! GF_SERVER_HTTP_PORT (default 3400 here), GF_SERVER_HTTP_ADDR, GF_AUTH_ANONYMOUS_ENABLED,
//! GF_AUTH_ANONYMOUS_ORG_ROLE, and every other GF_<SECTION>_<KEY>. GF_RS_CONFIG: an extra ini file.
//! GF_RS_THREAD_STACK_KB: stack size of each connection thread (default 128).
use grafana::{App, Settings};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

const DEFAULT_HOME: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../upstream-bin/grafana-13.3.1");
const DEFAULT_DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data");

/// The store and the app, for the idle cleanup (the idle hook is a plain fn).
static STORE: std::sync::OnceLock<Arc<store_sqlite::SqliteStore>> = std::sync::OnceLock::new();
static APP: std::sync::OnceLock<Arc<App>> = std::sync::OnceLock::new();

#[cfg(target_env = "gnu")]
mod mem {
    use std::sync::{Condvar, Mutex};
    use std::time::Duration;
    unsafe extern "C" {
        fn mallopt(param: i32, value: i32) -> i32;
        fn malloc_trim(pad: usize) -> i32;
    }
    /// glibc tuning for a long-running thread-per-connection server: few arenas (each thread would otherwise get
    /// its own heap), and a fixed mmap threshold so large buffers go back to the OS when freed.
    pub fn setup() {
        const M_ARENA_MAX: i32 = -8;
        const M_MMAP_THRESHOLD: i32 = -3;
        // SAFETY: mallopt only changes allocator settings of this process.
        unsafe {
            mallopt(M_ARENA_MAX, 2);
            mallopt(M_MMAP_THRESHOLD, 128 * 1024);
        }
    }
    /// Idle signals not handled yet, and the wake-up of the cleanup thread.
    static IDLE: Mutex<u64> = Mutex::new(0);
    static WAKE: Condvar = Condvar::new();
    /// Called when the last connection closes: the cleanup thread frees memory 1 s later, once per burst of signals.
    pub fn on_idle() {
        let mut n = IDLE.lock().unwrap_or_else(|p| p.into_inner());
        *n += 1;
        WAKE.notify_one();
    }
    /// Starts the cleanup thread. It sleeps until the server goes idle, waits until no new idle signal came for 1 s,
    /// then frees SQLite's page cache and prepared statements, closes idle data source connections, and gives free
    /// heap pages back to the OS.
    pub fn start_cleanup() {
        let _ = std::thread::Builder::new().name("mem-cleanup".into()).stack_size(64 * 1024).spawn(|| {
            loop {
                {
                    let mut n = IDLE.lock().unwrap_or_else(|p| p.into_inner());
                    while *n == 0 {
                        n = WAKE.wait(n).unwrap_or_else(|p| p.into_inner());
                    }
                    loop {
                        let seen = *n;
                        n = WAKE.wait_timeout(n, Duration::from_secs(1)).unwrap_or_else(|p| p.into_inner()).0;
                        if *n == seen {
                            break;
                        }
                    }
                    *n = 0;
                }
                if let Some(s) = super::STORE.get() {
                    s.release_memory();
                }
                // idle data source connections and their buffers (Go's transport also closes idle connections)
                if let Some(a) = super::APP.get() {
                    a.client.clear_idle();
                }
                // SAFETY: malloc_trim only releases free pages.
                unsafe {
                    malloc_trim(0);
                }
            }
        });
    }
}
#[cfg(not(target_env = "gnu"))]
mod mem {
    pub fn setup() {}
    pub fn on_idle() {}
    pub fn start_cleanup() {}
}

fn main() {
    let t0 = Instant::now();
    mem::setup();
    let home = PathBuf::from(std::env::var("GF_PATHS_HOME").unwrap_or_else(|_| DEFAULT_HOME.to_string()));
    let cfg = Settings::load(home, &[("server", "http_port", "3400"), ("paths", "data", DEFAULT_DATA)]);
    let db_path = cfg.data.join("grafana-rs.db");
    let store = match store_sqlite::SqliteStore::open(&db_path) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("grafana-rs: cannot open database {}: {e}", db_path.display());
            std::process::exit(1);
        }
    };
    let _ = STORE.set(store.clone());
    if let Err(e) = grafana::seed::seed(&*store, &cfg) {
        eprintln!("grafana-rs: cannot seed database: {e}");
        std::process::exit(1);
    }
    let addr = format!("{}:{}", if cfg.http_addr.is_empty() { "0.0.0.0" } else { cfg.http_addr.as_str() }, cfg.http_port);
    let mut scfg = http::ServerConfig::default();
    scfg.on_idle = Some(mem::on_idle);
    // GF_RS_THREAD_STACK_KB: stack per connection thread (default 128; only the pages a request touches use RAM).
    // 64 KB was too small for gzip decoding (flate2 builds its 48 KB decoder state on the stack). Debug builds need
    // more (try 512).
    scfg.stack_size = 128 * 1024;
    if let Some(kb) = std::env::var("GF_RS_THREAD_STACK_KB").ok().and_then(|v| v.parse::<usize>().ok()) {
        scfg.stack_size = kb * 1024;
    }
    if let Ok(v) = std::env::var("GF_SERVER_ENABLE_GZIP") {
        scfg.gzip = v != "false";
    }
    let server = match http::Server::bind(&addr, scfg) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("grafana-rs: cannot listen on {addr}: {e}");
            std::process::exit(1);
        }
    };
    let app = Arc::new(App::new(cfg, store));
    let _ = APP.set(app.clone());
    eprintln!("grafana-rs: listening on http://{addr} (started in {:.1} ms, home {})", t0.elapsed().as_secs_f64() * 1000.0, app.cfg.home.display());
    mem::start_cleanup();
    mem::on_idle();
    server.run(app);
}
