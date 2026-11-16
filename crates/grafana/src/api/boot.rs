//! The index page (`public/views/index.html` with `window.grafanaBootData`), `/bootdata`, `/api/frontend/settings`,
//! `/api/frontend/assets`, `/api/access-control/user/actions`, and the frontend page routes (agent A).
//! Upstream: pkg/api/index.go (setIndexViewData), pkg/api/bootdata.go (getFrontendSettings, getFSDataSources),
//! pkg/api/frontendsettings/frontendsettings.go (GetBaseFrontendSettings), pkg/services/navtree/navtreeimpl,
//! pkg/api/api.go (page routes).
//!
//! Cost: the static part of the settings (everything but data sources, the default data source, allowOrgCreate,
//! the license URL, the namespace and the hidden version) is serialized once into text segments; a request writes
//! the segments and the few dynamic values. The permissions map is prebuilt per basic role (`access`). The nav
//! tree is small and built per request.
use crate::access;
use crate::app::App;
use crate::ctx::{Access, Ctx, Role, Routes, SignedInUser};
use crate::error::ApiError;
use crate::settings::Settings;
use http::Response;
use json::Json;
use std::sync::OnceLock;

pub fn routes(r: &mut Routes) {
    // Page routes of upstream pkg/api/api.go that render the index. Fine-grained `authorize(...)` page checks
    // redirect home when they fail (accesscontrol deny for a page).
    r.get("/", Access::SignedIn, index);
    r.get("/invite/:code", Access::Optional, index_public);
    r.get("/profile", Access::NoAnonymous, index);
    r.get("/profile/password", Access::NoAnonymous, index);
    r.get("/profile/notifications", Access::NoAnonymous, index);
    r.get("/org", Access::SignedIn, page_org_prefs);
    r.get("/org/new", Access::GrafanaAdmin, index);
    r.get("/org/users", Access::SignedIn, page_org_users);
    r.get("/org/users/new", Access::Role(Role::Admin), index);
    r.get("/org/users/invite", Access::SignedIn, page_org_users);
    r.get("/org/teams", Access::SignedIn, page_teams);
    r.get("/org/teams/*", Access::SignedIn, page_teams);
    r.get("/org/serviceaccounts", Access::SignedIn, page_service_accounts);
    r.get("/org/serviceaccounts/*", Access::SignedIn, page_service_accounts);
    r.get("/datasources", Access::SignedIn, page_ds_config);
    r.get("/datasources/new", Access::SignedIn, page_ds_new);
    r.get("/datasources/edit/*", Access::SignedIn, page_ds_edit);
    r.get("/datasources/correlations", Access::SignedIn, page_explore);
    r.get("/connections", Access::SignedIn, index);
    r.get("/connections/*", Access::SignedIn, index);
    r.get("/connections/datasources", Access::SignedIn, page_ds_config);
    r.get("/connections/datasources/new", Access::SignedIn, page_ds_new);
    r.get("/connections/datasources/edit/*", Access::SignedIn, page_ds_edit);
    r.get("/connections/add-new-connection", Access::SignedIn, page_ds_config);
    r.get("/admin", Access::Role(Role::Admin), index);
    r.get("/admin/*", Access::SignedIn, page_admin);
    r.get("/plugins", Access::SignedIn, page_plugins);
    r.get("/plugins/*", Access::SignedIn, page_plugins);
    r.get("/a/:id", Access::SignedIn, index);
    r.get("/a/:id/*", Access::SignedIn, index);
    r.get("/d/:uid", Access::SignedIn, index);
    r.get("/d/:uid/:slug", Access::SignedIn, index);
    r.get("/d-solo/:uid", Access::SignedIn, index);
    r.get("/d-solo/:uid/:slug", Access::SignedIn, index);
    r.get("/dashboard/new", Access::SignedIn, index);
    r.get("/dashboard/import", Access::SignedIn, index);
    r.get("/dashboard/*", Access::SignedIn, index);
    r.get("/dashboard-solo/*", Access::SignedIn, index);
    r.get("/import/dashboard", Access::SignedIn, index);
    r.get("/dashboards", Access::SignedIn, index);
    r.get("/dashboards/*", Access::SignedIn, index);
    r.get("/explore", Access::SignedIn, page_explore);
    r.get("/drilldown", Access::SignedIn, page_explore);
    r.get("/playlists", Access::SignedIn, index);
    r.get("/playlists/*", Access::SignedIn, index);
    r.get("/alerting", Access::SignedIn, index);
    r.get("/alerting/*", Access::SignedIn, index);
    r.get("/library-panels", Access::SignedIn, index);
    r.get("/bookmarks", Access::SignedIn, index);
    r.get("/styleguide", Access::SignedIn, index);
    r.get("/support-bundles", Access::SignedIn, index);
    r.get("/support-bundles/*", Access::SignedIn, index);
    r.get("/verify", Access::Optional, index_public);
    r.get("/signup", Access::Optional, index_public);
    r.get("/user/password/send-reset-email", Access::Optional, index_public);
    r.get("/user/password/reset", Access::Optional, index_public);

    r.get("/bootdata", Access::Optional, get_bootdata);
    r.get("/api/frontend/settings", Access::Optional, get_frontend_settings);
    r.get("/api/frontend/assets", Access::Optional, get_frontend_assets);
    r.get("/api/access-control/user/actions", Access::SignedIn, get_user_actions);
}

// ---------------------------------------------------------------- page checks (accesscontrol authorize on pages)

fn page_if(c: &mut Ctx<'_>, ok: bool) -> Result<Response, ApiError> {
    if ok { index(c) } else { Ok(Response::redirect(302, &format!("{}/", c.app.cfg.app_sub_url))) }
}
fn page_org_prefs(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let u = &c.user;
    let ok = (access::has(u, "orgs:read") && access::has(u, "orgs:write")) || (access::has(u, "orgs.preferences:read") && access::has(u, "orgs.preferences:write"));
    page_if(c, ok)
}
fn page_org_users(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let ok = access::has(&c.user, "org.users:read");
    page_if(c, ok)
}
fn page_teams(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let ok = teams_access(&c.user);
    page_if(c, ok)
}
fn page_service_accounts(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let ok = access::has(&c.user, "serviceaccounts:read");
    page_if(c, ok)
}
fn page_ds_config(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let ok = ds_config_access(&c.user);
    page_if(c, ok)
}
fn page_ds_new(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let ok = access::has(&c.user, "datasources:read") && access::has(&c.user, "datasources:create");
    page_if(c, ok)
}
fn page_ds_edit(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let ok = access::has(&c.user, "datasources:read") && access::has(&c.user, "datasources:write");
    page_if(c, ok)
}
fn page_explore(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let ok = access::has(&c.user, "datasources:explore");
    page_if(c, ok)
}
fn page_plugins(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let ok = can_admin_plugins(c.app, &c.user);
    page_if(c, ok)
}
fn page_admin(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let ok = c.user.has_role(Role::Admin) || c.user.is_grafana_admin || access::has(&c.user, "org.users:read");
    page_if(c, ok)
}
/// Pages without an auth requirement (invite, sign up, password reset).
fn index_public(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    index(c)
}

fn teams_access(u: &SignedInUser) -> bool {
    access::has(u, "teams:create") || (access::has(u, "teams:read") && (access::has(u, "teams:write") || access::has(u, "teams.permissions:write")))
}
fn ds_config_access(u: &SignedInUser) -> bool {
    access::has(u, "datasources:create") || (access::has(u, "datasources:read") && (access::has(u, "datasources:delete") || access::has(u, "datasources:write")))
}
fn can_admin_plugins(app: &App, u: &SignedInUser) -> bool {
    (app.cfg.get_bool("plugins", "plugin_admin_enabled") && u.is_grafana_admin) || access::has_any(u, &["plugins:write", "plugins:install"])
}

// ---------------------------------------------------------------- assets

