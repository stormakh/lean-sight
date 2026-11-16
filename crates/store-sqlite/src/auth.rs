use crate::SqliteStore;
use grafana::StoreError;
use grafana::store::{ApiKey, AuthStore, SResult, UserAuthToken};
use rusqlite::params;

macro_rules! tcols {
    () => {
        "id, user_id, auth_token, prev_auth_token, user_agent, client_ip, auth_token_seen, seen_at, rotated_at, created_at, updated_at, revoked_at"
    };
}
macro_rules! kcols {
    () => {
        "id, org_id, name, key, role, expires, service_account_id, last_used_at, is_revoked, created, updated"
    };
}

fn trow(r: &rusqlite::Row<'_>) -> rusqlite::Result<UserAuthToken> {
    Ok(UserAuthToken {
        id: r.get(0)?,
        user_id: r.get(1)?,
        auth_token: r.get(2)?,
        prev_auth_token: r.get(3)?,
        user_agent: r.get(4)?,
        client_ip: r.get(5)?,
        auth_token_seen: r.get(6)?,
        seen_at: r.get(7)?,
        rotated_at: r.get(8)?,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
        revoked_at: r.get(11)?,
    })
}
fn krow(r: &rusqlite::Row<'_>) -> rusqlite::Result<ApiKey> {
    Ok(ApiKey {
        id: r.get(0)?,
        org_id: r.get(1)?,
        name: r.get(2)?,
        key: r.get(3)?,
        role: r.get(4)?,
        expires: r.get(5)?,
        service_account_id: r.get(6)?,
        last_used_at: r.get(7)?,
        is_revoked: r.get(8)?,
        created: r.get(9)?,
        updated: r.get(10)?,
    })
}

impl AuthStore for SqliteStore {
    fn token_create(&self, t: &UserAuthToken) -> SResult<i64> {
        self.db(|c| {
            c.execute(
                "INSERT INTO user_auth_token (user_id, auth_token, prev_auth_token, user_agent, client_ip, auth_token_seen, seen_at, rotated_at, created_at, updated_at, revoked_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![t.user_id, t.auth_token, t.prev_auth_token, t.user_agent, t.client_ip, t.auth_token_seen, t.seen_at, t.rotated_at, t.created_at, t.updated_at, t.revoked_at],
            )?;
            Ok(c.last_insert_rowid())
        })
    }
    fn token_by_hash(&self, hashed: &str) -> SResult<UserAuthToken> {
        self.db(|c| {
            c.prepare_cached(concat!("SELECT ", tcols!(), " FROM user_auth_token WHERE auth_token = ?1 UNION ALL SELECT ", tcols!(), " FROM user_auth_token WHERE prev_auth_token = ?1 LIMIT 1"))?
                .query_row(params![hashed], trow)
        })
    }
    fn token_by_id(&self, id: i64) -> SResult<UserAuthToken> {
        self.db(|c| c.prepare_cached(concat!("SELECT ", tcols!(), " FROM user_auth_token WHERE id = ?1"))?.query_row(params![id], trow))
    }
    fn token_update(&self, t: &UserAuthToken) -> SResult<()> {
        let n = self.db(|c| {
            c.prepare_cached(
                "UPDATE user_auth_token SET user_id = ?2, auth_token = ?3, prev_auth_token = ?4, user_agent = ?5, client_ip = ?6, auth_token_seen = ?7, seen_at = ?8, rotated_at = ?9, created_at = ?10, updated_at = ?11, revoked_at = ?12 WHERE id = ?1",
            )?
            .execute(params![t.id, t.user_id, t.auth_token, t.prev_auth_token, t.user_agent, t.client_ip, t.auth_token_seen, t.seen_at, t.rotated_at, t.created_at, t.updated_at, t.revoked_at])
        })?;
        if n == 0 { Err(StoreError::NotFound) } else { Ok(()) }
    }
    fn token_mark_seen(&self, id: i64, hashed: &str, at: i64) -> SResult<()> {
        self.db(|c| {
            c.prepare_cached("UPDATE user_auth_token SET auth_token_seen = 1, seen_at = ?3 WHERE id = ?1 AND auth_token = ?2 AND auth_token_seen = 0")?
                .execute(params![id, hashed, at])
                .map(|_| ())
        })
    }
    fn token_rotate(&self, id: i64, expected: &str, new_hash: &str, user_agent: &str, client_ip: &str, at: i64) -> SResult<bool> {
        // upstream rotate_token.sql, plus the expected current token so that two concurrent rotations cannot both win
        let n = self.db(|c| {
            c.prepare_cached(
                "UPDATE user_auth_token SET seen_at = 0, user_agent = ?4, client_ip = ?5, prev_auth_token = auth_token, auth_token = ?3, auth_token_seen = 0, rotated_at = ?6, updated_at = ?6 WHERE id = ?1 AND auth_token = ?2",
            )?
            .execute(params![id, expected, new_hash, user_agent, client_ip, at])
        })?;
        Ok(n > 0)
    }
    fn token_delete(&self, id: i64) -> SResult<()> {
        self.db(|c| c.execute("DELETE FROM user_auth_token WHERE id = ?1", params![id]).map(|_| ()))
    }
    fn tokens_of_user(&self, user_id: i64) -> SResult<Vec<UserAuthToken>> {
        self.db(|c| {
            let mut st = c.prepare_cached(concat!("SELECT ", tcols!(), " FROM user_auth_token WHERE user_id = ?1 AND revoked_at = 0 ORDER BY created_at DESC"))?;
            let rows = st.query_map(params![user_id], trow)?;
            rows.collect()
        })
    }
    fn tokens_delete_for_user(&self, user_id: i64) -> SResult<()> {
        self.db(|c| c.execute("DELETE FROM user_auth_token WHERE user_id = ?1", params![user_id]).map(|_| ()))
    }
    fn api_key_by_hash(&self, hashed: &str) -> SResult<ApiKey> {
        self.db(|c| c.prepare_cached(concat!("SELECT ", kcols!(), " FROM api_key WHERE key = ?1"))?.query_row(params![hashed], krow))
    }
    fn api_key_create(&self, k: &ApiKey) -> SResult<i64> {
        self.db(|c| {
            c.execute(
                "INSERT INTO api_key (org_id, name, key, role, expires, service_account_id, last_used_at, is_revoked, created, updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![k.org_id, k.name, k.key, k.role, k.expires, k.service_account_id, k.last_used_at, k.is_revoked, k.created, k.updated],
            )?;
            Ok(c.last_insert_rowid())
        })
    }
    fn api_key_set_last_used(&self, id: i64, at: i64) -> SResult<()> {
        self.db(|c| c.prepare_cached("UPDATE api_key SET last_used_at = ?2 WHERE id = ?1")?.execute(params![id, at]).map(|_| ()))
    }
}
