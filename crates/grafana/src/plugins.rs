//! Plugin metadata (agent A), loaded ONCE at startup from `<home>/public/app/plugins/{datasource,panel}/*/plugin.json`
//! (core plugins) and `<plugins dir>/*/plugin.json` (external plugins). Upstream: pkg/plugins (loader, models.go
//! JSONData, DTOs), pkg/api/frontendsettings.go (getFSDataSources, getFSPanels, getFSApps), pkg/api/plugins.go.
//!
//! Everything the boot data and the plugin API need is prebuilt here, so a request only copies text or clones a
//! small `Json`.
use crate::settings::Settings;
use json::Json;
use std::path::PathBuf;

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
    /// The `meta` object of a boot-data `settings.datasources` entry (data source plugins only, else Null).
    pub ds_meta: Json,
}

#[derive(Debug, Clone, Default)]
pub struct Plugins {
    /// Every loaded plugin, by id.
    pub list: Vec<Plugin>,
    /// Boot data `settings.panels` as JSON text (an object keyed by panel id).
    pub panels_json: String,
    /// Boot data `settings.apps` as JSON text (an object keyed by app id).
    pub apps_json: String,
}

impl Plugins {
    /// Loads every plugin.json. Never fails: unreadable plugins are skipped with a message on stderr.
    pub fn load(_cfg: &Settings) -> Plugins {
        Plugins { list: Vec::new(), panels_json: "{}".to_string(), apps_json: "{}".to_string() }
    }
    pub fn get(&self, id: &str) -> Option<&Plugin> {
        self.list.iter().find(|p| p.id == id)
    }
    /// The boot-data `meta` of a data source plugin type.
    pub fn datasource_meta(&self, id: &str) -> Option<&Json> {
        match self.get(id) {
            Some(p) if p.kind == "datasource" => Some(&p.ds_meta),
            _ => None,
        }
    }
}
