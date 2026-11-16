use crate::SqliteStore;
use grafana::store::{SResult, Team, TeamStore};
use rusqlite::params;

const SEL: &str = "SELECT t.id, t.uid, t.org_id, t.name, t.email, (SELECT COUNT(*) FROM team_member m WHERE m.team_id = t.id), t.created, t.updated FROM team t";
fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Team> {
    Ok(Team { id: r.get(0)?, uid: r.get(1)?, org_id: r.get(2)?, name: r.get(3)?, email: r.get(4)?, member_count: r.get(5)?, created: r.get(6)?, updated: r.get(7)? })
}

impl TeamStore for SqliteStore {
    fn team_search(&self, org_id: i64, query: &str, limit: i64, offset: i64) -> SResult<(Vec<Team>, i64)> {
        let like = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
        self.db(|c| {
            let total: i64 = c.query_row("SELECT COUNT(*) FROM team WHERE org_id = ?1 AND (?2 = '' OR name LIKE ?3 ESCAPE '\\')", params![org_id, query, like], |r| r.get(0))?;
            let mut st = c.prepare(&format!("{SEL} WHERE t.org_id = ?1 AND (?2 = '' OR t.name LIKE ?3 ESCAPE '\\') ORDER BY t.name LIMIT ?4 OFFSET ?5"))?;
            let rows: rusqlite::Result<Vec<Team>> = st.query_map(params![org_id, query, like, limit, offset], row)?.collect();
            Ok((rows?, total))
        })
    }
    fn teams_of_user(&self, org_id: i64, user_id: i64) -> SResult<Vec<Team>> {
        self.db(|c| {
            let mut st = c.prepare(&format!("{SEL} WHERE t.org_id = ?1 AND t.id IN (SELECT team_id FROM team_member WHERE user_id = ?2) ORDER BY t.name"))?;
            let rows = st.query_map(params![org_id, user_id], row)?;
            rows.collect()
        })
    }
}