/// `assets` of the boot data from `public/build/assets-manifest.json` (pkg/api/webassets). Loaded once by `App::new`.
pub fn load_assets(app_home: &std::path::Path) -> Json {
    let text = std::fs::read_to_string(app_home.join("public/build/assets-manifest.json")).unwrap_or_default();
    let m = Json::parse(&text).unwrap_or(Json::Null);
    let integrity = |src: &str| -> String {
        if let Some(o) = m.as_obj() {
            for (_, v) in o.iter() {
                if v.str_of("src") == src {
                    return v.str_of("integrity").to_string();
                }
            }
        }
        String::new()
    };
    let files = |path: &str| -> Json {
        let mut a = Json::arr();
        if let Some(list) = m.path(path).and_then(|v| v.as_arr()) {
            for f in list.iter() {
                let src = f.as_str().unwrap_or("");
                a.push(Json::obj().with("filePath", src).with("integrity", integrity(src)));
            }
        }
        a
    };
    let first = |path: &str| -> String { m.path(path).and_then(|v| v.at(0)).and_then(|v| v.as_str()).unwrap_or("").to_string() };
    Json::obj()
        .with("jsFiles", files("entrypoints.app.assets.js"))
        .with("cssFiles", files("entrypoints.app.assets.css"))
        .with("dark", first("entrypoints.dark.assets.css"))
        .with("light", first("entrypoints.light.assets.css"))
}

// ---------------------------------------------------------------- static settings

/// The frontend settings with every per-request value replaced by a marker, serialized and split at the markers.
struct SettingsTpl {
    parts: Vec<Part>,
}
enum Part {
    Text(String),
    DefaultDs,
    Datasources,
    AllowOrgCreate,
    LicenseUrl,
    Namespace,
    BuildInfo,
}
const MARKERS: [&str; 6] = ["\u{1}defaultDatasource", "\u{1}datasources", "\u{1}allowOrgCreate", "\u{1}licenseUrl", "\u{1}namespace", "\u{1}buildInfo"];

fn marker(i: usize) -> Json {
    Json::Str(MARKERS[i].to_string())
}

fn cfg_str(cfg: &Settings, s: &str, k: &str) -> Json {
    Json::Str(cfg.get(s, k).to_string())
}
/// The setting, or `default` when it is unset or empty (setting.valueAsString).
pub fn cfg_or(cfg: &Settings, s: &str, k: &str, default: &str) -> String {
    match cfg.get(s, k) {
        "" => default.to_string(),
        v => v.to_string(),
    }
}
fn cfg_bool(cfg: &Settings, s: &str, k: &str, default: bool) -> bool {
    match cfg.get(s, k) {
        "" => default,
        v => v == "true" || v == "1",
    }
}
fn str_list(cfg: &Settings, s: &str, k: &str) -> Json {
    let mut a = Json::arr();
    for p in cfg.get(s, k).split(|c: char| c == ',' || c == ' ') {
        let p = p.trim();
        if !p.is_empty() {
            a.push(Json::Str(p.to_string()));
        }
    }
    a
}
fn ms_of(cfg: &Settings, s: &str, k: &str, default_ms: i64) -> i64 {
    match crate::settings::parse_duration_secs(cfg.get(s, k)) {
        Some(v) => v * 1000,
        None => default_ms,
    }
}

