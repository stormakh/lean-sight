use crate::{SqliteStore, map_err};
use grafana::StoreError;
use grafana::store::{Folder, FolderStore, SResult};
use rusqlite::params;

macro_rules! cols {
    () => {
        "id, uid, org_id, title, description, parent_uid, version, created, updated, created_by, updated_by"
    };
}
fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Folder> {
    Ok(Folder {
        id: r.get(0)?,
        uid: r.get(1)?,
        org_id: r.get(2)?,
        title: r.get(3)?,
        description: r.get(4)?,
        parent_uid: r.get(5)?,
        version: r.get(6)?,
        created: r.get(7)?,
        updated: r.get(8)?,
        created_by: r.get(9)?,
        updated_by: r.get(10)?,
    })
}

impl FolderStore for SqliteStore {
    fn folder_by_uid(&self, org_id: i64, uid: &str) -> SResult<Folder> {
        self.db(|c| c.prepare_cached(concat!("SELECT ", cols!(), " FROM folder WHERE org_id = ?1 AND uid = ?2"))?.query_row(params![org_id, uid], row))
    }
    fn folder_by_id(&self, org_id: i64, id: i64) -> SResult<Folder> {
        self.db(|c| c.prepare_cached(concat!("SELECT ", cols!(), " FROM folder WHERE org_id = ?1 AND id = ?2"))?.query_row(params![org_id, id], row))
    }
    fn folders(&self, org_id: i64) -> SResult<Vec<Folder>> {
        self.db(|c| {
            let mut st = c.prepare_cached(concat!("SELECT ", cols!(), " FROM folder WHERE org_id = ?1 ORDER BY title COLLATE NOCASE"))?;
            let rows = st.query_map(params![org_id], row)?;
            rows.collect()
        })
    }
    fn folder_create(&self, f: &Folder) -> SResult<i64> {
        self.db(|c| {
            c.execute(
                "INSERT INTO folder (uid, org_id, title, description, parent_uid, version, created, updated, created_by, updated_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![f.uid, f.org_id, f.title, f.description, f.parent_uid, f.version, f.created, f.updated, f.created_by, f.updated_by],
            )?;
            Ok(c.last_insert_rowid())
        })
    }
    fn folder_update(&self, f: &Folder) -> SResult<()> {
        let n = self.db(|c| {
            c.execute(
                "UPDATE folder SET title = ?3, description = ?4, parent_uid = ?5, version = ?6, updated = ?7, updated_by = ?8 WHERE org_id = ?1 AND uid = ?2",
                params![f.org_id, f.uid, f.title, f.description, f.parent_uid, f.version, f.updated, f.updated_by],
            )
        })?;
        if n == 0 { Err(StoreError::NotFound) } else { Ok(()) }
    }
    fn folder_delete(&self, org_id: i64, uid: &str) -> SResult<()> {
        self.tx(|c| {
            let ids: Vec<i64> = {
                let mut st = c.prepare("SELECT id FROM dashboard WHERE org_id = ?1 AND folder_uid = ?2").map_err(map_err)?;
                let rows = st.query_map(params![org_id, uid], |r| r.get(0)).map_err(map_err)?;
                rows.collect::<rusqlite::Result<Vec<i64>>>().map_err(map_err)?
            };
            for id in ids {
                crate::dashboard::delete_dashboard_rows(c, org_id, id).map_err(map_err)?;
            }
            let n = c.execute("DELETE FROM folder WHERE org_id = ?1 AND uid = ?2", params![org_id, uid]).map_err(map_err)?;
            if n == 0 { Err(StoreError::NotFound) } else { Ok(()) }
        })
    }
}
