//! Settings, as Grafana loads them: `<home>/conf/defaults.ini`, then an optional custom ini (`GF_RS_CONFIG` or
//! `<home>/conf/custom.ini`), then `GF_<SECTION>_<KEY>` environment variables. Typed fields hold the values that hot
//! paths read; `get` reads any other key.
use crate::ctx::Role;
use std::path::PathBuf;

pub const BUILD_VERSION: &str = "13.3.1";
pub const BUILD_COMMIT: &str = "da3aa3ff574e7dbbf808c66b6daa744defed0441";

#[derive(Debug, Clone)]
pub struct Settings {
    pub home: PathBuf,
    pub data: PathBuf,
    pub http_addr: String,
    pub http_port: u16,
    pub domain: String,
    /// `[server] root_url` with `%(protocol)s`, `%(domain)s`, `%(http_port)s` filled, ending in '/'.
    pub app_url: String,
    /// Path part of app_url without the trailing '/', "" when served from the root.
    pub app_sub_url: String,
    pub enable_gzip: bool,
    pub env: String,
    pub secret_key: String,
    pub login_cookie_name: String,
    pub login_max_inactive_secs: i64,
    pub login_max_lifetime_secs: i64,
    pub token_rotation_minutes: i64,
    pub cookie_secure: bool,
    pub cookie_samesite: String,
    pub admin_user: String,
    pub admin_password: String,
    pub admin_email: String,
    pub anonymous_enabled: bool,
    pub anonymous_org_name: String,
    pub anonymous_org_role: Role,
    pub anonymous_hide_version: bool,
    pub basic_auth_enabled: bool,
    pub disable_login_form: bool,
    pub app_mode_development: bool,
    pub build_version: String,
    pub build_commit: String,
    sections: Vec<(String, Vec<(String, String)>)>,
}

/// Parses a Grafana duration: "10m", "6h", "7d", "2w", "1M", "1y", "30s", or a bare number of seconds.
pub fn parse_duration_secs(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (num, unit) = match s.find(|c: char| !c.is_ascii_digit()) {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, "s"),
    };
    let n: i64 = num.parse().ok()?;
    let mul = match unit {
        "ms" => return Some(n / 1000),
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86400,
        "w" => 7 * 86400,
        "M" => 30 * 86400,
        "y" => 365 * 86400,
        _ => return None,
    };
    Some(n * mul)
}

fn parse_ini(text: &str, sections: &mut Vec<(String, Vec<(String, String)>)>) {
    let mut cur = String::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with(';') {
            continue;
        }
        if l.starts_with('[') && l.ends_with(']') {
            cur = l[1..l.len() - 1].trim().to_string();
            continue;
        }
        if let Some((k, v)) = l.split_once('=') {
            let mut v = v.trim();
            if v.len() >= 2 && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('`') && v.ends_with('`'))) {
                v = &v[1..v.len() - 1];
            }
            set_raw(sections, &cur, k.trim(), v);
        }
    }
}

fn set_raw(sections: &mut Vec<(String, Vec<(String, String)>)>, section: &str, key: &str, value: &str) {
    for s in sections.iter_mut() {
        if s.0 == section {
            for kv in s.1.iter_mut() {
                if kv.0 == key {
                    kv.1 = value.to_string();
                    return;
                }
            }
            s.1.push((key.to_string(), value.to_string()));
            return;
        }
    }
    sections.push((section.to_string(), vec![(key.to_string(), value.to_string())]));
}

/// `GF_` env name for a section and key: dots and dashes become underscores, all upper case.
pub fn env_name(section: &str, key: &str) -> String {
    let mut s = String::from("GF_");
    for c in section.chars().chain(std::iter::once('_')).chain(key.chars()) {
        if c == '.' || c == '-' {
            s.push('_');
        } else {
            s.push(c.to_ascii_uppercase());
        }
    }
    s
}