/// GetBaseFrontendSettings + the static parts of getFrontendSettings, in Grafana's key order.
fn build_static_settings(app: &App) -> Json {
    let cfg = &app.cfg;
    let mut toggles: Vec<String> = crate::api::ofrep::enabled_flags(cfg).to_vec();
    toggles.push("topnav".to_string());
    toggles.sort();
    toggles.dedup();
    let mut ft = Json::obj();
    let mut placed = false;
    for t in toggles.iter() {
        if !placed && t.as_str() > "dashboardNewLayouts" {
            ft.set("dashboardNewLayouts", false);
            placed = true;
        }
        ft.set(t, true);
    }
    if !placed {
        ft.set("dashboardNewLayouts", false);
    }
    let date_formats = Json::obj()
        .with("fullDate", cfg_or(cfg, "date_formats", "full_date", "YYYY-MM-DD HH:mm:ss"))
        .with("useBrowserLocale", cfg_bool(cfg, "date_formats", "use_browser_locale", false))
        .with(
            "interval",
            Json::obj()
                .with("millisecond", cfg_or(cfg, "date_formats", "interval_millisecond", "HH:mm:ss.SSS"))
                .with("second", cfg_or(cfg, "date_formats", "interval_second", "HH:mm:ss"))
                .with("minute", cfg_or(cfg, "date_formats", "interval_minute", "HH:mm"))
                .with("hour", cfg_or(cfg, "date_formats", "interval_hour", "MM-DD HH:mm"))
                .with("day", cfg_or(cfg, "date_formats", "interval_day", "YYYY-MM-DD"))
                .with("month", cfg_or(cfg, "date_formats", "interval_month", "YYYY-MM"))
                .with("year", "YYYY"),
        )
        .with("defaultTimezone", cfg_or(cfg, "date_formats", "default_timezone", "browser"))
        .with("defaultWeekStart", cfg_or(cfg, "date_formats", "default_week_start", "browser"));
    let preinstalled = Json::parse(PREINSTALLED).unwrap_or(Json::arr());
    let js_agent = Json::obj()
        .with("enabled", cfg_bool(cfg, "log.frontend", "enabled", false))
        .with("customEndpoint", cfg_str(cfg, "log.frontend", "custom_endpoint"))
        .with("apiKey", cfg_str(cfg, "log.frontend", "api_key"))
        .with("internalLoggerLevel", cfg.get_int("log.frontend", "internal_logger_level", 0))
        .with("consoleInstrumentalizationEnabled", cfg_bool(cfg, "log.frontend", "instrumentations_console_enabled", true))
        .with("performanceInstrumentalizationEnabled", cfg_bool(cfg, "log.frontend", "instrumentations_webvitals_enabled", true))
        .with("cspInstrumentalizationEnabled", cfg_bool(cfg, "log.frontend", "instrumentations_csp_enabled", true))
        .with("tracingInstrumentalizationEnabled", cfg_bool(cfg, "log.frontend", "instrumentations_tracing_enabled", true))
        .with("botFilterEnabled", cfg_bool(cfg, "log.frontend", "bot_filter_enabled", false));
    let anon_device_limit = cfg.get_int("auth.anonymous", "device_limit", 0);
    let renderer_scale = match cfg.get("rendering", "default_image_scale").parse::<i64>() {
        Ok(v) => v,
        Err(_) => 1,
    };
    let protocol = cfg.get("server", "protocol");
    Json::obj()
        .with("defaultDatasource", marker(0))
        .with("datasources", marker(1))
        .with("minRefreshInterval", cfg_str(cfg, "dashboards", "min_refresh_interval"))
        .with("panels", Json::Str("\u{1}panels".to_string()))
        .with("apps", Json::Str("\u{1}apps".to_string()))
        .with("appUrl", cfg.app_url.as_str())
        .with("appSubUrl", cfg.app_sub_url.as_str())
        .with("allowOrgCreate", marker(2))
        .with("authProxyEnabled", cfg_bool(cfg, "auth.proxy", "enabled", false))
        .with("ldapEnabled", cfg_bool(cfg, "auth.ldap", "enabled", false))
        .with("jwtHeaderName", if cfg_bool(cfg, "auth.jwt", "enabled", false) { cfg_str(cfg, "auth.jwt", "header_name") } else { Json::Str(String::new()) })
        .with("jwtUrlLogin", cfg_bool(cfg, "auth.jwt", "enabled", false) && cfg_bool(cfg, "auth.jwt", "url_login", false))
        // Live (websocket) is out of scope: the UI then does not open /api/live/ws.
        .with("liveEnabled", false)
        .with("liveMessageSizeLimit", cfg.get_int("live", "message_size_limit", 8388608))
        .with("liveNamespaced", true)
        .with("autoAssignOrg", cfg_bool(cfg, "users", "auto_assign_org", true))
        .with("verifyEmailEnabled", cfg_bool(cfg, "users", "verify_email_enabled", false))
        .with("sigV4AuthEnabled", cfg_bool(cfg, "auth", "sigv4_auth_enabled", false))
        .with("azureAuthEnabled", cfg_bool(cfg, "auth", "azure_auth_enabled", false))
        .with("rbacEnabled", true)
        .with("exploreEnabled", cfg_bool(cfg, "explore", "enabled", true))
        .with("helpEnabled", cfg_bool(cfg, "help", "enabled", true))
        .with("profileEnabled", cfg_bool(cfg, "profile", "enabled", true))
        .with("newsFeedEnabled", cfg_bool(cfg, "news", "news_feed_enabled", true))
        .with("queryHistoryEnabled", cfg_bool(cfg, "query_history", "enabled", true))
        .with("annotationAppPlatformEnabled", false)
        .with("googleAnalyticsId", cfg_str(cfg, "analytics", "google_analytics_ua_id"))
        .with("googleAnalytics4Id", cfg_str(cfg, "analytics", "google_analytics_4_id"))
        .with("GoogleAnalytics4SendManualPageViews", cfg_bool(cfg, "analytics", "google_analytics_4_send_manual_page_views", false))
        .with("rudderstackWriteKey", cfg_str(cfg, "analytics", "rudderstack_write_key"))
        .with("rudderstackDataPlaneUrl", cfg_str(cfg, "analytics", "rudderstack_data_plane_url"))
        .with("rudderstackSdkUrl", cfg_str(cfg, "analytics", "rudderstack_sdk_url"))
        .with("rudderstackV3SdkUrl", cfg_str(cfg, "analytics", "rudderstack_v3_sdk_url"))
        .with("rudderstackConfigUrl", cfg_str(cfg, "analytics", "rudderstack_config_url"))
        .with("rudderstackIntegrationsUrl", cfg_str(cfg, "analytics", "rudderstack_integrations_url"))
        .with("rudderstackBatchInterval", 0)
        .with("postHogToken", "")
        .with("postHogHost", "")
        .with("analyticsConsoleReporting", cfg_bool(cfg, "analytics", "browser_console_reporter", false))
        .with("pluginImportTelemetryPackages", Json::arr())
        .with("dashboardPerformanceMetrics", Json::arr())
        .with("panelSeriesLimit", 0)
        .with("dashboardDefaultPreload", false)
        .with("feedbackLinksEnabled", cfg_bool(cfg, "analytics", "feedback_links_enabled", true))
        .with("applicationInsightsConnectionString", cfg_str(cfg, "analytics", "application_insights_connection_string"))
        .with("applicationInsightsEndpointUrl", cfg_str(cfg, "analytics", "application_insights_endpoint_url"))
        .with("applicationInsightsAutoRouteTracking", cfg_bool(cfg, "analytics", "application_insights_auto_route_tracking", true))
        .with("disableLoginForm", cfg.disable_login_form)
        .with("disableUserSignUp", !cfg_bool(cfg, "users", "allow_sign_up", false))
        .with("loginHint", cfg_str(cfg, "users", "login_hint"))
        .with("passwordHint", cfg_str(cfg, "users", "password_hint"))
        .with("externalUserMngInfo", cfg_str(cfg, "users", "external_manage_info"))
        .with("externalUserMngLinkUrl", cfg_str(cfg, "users", "external_manage_link_url"))
        .with("externalUserMngLinkName", cfg_str(cfg, "users", "external_manage_link_name"))
        .with("externalUserMngAnalytics", false)
        .with("externalUserMngAnalyticsParams", "")
        .with("externalUserUpgradeLinkUrl", "")
        .with("viewersCanEdit", cfg_bool(cfg, "users", "viewers_can_edit", false))
        .with("disableSanitizeHtml", cfg_bool(cfg, "panels", "disable_sanitize_html", false))
        .with("trustedTypesDefaultPolicyEnabled", false)
        .with("cspReportOnlyEnabled", cfg_bool(cfg, "security", "content_security_policy_report_only", false))
        .with("enableFrontendSandboxForPlugins", Json::arr().push_item(Json::Str(cfg.get("security", "enable_frontend_sandbox_for_plugins").to_string())))
        .with("pluginRestrictedAPIsAllowList", Json::parse(r#"{"dashboardMutationAPI":["grafana-assistant-app"],"extensionSidebar":["grafana-assistant-app"]}"#).unwrap_or(Json::obj()))
        .with("pluginRestrictedAPIsBlockList", Json::obj())
        .with("exploreDefaultTimeOffset", if cfg.get("explore", "defaultTimeOffset").is_empty() { Json::Str("1h".to_string()) } else { cfg_str(cfg, "explore", "defaultTimeOffset") })
        .with("exploreHideLogsDownload", cfg_bool(cfg, "explore", "hide_logs_download", false))
        .with(
            "auth",
            Json::obj()
                .with("disableLogin", cfg_bool(cfg, "auth", "disable_login", false))
                .with("basicAuthStrongPasswordPolicy", cfg_bool(cfg, "auth.basic", "password_policy", false))
                .with("disableSignoutMenu", cfg_bool(cfg, "auth", "disable_signout_menu", false)),
        )
        .with("buildInfo", marker(5))
        .with(
            "licenseInfo",
            Json::obj().with("expiry", 0).with("stateInfo", "").with("licenseUrl", marker(3)).with("edition", "Open Source").with("enabledFeatures", Json::obj()),
        )
        .with("featureToggles", ft)
        .with("anonymousEnabled", cfg.anonymous_enabled)
        .with("anonymousDeviceLimit", anon_device_limit)
        .with("rendererAvailable", false)
        .with("rendererVersion", "")
        .with("rendererDefaultImageWidth", cfg.get_int("rendering", "default_image_width", 1000))
        .with("rendererDefaultImageHeight", cfg.get_int("rendering", "default_image_height", 500))
        .with("rendererDefaultImageScale", renderer_scale)
        .with("http2Enabled", protocol == "h2" || protocol == "socket_h2")
        .with("grafanaJavascriptAgent", js_agent)
        .with("pluginCatalogURL", cfg_str(cfg, "plugins", "plugin_catalog_url"))
        .with("pluginAdminEnabled", cfg_bool(cfg, "plugins", "plugin_admin_enabled", true))
        .with("pluginAdminExternalManageEnabled", cfg_bool(cfg, "plugins", "plugin_admin_enabled", true) && cfg_bool(cfg, "plugins", "plugin_admin_external_manage_enabled", false))
        .with("pluginCatalogHiddenPlugins", str_list(cfg, "plugins", "plugin_catalog_hidden_plugins"))
        .with("pluginCatalogManagedPlugins", Json::arr())
        .with("pluginCatalogPreinstalledPlugins", preinstalled)
        .with("pluginCatalogPreinstalledAutoUpdate", true)
        .with("expressionsEnabled", cfg_bool(cfg, "expressions", "enabled", true))
        .with("awsAllowedAuthProviders", str_list(cfg, "aws", "allowed_auth_providers"))
        .with("awsAssumeRoleEnabled", cfg_bool(cfg, "aws", "assume_role_enabled", true))
        .with("awsPerDatasourceHTTPProxyEnabled", cfg_bool(cfg, "aws", "per_datasource_http_proxy_enabled", false))
        .with("supportBundlesEnabled", cfg_bool(cfg, "support_bundles", "enabled", true))
        .with("snapshotEnabled", cfg_bool(cfg, "snapshots", "enabled", true))
        .with("secureSocksDSProxyEnabled", false)
        .with("reportingStaticContext", Json::obj())
        .with("azure", Json::obj().with("cloud", if cfg.get("azure", "cloud").is_empty() { Json::Str("AzureCloud".to_string()) } else { cfg_str(cfg, "azure", "cloud") }))
        .with("defaultDatasourceManageAlertsUiToggle", true)
        .with("defaultAllowRecordingRulesTargetAlertsUiToggle", true)
        .with(
            "caching",
            Json::obj()
                .with("enabled", cfg_bool(cfg, "caching", "enabled", true))
                .with("cleanCacheEnabled", cfg_bool(cfg, "caching", "clean_cache_enabled", true))
                .with("defaultTTLMs", ms_of(cfg, "caching", "ttl", 300000)),
        )
        .with("recordedQueries", Json::obj().with("enabled", cfg_bool(cfg, "recorded_queries", "enabled", true)))
        .with("reporting", Json::obj().with("enabled", cfg_bool(cfg, "reporting", "enabled", true)))
        .with("analytics", Json::obj().with("enabled", cfg_bool(cfg, "analytics", "enabled", true)))
        .with("unifiedAlertingEnabled", true)
        .with(
            "unifiedAlerting",
            Json::parse(r#"{"minInterval":"10s","stateHistory":{"backend":"annotations","prometheusMetricName":"GRAFANA_ALERTS"},"recordingRulesEnabled":true,"notificationHistoryEnabled":false,"alertStateHistoryBackend":"annotations"}"#).unwrap_or(Json::obj()),
        )
        .with("oauth", Json::obj())
        .with("samlEnabled", false)
        .with("samlName", "")
        .with("tokenExpirationDayLimit", cfg.get_int("service_accounts", "token_expiration_day_limit", -1))
        .with("sharedWithMeFolderUID", "sharedwithme")
        .with("rootFolderUID", "general")
        .with("geomapDisableCustomBaseLayer", cfg_bool(cfg, "geomap", "disable_custom_baselayers", false))
        .with("publicDashboardAccessToken", "")
        .with("publicDashboardsEnabled", cfg_bool(cfg, "public_dashboards", "enabled", true))
        .with("cloudMigrationEnabled", true)
        .with("cloudMigrationIsTarget", false)
        .with("cloudMigrationPollIntervalMs", 2000)
        .with("dateFormats", date_formats)
        .with("namespace", marker(4))
        .with("sqlConnectionLimits", Json::obj().with("maxOpenConns", 100).with("maxIdleConns", 100).with("connMaxLifetime", 14400))
        .with("localFileSystemAvailable", true)
        .with("provisioningEnabled", true)
        .with("listScopesEndpoint", "")
        .with("listDashboardScopesEndpoint", "")
        .with("openFeatureContext", Json::obj().with("grafana_version", cfg.build_version.as_str()).with("namespace", "default"))
}

/// Preinstalled plugins of `[plugins] preinstall` (Grafana builds this list from a map; the order is the recording's).
const PREINSTALLED: &str = r#"[{"id":"influxdb","version":""},{"id":"zipkin","version":""},{"id":"loki","version":""},{"id":"mssql","version":""},{"id":"mysql","version":""},{"id":"cloudwatch","version":""},{"id":"grafana-postgresql-datasource","version":""},{"id":"tempo","version":""},{"id":"grafana-pyroscope-app","version":""},{"id":"opentsdb","version":""},{"id":"jaeger","version":""},{"id":"grafana-pyroscope-datasource","version":""},{"id":"grafana-metricsdrilldown-app","version":""},{"id":"grafana-advisor-app","version":""},{"id":"grafana-lokiexplore-app","version":""},{"id":"prometheus","version":""},{"id":"stackdriver","version":""},{"id":"grafana-exploretraces-app","version":""},{"id":"elasticsearch","version":""}]"#;

fn settings_tpl(app: &App) -> &'static SettingsTpl {
    static T: OnceLock<SettingsTpl> = OnceLock::new();
    T.get_or_init(|| {
        let mut text = build_static_settings(app).stringify();
        // static plugin sections go in as text
        text = text.replace(&json::quote("\u{1}panels"), &app.plugins.panels_json);
        text = text.replace(&json::quote("\u{1}apps"), &app.plugins.apps_json);
        let quoted: Vec<String> = MARKERS.iter().map(|m| json::quote(m)).collect();
        let mut parts = Vec::new();
        let mut rest = text.as_str();
        loop {
            let mut best: Option<(usize, usize)> = None;
            for (i, q) in quoted.iter().enumerate() {
                if let Some(p) = rest.find(q.as_str()) {
                    if best.map_or(true, |b| p < b.0) {
                        best = Some((p, i));
                    }
                }
            }
            match best {
                Some((p, i)) => {
                    parts.push(Part::Text(rest[..p].to_string()));
                    parts.push(match i {
                        0 => Part::DefaultDs,
                        1 => Part::Datasources,
                        2 => Part::AllowOrgCreate,
                        3 => Part::LicenseUrl,
                        4 => Part::Namespace,
                        _ => Part::BuildInfo,
                    });
                    rest = &rest[p + quoted[i].len()..];
                }
                None => {
                    parts.push(Part::Text(rest.to_string()));
                    break;
                }
            }
        }
        SettingsTpl { parts }
    })
}

/// Grafana's namespace mapper: org 1 is "default", others "org-<id>".
pub fn namespace(org_id: i64) -> String {
    if org_id == 1 { "default".to_string() } else { format!("org-{org_id}") }
}

/// The `meta` JSON text of every data source plugin, serialized once.
fn meta_text<'a>(app: &'a App, id: &str) -> Option<&'a str> {
    static M: OnceLock<Vec<(String, String)>> = OnceLock::new();
    let all = M.get_or_init(|| {
        let mut v = Vec::new();
        for p in app.plugins.list.iter() {
            if p.kind == "datasource" {
                v.push((p.id.clone(), p.ds_meta.stringify()));
            }
        }
        v
    });
    let pid = match app.plugins.get(id) {
        Some(p) => p.id.as_str(),
        None => id,
    };
    for (k, t) in all.iter() {
        if k == pid {
            return Some(t.as_str());
        }
    }
    None
}

