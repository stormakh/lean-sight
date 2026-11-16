use crate::{SqliteStore, map_err};
use grafana::StoreError;
use grafana::store::{Dashboard, DashboardMeta, DashboardStore, DashboardVersion, SResult, SaveDashboard};
use rusqlite::{Connection, params};

macro_rules! cols {
    () => {
        "id, uid, org_id, version, slug, title, data, folder_uid, created, updated, created_by, updated_by, gnet_id, plugin_id, api_version, meta"
    };
}
macro_rules! vcols {
    () => {
        "id, dashboard_id, dashboard_uid, parent_version, restored_from, version, created, created_by, message, data, api_version, meta"
    };
}
fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Dashboard> {
    Ok(Dashboard {
        id: r.get(0)?,
        uid: r.get(1)?,
        org_id: r.get(2)?,
        version: r.get(3)?,
        slug: r.get(4)?,
        title: r.get(5)?,
        data: r.get(6)?,
        folder_uid: r.get(7)?,
        created: r.get(8)?,
        updated: r.get(9)?,
        created_by: r.get(10)?,
        updated_by: r.get(11)?,
        gnet_id: r.get(12)?,
        plugin_id: r.get(13)?,
        api_version: r.get(14)?,
        tags: Vec::new(),
        meta: r.get(15)?,
    })
}
fn vrow(r: &rusqlite::Row<'_>) -> rusqlite::Result<DashboardVersion> {
    Ok(DashboardVersion {
        id: r.get(0)?,
        dashboard_id: r.get(1)?,
        dashboard_uid: r.get(2)?,
        parent_version: r.get(3)?,
        restored_from: r.get(4)?,
        version: r.get(5)?,
        created: r.get(6)?,
        created_by: r.get(7)?,
        message: r.get(8)?,
        data: r.get(9)?,
        api_version: r.get(10)?,
        meta: r.get(11)?,
    })
}
fn tags_of(c: &Connection, id: i64) -> rusqlite::Result<Vec<String>> {
    let mut st = c.prepare_cached("SELECT term FROM dashboard_tag WHERE dashboard_id = ?1 ORDER BY id")?;
    let rows = st.query_map(params![id], |r| r.get(0))?;
    rows.collect()
}

/// Deletes a dashboard row and everything that hangs on it (inside the caller's transaction).
pub(crate) fn delete_dashboard_rows(c: &Connection, org_id: i64, id: i64) -> rusqlite::Result<usize> {
    let uid: String = match c.query_row("SELECT uid FROM dashboard WHERE org_id = ?1 AND id = ?2", params![org_id, id], |r| r.get(0)) {
        Ok(u) => u,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(0),
        Err(e) => return Err(e),
    };
    c.execute("DELETE FROM dashboard_tag WHERE dashboard_id = ?1", params![id])?;
    c.execute("DELETE FROM dashboard_version WHERE dashboard_id = ?1", params![id])?;
    c.execute("DELETE FROM star WHERE org_id = ?1 AND dashboard_uid = ?2", params![org_id, uid])?;
    c.execute("DELETE FROM dashboard WHERE id = ?1", params![id])
}

