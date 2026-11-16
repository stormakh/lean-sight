use crate::{SqliteStore, map_err};
use grafana::StoreError;
use grafana::store::{QueryHistory, QueryHistoryQuery, QueryHistoryStore, SResult};
use rusqlite::params;
use rusqlite::types::Value;

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<QueryHistory> {
    Ok(QueryHistory {
        id: r.get(0)?,
        uid: r.get(1)?,
        org_id: r.get(2)?,
        datasource_uid: r.get(3)?,
        created_by: r.get(4)?,
        created_at: r.get(5)?,
        comment: r.get(6)?,
        queries: r.get(7)?,
        starred: r.get(8)?,
    })
}
const SEL: &str = "SELECT q.id, q.uid, q.org_id, q.datasource_uid, q.created_by, q.created_at, q.comment, q.queries,
    EXISTS (SELECT 1 FROM query_history_star s WHERE s.query_uid = q.uid AND s.user_id = ?1) FROM query_history q";

impl QueryHistoryStore for SqliteStore {
    fn query_history_create(&self, q: &QueryHistory) -> SResult<i64> {
        self.db(|c| {
            c.execute(
                "INSERT INTO query_history (uid, org_id, datasource_uid, created_by, created_at, comment, queries) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![q.uid, q.org_id, q.datasource_uid, q.created_by, q.created_at, q.comment, q.queries],
            )?;
            Ok(c.last_insert_rowid())
        })
    }
    fn query_history_search(&self, q: &QueryHistoryQuery) -> SResult<(Vec<QueryHistory>, i64)> {
        let mut filter = String::from(" WHERE q.org_id = ?2 AND q.created_by = ?1");
        let mut args: Vec<Value> = vec![Value::Integer(q.user_id), Value::Integer(q.org_id)];
        if !q.datasource_uids.is_empty() {
            filter.push_str(" AND q.datasource_uid IN (");
            for (i, u) in q.datasource_uids.iter().enumerate() {
                if i > 0 {
                    filter.push(',');
                }
                filter.push('?');
                filter.push_str(&(args.len() + 1).to_string());
                args.push(Value::Text(u.clone()));
            }
            filter.push(')');
        }
        if !q.search.is_empty() {
            let n = args.len() + 1;
            filter.push_str(&format!(" AND (q.queries LIKE ?{n} OR q.comment LIKE ?{n})"));
            args.push(Value::Text(format!("%{}%", q.search)));
        }
        if q.only_starred {
            filter.push_str(" AND EXISTS (SELECT 1 FROM query_history_star s WHERE s.query_uid = q.uid AND s.user_id = ?1)");
        }
        if q.from > 0 {
            filter.push_str(&format!(" AND q.created_at >= ?{}", args.len() + 1));
            args.push(Value::Integer(q.from));
        }
        if q.to > 0 {
            filter.push_str(&format!(" AND q.created_at <= ?{}", args.len() + 1));
            args.push(Value::Integer(q.to));
        }
        let order = if q.sort == "time-asc" { " ORDER BY q.created_at ASC" } else { " ORDER BY q.created_at DESC" };
        let limit = if q.limit > 0 { q.limit } else { 100 };
        let page = if q.page > 0 { q.page } else { 1 };
        self.db(|c| {
            let total: i64 = c.query_row(&format!("SELECT COUNT(*) FROM query_history q{filter}"), rusqlite::params_from_iter(args.iter()), |r| r.get(0))?;
            let sql = format!("{SEL}{filter}{order} LIMIT {limit} OFFSET {}", (page - 1) * limit);
            let mut st = c.prepare(&sql)?;
            let rows: rusqlite::Result<Vec<QueryHistory>> = st.query_map(rusqlite::params_from_iter(args.iter()), row)?.collect();
            Ok((rows?, total))
        })
    }
    fn query_history_by_uid(&self, org_id: i64, user_id: i64, uid: &str) -> SResult<QueryHistory> {
        self.db(|c| c.query_row(&format!("{SEL} WHERE q.org_id = ?2 AND q.created_by = ?1 AND q.uid = ?3"), params![user_id, org_id, uid], row))
    }
    fn query_history_delete(&self, org_id: i64, user_id: i64, uid: &str) -> SResult<()> {
        self.tx(|c| {
            let n = c.execute("DELETE FROM query_history WHERE org_id = ?1 AND created_by = ?2 AND uid = ?3", params![org_id, user_id, uid]).map_err(map_err)?;
            if n == 0 {
                return Err(StoreError::NotFound);
            }
            c.execute("DELETE FROM query_history_star WHERE query_uid = ?1", params![uid]).map_err(map_err)?;
            Ok(())
        })
    }
    fn query_history_set_comment(&self, org_id: i64, user_id: i64, uid: &str, comment: &str) -> SResult<()> {
        let n = self.db(|c| c.execute("UPDATE query_history SET comment = ?4 WHERE org_id = ?1 AND created_by = ?2 AND uid = ?3", params![org_id, user_id, uid, comment]))?;
        if n == 0 { Err(StoreError::NotFound) } else { Ok(()) }
    }
    fn query_history_star(&self, org_id: i64, user_id: i64, uid: &str, starred: bool) -> SResult<()> {
        self.db(|c| {
            if starred {
                c.execute("INSERT OR IGNORE INTO query_history_star (query_uid, user_id, org_id) VALUES (?1, ?2, ?3)", params![uid, user_id, org_id])?;
            } else {
                c.execute("DELETE FROM query_history_star WHERE query_uid = ?1 AND user_id = ?2", params![uid, user_id])?;
            }
            Ok(())
        })
    }
}