/// getFSDataSources: the org's data sources (by name, Go map order = sorted keys) plus the built-in ones.
fn write_datasources(c: &Ctx<'_>, out: &mut String) -> String {
    let app = c.app;
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut default_ds = String::from("-- Grafana --");
    if c.user.org_id != 0 && access::has(&c.user, "datasources:read") {
        if let Ok(list) = app.store.datasources(c.user.org_id) {
            for ds in list.iter() {
                let meta = match app.plugins.datasource_meta(&ds.type_) {
                    Some(m) => m,
                    None => continue,
                };
                let plugin_id = match app.plugins.get(&ds.type_) {
                    Some(p) => p.id.clone(),
                    None => ds.type_.clone(),
                };
                let module = meta.str_of("module").to_string();
                let url = if ds.access == "proxy" { format!("/api/datasources/proxy/uid/{}", ds.uid) } else { ds.url.clone() };
                let mut jd = Json::parse(&ds.json_data).unwrap_or(Json::obj());
                if !matches!(jd, Json::Obj(_)) {
                    jd = Json::obj();
                }
                if matches!(ds.type_.as_str(), "prometheus" | "grafana-amazonprometheus-datasource" | "grafana-azureprometheus-datasource") {
                    jd.set("directUrl", ds.url.as_str());
                }
                if matches!(ds.type_.as_str(), "mssql" | "mysql" | "grafana-postgresql-datasource" | "postgres") && jd.str_of("database").is_empty() {
                    jd.set("database", ds.database.as_str());
                }
                sort_obj(&mut jd);
                let mut o = Json::obj();
                if ds.id != 0 {
                    o.set("id", ds.id);
                }
                if !ds.uid.is_empty() {
                    o.set("uid", ds.uid.as_str());
                }
                o.set("type", plugin_id.as_str());
                o.set("name", ds.name.as_str());
                o.set("meta", Json::Str("\u{1}meta".to_string()));
                if !url.is_empty() {
                    o.set("url", url.as_str());
                }
                o.set("isDefault", ds.is_default);
                if !ds.access.is_empty() {
                    o.set("access", ds.access.as_str());
                }
                o.set("preload", meta.bool_of("preload"));
                if !module.is_empty() {
                    o.set("module", module.as_str());
                }
                o.set("jsonData", jd);
                o.set("readOnly", ds.read_only);
                if !ds.api_version.is_empty() {
                    o.set("apiVersion", ds.api_version.as_str());
                }
                if ds.access == "direct" && ds.with_credentials {
                    o.set("withCredentials", true);
                }
                o.set("cachingConfig", Json::obj().with("enabled", false).with("TTLMs", 0));
                if matches!(ds.type_.as_str(), "influxdb" | "elasticsearch") && !ds.database.is_empty() {
                    o.set("database", ds.database.as_str());
                }
                let text = o.stringify().replace(&json::quote("\u{1}meta"), meta_text(app, &ds.type_).unwrap_or("null"));
                if ds.is_default {
                    default_ds = ds.name.clone();
                }
                entries.push((ds.name.clone(), text));
            }
        }
    }
    for p in app.plugins.builtin_datasources() {
        let mut o = Json::obj();
        if p.id == "grafana" {
            o.set("id", -1);
            o.set("uid", "grafana");
        }
        o.set("type", p.kind.as_str());
        o.set("name", p.name.as_str());
        o.set("meta", Json::Str("\u{1}meta".to_string()));
        o.set("isDefault", false);
        o.set("preload", false);
        o.set("jsonData", Json::obj());
        o.set("readOnly", false);
        o.set("cachingConfig", Json::obj().with("enabled", false).with("TTLMs", 0));
        let text = o.stringify().replace(&json::quote("\u{1}meta"), meta_text(app, &p.id).unwrap_or("null"));
        entries.retain(|e| e.0 != p.name);
        entries.push((p.name.clone(), text));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    out.push('{');
    for (i, (name, text)) in entries.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        json::write_str(name, out);
        out.push(':');
        out.push_str(text);
    }
    out.push('}');
    default_ds
}

fn sort_obj(j: &mut Json) {
    if let Json::Obj(o) = j {
        o.sort_by(|a, b| a.0.cmp(&b.0));
    }
}

