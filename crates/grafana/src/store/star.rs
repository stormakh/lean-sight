//! Starred dashboards (table `star`).
use super::SResult;

pub trait StarStore {
    /// Dashboard uids the user starred in the org.
    fn stars_of_user(&self, org_id: i64, user_id: i64) -> SResult<Vec<String>>;
    /// Idempotent.
    fn star_add(&self, org_id: i64, user_id: i64, dashboard_uid: &str) -> SResult<()>;
    /// Idempotent.
    fn star_remove(&self, org_id: i64, user_id: i64, dashboard_uid: &str) -> SResult<()>;
}
