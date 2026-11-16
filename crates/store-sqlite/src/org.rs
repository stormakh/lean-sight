use crate::SqliteStore;
use grafana::Role;
use grafana::store::{Org, OrgMember, OrgStore, SResult};
use grafana::util::now;
use rusqlite::params;

macro_rules! cols {
    () => {
        "id, name, created, updated"
    };
}
fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Org> {
    Ok(Org { id: r.get(0)?, name: r.get(1)?, created: r.get(2)?, updated: r.get(3)? })
}
fn role(s: &str) -> Role {
    Role::parse(s).unwrap_or(Role::None)
}

impl OrgStore for SqliteStore {
    fn org_by_id(&self, id: i64) -> SResult<Org> {
        self.db(|c| c.prepare_cached(concat!("SELECT ", cols!(), " FROM org WHERE id = ?1"))?.query_row(params![id], row))
    }
    fn org_by_name(&self, name: &str) -> SResult<Org> {
        self.db(|c| c.prepare_cached(concat!("SELECT ", cols!(), " FROM org WHERE name = ?1"))?.query_row(params![name], row))
    }
    fn org_create(&self, name: &str) -> SResult<i64> {
        let t = now();
        self.db(|c| {
            c.execute("INSERT INTO org (version, name, created, updated) VALUES (0, ?1, ?2, ?2)", params![name, t])?;
            Ok(c.last_insert_rowid())
        })
    }
    fn org_user_role(&self, org_id: i64, user_id: i64) -> SResult<Role> {
        let s: String = self.db(|c| c.prepare_cached("SELECT role FROM org_user WHERE org_id = ?1 AND user_id = ?2")?.query_row(params![org_id, user_id], |r| r.get(0)))?;
        Ok(role(&s))
    }
    fn org_user_set(&self, org_id: i64, user_id: i64, r: Role) -> SResult<()> {
        let t = now();
        self.db(|c| {
            c.execute(
                "INSERT INTO org_user (org_id, user_id, role, created, updated) VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT (org_id, user_id) DO UPDATE SET role = excluded.role, updated = excluded.updated",
                params![org_id, user_id, r.as_str(), t],
            )?;
            Ok(())
        })
    }
    fn orgs_of_user(&self, user_id: i64) -> SResult<Vec<(Org, Role)>> {
        self.db(|c| {
            let mut st = c.prepare_cached("SELECT o.id, o.name, o.created, o.updated, ou.role FROM org_user ou JOIN org o ON o.id = ou.org_id WHERE ou.user_id = ?1 ORDER BY o.name")?;
            let rows = st.query_map(params![user_id], |r| {
                let rs: String = r.get(4)?;
                Ok((row(r)?, role(&rs)))
            })?;
            rows.collect()
        })
    }
    fn org_members(&self, org_id: i64) -> SResult<Vec<OrgMember>> {
        self.db(|c| {
            let mut st = c.prepare_cached(
                "SELECT ou.org_id, ou.user_id, ou.role, u.login, u.email, u.name, u.last_seen_at, ou.created
                 FROM org_user ou JOIN user u ON u.id = ou.user_id WHERE ou.org_id = ?1 ORDER BY u.login",
            )?;
            let rows = st.query_map(params![org_id], |r| {
                let rs: String = r.get(2)?;
                Ok(OrgMember { org_id: r.get(0)?, user_id: r.get(1)?, role: role(&rs), login: r.get(3)?, email: r.get(4)?, name: r.get(5)?, last_seen_at: r.get(6)?, created: r.get(7)? })
            })?;
            rows.collect()
        })
    }
    fn org_update_name(&self, id: i64, name: &str) -> SResult<()> {
        let t = now();
        let n = self.db(|c| c.execute("UPDATE org SET name = ?2, updated = ?3, version = version + 1 WHERE id = ?1", params![id, name, t]))?;
        if n == 0 { Err(grafana::StoreError::NotFound) } else { Ok(()) }
    }
    fn org_delete(&self, id: i64) -> SResult<()> {
        self.tx(|c| {
            let n = c.execute("DELETE FROM org WHERE id = ?1", params![id]).map_err(crate::map_err)?;
            if n == 0 {
                return Err(grafana::StoreError::NotFound);
            }
            for sql in [
                "DELETE FROM preferences WHERE org_id = ?1",
                "DELETE FROM star WHERE org_id = ?1",
                "DELETE FROM data_source WHERE org_id = ?1",
                "DELETE FROM dashboard_tag WHERE dashboard_id IN (SELECT id FROM dashboard WHERE org_id = ?1)",
                "DELETE FROM dashboard_version WHERE dashboard_id IN (SELECT id FROM dashboard WHERE org_id = ?1)",
                "DELETE FROM dashboard WHERE org_id = ?1",
                "DELETE FROM folder WHERE org_id = ?1",
                "DELETE FROM annotation WHERE org_id = ?1",
                "DELETE FROM api_key WHERE org_id = ?1",
                "DELETE FROM team_member WHERE org_id = ?1",
                "DELETE FROM team WHERE org_id = ?1",
                "DELETE FROM kv_store WHERE org_id = ?1",
                "DELETE FROM org_user WHERE org_id = ?1",
                "UPDATE user SET org_id = COALESCE((SELECT ou.org_id FROM org_user ou WHERE ou.user_id = user.id ORDER BY ou.org_id LIMIT 1), 0) WHERE org_id = ?1",
            ] {
                c.execute(sql, params![id]).map_err(crate::map_err)?;
            }
            Ok(())
        })
    }
    fn org_search(&self, query: &str, limit: i64, offset: i64) -> SResult<(Vec<Org>, i64)> {
        let like = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
        self.db(|c| {
            let total: i64 = c.query_row("SELECT COUNT(*) FROM org WHERE (?1 = '' OR name LIKE ?2 ESCAPE '\\')", params![query, like], |r| r.get(0))?;
            let mut st = c.prepare(concat!("SELECT ", cols!(), " FROM org WHERE (?1 = '' OR name LIKE ?2 ESCAPE '\\') ORDER BY name LIMIT ?3 OFFSET ?4"))?;
            let rows: rusqlite::Result<Vec<Org>> = st.query_map(params![query, like, limit, offset], row)?.collect();
            Ok((rows?, total))
        })
    }
    fn org_user_remove(&self, org_id: i64, user_id: i64) -> SResult<()> {
        self.tx(|c| {
            let n = c.execute("DELETE FROM org_user WHERE org_id = ?1 AND user_id = ?2", params![org_id, user_id]).map_err(crate::map_err)?;
            if n == 0 {
                return Err(grafana::StoreError::NotFound);
            }
            c.execute(
                "UPDATE user SET org_id = COALESCE((SELECT ou.org_id FROM org_user ou WHERE ou.user_id = ?2 ORDER BY ou.org_id LIMIT 1), 0) WHERE id = ?2 AND org_id = ?1",
                params![org_id, user_id],
            )
            .map_err(crate::map_err)?;
            Ok(())
        })
    }
}

