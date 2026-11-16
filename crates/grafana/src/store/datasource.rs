//! Data sources (table `data_source`). `json_data` is JSON text. `secure_json_data` is the encrypted blob from
//! `secrets::encrypt_map` (never send it to clients; send `secureJsonFields` instead).
use super::SResult;

#[derive(Debug, Clone, Default)]
pub struct DataSource {
    pub id: i64,
    pub uid: String,
    pub org_id: i64,
    pub version: i64,
    pub type_: String,
    pub name: String,
    pub access: String,
    pub url: String,
    pub user: String,
    pub database: String,
    pub basic_auth: bool,
    pub basic_auth_user: String,
    pub with_credentials: bool,
    pub is_default: bool,
    pub json_data: String,
    pub secure_json_data: String,
    pub read_only: bool,
    pub api_version: String,
    pub created: i64,
    pub updated: i64,
}

pub trait DataSourceStore {
    /// Every data source of the org, by name.
    fn datasources(&self, org_id: i64) -> SResult<Vec<DataSource>>;
    fn datasource_by_uid(&self, org_id: i64, uid: &str) -> SResult<DataSource>;
    fn datasource_by_id(&self, org_id: i64, id: i64) -> SResult<DataSource>;
    fn datasource_by_name(&self, org_id: i64, name: &str) -> SResult<DataSource>;
    /// Inserts and returns the id. Conflict when the name or uid exists. When `is_default`, clears the others.
    fn datasource_create(&self, d: &DataSource) -> SResult<i64>;
    /// Writes every column by (org_id, id). When `is_default`, clears the others.
    fn datasource_update(&self, d: &DataSource) -> SResult<()>;
    fn datasource_delete(&self, org_id: i64, id: i64) -> SResult<()>;
}
