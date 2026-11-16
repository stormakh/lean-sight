use crate::{SqliteStore, tags_from_text, tags_to_text};
use grafana::StoreError;
use grafana::store::{Annotation, AnnotationQuery, AnnotationStore, SResult};
use rusqlite::params;
use rusqlite::types::Value;

macro_rules! cols {
    () => {
        "id, org_id, user_id, dashboard_id, dashboard_uid, panel_id, alert_id, text, epoch, epoch_end, tags, data, prev_state, new_state, created, updated"
    };
}
fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Annotation> {
    let tags: String = r.get(10)?;
    Ok(Annotation {
        id: r.get(0)?,
        org_id: r.get(1)?,
        user_id: r.get(2)?,
        dashboard_id: r.get(3)?,
        dashboard_uid: r.get(4)?,
        panel_id: r.get(5)?,
        alert_id: r.get(6)?,
        text: r.get(7)?,
        epoch_ms: r.get(8)?,
        epoch_end_ms: r.get(9)?,
        tags: tags_from_text(&tags),
        data: r.get(11)?,
        prev_state: r.get(12)?,
        new_state: r.get(13)?,
        created_ms: r.get(14)?,
        updated_ms: r.get(15)?,
    })
}

impl AnnotationStore for SqliteStore {
    fn annotation_create(&self, a: &Annotation) -> SResult<i64> {
        self.db(|c| {
            c.execute(
                "INSERT INTO annotation (org_id, user_id, dashboard_id, dashboard_uid, panel_id, alert_id, text, epoch, epoch_end, tags, data, prev_state, new_state, created, updated)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![a.org_id, a.user_id, a.dashboard_id, a.dashboard_uid, a.panel_id, a.alert_id, a.text, a.epoch_ms, a.epoch_end_ms, tags_to_text(&a.tags), a.data, a.prev_state, a.new_state, a.created_ms, a.updated_ms],
            )?;
            Ok(c.last_insert_rowid())
        })
    }
    fn annotation_update(&self, a: &Annotation) -> SResult<()> {
        let n = self.db(|c| {
            c.execute(
                "UPDATE annotation SET text = ?3, epoch = ?4, epoch_end = ?5, tags = ?6, data = ?7, updated = ?8 WHERE org_id = ?1 AND id = ?2",
                params![a.org_id, a.id, a.text, a.epoch_ms, a.epoch_end_ms, tags_to_text(&a.tags), a.data, a.updated_ms],
            )
        })?;
        if n == 0 { Err(StoreError::NotFound) } else { Ok(()) }
    }
    fn annotation_delete(&self, org_id: i64, id: i64) -> SResult<()> {
        let n = self.db(|c| c.execute("DELETE FROM annotation WHERE org_id = ?1 AND id = ?2", params![org_id, id]))?;
        if n == 0 { Err(StoreError::NotFound) } else { Ok(()) }
    }
    fn annotations_delete(&self, org_id: i64, dashboard_uid: &str, panel_id: i64) -> SResult<i64> {
        let n = self.db(|c| c.execute("DELETE FROM annotation WHERE org_id = ?1 AND dashboard_uid = ?2 AND panel_id = ?3", params![org_id, dashboard_uid, panel_id]))?;
        Ok(n as i64)
    }
    fn annotation_by_id(&self, org_id: i64, id: i64) -> SResult<Annotation> {
        self.db(|c| c.prepare_cached(concat!("SELECT ", cols!(), " FROM annotation WHERE org_id = ?1 AND id = ?2"))?.query_row(params![org_id, id], row))
    }
    fn annotations_find(&self, q: &AnnotationQuery) -> SResult<Vec<Annotation>> {
        let mut sql = String::from(concat!("SELECT ", cols!(), " FROM annotation WHERE org_id = ?"));
        let mut args: Vec<Value> = vec![Value::Integer(q.org_id)];
        if q.from_ms > 0 && q.to_ms > 0 {
            sql.push_str(" AND epoch <= ? AND epoch_end >= ?");
            args.push(Value::Integer(q.to_ms));
            args.push(Value::Integer(q.from_ms));
        }
        if !q.dashboard_uid.is_empty() {
            sql.push_str(" AND dashboard_uid = ?");
            args.push(Value::Text(q.dashboard_uid.clone()));
        }
        if q.dashboard_id > 0 {
            sql.push_str(" AND dashboard_id = ?");
            args.push(Value::Integer(q.dashboard_id));
        }
        if q.panel_id > 0 {
            sql.push_str(" AND panel_id = ?");
            args.push(Value::Integer(q.panel_id));
        }
        if q.user_id > 0 {
            sql.push_str(" AND user_id = ?");
            args.push(Value::Integer(q.user_id));
        }
        if q.alert_id > 0 {
            sql.push_str(" AND alert_id = ?");
            args.push(Value::Integer(q.alert_id));
        }
        match q.type_.as_str() {
            "alert" => sql.push_str(" AND alert_id > 0"),
            "annotation" => sql.push_str(" AND alert_id = 0"),
            _ => {}
        }
        sql.push_str(" ORDER BY epoch * 1000 + epoch_end DESC");
        let limit = if q.limit > 0 { q.limit } else { 100 };
        let rows: Vec<Annotation> = self.db(|c| {
            let mut st = c.prepare(&sql)?;
            let rows = st.query_map(rusqlite::params_from_iter(args.iter()), row)?;
            rows.collect()
        })?;
        // tag filter in Rust (tags are a JSON array column)
        let mut out = Vec::new();
        for a in rows {
            if !q.tags.is_empty() {
                let hit = |t: &String| a.tags.iter().any(|x| x == t);
                let ok = if q.match_any { q.tags.iter().any(hit) } else { q.tags.iter().all(hit) };
                if !ok {
                    continue;
                }
            }
            out.push(a);
            if out.len() as i64 >= limit {
                break;
            }
        }
        Ok(out)
    }
}