impl Settings {
    /// Loads settings for a Grafana home directory. `pre` values (section, key, value) apply after the ini files
    /// and before the environment, so they act as this program's defaults.
    pub fn load(home: PathBuf, pre: &[(&str, &str, &str)]) -> Settings {
        let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
        if let Ok(t) = std::fs::read_to_string(home.join("conf").join("defaults.ini")) {
            parse_ini(&t, &mut sections);
        }
        let custom = match std::env::var("GF_RS_CONFIG") {
            Ok(p) if !p.is_empty() => PathBuf::from(p),
            _ => home.join("conf").join("custom.ini"),
        };
        if let Ok(t) = std::fs::read_to_string(&custom) {
            parse_ini(&t, &mut sections);
        }
        for p in pre.iter() {
            set_raw(&mut sections, p.0, p.1, p.2);
        }
        // environment overrides for every known key
        let mut overrides: Vec<(String, String, String)> = Vec::new();
        for s in sections.iter() {
            for kv in s.1.iter() {
                if let Ok(v) = std::env::var(env_name(&s.0, &kv.0)) {
                    overrides.push((s.0.clone(), kv.0.clone(), v));
                }
            }
        }
        for (s, k, v) in overrides {
            set_raw(&mut sections, &s, &k, &v);
        }
        let mut st = Settings {
            home: home.clone(),
            data: PathBuf::new(),
            http_addr: String::new(),
            http_port: 3000,
            domain: String::new(),
            app_url: String::new(),
            app_sub_url: String::new(),
            enable_gzip: false,
            env: String::new(),
            secret_key: String::new(),
            login_cookie_name: String::new(),
            login_max_inactive_secs: 7 * 86400,
            login_max_lifetime_secs: 30 * 86400,
            token_rotation_minutes: 10,
            cookie_secure: false,
            cookie_samesite: String::new(),
            admin_user: String::new(),
            admin_password: String::new(),
            admin_email: String::new(),
            anonymous_enabled: false,
            anonymous_org_name: String::new(),
            anonymous_org_role: Role::Viewer,
            anonymous_hide_version: false,
            basic_auth_enabled: true,
            disable_login_form: false,
            app_mode_development: false,
            build_version: BUILD_VERSION.to_string(),
            build_commit: BUILD_COMMIT.to_string(),
            sections,
        };
        let data = st.get("paths", "data").to_string();
        st.data = if data.starts_with('/') { PathBuf::from(data) } else { home.join(data) };
        st.http_addr = st.get("server", "http_addr").to_string();
        st.http_port = st.get("server", "http_port").parse().unwrap_or(3000);
        st.domain = st.get("server", "domain").to_string();
        let protocol = st.get("server", "protocol").to_string();
        let mut root = st.get("server", "root_url").to_string();
        root = root.replace("%(protocol)s", &protocol).replace("%(domain)s", &st.domain).replace("%(http_port)s", &st.http_port.to_string());
        if !root.ends_with('/') {
            root.push('/');
        }
        // sub path: what follows host[:port]
        let after_scheme = match root.find("://") {
            Some(i) => &root[i + 3..],
            None => root.as_str(),
        };
        let sub = match after_scheme.find('/') {
            Some(i) => after_scheme[i..].trim_end_matches('/').to_string(),
            None => String::new(),
        };
        st.app_sub_url = sub;
        st.app_url = root;
        st.enable_gzip = st.get_bool("server", "enable_gzip");
        st.env = st.get("", "app_mode").to_string();
        st.app_mode_development = st.env == "development";
        st.secret_key = st.get("security", "secret_key").to_string();
        st.login_cookie_name = st.get("auth", "login_cookie_name").to_string();
        if let Some(d) = parse_duration_secs(st.get("auth", "login_maximum_inactive_lifetime_duration")) {
            st.login_max_inactive_secs = d;
        }
        if let Some(d) = parse_duration_secs(st.get("auth", "login_maximum_lifetime_duration")) {
            st.login_max_lifetime_secs = d;
        }
        st.token_rotation_minutes = st.get_int("auth", "token_rotation_interval_minutes", 10);
        st.cookie_secure = st.get_bool("security", "cookie_secure");
        st.cookie_samesite = st.get("security", "cookie_samesite").to_string();
        st.admin_user = st.get("security", "admin_user").to_string();
        st.admin_password = st.get("security", "admin_password").to_string();
        st.admin_email = st.get("security", "admin_email").to_string();
        st.anonymous_enabled = st.get_bool("auth.anonymous", "enabled");
        st.anonymous_org_name = st.get("auth.anonymous", "org_name").to_string();
        st.anonymous_org_role = Role::parse(st.get("auth.anonymous", "org_role")).unwrap_or(Role::Viewer);
        st.anonymous_hide_version = st.get_bool("auth.anonymous", "hide_version");
        st.basic_auth_enabled = st.get_bool("auth.basic", "enabled");
        st.disable_login_form = st.get_bool("auth", "disable_login_form");
        st
    }

    /// Raw value, "" when the key is unknown. Section "" is the top of the file (before any `[section]`).
    pub fn get(&self, section: &str, key: &str) -> &str {
        for s in self.sections.iter() {
            if s.0 == section {
                for kv in s.1.iter() {
                    if kv.0 == key {
                        return &kv.1;
                    }
                }
            }
        }
        ""
    }
    pub fn get_bool(&self, section: &str, key: &str) -> bool {
        let v = self.get(section, key);
        v.eq_ignore_ascii_case("true") || v == "1" || v.eq_ignore_ascii_case("yes") || v.eq_ignore_ascii_case("on")
    }
    pub fn get_int(&self, section: &str, key: &str, default: i64) -> i64 {
        self.get(section, key).trim().parse().unwrap_or(default)
    }
    /// Sets a value (startup only; settings are read-only once the server runs).
    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        set_raw(&mut self.sections, section, key, value);
    }
    /// Keys of a section, in file order.
    pub fn section(&self, section: &str) -> Vec<(String, String)> {
        for s in self.sections.iter() {
            if s.0 == section {
                return s.1.clone();
            }
        }
        Vec::new()
    }
    /// Path of the plugin directory (`[paths] plugins`).
    pub fn plugins_dir(&self) -> PathBuf {
        let p = self.get("paths", "plugins");
        if p.starts_with('/') { PathBuf::from(p) } else { self.home.join(p) }
    }
}