/// Writes the frontend settings JSON for the request identity.
pub fn write_settings(c: &Ctx<'_>, out: &mut String) {
    let app = c.app;
    let cfg = &app.cfg;
    let tpl = settings_tpl(app);
    let mut ds_text = String::with_capacity(8192);
    let default_ds = write_datasources(c, &mut ds_text);
    let hide_version = cfg.anonymous_hide_version && !c.user.is_signed_in();
    for p in tpl.parts.iter() {
        match p {
            Part::Text(t) => out.push_str(t),
            Part::DefaultDs => json::write_str(&default_ds, out),
            Part::Datasources => out.push_str(&ds_text),
            Part::AllowOrgCreate => {
                let v = (cfg_bool(cfg, "users", "allow_org_create", false) && c.user.is_signed_in()) || c.user.is_grafana_admin;
                out.push_str(if v { "true" } else { "false" });
            }
            Part::LicenseUrl => {
                if c.user.is_grafana_admin {
                    json::write_str(&format!("{}/admin/upgrading", cfg.app_sub_url), out);
                } else {
                    out.push_str("\"https://grafana.com/oss/grafana?utm_source=grafana_footer\"");
                }
            }
            Part::Namespace => json::write_str(&namespace(c.user.org_id), out),
            Part::BuildInfo => {
                let commit_short: String = cfg.build_commit.chars().take(10).collect();
                let (version, version_string, commit, short, stamp) = if hide_version {
                    (String::new(), "Grafana".to_string(), String::new(), String::new(), 0)
                } else {
                    (cfg.build_version.clone(), format!("Grafana v{} ({})", cfg.build_version, commit_short), cfg.build_commit.clone(), commit_short.clone(), BUILD_STAMP)
                };
                let b = Json::obj()
                    .with("hideVersion", false)
                    .with("version", version)
                    .with("versionString", version_string)
                    .with("commit", commit)
                    .with("commitShort", short)
                    .with("buildstamp", stamp)
                    .with("edition", "Open Source")
                    .with("latestVersion", "")
                    .with("hasUpdate", false)
                    .with("env", cfg.env.as_str());
                b.write(out);
            }
        }
    }
}

/// Build time of the Grafana 13.3.1 release binary (`setting.BuildStamp`).
const BUILD_STAMP: i64 = 1794326153;

// ---------------------------------------------------------------- user

/// The user's merged preferences as the boot data uses them (theme id, timezone, week start, language).
fn theme_type(id: &str) -> Option<&'static str> {
    for (t, ty) in THEMES.iter() {
        if *t == id {
            return Some(ty);
        }
    }
    None
}
/// pkg/services/preference/themes_generated.go: (id, type).
pub const THEMES: [(&str, &str); 21] = [
    ("light", "light"),
    ("dark", "dark"),
    ("system", "dark"),
    ("aubergine", "dark"),
    ("debug", "dark"),
    ("desertbloom", "light"),
    ("deut_prot_dark", "dark"),
    ("deut_prot_light", "light"),
    ("gildedgrove", "dark"),
    ("gloom", "dark"),
    ("mars", "dark"),
    ("matrix", "dark"),
    ("sapphiredusk", "dark"),
    ("synthwave", "dark"),
    ("tritanopia_dark", "dark"),
    ("tritanopia_light", "light"),
    ("tron", "dark"),
    ("victorian", "dark"),
    ("visual_refresh_dark", "dark"),
    ("visual_refresh_light", "light"),
    ("zen", "light"),
];

/// `/avatar/<sha256 of the lowercased email>` (dtos.GetGravatarUrl), or "" without an email.
pub fn gravatar_url(cfg: &Settings, email: &str) -> String {
    if cfg.get_bool("security", "disable_gravatar") {
        return format!("{}/public/img/user_profile.png", cfg.app_sub_url);
    }
    let s = email.trim();
    if s.is_empty() {
        return String::new();
    }
    format!("{}/avatar/{}", cfg.app_sub_url, crypto::hex(&crypto::sha256(s.to_lowercase().as_bytes())))
}

const PERM_MARKER: &str = "\u{1}permissions";
const SETTINGS_MARKER: &str = "\u{1}settings";

/// The boot data `user` (dtos.CurrentUser), with the permissions as a marker string.
fn user_json(c: &Ctx<'_>) -> (Json, String) {
    let u = &c.user;
    let app = c.app;
    let cfg = &app.cfg;
    let prefs = crate::api::users::merged_prefs(app, u);
    let url_theme = c.req.query_get("theme").unwrap_or_default();
    let theme = if !url_theme.is_empty() && theme_type(&url_theme).is_some() {
        url_theme
    } else if theme_type(&prefs.theme).is_some() {
        prefs.theme.clone()
    } else {
        cfg.get("users", "default_theme").to_string()
    };
    let ttype = theme_type(&theme).unwrap_or("dark");
    let locale = match c.req.header("accept-language") {
        Some(h) if !h.is_empty() => h.split(',').next().unwrap_or("en-US").to_string(),
        _ => "en-US".to_string(),
    };
    let url_lang = c.req.query_get("lang").unwrap_or_default();
    let language = if !url_lang.is_empty() { url_lang } else { prefs.language.clone() };
    let org_count = if u.user_id == 0 || u.is_anonymous {
        1
    } else {
        match app.store.orgs_of_user(u.user_id) {
            Ok(l) => l.len() as i64,
            Err(_) => 1,
        }
    };
    let role = if !u.is_identified() { "" } else { u.org_role.as_str() };
    let name = if u.name.is_empty() { u.login.as_str() } else { u.name.as_str() };
    let analytics = if u.is_signed_in() && !u.is_service_account { format!("{}@{}", u.email, cfg.app_url) } else { format!("@{}", cfg.app_url) };
    let j = Json::obj()
        .with("isSignedIn", u.is_signed_in())
        .with("id", if u.is_anonymous { 0 } else { u.user_id })
        .with("uid", u.uid.as_str())
        .with("login", u.login.as_str())
        .with("email", u.email.as_str())
        .with("name", name)
        .with("theme", theme.as_str())
        .with("lightTheme", ttype == "light")
        .with("orgCount", org_count)
        .with("orgId", u.org_id)
        .with("orgName", u.org_name.as_str())
        .with("orgRole", role)
        .with("isGrafanaAdmin", u.is_grafana_admin)
        .with("gravatarUrl", gravatar_url(cfg, &u.email))
        .with("timezone", prefs.timezone.as_str())
        .with("weekStart", prefs.week_start.as_str())
        .with("locale", locale)
        .with("language", language)
        .with("hasEditPermissionInFolders", access::has_any(u, &["dashboards:create", "folders:create"]))
        .with("authenticatedBy", "")
        .with("permissions", PERM_MARKER)
        .with("analytics", Json::obj().with("identifier", analytics));
    (j, ttype.to_string())
}

// ---------------------------------------------------------------- nav tree

#[derive(Default)]
struct Nav {
    id: String,
    text: String,
    sub_title: String,
    icon: String,
    img: String,
    url: String,
    sort_weight: i64,
    hide_from_tabs: bool,
    round_icon: bool,
    is_section: bool,
    children: Vec<Nav>,
    empty_message_id: String,
    plugin_id: String,
    is_create_action: bool,
    keywords: Vec<&'static str>,
}

fn nav(id: &str, text: &str, sub: &str, icon: &str, url: String) -> Nav {
    Nav { id: id.to_string(), text: text.to_string(), sub_title: sub.to_string(), icon: icon.to_string(), url, ..Nav::default() }
}

impl Nav {
    fn to_json(&self) -> Json {
        let mut o = Json::obj();
        if !self.id.is_empty() {
            o.set("id", self.id.as_str());
        }
        o.set("text", self.text.as_str());
        if !self.sub_title.is_empty() {
            o.set("subTitle", self.sub_title.as_str());
        }
        if !self.icon.is_empty() {
            o.set("icon", self.icon.as_str());
        }
        if !self.img.is_empty() {
            o.set("img", self.img.as_str());
        }
        if !self.url.is_empty() {
            o.set("url", self.url.as_str());
        }
        if self.sort_weight != 0 {
            o.set("sortWeight", self.sort_weight);
        }
        if self.hide_from_tabs {
            o.set("hideFromTabs", true);
        }
        if self.round_icon {
            o.set("roundIcon", true);
        }
        if self.is_section {
            o.set("isSection", true);
        }
        if !self.children.is_empty() {
            let mut a = Json::arr();
            for ch in self.children.iter() {
                a.push(ch.to_json());
            }
            o.set("children", a);
        }
        if !self.empty_message_id.is_empty() {
            o.set("emptyMessageId", self.empty_message_id.as_str());
        }
        if !self.plugin_id.is_empty() {
            o.set("pluginId", self.plugin_id.as_str());
        }
        if self.is_create_action {
            o.set("isCreateAction", true);
        }
        if !self.keywords.is_empty() {
            let mut a = Json::arr();
            for k in self.keywords.iter() {
                a.push(Json::Str(k.to_string()));
            }
            o.set("keywords", a);
        }
        o
    }
}

