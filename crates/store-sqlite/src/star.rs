use crate::SqliteStore;
use grafana::store::{SResult, StarStore};
use grafana::util::now;
use rusqlite::params;

impl StarStore for SqliteStore {
    fn stars_of_user(&self, org_id: i64, user_id: i64) -> SResult<Vec<String>> {
        self.db(|c| {
            let mut st = c.prepare_cached("SELECT dashboard_uid FROM star WHERE org_id = ?1 AND user_id = ?2 ORDER BY id")?;
            let rows = st.query_map(params![org_id, user_id], |r| r.get(0))?;
            rows.collect()
        })
    }
    fn star_add(&self, org_id: i64, user_id: i64, dashboard_uid: &str) -> SResult<()> {
        self.db(|c| c.execute("INSERT OR IGNORE INTO star (org_id, user_id, dashboard_uid, updated) VALUES (?1, ?2, ?3, ?4)", params![org_id, user_id, dashboard_uid, now()]).map(|_| ()))
    }
    fn star_remove(&self, org_id: i64, user_id: i64, dashboard_uid: &str) -> SResult<()> {
        self.db(|c| c.execute("DELETE FROM star WHERE org_id = ?1 AND user_id = ?2 AND dashboard_uid = ?3", params![org_id, user_id, dashboard_uid]).map(|_| ()))
    }
}
