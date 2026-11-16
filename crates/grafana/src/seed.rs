//! First start: org 1 "Main Org." and the admin user from `[security] admin_user` / `admin_password`
//! (upstream pkg/services/user/userimpl createDefaultAdmin and pkg/services/org auto-assign).
use crate::ctx::Role;
use crate::settings::Settings;
use crate::store::{SResult, Store, StoreError, User};
use crate::util::{now, short_uid};

pub const MAIN_ORG: &str = "Main Org.";

pub fn seed(store: &dyn Store, cfg: &Settings) -> SResult<()> {
    let org_id = match store.org_by_name(MAIN_ORG) {
        Ok(o) => o.id,
        Err(StoreError::NotFound) => store.org_create(MAIN_ORG)?,
        Err(e) => return Err(e),
    };
    if store.user_count()? > 0 {
        return Ok(());
    }
    let t = now();
    let salt = crypto::random_string(10, &[]);
    let u = User {
        uid: short_uid(),
        login: cfg.admin_user.clone(),
        email: if cfg.admin_email.is_empty() { "admin@localhost".to_string() } else { cfg.admin_email.clone() },
        password: crypto::encode_password(&cfg.admin_password, &salt),
        salt,
        rands: crypto::random_string(10, &[]),
        org_id,
        is_admin: true,
        created: t,
        updated: t,
        ..User::default()
    };
    let id = store.user_create(&u)?;
    store.org_user_set(org_id, id, Role::Admin)?;
    Ok(())
}
