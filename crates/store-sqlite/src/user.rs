use crate::SqliteStore;
use grafana::Role;
use grafana::store::{SResult, User, UserInOrg, UserStore};
use rusqlite::params;

macro_rules! cols {
    () => {
        "id, uid, version, login, email, name, password, salt, rands, company, org_id, is_admin, email_verified, theme, help_flags1, is_disabled, is_service_account, is_provisioned, created, updated, last_seen_at"
    };
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<User> {
    Ok(User {
        id: r.get(0)?,
        uid: r.get(1)?,
        version: r.get(2)?,
        login: r.get(3)?,
        email: r.get(4)?,
        name: r.get(5)?,
        password: r.get(6)?,
        salt: r.get(7)?,
        rands: r.get(8)?,
        company: r.get(9)?,
        org_id: r.get(10)?,
        is_admin: r.get(11)?,
        email_verified: r.get(12)?,
        theme: r.get(13)?,
        help_flags1: r.get(14)?,
        is_disabled: r.get(15)?,
        is_service_account: r.get(16)?,
        is_provisioned: r.get(17)?,
        created: r.get(18)?,
        updated: r.get(19)?,
        last_seen_at: r.get(20)?,
    })
}

impl UserStore for SqliteStore {
    fn user_by_id(&self, id: i64) -> SResult<User> {
        self.db(|c| c.prepare_cached(concat!("SELECT ", cols!(), " FROM user WHERE id = ?1"))?.query_row(params![id], row))
    }
    fn user_in_org(&self, id: i64, org_id: i64) -> SResult<UserInOrg> {
        let (user, req_role, req_name, cur_role, cur_name) = self.db(|c| {
            c.prepare_cached(concat!(
                "SELECT ",
                cols!(),
                // the requested org's subqueries run only when another org is asked for
                ", CASE WHEN ?2 > 0 AND ?2 <> u.org_id THEN (SELECT role FROM org_user WHERE org_id = ?2 AND user_id = u.id) END,",
                " CASE WHEN ?2 > 0 AND ?2 <> u.org_id THEN (SELECT name FROM org WHERE id = ?2) END,",
                " (SELECT role FROM org_user WHERE org_id = u.org_id AND user_id = u.id),",
                " (SELECT name FROM org WHERE id = u.org_id)",
                " FROM user u WHERE id = ?1"
            ))?
            .query_row(params![id, org_id], |r| {
                Ok((row(r)?, r.get::<_, Option<String>>(21)?, r.get::<_, Option<String>>(22)?, r.get::<_, Option<String>>(23)?, r.get::<_, Option<String>>(24)?))
            })
        })?;
        let role = |s: Option<String>| s.and_then(|s| Role::parse(&s)).unwrap_or(Role::None);
        Ok(match req_role {
            Some(r) => UserInOrg { user, org_id, role: role(Some(r)), org_name: req_name.unwrap_or_default() },
            None => UserInOrg { org_id: user.org_id, user, role: role(cur_role), org_name: cur_name.unwrap_or_default() },
        })
    }
    fn user_display_login(&self, id: i64) -> SResult<String> {
        self.db(|c| c.prepare_cached("SELECT CASE WHEN is_service_account THEN name ELSE login END FROM user WHERE id = ?1")?.query_row(params![id], |r| r.get(0)))
    }
    fn user_by_uid(&self, uid: &str) -> SResult<User> {
        self.db(|c| c.prepare_cached(concat!("SELECT ", cols!(), " FROM user WHERE uid = ?1"))?.query_row(params![uid], row))
    }
    fn user_by_login_or_email(&self, login_or_email: &str) -> SResult<User> {
        self.db(|c| {
            c.prepare_cached(concat!("SELECT ", cols!(), " FROM user WHERE login = ?1 OR email = ?1 ORDER BY (login = ?1) DESC LIMIT 1"))?
                .query_row(params![login_or_email], row)
        })
    }
    fn user_create(&self, u: &User) -> SResult<i64> {
        self.db(|c| {
            c.execute(
                "INSERT INTO user (uid, version, login, email, name, password, salt, rands, company, org_id, is_admin, email_verified, theme, help_flags1, is_disabled, is_service_account, is_provisioned, created, updated, last_seen_at)
                 VALUES (?1, 0, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
                params![u.uid, u.login, u.email, u.name, u.password, u.salt, u.rands, u.company, u.org_id, u.is_admin, u.email_verified, u.theme, u.help_flags1, u.is_disabled, u.is_service_account, u.is_provisioned, u.created, u.updated, u.last_seen_at],
            )?;
            Ok(c.last_insert_rowid())
        })
    }
    fn user_update(&self, u: &User) -> SResult<()> {
        let n = self.db(|c| {
            c.execute(
                "UPDATE user SET uid = ?2, version = version + 1, login = ?3, email = ?4, name = ?5, password = ?6, salt = ?7, rands = ?8, company = ?9, org_id = ?10, is_admin = ?11, email_verified = ?12, theme = ?13, help_flags1 = ?14, is_disabled = ?15, is_service_account = ?16, is_provisioned = ?17, updated = ?18, last_seen_at = ?19 WHERE id = ?1",
                params![u.id, u.uid, u.login, u.email, u.name, u.password, u.salt, u.rands, u.company, u.org_id, u.is_admin, u.email_verified, u.theme, u.help_flags1, u.is_disabled, u.is_service_account, u.is_provisioned, u.updated, u.last_seen_at],
            )
        })?;
        if n == 0 { Err(grafana::StoreError::NotFound) } else { Ok(()) }
    }
    fn user_set_last_seen(&self, id: i64, at: i64) -> SResult<()> {
        self.db(|c| c.prepare_cached("UPDATE user SET last_seen_at = ?2 WHERE id = ?1")?.execute(params![id, at]).map(|_| ()))
    }
    fn user_count(&self) -> SResult<i64> {
        self.db(|c| c.query_row("SELECT COUNT(*) FROM user WHERE is_service_account = 0", [], |r| r.get(0)))
    }
    fn user_search(&self, query: &str, limit: i64, offset: i64) -> SResult<(Vec<User>, i64)> {
        let like = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
        self.db(|c| {
            let filter = "is_service_account = 0 AND (?1 = '' OR login LIKE ?2 ESCAPE '\\' OR email LIKE ?2 ESCAPE '\\' OR name LIKE ?2 ESCAPE '\\')";
            let total: i64 = c.query_row(&format!("SELECT COUNT(*) FROM user WHERE {filter}"), params![query, like], |r| r.get(0))?;
            let mut st = c.prepare(&format!("SELECT {} FROM user WHERE {filter} ORDER BY login LIMIT ?3 OFFSET ?4", cols!()))?;
            let rows: rusqlite::Result<Vec<User>> = st.query_map(params![query, like, limit, offset], row)?.collect();
            Ok((rows?, total))
        })
    }
    fn user_delete(&self, id: i64) -> SResult<()> {
        self.tx(|c| {
            let n = c.execute("DELETE FROM user WHERE id = ?1", params![id]).map_err(crate::map_err)?;
            if n == 0 {
                return Err(grafana::StoreError::NotFound);
            }
            for sql in [
                "DELETE FROM org_user WHERE user_id = ?1",
                "DELETE FROM user_auth_token WHERE user_id = ?1",
                "DELETE FROM star WHERE user_id = ?1",
                "DELETE FROM preferences WHERE user_id = ?1",
                "DELETE FROM team_member WHERE user_id = ?1",
                "DELETE FROM query_history_star WHERE user_id = ?1",
            ] {
                c.execute(sql, params![id]).map_err(crate::map_err)?;
            }
            Ok(())
        })
    }
}
