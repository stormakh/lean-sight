//! Plugin metadata (agent A), loaded ONCE at startup from `<home>/public/app/plugins/{datasource,panel}/*/plugin.json`
//! (core plugins), `[paths] plugins` and `[paths] bundled_plugins` (external plugins: prometheus, apps, ...).
//! Upstream: pkg/plugins (models.go, plugins.go `ReadPluginJSON`, manager/sources, manager/pipeline/bootstrap,
//! manager/signature), pkg/services/pluginsintegration/pipeline (dedupe, disable_plugins, as_external),
//! pkg/api/bootdata.go (getFSDataSources, getFSPanels, getFSApps), pkg/api/plugins.go.
//!
//! Everything the boot data and the plugin API need is prebuilt here as JSON text (or a small `Json` for
//! `ds_meta`), so a request only copies text. Plugin settings are static: no `plugin_setting` rows, so apps are
//! enabled and pinned when `autoEnabled`, every other plugin is enabled (Grafana's defaults without a row).
//!
//! Differences from Grafana: the MANIFEST.txt PGP signature and the file hashes are not verified (a v2 manifest
//! whose plugin id and version match is "valid"); no Angular detection (always `detected: false`); no plugins CDN
//! (loadingStrategy is always "script"); nested (child) plugins are not discovered; no `moduleHash`.
use crate::settings::Settings;
use json::Json;
use std::path::{Path, PathBuf};

/// One loaded plugin.
#[derive(Debug, Clone)]
pub struct Plugin {
    pub id: String,
    /// "datasource" | "panel" | "app".
    pub kind: String,
    pub name: String,
    /// The plugin directory.
    pub dir: PathBuf,
    /// Core plugin (from public/app/plugins), signature "internal".
    pub core: bool,
    /// The `meta` object of a boot-data `settings.datasources` entry as JSON text (data source plugins only, else
    /// empty). For built-in data sources (grafana, mixed, dashboard) it is the built-in variant Grafana writes.
    /// Kept as text, not a `Json` tree: the tree costs several times more memory.
    pub ds_meta: ZText,
    /// `module`, `preload` and `aliasIDs` of `ds_meta` (the only fields read from it).
    pub ds_module: String,
    pub ds_preload: bool,
    pub ds_alias_ids: Vec<String>,
    /// `builtIn` in plugin.json (data sources "-- Grafana --", "-- Mixed --", "-- Dashboard --").
    pub builtin: bool,
    /// `info.version`.
    pub version: String,
    pub auto_enabled: bool,
    /// Release state "alpha" (hidden unless `[plugins] enable_alpha`).
    pub alpha: bool,
    /// Static plugin setting: apps are enabled when autoEnabled, every other plugin is enabled.
    pub enabled: bool,
    /// `GET /api/plugins/:id/settings` body.
    pub settings_json: ZText,
    /// `GET /api/plugins` item without `accessControl`, `angularDetected` and the closing brace.
    pub list_head: ZText,
}

#[derive(Debug, Clone, Default)]
pub struct Plugins {
    /// Every loaded plugin, in load order (core, then external, then bundled).
    pub list: Vec<Plugin>,
    /// Boot data `settings.panels` as JSON text (an object keyed by panel id).
    pub panels_json: ZText,
    /// Boot data `settings.apps` as JSON text (an object keyed by app id).
    pub apps_json: ZText,
    /// `GET /api/plugins/errors` body: plugins that failed signature validation.
    pub errors_json: String,
    /// Plugin errors: (plugin id, public message), for `GET /api/plugins/:id/settings`.
    pub errors: Vec<(String, String)>,
    /// Indexes into `list`, sorted by name (`GET /api/plugins` order).
    pub by_name: Vec<usize>,
    /// `[plugins] enable_alpha`.
    pub enable_alpha: bool,
}

/// Options read from the settings once.
struct Opts {
    disabled: Vec<String>,
    allow_unsigned: Vec<String>,
    dev: bool,
    enable_alpha: bool,
    sub_url: String,
}

/// A plugin.json found on disk, before validation.
struct Found {
    raw: Json,
    dir: PathBuf,
    core: bool,
}

