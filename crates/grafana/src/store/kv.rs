//! Key-value store (table `kv_store`), for small server state (for example the secret check or plugin state).
use super::SResult;

pub trait KvStore {
    fn kv_get(&self, org_id: i64, namespace: &str, key: &str) -> SResult<String>;
    fn kv_set(&self, org_id: i64, namespace: &str, key: &str, value: &str) -> SResult<()>;
    fn kv_delete(&self, org_id: i64, namespace: &str, key: &str) -> SResult<()>;
    /// (key, value) pairs of a namespace, by key.
    fn kv_list(&self, org_id: i64, namespace: &str) -> SResult<Vec<(String, String)>>;
}