impl DashboardStore for SqliteStore {
    fn dashboard_by_uid(&self, org_id: i64, uid: &str) -> SResult<Dashboard> {
        self.db(|c| {
            let mut d = c.prepare_cached(concat!("SELECT ", cols!(), " FROM dashboard WHERE org_id = ?1 AND uid = ?2"))?.query_row(params![org_id, uid], row)?;
            d.tags = tags_of(c, d.id)?;
            Ok(d)
        })
    }
    fn dashboard_by_uid_with_logins(&self, org_id: i64, uid: &str) -> SResult<(Dashboard, Option<String>, Option<String>)> {
        self.db(|c| {
            let (mut d, cl, ul) = c
                .prepare_cached(concat!(
                    "SELECT ",
                    cols!(),
                    ", (SELECT CASE WHEN u.is_service_account THEN u.name ELSE u.login END FROM user u WHERE u.id = dashboard.created_by),",
                    " (SELECT CASE WHEN u.is_service_account THEN u.name ELSE u.login END FROM user u WHERE u.id = dashboard.updated_by)",
                    " FROM dashboard WHERE org_id = ?1 AND uid = ?2"
                ))?
                .query_row(params![org_id, uid], |r| Ok((row(r)?, r.get::<_, Option<String>>(16)?, r.get::<_, Option<String>>(17)?)))?;
            d.tags = tags_of(c, d.id)?;
            Ok((d, cl, ul))
        })
    }
    fn dashboard_by_id(&self, org_id: i64, id: i64) -> SResult<Dashboard> {
        self.db(|c| {
            let mut d = c.prepare_cached(concat!("SELECT ", cols!(), " FROM dashboard WHERE org_id = ?1 AND id = ?2"))?.query_row(params![org_id, id], row)?;
            d.tags = tags_of(c, d.id)?;
            Ok(d)
        })
    }
    fn dashboard_save(&self, s: &SaveDashboard) -> SResult<()> {
        let d = &s.dashboard;
        self.tx(|c| {
            if s.is_new {
                c.prepare_cached(
                    "INSERT INTO dashboard (id, uid, org_id, version, slug, title, data, folder_uid, created, updated, created_by, updated_by, gnet_id, plugin_id, api_version, meta)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                )
                .and_then(|mut st| {
                    st.execute(params![d.id, d.uid, d.org_id, d.version, d.slug, d.title, d.data, d.folder_uid, d.created, d.updated, d.created_by, d.updated_by, d.gnet_id, d.plugin_id, d.api_version, d.meta])
                })
                .map_err(map_err)?;
            } else {
                let n = c
                    .prepare_cached(
                        "UPDATE dashboard SET uid = ?3, version = ?4, slug = ?5, title = ?6, data = ?7, folder_uid = ?8, updated = ?9, updated_by = ?10, gnet_id = ?11, plugin_id = ?12, api_version = ?13, meta = ?14
                         WHERE org_id = ?1 AND id = ?2",
                    )
                    .and_then(|mut st| st.execute(params![d.org_id, d.id, d.uid, d.version, d.slug, d.title, d.data, d.folder_uid, d.updated, d.updated_by, d.gnet_id, d.plugin_id, d.api_version, d.meta]))
                    .map_err(map_err)?;
                if n == 0 {
                    return Err(StoreError::NotFound);
                }
                c.prepare_cached("DELETE FROM dashboard_tag WHERE dashboard_id = ?1").and_then(|mut st| st.execute(params![d.id])).map_err(map_err)?;
            }
            {
                let mut st = c.prepare_cached("INSERT INTO dashboard_tag (dashboard_id, org_id, term) VALUES (?1, ?2, ?3)").map_err(map_err)?;
                for tag in d.tags.iter() {
                    st.execute(params![d.id, d.org_id, tag]).map_err(map_err)?;
                }
            }
            let parent = if d.version > 1 { d.version - 1 } else { 0 };
            c.prepare_cached(
                "INSERT OR REPLACE INTO dashboard_version (dashboard_id, dashboard_uid, parent_version, restored_from, version, created, created_by, message, data, api_version, meta)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )
            .and_then(|mut st| st.execute(params![d.id, d.uid, parent, s.restored_from, d.version, d.updated, d.updated_by, s.message, d.data, d.api_version, d.meta]))
            .map_err(map_err)?;
            Ok(())
        })
    }
    fn dashboard_delete(&self, org_id: i64, uid: &str) -> SResult<()> {
        self.tx(|c| {
            let id: i64 = c.query_row("SELECT id FROM dashboard WHERE org_id = ?1 AND uid = ?2", params![org_id, uid], |r| r.get(0)).map_err(map_err)?;
            delete_dashboard_rows(c, org_id, id).map_err(map_err)?;
            Ok(())
        })
    }
    fn dashboards(&self, org_id: i64) -> SResult<Vec<DashboardMeta>> {
        self.db(|c| {
            // one statement: a row per (dashboard, tag), in dashboard id then tag id order
            let mut st = c.prepare_cached(
                "SELECT d.id, d.uid, d.org_id, d.version, d.slug, d.title, d.folder_uid, d.created, d.updated, t.term
                 FROM dashboard d LEFT JOIN dashboard_tag t ON t.dashboard_id = d.id WHERE d.org_id = ?1 ORDER BY d.id, t.id",
            )?;
            let mut rows = st.query(params![org_id])?;
            let mut list: Vec<DashboardMeta> = Vec::new();
            while let Some(r) = rows.next()? {
                let id: i64 = r.get(0)?;
                if list.last().map(|d| d.id) != Some(id) {
                    list.push(DashboardMeta { id, uid: r.get(1)?, org_id: r.get(2)?, version: r.get(3)?, slug: r.get(4)?, title: r.get(5)?, folder_uid: r.get(6)?, created: r.get(7)?, updated: r.get(8)?, tags: Vec::new() });
                }
                if let Some(term) = r.get::<_, Option<String>>(9)? {
                    if let Some(d) = list.last_mut() {
                        d.tags.push(term);
                    }
                }
            }
            Ok(list)
        })
        .map(|mut list| {
            // sorted after the connection lock is released
            list.sort_by_cached_key(|d| d.title.to_lowercase());
            list
        })
    }
    fn dashboards_full(&self, org_id: i64, after_uid: &str, limit: i64) -> SResult<Vec<Dashboard>> {
        self.db(|c| {
            let mut st = c.prepare_cached(concat!("SELECT ", cols!(), " FROM dashboard WHERE org_id = ?1 AND uid > ?2 ORDER BY uid LIMIT ?3"))?;
            let mut list = st.query_map(params![org_id, after_uid, limit], row)?.collect::<rusqlite::Result<Vec<_>>>()?;
            for d in list.iter_mut() {
                d.tags = tags_of(c, d.id)?;
            }
            Ok(list)
        })
    }
    fn dashboard_versions(&self, dashboard_id: i64, limit: i64, offset: i64) -> SResult<Vec<DashboardVersion>> {
        self.db(|c| {
            let mut st = c.prepare_cached(concat!("SELECT ", vcols!(), " FROM dashboard_version WHERE dashboard_id = ?1 ORDER BY version DESC LIMIT ?2 OFFSET ?3"))?;
            let rows = st.query_map(params![dashboard_id, limit, offset], vrow)?;
            rows.collect()
        })
    }
    fn dashboard_version(&self, dashboard_id: i64, version: i64) -> SResult<DashboardVersion> {
        self.db(|c| c.prepare_cached(concat!("SELECT ", vcols!(), " FROM dashboard_version WHERE dashboard_id = ?1 AND version = ?2"))?.query_row(params![dashboard_id, version], vrow))
    }
    fn dashboard_tags(&self, org_id: i64) -> SResult<Vec<(String, i64)>> {
        self.db(|c| {
            let mut st = c.prepare_cached("SELECT term, COUNT(*) FROM dashboard_tag WHERE org_id = ?1 GROUP BY term ORDER BY term")?;
            let rows = st.query_map(params![org_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect()
        })
    }
}