/// navtree.Sort: stable by weight, where 0 means the position (index + 1).
fn sort_nav(nodes: &mut Vec<Nav>) {
    let mut keyed: Vec<(i64, usize, Nav)> = Vec::with_capacity(nodes.len());
    for (i, n) in nodes.drain(..).enumerate() {
        let w = if n.sort_weight == 0 { i as i64 + 1 } else { n.sort_weight };
        keyed.push((w, i, n));
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    for (_, _, mut n) in keyed.into_iter() {
        sort_nav(&mut n.children);
        nodes.push(n);
    }
}

// Weights (navtree.Weight*): (iota - 40) * 100 in declaration order.
const W_HOME: i64 = -4000;
const W_BOOKMARKS: i64 = -3900;
const W_SAVED: i64 = -3800;
const W_DASHBOARD: i64 = -3700;
const W_EXPLORE: i64 = -3500;
const W_DRILLDOWN: i64 = -3400;
const W_ALERTING: i64 = -3100;
const W_CONNECTIONS: i64 = -1900;
const W_APPS: i64 = -1800;
const W_PLUGIN: i64 = -1700;
const W_CONFIG: i64 = -1600;
const W_PROFILE: i64 = -1500;
const W_HELP: i64 = -1400;

/// Where known app plugins go (navtreeimpl applinks.go navigationAppConfig, for the apps Grafana OSS ships).
fn app_nav_config(id: &str) -> Option<(&'static str, i64, &'static str, &'static str)> {
    match id {
        "grafana-metricsdrilldown-app" => Some(("drilldown", 1, "Metrics", "")),
        "grafana-lokiexplore-app" => Some(("drilldown", 2, "Logs", "")),
        "grafana-exploretraces-app" => Some(("drilldown", 3, "Traces", "")),
        "grafana-pyroscope-app" => Some(("drilldown", 4, "Profiles", "")),
        "grafana-sqldrilldown-app" => Some(("drilldown", 5, "SQL", "")),
        "grafana-advisor-app" => Some(("cfg", 0, "Advisor", "Run checks and get suggestions to fix issues")),
        _ => None,
    }
}

/// The path (child indexes) of the node with `id`, depth first.
fn find_path(nodes: &[Nav], id: &str, path: &mut Vec<usize>) -> bool {
    for (i, n) in nodes.iter().enumerate() {
        path.push(i);
        if n.id == id || find_path(&n.children, id, path) {
            return true;
        }
        path.pop();
    }
    false
}

fn find_mut<'n>(nodes: &'n mut Vec<Nav>, id: &str) -> Option<&'n mut Nav> {
    let mut path = Vec::new();
    if !find_path(nodes, id, &mut path) {
        return None;
    }
    let mut cur = &mut nodes[path[0]];
    for i in path[1..].iter() {
        cur = &mut cur.children[*i];
    }
    Some(cur)
}