impl Plugins {
    /// Loads every plugin.json. Never fails: unreadable plugins are skipped with a message on stderr.
    pub fn load(cfg: &Settings) -> Plugins {
        let opts = Opts {
            disabled: split_list(cfg.get("plugins", "disable_plugins")),
            allow_unsigned: split_list(cfg.get("plugins", "allow_loading_unsigned_plugins")),
            dev: cfg.app_mode_development,
            enable_alpha: cfg.get_bool("plugins", "enable_alpha"),
            sub_url: cfg.app_sub_url.clone(),
        };
        let mut found: Vec<Found> = Vec::new();
        let static_root = abs_path(&cfg.home, or_default(cfg.get("server", "static_root_path"), "public"));
        for sub in ["datasource", "panel"] {
            for d in sorted_dirs(&static_root.join("app/plugins").join(sub)) {
                let dir = plugin_dir(d);
                if let Some(raw) = read_plugin_json(&dir.join("plugin.json")) {
                    found.push(Found { raw, dir, core: true });
                }
            }
        }
        let external = [cfg.plugins_dir(), abs_path(&cfg.home, or_default(cfg.get("paths", "bundled_plugins"), "data/plugins-bundled"))];
        for root in external.iter() {
            for d in sorted_dirs(root) {
                let dir = plugin_dir(d);
                if let Some(raw) = read_plugin_json(&dir.join("plugin.json")) {
                    found.push(Found { raw, dir, core: false });
                }
            }
        }

        let mut out = Plugins { enable_alpha: opts.enable_alpha, ..Plugins::default() };
        let mut errors: Vec<(String, &'static str)> = Vec::new();
        // Boot data `settings.panels` and `settings.apps`, keyed by id (Go map order: sorted keys).
        let mut panels: Vec<(String, Json)> = Vec::new();
        let mut apps: Vec<(String, Json)> = Vec::new();
        for f in found.iter() {
            let id = f.raw.str_of("id").to_string();
            let kind = f.raw.str_of("type").to_string();
            if id.is_empty() || !matches!(kind.as_str(), "datasource" | "panel" | "app" | "renderer") {
                eprintln!("grafana-rs: skipping plugin in {}: did not find valid type or id properties in plugin.json", f.dir.display());
                continue;
            }
            if kind == "renderer" {
                continue;
            }
            if out.list.iter().any(|p| p.id == id) {
                eprintln!("grafana-rs: skipping loading of plugin as it's a duplicate: {id}");
                continue;
            }
            if opts.disabled.iter().any(|d| *d == id) {
                continue;
            }
            if f.core && cfg.get(&format!("plugin.{id}"), "as_external") == "true" {
                continue;
            }
            let (sig, sig_type, sig_org) = if f.core { ("internal", String::new(), String::new()) } else { signature(&f.dir, &id, &f.raw) };
            match sig {
                "internal" | "valid" => {}
                "unsigned" if opts.dev || opts.allow_unsigned.iter().any(|a| *a == id) => {
                    eprintln!("grafana-rs: permitting unsigned plugin {id}. This is not recommended");
                }
                _ => {
                    eprintln!("grafana-rs: skipping loading plugin {id} due to problem with signature: {sig}");
                    errors.push((id, sig));
                    continue;
                }
            }
            if !f.core && !f.dir.join("module.js").is_file() {
                eprintln!("grafana-rs: plugin {id} is missing module.js, skipping");
                continue;
            }
            let (p, boot) = build(f, &id, &kind, sig, &sig_type, &sig_org, &opts);
            if let Some(b) = boot {
                if kind == "panel" { panels.push((id.clone(), b)) } else { apps.push((id.clone(), b)) }
            }
            out.list.push(p);
        }

        json::sort::sort_by(&mut panels, |a, b| a.0.cmp(&b.0));
        json::sort::sort_by(&mut apps, |a, b| a.0.cmp(&b.0));
        out.panels_json = ZText::new(&Json::Obj(panels).stringify());
        out.apps_json = ZText::new(&Json::Obj(apps).stringify());

        json::sort::sort_by(&mut errors, |a, b| a.0.cmp(&b.0));
        let mut ej = Json::arr();
        for (id, sig) in errors.iter() {
            let (code, msg) = match *sig {
                "unsigned" => ("signatureMissing", "Plugin signature is missing"),
                "modified" => ("signatureModified", "Plugin signature does not match"),
                _ => ("signatureInvalid", "Invalid plugin signature"),
            };
            ej.push(Json::obj().with("errorCode", code).with("pluginId", id.as_str()).with("status", *sig));
            out.errors.push((id.clone(), msg.to_string()));
        }
        out.errors_json = tight(ej.stringify());

        let mut idx: Vec<usize> = (0..out.list.len()).collect();
        json::sort::sort_by(&mut idx, |a, b| out.list[*a].name.cmp(&out.list[*b].name));
        out.by_name = idx;
        out
    }
    pub fn get(&self, id: &str) -> Option<&Plugin> {
        self.list.iter().find(|p| p.id == id)
    }
    /// The boot-data `meta` of a data source plugin type (also matches a plugin's `aliasIDs`).
    pub fn datasource_meta(&self, id: &str) -> Option<&Plugin> {
        match self.get(id) {
            Some(p) if p.kind == "datasource" => Some(p),
            _ => self.list.iter().find(|p| p.kind == "datasource" && p.ds_alias_ids.iter().any(|a| a == id)),
        }
    }
    /// Data source plugins with `builtIn: true` (boot data adds them to `settings.datasources` by name).
    pub fn builtin_datasources(&self) -> impl Iterator<Item = &Plugin> {
        self.list.iter().filter(|p| p.kind == "datasource" && p.builtin)
    }
    /// The public message of a plugin load error (signature problems), if the plugin failed to load.
    pub fn error_for(&self, id: &str) -> Option<&str> {
        self.errors.iter().find(|e| e.0 == id).map(|e| e.1.as_str())
    }
}

/// A text kept gzip-compressed: the prebuilt plugin JSON is read rarely (page loads, plugin pages) and
/// compresses about 6X, which saves about 200 KB of resident memory. `text()` gives back the same bytes.
#[derive(Debug, Clone, Default)]
pub struct ZText(Vec<u8>);
impl ZText {
    pub fn new(s: &str) -> ZText {
        use std::io::Write;
        let mut e = flate2::write::GzEncoder::new(Vec::with_capacity(s.len() / 4 + 16), flate2::Compression::default());
        if e.write_all(s.as_bytes()).is_err() {
            return ZText::default();
        }
        let mut v = e.finish().unwrap_or_default();
        v.shrink_to_fit();
        ZText(v)
    }
    pub fn text(&self) -> String {
        use std::io::Read;
        let mut out = String::new();
        let _ = flate2::read::GzDecoder::new(&self.0[..]).read_to_string(&mut out);
        out
    }
}

/// Gives back the spare capacity of a text kept for the life of the process (a JSON writer grows its buffer by
/// doubling, so the capacity can be up to twice the length).
fn tight(mut s: String) -> String {
    s.shrink_to_fit();
    s
}

/// Builds the prebuilt texts of one plugin, and its boot-data entry (`settings.panels` or `settings.apps`).
fn build(f: &Found, id: &str, kind: &str, sig: &str, sig_type: &str, sig_org: &str, opts: &Opts) -> (Plugin, Option<Json>) {
    let raw = &f.raw;
    let nj = normalize(raw, id, kind);
    let is_app = kind == "app";
    let auto_enabled = gbool(raw, "autoEnabled");
    let enabled = if is_app { auto_enabled } else { true };
    let pinned = is_app && auto_enabled;
    let base = base_url(id);
    let module = module_path(id, &f.dir, f.core);
    let tr = translations(raw, id);
    let version = nj.path("info.version").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let state = gstr(raw, "state");
    let name = gstr(raw, "name");
    let builtin = gbool(raw, "builtIn");

    // pluginstore DefaultNavURL (apps only), then path.Join(AppSubURL, DefaultNavURL).
    let mut default_nav = String::new();
    if is_app {
        if let Some(incs) = nj.get("includes").and_then(|v| v.as_arr()) {
            for inc in incs.iter() {
                if !inc.bool_of("defaultNav") {
                    continue;
                }
                match inc.str_of("type") {
                    "page" => default_nav = join(&["/plugins/", id, "/page/", inc.str_of("slug")]),
                    "dashboard" if !inc.str_of("uid").is_empty() => default_nav = format!("/d/{}", inc.str_of("uid")),
                    _ => {}
                }
            }
        }
    }
    let nav_joined = join(&[&opts.sub_url, &default_nav]);

    // GET /api/plugins/:id/settings (dtos.PluginSetting).
    let mut s = Json::obj()
        .with("name", name.as_str())
        .with("type", kind)
        .with("id", id)
        .with("enabled", is_app && auto_enabled)
        .with("pinned", is_app && auto_enabled)
        .with("autoEnabled", is_app && auto_enabled)
        .with("module", module.as_str())
        .with("baseUrl", base.as_str())
        .with("info", nj.get("info").cloned().unwrap_or(Json::Null))
        .with("includes", nj.get("includes").cloned().unwrap_or(Json::Null))
        .with("dependencies", nj.get("dependencies").cloned().unwrap_or(Json::Null))
        .with("extensions", nj.get("extensions").cloned().unwrap_or(Json::Null))
        .with("jsonData", Json::Null)
        .with("secureJsonFields", Json::obj())
        .with("defaultNavUrl", nav_joined.as_str())
        .with("latestVersion", "")
        .with("hasUpdate", false)
        .with("state", state.as_str())
        .with("signature", sig)
        .with("signatureType", sig_type)
        .with("signatureOrg", sig_org)
        .with("angularDetected", false)
        .with("loadingStrategy", "script");
    if !tr.is_empty() {
        s.set("translations", map_json(&tr));
    }

    // GET /api/plugins item (dtos.PluginListItem), up to signatureOrg.
    let list_nav = if nav_joined.is_empty() || !enabled { format!("{}/plugins/{}/", opts.sub_url, id) } else { nav_joined.clone() };
    let item = Json::obj()
        .with("name", name.as_str())
        .with("type", kind)
        .with("id", id)
        .with("enabled", enabled)
        .with("pinned", pinned)
        .with("info", nj.get("info").cloned().unwrap_or(Json::Null))
        .with("dependencies", nj.get("dependencies").cloned().unwrap_or(Json::Null))
        .with("latestVersion", "")
        .with("hasUpdate", false)
        .with("defaultNavUrl", list_nav.as_str())
        .with("category", gstr(raw, "category"))
        .with("state", state.as_str())
        .with("signature", sig)
        .with("signatureType", sig_type)
        .with("signatureOrg", sig_org);
    let mut list_head = item.stringify();
    list_head.pop();

    // Boot data `settings.datasources[*].meta` (plugins.PluginMetaDTO).
    let ds_meta = if kind == "datasource" {
        let mut m = nj.clone();
        m.remove("extensions");
        m.remove("multiValueFilterOperators");
        m.set("signature", sig);
        m.set("module", module.as_str());
        m.set("baseUrl", base.as_str());
        m.set("angular", angular());
        m.set("multiValueFilterOperators", !builtin && gbool(raw, "multiValueFilterOperators"));
        m.set("loadingStrategy", if builtin { "" } else { "script" });
        m.set("extensions", null_extensions());
        if !tr.is_empty() {
            m.set("translations", map_json(&tr));
        }
        m
    } else {
        Json::Null
    };
    let ds_module = ds_meta.str_of("module").to_string();
    let ds_preload = ds_meta.bool_of("preload");
    let mut ds_alias_ids = Vec::new();
    if let Some(a) = ds_meta.get("aliasIDs").and_then(|a| a.as_arr()) {
        for v in a.iter() {
            if let Some(t) = v.as_str() {
                ds_alias_ids.push(t.to_string());
            }
        }
    }
    let ds_meta = if kind == "datasource" { ZText::new(&ds_meta.stringify()) } else { ZText::default() };

    // Boot data entry: plugins.PanelDTO (getFSPanels) or plugins.AppDTO (newAppDTO).
    let boot = if kind == "panel" && (state != "alpha" || opts.enable_alpha) {
        let mut o = Json::obj().with("id", id).with("name", name.as_str());
        if let Some(a) = nj.get("aliasIDs") {
            o.set("aliasIDs", a.clone());
        }
        o.set("info", nj.get("info").cloned().unwrap_or(Json::Null));
        o.set("hideFromList", gbool(raw, "hideFromList"));
        o.set("sort", panel_sort(id));
        o.set("skipDataQuery", gbool(raw, "skipDataQuery"));
        if gbool(raw, "suggestions") {
            o.set("suggestions", true);
        }
        o.set("state", state.as_str());
        o.set("baseUrl", base.as_str());
        o.set("signature", sig);
        o.set("module", module.as_str());
        o.set("angular", angular());
        o.set("loadingStrategy", "script");
        if !tr.is_empty() {
            o.set("translations", map_json(&tr));
        }
        Some(o)
    } else if is_app {
        let mut o = Json::obj()
            .with("id", id)
            .with("path", module.as_str())
            .with("version", version.as_str())
            .with("preload", enabled && gbool(raw, "preload"))
            .with("angular", angular())
            .with("loadingStrategy", "script")
            .with("extensions", nj.get("extensions").cloned().unwrap_or(Json::Null))
            .with("dependencies", nj.get("dependencies").cloned().unwrap_or(Json::Null));
        if !tr.is_empty() {
            o.set("translations", map_json(&tr));
        }
        let bm = gstr(raw, "buildMode");
        if !bm.is_empty() {
            o.set("buildMode", bm);
        }
        Some(o)
    } else {
        None
    };

    let p = Plugin {
        id: id.to_string(),
        kind: kind.to_string(),
        name,
        dir: f.dir.clone(),
        core: f.core,
        ds_meta,
        ds_module,
        ds_preload,
        ds_alias_ids,
        builtin,
        version,
        auto_enabled,
        alpha: state == "alpha",
        enabled,
        settings_json: ZText::new(&s.stringify()),
        list_head: ZText::new(&list_head),
    };
    (p, boot)
}

// ---- JSONData as Go decodes and re-encodes it (pkg/plugins/plugins.go, models.go) ----

/// The plugin.json as Grafana's `JSONData` struct writes it: every field in struct order with Go's defaults,
/// `omitempty` honored, and the loader's changes (logo/screenshot paths, %VERSION%, include roles and slugs,
/// tempo alerting off).
fn normalize(raw: &Json, id: &str, kind: &str) -> Json {
    let is_app = kind == "app";
    let mut o = Json::obj().with("id", id).with("type", kind).with("name", gstr(raw, "name"));
    let aliases = gstrs(raw, "aliasIDs");
    if non_empty_arr(&aliases) {
        o.set("aliasIDs", aliases);
    }
    o.set("info", norm_info(fld(raw, "info"), id, kind));
    o.set("dependencies", norm_deps(fld(raw, "dependencies")));
    o.set("includes", arr_or_null(fld(raw, "includes"), |v| norm_include(v, is_app)));
    let state = gstr(raw, "state");
    if !state.is_empty() {
        o.set("state", state);
    }
    o.set("category", gstr(raw, "category"));
    if gbool(raw, "hideFromList") {
        o.set("hideFromList", true);
    }
    o.set("preload", gbool(raw, "preload"));
    o.set("backend", gbool(raw, "backend"));
    o.set("routes", arr_or_null(fld(raw, "routes"), norm_route));
    let roles = arr_or_null(fld(raw, "roles"), norm_role);
    if non_empty_arr(&roles) {
        o.set("roles", roles);
    }
    let sets = arr_or_null(fld(raw, "actionSets"), |v| Json::obj().with("action", gstr(v, "action")).with("actions", gstrs(v, "actions")));
    if non_empty_arr(&sets) {
        o.set("actionSets", sets);
    }
    o.set("skipDataQuery", gbool(raw, "skipDataQuery"));
    if gbool(raw, "suggestions") {
        o.set("suggestions", true);
    }
    o.set("autoEnabled", gbool(raw, "autoEnabled"));
    o.set("extensions", norm_extensions(fld(raw, "extensions")));
    o.set("annotations", gbool(raw, "annotations"));
    o.set("metrics", gbool(raw, "metrics"));
    // disableAlertingForTempoDecorateFunc: the tempoAlerting feature toggle is off.
    o.set("alerting", gbool(raw, "alerting") && id != "tempo");
    o.set("explore", gbool(raw, "explore"));
    o.set("tables", gbool(raw, "tables"));
    o.set("logs", gbool(raw, "logs"));
    o.set("tracing", gbool(raw, "tracing"));
    if let Some(Json::Obj(q)) = fld(raw, "queryOptions") {
        let mut m: Vec<(String, Json)> = Vec::new();
        for (k, v) in q.iter() {
            let b = v.as_bool().unwrap_or(false);
            match m.iter_mut().find(|e| e.0 == *k) {
                Some(e) => e.1 = Json::Bool(b),
                None => m.push((k.clone(), Json::Bool(b))),
            }
        }
        if !m.is_empty() {
            json::sort::sort_by(&mut m, |a, b| a.0.cmp(&b.0));
            o.set("queryOptions", Json::Obj(m));
        }
    }
    for k in ["builtIn", "mixed"] {
        if gbool(raw, k) {
            o.set(k, true);
        }
    }
    o.set("streaming", gbool(raw, "streaming"));
    for k in ["sdk", "multiValueFilterOperators"] {
        if gbool(raw, k) {
            o.set(k, true);
        }
    }
    let exe = gstr(raw, "executable");
    if !exe.is_empty() {
        o.set("executable", exe);
    }
    if let Some(iam) = fld(raw, "iam") {
        o.set("iam", iam.clone());
    }
    let langs = gstrs(raw, "languages");
    if non_empty_arr(&langs) {
        o.set("languages", langs);
    }
    let bm = gstr(raw, "buildMode");
    if !bm.is_empty() {
        o.set("buildMode", bm);
    }
    o
}

fn norm_info(info: Option<&Json>, id: &str, kind: &str) -> Json {
    let null = Json::Null;
    let i = info.unwrap_or(&null);
    let author = fld(i, "author").unwrap_or(&null);
    let logos = fld(i, "logos").unwrap_or(&null);
    let logo = |k: &str| -> String {
        let v = gstr(logos, k);
        if v.is_empty() { format!("public/img/icn-{kind}.svg") } else { join(&["public/plugins", id, &v]) }
    };
    let mut build = Json::obj();
    if let Some(t) = fld(i, "build").and_then(|b| fld(b, "time")).and_then(|t| t.as_i64()) {
        if t != 0 {
            build.set("time", t);
        }
    }
    let mut version = gstr(i, "version");
    if version == "%VERSION%" {
        version.clear();
    }
    let mut updated = gstr(i, "updated");
    if updated == "%TODAY%" {
        updated.clear();
    }
    Json::obj()
        .with("author", Json::obj().with("name", gstr(author, "name")).with("url", gstr(author, "url")))
        .with("description", gstr(i, "description"))
        .with("links", arr_or_null(fld(i, "links"), |l| Json::obj().with("name", gstr(l, "name")).with("url", gstr(l, "url"))))
        .with("logos", Json::obj().with("small", logo("small")).with("large", logo("large")))
        .with("build", build)
        .with("screenshots", arr_or_null(fld(i, "screenshots"), |s| Json::obj().with("name", gstr(s, "name")).with("path", join(&["public/plugins", id, &gstr(s, "path")]))))
        .with("version", version)
        .with("updated", updated)
        .with("keywords", gstrs(i, "keywords"))
}

fn norm_deps(d: Option<&Json>) -> Json {
    let null = Json::Null;
    let d = d.unwrap_or(&null);
    let mut gv = gstr(d, "grafanaVersion");
    if gv.is_empty() {
        gv = "*".to_string();
    }
    let mut plugins = arr_or_null(fld(d, "plugins"), |p| Json::obj().with("id", gstr(p, "id")).with("type", gstr(p, "type")).with("name", gstr(p, "name")));
    if !non_empty_arr(&plugins) {
        plugins = Json::arr();
    }
    let mut exposed = fld(d, "extensions").map(|e| gstrs(e, "exposedComponents")).unwrap_or(Json::Null);
    if !non_empty_arr(&exposed) {
        exposed = Json::arr();
    }
    Json::obj()
        .with("grafanaDependency", gstr(d, "grafanaDependency"))
        .with("grafanaVersion", gv)
        .with("plugins", plugins)
        .with("extensions", Json::obj().with("exposedComponents", exposed))
}

fn norm_include(v: &Json, is_app: bool) -> Json {
    let mut role = gstr(v, "role");
    if role.is_empty() {
        role = "Viewer".to_string();
    }
    let mut action = gstr(v, "action");
    if is_app && role == "Viewer" && action.is_empty() {
        action = "plugins.app:access".to_string();
    }
    let name = gstr(v, "name");
    let mut slug = gstr(v, "slug");
    if is_app && slug.is_empty() {
        slug = crate::util::slugify(&name);
    }
    let mut o = Json::obj().with("name", name).with("path", gstr(v, "path")).with("type", gstr(v, "type")).with("component", gstr(v, "component")).with("role", role);
    if !action.is_empty() {
        o.set("action", action);
    }
    o.with("addToNav", gbool(v, "addToNav")).with("defaultNav", gbool(v, "defaultNav")).with("slug", slug).with("icon", gstr(v, "icon")).with("uid", gstr(v, "uid"))
}

fn norm_route(v: &Json) -> Json {
    let pair = |p: &Json| Json::obj().with("name", gstr(p, "name")).with("content", gstr(p, "content"));
    let token = |k: &str| -> Json {
        match fld(v, k) {
            Some(t @ Json::Obj(_)) => {
                let mut params: Vec<(String, Json)> = Vec::new();
                if let Some(Json::Obj(ps)) = fld(t, "params") {
                    for (pk, pv) in ps.iter() {
                        params.retain(|e| e.0 != *pk);
                        params.push((pk.clone(), Json::Str(pv.as_str().unwrap_or("").to_string())));
                    }
                }
                json::sort::sort_by(&mut params, |a, b| a.0.cmp(&b.0));
                let params = if fld(t, "params").is_some() { Json::Obj(params) } else { Json::Null };
                Json::obj().with("url", gstr(t, "url")).with("scopes", gstrs(t, "scopes")).with("params", params)
            }
            _ => Json::Null,
        }
    };
    Json::obj()
        .with("path", gstr(v, "path"))
        .with("method", gstr(v, "method"))
        .with("reqRole", gstr(v, "reqRole"))
        .with("reqAction", gstr(v, "reqAction"))
        .with("url", gstr(v, "url"))
        .with("urlParams", arr_or_null(fld(v, "urlParams"), pair))
        .with("headers", arr_or_null(fld(v, "headers"), pair))
        .with("authType", gstr(v, "authType"))
        .with("tokenAuth", token("tokenAuth"))
        .with("jwtTokenAuth", token("jwtTokenAuth"))
        .with("body", fld(v, "body").cloned().unwrap_or(Json::Null))
}

fn norm_role(v: &Json) -> Json {
    let null = Json::Null;
    let r = fld(v, "role").unwrap_or(&null);
    let perms = arr_or_null(fld(r, "permissions"), |p| Json::obj().with("action", gstr(p, "action")).with("scope", gstr(p, "scope")));
    Json::obj()
        .with("role", Json::obj().with("name", gstr(r, "name")).with("description", gstr(r, "description")).with("permissions", perms))
        .with("grants", gstrs(v, "grants"))
}

/// `Extensions` after ReadPluginJSON: V2 object, or the deprecated V1 array; every nil list becomes `[]`.
fn norm_extensions(e: Option<&Json>) -> Json {
    let added = |x: &Json| Json::obj().with("targets", gstrs(x, "targets")).with("title", gstr(x, "title")).with("description", gstr(x, "description"));
    let exposed = |x: &Json| Json::obj().with("id", gstr(x, "id")).with("title", gstr(x, "title")).with("description", gstr(x, "description"));
    let mut lists: [Json; 5] = [Json::Null, Json::Null, Json::Null, Json::Null, Json::Null];
    match e {
        Some(o @ Json::Obj(_)) => {
            lists[0] = arr_or_null(fld(o, "addedLinks"), added);
            lists[1] = arr_or_null(fld(o, "addedComponents"), added);
            lists[2] = arr_or_null(fld(o, "exposedComponents"), exposed);
            lists[3] = arr_or_null(fld(o, "extensionPoints"), exposed);
            lists[4] = arr_or_null(fld(o, "addedFunctions"), added);
        }
        Some(Json::Arr(v1)) => {
            for x in v1.iter() {
                let item = Json::obj().with("targets", Json::arr().push_item(gstr(x, "extensionPointId"))).with("title", gstr(x, "title")).with("description", gstr(x, "description"));
                let slot = match gstr(x, "type").as_str() {
                    "link" => 0,
                    "component" => 1,
                    _ => continue,
                };
                if lists[slot].is_null() {
                    lists[slot] = Json::arr();
                }
                lists[slot].push(item);
            }
        }
        _ => {}
    }
    let names = ["addedLinks", "addedComponents", "exposedComponents", "extensionPoints", "addedFunctions"];
    let mut o = Json::obj();
    for (i, l) in lists.into_iter().enumerate() {
        o.set(names[i], if l.is_null() { Json::arr() } else { l });
    }
    o
}

/// The zero `Extensions` of `PluginMetaDTO` (never set by getFSDataSources): every list is null.
fn null_extensions() -> Json {
    Json::obj().with("addedLinks", Json::Null).with("addedComponents", Json::Null).with("exposedComponents", Json::Null).with("extensionPoints", Json::Null).with("addedFunctions", Json::Null)
}

fn angular() -> Json {
    Json::obj().with("detected", false)
}

fn base_url(id: &str) -> String {
    join(&["public/plugins", id])
}

/// pluginassets.LocalProvider.Module: core plugins load from the frontend bundle, others from module.js.
fn module_path(id: &str, dir: &Path, core: bool) -> String {
    let base = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if core && base != "dist" { join(&["core:plugin", base]) } else { join(&["public/plugins", id, "module.js"]) }
}

/// bootstrap getTranslations: language -> `public/plugins/<id>/locales/<lang>/<id>.json`, sorted by language.
fn translations(raw: &Json, id: &str) -> Vec<(String, String)> {
    let mut m: Vec<(String, String)> = Vec::new();
    if let Some(Json::Arr(langs)) = fld(raw, "languages") {
        let file = format!("{id}.json");
        for l in langs.iter() {
            if let Some(l) = l.as_str() {
                if !m.iter().any(|e| e.0 == l) {
                    m.push((l.to_string(), join(&["public/plugins", id, "locales", l, &file])));
                }
            }
        }
    }
    json::sort::sort_by(&mut m, |a, b| a.0.cmp(&b.0));
    m
}

fn map_json(m: &[(String, String)]) -> Json {
    let mut o = Json::obj();
    for (k, v) in m.iter() {
        o.set(k, v.as_str());
    }
    o
}

/// getPanelSort (pkg/api/bootdata.go).
fn panel_sort(id: &str) -> i64 {
    match id {
        "timeseries" => 1,
        "barchart" => 2,
        "stat" => 3,
        "gauge" => 4,
        "bargauge" => 5,
        "table" => 6,
        "singlestat" => 7,
        "piechart" => 8,
        "state-timeline" => 9,
        "heatmap" => 10,
        "status-history" => 11,
        "histogram" => 12,
        "graph" => 13,
        "text" => 14,
        "alertlist" => 15,
        "dashlist" => 16,
        "news" => 17,
        _ => 100,
    }
}

/// Signature status from MANIFEST.txt: (status, signatureType, signedByOrgName). The PGP signature and the file
/// hashes are not checked (see the module comment).
fn signature(dir: &Path, id: &str, raw: &Json) -> (&'static str, String, String) {
    let text = match std::fs::read_to_string(dir.join("MANIFEST.txt")) {
        Ok(t) => t,
        Err(_) => return ("unsigned", String::new(), String::new()),
    };
    let start = text.find('{');
    let end = text.find("-----BEGIN PGP SIGNATURE-----");
    let m = match (start, end) {
        (Some(s), Some(e)) if s < e => match Json::parse(text[s..e].trim()) {
            Ok(m) => m,
            Err(_) => return ("invalid", String::new(), String::new()),
        },
        _ => return ("invalid", String::new(), String::new()),
    };
    if !m.str_of("manifestVersion").starts_with("2.") {
        return ("invalid", String::new(), String::new());
    }
    let version = raw.path("info.version").and_then(|v| v.as_str()).unwrap_or("");
    if m.str_of("plugin") != id || m.str_of("version") != version {
        return ("modified", String::new(), String::new());
    }
    ("valid", m.str_of("signatureType").to_string(), m.str_of("signedByOrgName").to_string())
}

// ---- small helpers ----

/// A field as Go's decoder finds it: case-insensitive, the last match wins; JSON null counts as absent.
fn fld<'a>(j: &'a Json, k: &str) -> Option<&'a Json> {
    let mut found: Option<&'a Json> = None;
    if let Json::Obj(o) = j {
        for e in o.iter() {
            if e.0.eq_ignore_ascii_case(k) {
                found = Some(&e.1);
            }
        }
    }
    match found {
        Some(Json::Null) => None,
        x => x,
    }
}

