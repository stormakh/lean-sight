use crate::SqliteStore;
use grafana::store::{KvStore, SResult};
use grafana::util::now;
use rusqlite::params;

impl KvStore for SqliteStore {
    fn kv_get(&self, org_id: i64, namespace: &str, key: &str) -> SResult<String> {
        self.db(|c| c.prepare_cached("SELECT value FROM kv_store WHERE org_id = ?1 AND namespace = ?2 AND key = ?3")?.query_row(params![org_id, namespace, key], |r| r.get(0)))
    }
    fn kv_set(&self, org_id: i64, namespace: &str, key: &str, value: &str) -> SResult<()> {
        let t = now();
        self.db(|c| {
            c.execute(
                "INSERT INTO kv_store (org_id, namespace, key, value, created, updated) VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT (org_id, namespace, key) DO UPDATE SET value = excluded.value, updated = excluded.updated",
                params![org_id, namespace, key, value, t],
            )
            .map(|_| ())
        })
    }
    fn kv_delete(&self, org_id: i64, namespace: &str, key: &str) -> SResult<()> {
        self.db(|c| c.execute("DELETE FROM kv_store WHERE org_id = ?1 AND namespace = ?2 AND key = ?3", params![org_id, namespace, key]).map(|_| ()))
    }
    fn kv_list(&self, org_id: i64, namespace: &str) -> SResult<Vec<(String, String)>> {
        self.db(|c| {
            let mut st = c.prepare_cached("SELECT key, value FROM kv_store WHERE org_id = ?1 AND namespace = ?2 ORDER BY key")?;
            let rows = st.query_map(params![org_id, namespace], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect()
        })
    }
}