/// GetNavTree + the index-data hooks and cleanups of setIndexViewData.
fn nav_tree(c: &Ctx<'_>, prefs: &crate::api::users::Prefs) -> Vec<Nav> {
    let app = c.app;
    let cfg = &app.cfg;
    let sub = cfg.app_sub_url.as_str();
    let u = &c.user;
    let has = |a: &str| access::has(u, a);
    let mut root: Vec<Nav> = Vec::new();

    let mut home_url = format!("{sub}/");
    if !u.is_signed_in() && !cfg.anonymous_enabled {
        home_url = format!("{sub}/login");
    } else {
        let hp = cfg.get("users", "home_page");
        if prefs.home_dashboard_uid.is_empty() && !hp.is_empty() {
            home_url = hp.to_string();
        }
    }
    let mut home = nav("home", "Home", "", "home-alt", home_url);
    home.sort_weight = W_HOME;
    root.push(home);

    if has("dashboards:read") {
        let mut n = nav("starred", "Starred", "", "star", format!("{sub}/dashboards?starred"));
        n.sort_weight = W_SAVED;
        n.empty_message_id = "starred-empty".to_string();
        root.push(n);
    }
    if access::has_any(u, &["folders:read", "folders:create", "dashboards:read", "dashboards:create"]) {
        let mut ch: Vec<Nav> = Vec::new();
        if (u.is_signed_in() || u.is_anonymous) && u.has_role(Role::Viewer) {
            ch.push(nav("dashboards/playlists", "Playlists", "Groups of dashboards that are displayed in a sequence", "presentation-play", format!("{sub}/playlists")));
        }
        if u.is_signed_in() {
            if cfg_bool(cfg, "snapshots", "enabled", true) && has("snapshots:read") {
                ch.push(nav("dashboards/snapshots", "Snapshots", "Interactive, publicly available, point-in-time representations of dashboards", "camera", format!("{sub}/dashboard/snapshots")));
            }
            ch.push(nav("dashboards/library-panels", "Library panels", "Reusable panels that can be added to multiple dashboards", "library-panel", format!("{sub}/library-panels")));
            if cfg_bool(cfg, "public_dashboards", "enabled", true) {
                ch.push(nav("dashboards/public", "Public dashboards", "", "library-panel", format!("{sub}/dashboard/public")));
            }
            ch.push(nav("dashboards/recently-deleted", "Recently deleted", "Any items listed here for more than 30 days will be automatically deleted.", "", format!("{sub}/dashboard/recently-deleted")));
        }
        if has("dashboards:create") {
            let mut n = nav("dashboards/new", "New dashboard", "", "plus", format!("{sub}/dashboard/new"));
            n.hide_from_tabs = true;
            n.is_create_action = true;
            ch.push(n);
            let mut n = nav("dashboards/import", "Import dashboard", "Import dashboard from file or Grafana.com", "plus", format!("{sub}/dashboard/import"));
            n.hide_from_tabs = true;
            n.is_create_action = true;
            ch.push(n);
        }
        let mut n = nav("dashboards/browse", "Dashboards", "Create and manage dashboards to visualize your data", "apps", format!("{sub}/dashboards"));
        n.sort_weight = W_DASHBOARD;
        n.children = ch;
        root.push(n);
    }
    if cfg_bool(cfg, "explore", "enabled", true) && has("datasources:explore") {
        let mut n = nav("explore", "Explore", "Explore your data", "compass", format!("{sub}/explore"));
        n.sort_weight = W_EXPLORE;
        root.push(n);
    }
    if has("datasources:explore") {
        let mut n = nav("drilldown", "Drilldown", "Drill down into your data using Grafana's powerful queryless apps", "drilldown", format!("{sub}/drilldown"));
        n.sort_weight = W_DRILLDOWN;
        root.push(n);
    }
    if cfg_bool(cfg, "profile", "enabled", true) && u.is_signed_in() {
        let mut ch = vec![nav("profile/settings", "Profile", "", "sliders-v-alt", format!("{sub}/profile"))];
        ch.push(nav("profile/notifications", "Notification history", "", "bell", format!("{sub}/profile/notifications")));
        if !cfg.disable_login_form && !cfg_bool(cfg, "auth", "disable_login", false) {
            ch.push(nav("profile/password", "Change password", "", "lock", format!("{sub}/profile/password")));
        }
        let name = if u.name.is_empty() { u.login.as_str() } else { u.name.as_str() };
        let mut n = nav("profile", name, if u.login != name { u.login.as_str() } else { "" }, "", format!("{sub}/profile"));
        n.img = gravatar_url(cfg, &u.email);
        n.sort_weight = W_PROFILE;
        n.round_icon = true;
        n.children = ch;
        root.push(n);
    }
    // Alerting (unified alerting enabled, navigation V2 as the recorded feature flags say)
    {
        let mut ch: Vec<Nav> = Vec::new();
        if access::has_any(u, &["alert.rules:read", "alert.rules.external:read"]) {
            ch.push(nav("alert-rules", "Alert rules", "Rules that determine whether an alert will fire", "list-ul", format!("{sub}/alerting/list")));
        }
        let cp = access::has_any(
            u,
            &[
                "alert.notifications:read",
                "alert.notifications.external:read",
                "alert.notifications.receivers:read",
                "alert.notifications.receivers.secrets:read",
                "alert.notifications.receivers:create",
                "alert.notifications.templates:read",
                "alert.notifications.templates:write",
                "alert.notifications.templates:delete",
            ],
        );
        let np = access::has_any(
            u,
            &[
                "alert.notifications:read",
                "alert.notifications.external:read",
                "alert.notifications.routes:read",
                "alert.notifications.routes:write",
                "alert.notifications.time-intervals:read",
                "alert.notifications.time-intervals:write",
                "alert.notifications.managed-routes:read",
                "alert.notifications.managed-routes:write",
            ],
        );
        if cp || np {
            ch.push(nav("notification-config", "Notification configuration", "Manage contact points, notification policies, templates, and time intervals", "comment-alt-share", format!("{sub}/alerting/notifications")));
        }
        if access::has_any(u, &["alert.instances:read", "alert.instances.external:read", "alert.silences:read"]) {
            ch.push(nav("silences", "Silences", "Stop notifications from one or more alerting rules", "bell-slash", format!("{sub}/alerting/silences")));
        }
        if access::has_any(u, &["alert.instances:read", "alert.instances.external:read"]) {
            ch.push(nav("groups", "Alert groups", "See grouped alerts with active notifications", "layer-group", format!("{sub}/alerting/groups")));
        }
        if u.org_role == Role::Admin {
            ch.push(nav("alerting-admin", "Settings", "", "cog", format!("{sub}/alerting/admin")));
        }
        if access::has_any(u, &["alert.rules:create", "alert.rules.external:write"]) {
            let mut n = nav("alert", "Create alert rule", "Create an alert rule", "plus", format!("{sub}/alerting/new"));
            n.hide_from_tabs = true;
            n.is_create_action = true;
            ch.push(n);
        }
        if !ch.is_empty() {
            let mut n = nav("alerting", "Alerting", "Learn about problems in your systems moments after they occur", "bell", format!("{sub}/alerting"));
            n.sort_weight = W_ALERTING;
            n.children = ch;
            root.push(n);
        }
    }
    {
        let mut ch: Vec<Nav> = Vec::new();
        if ds_config_access(u) {
            let mut n = nav("connections-add-new-connection", "Add new connection", "Browse and create new connections", "", format!("{sub}/connections/add-new-connection"));
            n.keywords = vec!["csv", "graphite", "json", "loki", "prometheus", "sql", "tempo"];
            ch.push(n);
            ch.push(nav("connections-datasources", "Data sources", "View and manage your connected data source connections", "", format!("{sub}/connections/datasources")));
        }
        let mut n = nav("connections", "Connections", "", "adjust-circle", format!("{sub}/connections"));
        n.sort_weight = W_CONNECTIONS;
        n.children = ch;
        root.push(n);
    }
    {
        let mut general: Vec<Nav> = Vec::new();
        if (has("orgs:read") && has("orgs:write")) || (has("orgs.preferences:read") && has("orgs.preferences:write")) {
            general.push(nav("org-settings", "Default preferences", "Manage preferences across an organization", "sliders-v-alt", format!("{sub}/org")));
        }
        if u.is_grafana_admin && has("settings:read") {
            general.push(nav("server-settings", "Settings", "View the settings defined in your Grafana config", "sliders-v-alt", format!("{sub}/admin/settings")));
        }
        if u.is_grafana_admin && has("orgs:read") {
            general.push(nav("global-orgs", "Organizations", "Isolated instances of Grafana running on the same server", "building", format!("{sub}/admin/orgs")));
        }
        if has("migrationassistant:migrate") {
            general.push(nav("migrate-to-cloud", "Migrate to Grafana Cloud", "Copy resources from your self-managed installation to a cloud stack", "", format!("{sub}/admin/migrate-to-cloud")));
        }
        if u.has_role(Role::Admin) {
            let mut n = nav("provisioning", "Provisioning", "View and manage your provisioning connections", "", format!("{sub}/admin/provisioning"));
            n.keywords = vec!["git sync", "git-sync", "repository", "version control", "as code"];
            general.push(n);
        }
        // licensing index-data hook (OSS): "Stats and license" for server admins
        if u.is_grafana_admin {
            let mut n = nav("upgrading", "Stats and license", "", "unlock", format!("{sub}/admin/upgrading"));
            n.sort_weight = -1;
            general.push(n);
        }
        let mut plugins: Vec<Nav> = Vec::new();
        if can_admin_plugins(app, u) {
            plugins.push(nav("plugins", "Plugins", "Extend the Grafana experience with plugins", "plug", format!("{sub}/plugins")));
        }
        if has("datasources:explore") {
            plugins.push(nav("correlations", "Correlations", "Add and configure correlations", "gf-glue", format!("{sub}/datasources/correlations")));
        }
        let mut users: Vec<Nav> = Vec::new();
        if has("org.users:read") || (u.is_grafana_admin && has("users:read")) {
            users.push(nav("global-users", "Users", "Manage users in Grafana", "user", format!("{sub}/admin/users")));
        }
        if teams_access(u) {
            users.push(nav("teams", "Teams", "Groups of users that have common dashboard and permission needs", "users-alt", format!("{sub}/org/teams")));
        }
        if access::has_any(u, &["serviceaccounts:read", "serviceaccounts:create"]) {
            users.push(nav("serviceaccounts", "Service accounts", "Use service accounts to run automated workloads in Grafana", "gf-service-account", format!("{sub}/org/serviceaccounts")));
        }
        let mut cfg_nodes: Vec<Nav> = Vec::new();
        let mut g = nav("cfg/general", "General", "Manage default preferences and settings across Grafana", "shield", format!("{sub}/admin/general"));
        g.children = general;
        cfg_nodes.push(g);
        let mut p = nav("cfg/plugins", "Plugins and data", "Install plugins and define the relationships between data", "shield", format!("{sub}/admin/plugins"));
        p.children = plugins;
        cfg_nodes.push(p);
        let mut a = nav("cfg/access", "Users and access", "Configure access for individual users, teams, and service accounts", "shield", format!("{sub}/admin/access"));
        a.children = users;
        cfg_nodes.push(a);
        // OAuth settings UI: server admins can read the SSO settings of the configurable providers.
        if u.is_grafana_admin {
            let mut n = nav("authentication", "Authentication", "Manage your auth settings and configure single sign-on", "signin", format!("{sub}/admin/authentication"));
            n.is_section = true;
            cfg_nodes.push(n);
        }
        let mut n = nav("cfg", "Administration", &format!("Organization: {}", u.org_name), "cog", format!("{sub}/admin"));
        n.sort_weight = W_CONFIG;
        n.children = cfg_nodes;
        root.push(n);
    }
    if cfg_bool(cfg, "help", "enabled", true) {
        let mut n = nav("help", "Help", "", "question-circle", "#".to_string());
        n.sort_weight = W_HELP;
        if cfg_bool(cfg, "support_bundles", "enabled", true) && access::has_any(u, &["support.bundles:read", "support.bundles:create"]) {
            let mut s = nav("support-bundles", "Support bundles", "", "wrench", format!("{sub}/support-bundles"));
            s.sort_weight = W_HELP;
            n.children.push(s);
        }
        root.push(n);
    }
    add_app_links(c, &mut root);
    if u.is_signed_in() {
        let mut n = nav("bookmarks", "Bookmarks", "", "bookmark", format!("{sub}/bookmarks"));
        n.sort_weight = W_BOOKMARKS;
        n.empty_message_id = "bookmarks-empty".to_string();
        root.push(n);
    }

    // RemoveEmptyAdminSections, RemoveEmptyConnectionsSection, RemoveEmptyDrilldownSection
    if let Some(cfgn) = find_mut(&mut root, "cfg") {
        cfgn.children.retain(|n| !(matches!(n.id.as_str(), "cfg/general" | "cfg/plugins" | "cfg/access") && n.children.is_empty()));
    }
    root.retain(|n| !(matches!(n.id.as_str(), "cfg" | "connections" | "drilldown") && n.children.is_empty()));
    sort_nav(&mut root);
    root
}

/// An app plugin's nav data, parsed once from its settings.
struct AppNav {
    id: String,
    name: String,
    description: String,
    logo: String,
    /// (name, icon, path, slug, defaultNav, action, role)
    pages: Vec<(String, String, String, String, bool, String, String)>,
}