fn gstr(j: &Json, k: &str) -> String {
    fld(j, k).and_then(|v| v.as_str()).unwrap_or("").to_string()
}

fn gbool(j: &Json, k: &str) -> bool {
    fld(j, k).and_then(|v| v.as_bool()).unwrap_or(false)
}

/// A `[]string` field: the strings, or null when absent.
fn gstrs(j: &Json, k: &str) -> Json {
    match fld(j, k) {
        Some(Json::Arr(a)) => {
            let mut out = Json::arr();
            for v in a.iter() {
                out.push(v.as_str().unwrap_or(""));
            }
            out
        }
        _ => Json::Null,
    }
}

/// A slice field: each element mapped, or null when absent.
fn arr_or_null(v: Option<&Json>, f: impl Fn(&Json) -> Json) -> Json {
    match v {
        Some(Json::Arr(a)) => {
            let mut out = Json::arr();
            for x in a.iter() {
                out.push(f(x));
            }
            out
        }
        _ => Json::Null,
    }
}

fn non_empty_arr(v: &Json) -> bool {
    matches!(v, Json::Arr(a) if !a.is_empty())
}

/// Go `util.SplitString`: comma or space separated, empty items dropped.
fn split_list(s: &str) -> Vec<String> {
    s.split([',', ' ']).map(|x| x.trim()).filter(|x| !x.is_empty()).map(|x| x.to_string()).collect()
}

