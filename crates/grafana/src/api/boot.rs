//! The index page (`public/views/index.html` with `window.grafanaBootData`) and the frontend page routes
//! (agent A). Upstream: pkg/api/index.go (setIndexViewData), pkg/api/frontendsettings.go, pkg/api/api.go.
//!
//! FOUNDATION STATE: `index` renders the real template with a MINIMAL boot data (user, a few settings, empty
//! navTree, real assets). Agent A replaces `boot_data` with the full port of setIndexViewData.
use crate::app::App;
use crate::ctx::{Access, Ctx, Role, Routes};
use crate::error::ApiError;
use http::Response;
use json::Json;

pub fn routes(r: &mut Routes) {
    // Page routes seen in tests-data/recordings; agent A adds the rest from upstream pkg/api/api.go.
    r.get("/", Access::SignedIn, index);
    r.get("/d/:uid", Access::SignedIn, index);
    r.get("/d/:uid/:slug", Access::SignedIn, index);
    r.get("/dashboards", Access::SignedIn, index);
    r.get("/dashboard/new", Access::SignedIn, index);
    r.get("/explore", Access::SignedIn, index);
    r.get("/connections/*", Access::SignedIn, index);
    r.get("/profile", Access::NoAnonymous, index);
    r.get("/login", Access::Optional, index);
}

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

/// The boot data (`window.grafanaBootData` user and settings). MINIMAL: agent A ports the full version.
pub fn boot_data(c: &Ctx<'_>) -> (Json, Json) {
    let u = &c.user;
    let cfg = &c.app.cfg;
    let theme = if u.theme.is_empty() { "dark".to_string() } else { u.theme.clone() };
    let user = Json::obj()
        .with("isSignedIn", u.is_signed_in())
        .with("id", u.user_id)
        .with("uid", u.uid.as_str())
        .with("login", u.login.as_str())
        .with("email", u.email.as_str())
        .with("name", u.name.as_str())
        .with("theme", theme.as_str())
        .with("lightTheme", theme == "light")
        .with("orgCount", 1)
        .with("orgId", u.org_id)
        .with("orgName", u.org_name.as_str())
        .with("orgRole", if u.org_role == Role::None { "" } else { u.org_role.as_str() })
        .with("isGrafanaAdmin", u.is_grafana_admin)
        .with("gravatarUrl", "")
        .with("timezone", "browser")
        .with("weekStart", "browser")
        .with("locale", "en-US")
        .with("language", "en-US")
        .with("hasEditPermissionInFolders", u.has_role(Role::Editor))
        .with("authenticatedBy", "")
        .with("permissions", Json::obj());
    let settings = Json::obj()
        .with("appUrl", cfg.app_url.as_str())
        .with("appSubUrl", cfg.app_sub_url.as_str())
        .with("anonymousEnabled", cfg.anonymous_enabled)
        .with("buildInfo", Json::obj().with("version", cfg.build_version.as_str()).with("commit", cfg.build_commit.as_str()).with("edition", "Open Source").with("env", cfg.env.as_str()))
        .with("datasources", Json::obj())
        .with("panels", Json::obj())
        .with("featureToggles", Json::obj().with("dashboardNewLayouts", false));
    (user, settings)
}

/// Renders the index page for the request identity.
pub fn index(c: &mut Ctx<'_>) -> Result<Response, ApiError> {
    let app: &App = c.app;
    let tpl = match &app.index_template {
        Some(t) => t,
        None => return Err(ApiError::internal("Failed to get settings")),
    };
    let (user, settings) = boot_data(c);
    let theme = user.str_of("theme").to_string();
    let data = Json::obj()
        .with("User", user)
        .with("Settings", settings)
        .with("NavTree", Json::arr())
        .with("Assets", app.assets.clone())
        .with("AppTitle", "Grafana")
        .with("AppSubUrl", app.cfg.app_sub_url.as_str())
        .with("AppNameBodyClass", "app-grafana")
        .with("FavIcon", "public/build/img/fav32.png")
        .with("AppleTouchIcon", "public/build/img/apple-touch-icon.png")
        .with("LoadingLogo", "public/build/img/grafana_icon.svg")
        .with("ThemeType", theme)
        .with("Nonce", "")
        .with("UseLuxon", true)
        .with("LegacyFeatureToggleMode", "off")
        .with("ESModuleAssetsEnabled", true)
        .with("AssetSriChecksEnabled", false)
        .with("IsDevelopmentEnv", app.cfg.app_mode_development);
    Ok(Response::html(200, tpl.render(&data)))
}
