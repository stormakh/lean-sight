use crate::{SqliteStore, map_err};
use grafana::StoreError;
use grafana::store::{DataSource, DataSourceStore, SResult};
use rusqlite::params;

macro_rules! cols {
    () => {
        "id, uid, org_id, version, type, name, access, url, user, database, basic_auth, basic_auth_user, with_credentials, is_default, json_data, secure_json_data, read_only, api_version, created, updated"
    };
}
fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<DataSource> {
    Ok(DataSource {
        id: r.get(0)?,
        uid: r.get(1)?,
        org_id: r.get(2)?,
        version: r.get(3)?,
        type_: r.get(4)?,
        name: r.get(5)?,
        access: r.get(6)?,
        url: r.get(7)?,
        user: r.get(8)?,
        database: r.get(9)?,
        basic_auth: r.get(10)?,
        basic_auth_user: r.get(11)?,
        with_credentials: r.get(12)?,
        is_default: r.get(13)?,
        json_data: r.get(14)?,
        secure_json_data: r.get(15)?,
        read_only: r.get(16)?,
        api_version: r.get(17)?,
        created: r.get(18)?,
        updated: r.get(19)?,
    })
}

impl DataSourceStore for SqliteStore {
    fn datasources(&self, org_id: i64) -> SResult<Vec<DataSource>> {
        self.db(|c| {
            let mut st = c.prepare_cached(concat!("SELECT ", cols!(), " FROM data_source WHERE org_id = ?1 ORDER BY name COLLATE NOCASE"))?;
            let rows = st.query_map(params![org_id], row)?;
            rows.collect()
        })
    }
    fn datasource_by_uid(&self, org_id: i64, uid: &str) -> SResult<DataSource> {
        self.db(|c| c.prepare_cached(concat!("SELECT ", cols!(), " FROM data_source WHERE org_id = ?1 AND uid = ?2"))?.query_row(params![org_id, uid], row))
    }
    fn datasource_by_id(&self, org_id: i64, id: i64) -> SResult<DataSource> {
        self.db(|c| c.prepare_cached(concat!("SELECT ", cols!(), " FROM data_source WHERE org_id = ?1 AND id = ?2"))?.query_row(params![org_id, id], row))
    }
    fn datasource_by_name(&self, org_id: i64, name: &str) -> SResult<DataSource> {
        self.db(|c| c.prepare_cached(concat!("SELECT ", cols!(), " FROM data_source WHERE org_id = ?1 AND name = ?2"))?.query_row(params![org_id, name], row))
    }
    fn datasource_create(&self, d: &DataSource) -> SResult<i64> {
        self.tx(|c| {
            if d.is_default {
                c.execute("UPDATE data_source SET is_default = 0 WHERE org_id = ?1", params![d.org_id]).map_err(map_err)?;
            }
            c.execute(
                "INSERT INTO data_source (uid, org_id, version, type, name, access, url, user, database, basic_auth, basic_auth_user, with_credentials, is_default, json_data, secure_json_data, read_only, api_version, created, updated)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
                params![d.uid, d.org_id, d.version, d.type_, d.name, d.access, d.url, d.user, d.database, d.basic_auth, d.basic_auth_user, d.with_credentials, d.is_default, d.json_data, d.secure_json_data, d.read_only, d.api_version, d.created, d.updated],
            )
            .map_err(map_err)?;
            Ok(c.last_insert_rowid())
        })
    }
    fn datasource_update(&self, d: &DataSource) -> SResult<()> {
        self.tx(|c| {
            if d.is_default {
                c.execute("UPDATE data_source SET is_default = 0 WHERE org_id = ?1 AND id != ?2", params![d.org_id, d.id]).map_err(map_err)?;
            }
            let n = c
                .execute(
                    "UPDATE data_source SET uid = ?3, version = ?4, type = ?5, name = ?6, access = ?7, url = ?8, user = ?9, database = ?10, basic_auth = ?11, basic_auth_user = ?12,
                       with_credentials = ?13, is_default = ?14, json_data = ?15, secure_json_data = ?16, read_only = ?17, api_version = ?18, updated = ?19
                     WHERE org_id = ?1 AND id = ?2",
                    params![d.org_id, d.id, d.uid, d.version, d.type_, d.name, d.access, d.url, d.user, d.database, d.basic_auth, d.basic_auth_user, d.with_credentials, d.is_default, d.json_data, d.secure_json_data, d.read_only, d.api_version, d.updated],
                )
                .map_err(map_err)?;
            if n == 0 { Err(StoreError::NotFound) } else { Ok(()) }
        })
    }
    fn datasource_delete(&self, org_id: i64, id: i64) -> SResult<()> {
        let n = self.db(|c| c.execute("DELETE FROM data_source WHERE org_id = ?1 AND id = ?2", params![org_id, id]))?;
        if n == 0 { Err(StoreError::NotFound) } else { Ok(()) }
    }
}
