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
}