fn or_default<'a>(v: &'a str, d: &'a str) -> &'a str {
    if v.is_empty() { d } else { v }
}

fn abs_path(home: &Path, p: &str) -> PathBuf {
    if p.starts_with('/') { PathBuf::from(p) } else { home.join(p) }
}

/// Subdirectories (or symlinks to them) of `root`, sorted by name.
fn sorted_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.push((e.file_name().to_string_lossy().to_string(), p));
            }
        }
    }
    json::sort::sort_by(&mut out, |a, b| a.0.cmp(&b.0));
    out.into_iter().map(|e| e.1).collect()
}

/// plugins.Walk: a directory with a `dist` folder is read from `dist` only (built core plugins such as graphite
/// and azuremonitor: their module then loads from public/plugins/<id>/module.js, not the core bundle).
fn plugin_dir(d: PathBuf) -> PathBuf {
    let dist = d.join("dist");
    if dist.is_dir() { dist } else { d }
}

fn read_plugin_json(path: &Path) -> Option<Json> {
    let text = std::fs::read_to_string(path).ok()?;
    match Json::parse(&text) {
        Ok(j) => Some(j),
        Err(e) => {
            eprintln!("grafana-rs: skipping plugin loading as its plugin.json could not be read: {}: {e}", path.display());
            None
        }
    }
}

/// Go `path.Join`: joins the non-empty parts with '/' and cleans the result ("" when every part is empty).
pub fn join(parts: &[&str]) -> String {
    let mut s = String::new();
    for p in parts.iter() {
        if p.is_empty() {
            continue;
        }
        if !s.is_empty() {
            s.push('/');
        }
        s.push_str(p);
    }
    if s.is_empty() { s } else { clean(&s) }
}

/// Go `path.Clean`.
pub fn clean(p: &str) -> String {
    let rooted = p.starts_with('/');
    let mut segs: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if !segs.is_empty() && *segs.last().unwrap_or(&"") != ".." {
                    segs.pop();
                } else if !rooted {
                    segs.push("..");
                }
            }
            s => segs.push(s),
        }
    }
    let body = segs.join("/");
    if rooted {
        format!("/{body}")
    } else if body.is_empty() {
        ".".to_string()
    } else {
        body
    }
}