fn app_navs(app: &App) -> &'static [AppNav] {
    static A: OnceLock<Vec<AppNav>> = OnceLock::new();
    A.get_or_init(|| {
        let mut out = Vec::new();
        for p in app.plugins.list.iter() {
            if p.kind != "app" || !p.enabled {
                continue;
            }
            let pj = match Json::parse(&p.settings_json) {
                Ok(j) => j,
                Err(_) => continue,
            };
            let mut pages = Vec::new();
            if let Some(incs) = pj.get("includes").and_then(|v| v.as_arr()) {
                for inc in incs.iter() {
                    if inc.str_of("type") != "page" || !inc.bool_of("addToNav") {
                        continue;
                    }
                    pages.push((
                        inc.str_of("name").to_string(),
                        inc.str_of("icon").to_string(),
                        inc.str_of("path").to_string(),
                        inc.str_of("slug").to_string(),
                        inc.bool_of("defaultNav"),
                        inc.str_of("action").to_string(),
                        inc.str_of("role").to_string(),
                    ));
                }
            }
            out.push(AppNav {
                id: p.id.clone(),
                name: p.name.clone(),
                description: pj.path("info.description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                logo: pj.path("info.logos.small").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                pages,
            });
        }
        out
    })
}

/// addAppLinks for the enabled app plugins: a page include with addToNav becomes a child of the app node.
fn add_app_links(c: &Ctx<'_>, root: &mut Vec<Nav>) {
    let app = c.app;
    let u = &c.user;
    let sub = app.cfg.app_sub_url.as_str();
    if !u.is_identified() {
        return;
    }
    for a in app_navs(app).iter() {
        let mut app_link = nav(&format!("plugin-page-{}", a.id), &a.name, &a.description, "", format!("{sub}/a/{}", a.id));
        app_link.img = a.logo.clone();
        app_link.sort_weight = W_PLUGIN;
        app_link.is_section = true;
        app_link.plugin_id = a.id.clone();
        for (name, icon, path, slug, default_nav, action, role) in a.pages.iter() {
            // hasAccessToInclude: the include's action when set, else its role
            let ok = if !action.is_empty() { access::has(u, action) } else { u.has_role(Role::parse(role).unwrap_or(Role::Viewer)) };
            if !ok {
                continue;
            }
            let url = if !path.is_empty() { format!("{sub}{path}") } else { format!("{sub}/plugins/{}/page/{}", a.id, slug) };
            if *default_nav && !path.is_empty() {
                app_link.url = url.clone();
            }
            let mut l = nav("", name, "", icon, url);
            l.plugin_id = a.id.clone();
            app_link.children.push(l);
        }
        if app_link.children.is_empty() {
            continue;
        }
        if app_link.children.len() == 1 && app_link.children[0].url == app_link.url {
            app_link.children.clear();
        }
        let mut section = "apps";
        if let Some((sec, w, text, subt)) = app_nav_config(&a.id) {
            section = sec;
            app_link.sort_weight = w;
            if !text.is_empty() {
                app_link.text = text.to_string();
            }
            if !subt.is_empty() {
                app_link.sub_title = subt.to_string();
            }
        }
        match find_mut(root, section) {
            Some(s) => s.children.push(app_link),
            None => {
                if section == "apps" {
                    let mut n = nav("apps", "More apps", "App plugins that extend the Grafana experience", "layer-group", format!("{sub}/apps"));
                    n.sort_weight = W_APPS;
                    n.children.push(app_link);
                    root.push(n);
                }
            }
        }
    }
}

// ---------------------------------------------------------------- index, bootdata

/// Builds the boot data parts. Returns (user Json with a permissions marker, theme type, navTree Json).
fn boot_parts(c: &Ctx<'_>) -> (Json, String, Json) {
    let prefs = crate::api::users::merged_prefs(c.app, &c.user);
    let (user, ttype) = user_json(c);
    let mut nt = Json::arr();
    for n in nav_tree(c, &prefs).iter() {
        nt.push(n.to_json());
    }
    (user, ttype, nt)
}

fn fill_markers(text: String, c: &Ctx<'_>) -> String {
    let mut out = String::with_capacity(text.len() + 96 * 1024);
    let pm = json::quote(PERM_MARKER);
    let sm = json::quote(SETTINGS_MARKER);
    let mut rest = text.as_str();
    loop {
        let p1 = rest.find(pm.as_str());
        let p2 = rest.find(sm.as_str());
        let (pos, is_perm) = match (p1, p2) {
            (Some(a), Some(b)) => if a < b { (a, true) } else { (b, false) },
            (Some(a), None) => (a, true),
            (None, Some(b)) => (b, false),
            (None, None) => break,
        };
        out.push_str(&rest[..pos]);
        if is_perm {
            out.push_str(access::permissions_json(&c.user));
            rest = &rest[pos + pm.len()..];
        } else {
            write_settings(c, &mut out);
            rest = &rest[pos + sm.len()..];
        }
    }
    out.push_str(rest);
    out
}

/// Renders the index page for the request identity.
pub fn index(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let app: &App = c.app;
    let tpl = match &app.index_template {
        Some(t) => t,
        None => return Err(ApiError::internal("Failed to get settings")),
    };
    let (user, ttype, nav_tree) = boot_parts(c);
    let data = Json::obj()
        .with("User", user)
        .with("Settings", SETTINGS_MARKER)
        .with("NavTree", nav_tree)
        .with("Assets", app.assets.clone())
        .with("AppTitle", "Grafana")
        .with("AppSubUrl", app.cfg.app_sub_url.as_str())
        .with("AppNameBodyClass", "app-grafana")
        .with("FavIcon", "public/build/img/fav32.png")
        .with("AppleTouchIcon", "public/build/img/apple-touch-icon.png")
        .with("LoadingLogo", "public/build/img/grafana_icon.svg")
        .with("ThemeType", ttype)
        .with("Nonce", "")
        .with("UseLuxon", true)
        .with("LegacyFeatureToggleMode", "off")
        .with("ESModuleAssetsEnabled", true)
        .with("AssetSriChecksEnabled", false)
        .with("IsDevelopmentEnv", app.cfg.app_mode_development);
    let html = fill_markers(tpl.render(&data), c);
    Ok(Response::html(200, html))
}

/// `GET /bootdata` (GetBootdata): the same data as JSON.
fn get_bootdata(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let (user, _, nav_tree) = boot_parts(c);
    let j = Json::obj().with("user", user).with("settings", SETTINGS_MARKER).with("navTree", nav_tree).with("assets", c.app.assets.clone());
    Ok(Response::json_text(200, fill_markers(j.stringify(), c)))
}

/// `GET /api/frontend/settings`.
fn get_frontend_settings(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let mut out = String::with_capacity(96 * 1024);
    write_settings(c, &mut out);
    Ok(Response::json_text(200, out))
}

/// `GET /api/frontend/assets` (GetFrontendAssets): hashes that change when the frontend must reload.
fn get_frontend_assets(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    static T: OnceLock<String> = OnceLock::new();
    let app = c.app;
    let body = T.get_or_init(|| {
        let cfg = &app.cfg;
        let hex = |parts: &[&str]| -> String {
            let mut h = crypto::Sha256::new();
            for p in parts.iter() {
                h.update(p.as_bytes());
            }
            crypto::hex(&h.finish())
        };
        let version = hex(&[cfg.build_version.as_str(), cfg.build_commit.as_str()]);
        let mut plugins: Vec<String> = app.plugins.list.iter().map(|p| format!("{}@{}", p.name, p.version)).collect();
        plugins.sort();
        let pr: Vec<&str> = plugins.iter().map(|s| s.as_str()).collect();
        let mut flags: Vec<String> = crate::api::ofrep::enabled_flags(cfg).to_vec();
        flags.sort();
        let fr: Vec<&str> = flags.iter().map(|s| s.as_str()).collect();
        let a = &app.assets;
        let mut parts: Vec<String> = vec![String::new(), a.str_of("dark").to_string(), a.str_of("light").to_string()];
        if let Some(js) = a.get("jsFiles").and_then(|v| v.as_arr()) {
            for f in js.iter() {
                parts.push(f.str_of("filePath").to_string());
                parts.push(f.str_of("integrity").to_string());
            }
        }
        let ar: Vec<&str> = parts.iter().map(|s| s.as_str()).collect();
        Json::obj().with("assets", hex(&ar)).with("flags", hex(&fr)).with("plugins", hex(&pr)).with("version", version).stringify()
    });
    Ok(Response::json_text(200, body.clone()))
}

/// `GET /api/access-control/user/actions`: the permissions map of the signed-in identity.
fn get_user_actions(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    Ok(Response::json_text(200, access::permissions_json(&c.user).to_string()))
}
