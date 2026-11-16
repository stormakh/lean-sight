//! RBAC actions of the basic roles (agent A), for `user.permissions` in the boot data,
//! `GET /api/access-control/user/actions`, the nav tree, and fine-grained checks in handlers.
//! Upstream: pkg/services/accesscontrol (basic roles = fixed roles granted to Viewer/Editor/Admin/Grafana Admin).
//! The tables are the unscoped action sets Grafana 13.3.1 reports for each basic role (captured from a real
//! instance), sorted. Different from Grafana: no custom roles, no managed (per-resource) permissions; every user of
//! a role has the same actions.
use crate::ctx::{Role, SignedInUser};
use std::sync::OnceLock;

/// The actions of the user's basic role (plus the server admin actions), sorted.
pub fn actions(u: &SignedInUser) -> Vec<&'static str> {
    let base = role_table(u.org_role);
    let mut out: Vec<&'static str> = base.to_vec();
    if u.is_grafana_admin {
        out.extend_from_slice(SERVER_ADMIN);
        json::sort::sort(&mut out);
        out.dedup();
    }
    out
}

fn role_table(r: Role) -> &'static [&'static str] {
    match r {
        Role::None => NONE,
        Role::Viewer => VIEWER,
        Role::Editor => EDITOR,
        Role::Admin => ADMIN,
    }
}

/// True when the user's basic role grants `action`.
pub fn has(u: &SignedInUser, action: &str) -> bool {
    // Nobody (not even anonymous) has no evaluated permissions, though its permissions map lists folders:read.
    if !u.is_identified() {
        return false;
    }
    if role_table(u.org_role).binary_search(&action).is_ok() {
        return true;
    }
    u.is_grafana_admin && SERVER_ADMIN.binary_search(&action).is_ok()
}

/// True when the user has any of `actions`.
pub fn has_any(u: &SignedInUser, actions: &[&str]) -> bool {
    actions.iter().any(|a| has(u, a))
}

/// The permissions map `{"action":true,...}` as JSON text, prebuilt once per (role, server admin).
pub fn permissions_json(u: &SignedInUser) -> &'static str {
    static CACHE: OnceLock<Vec<String>> = OnceLock::new();
    let all = CACHE.get_or_init(|| {
        let mut v = Vec::with_capacity(8);
        for r in [Role::None, Role::Viewer, Role::Editor, Role::Admin] {
            for admin in [false, true] {
                let u = SignedInUser { org_role: r, is_grafana_admin: admin, ..SignedInUser::default() };
                let mut s = String::with_capacity(8192);
                s.push('{');
                for (i, a) in actions(&u).iter().enumerate() {
                    if i > 0 {
                        s.push(',');
                    }
                    json::write_str(a, &mut s);
                    s.push_str(":true");
                }
                s.push('}');
                v.push(s);
            }
        }
        v
    });
    let i = match u.org_role {
        Role::None => 0,
        Role::Viewer => 2,
        Role::Editor => 4,
        Role::Admin => 6,
    } + if u.is_grafana_admin { 1 } else { 0 };
    &all[i]
}

/// Org role None (and nobody): captured from Grafana 13.3.1.
pub static NONE: &[&str] = &[
    "folders:read",
];
/// Org role Viewer, captured from Grafana 13.3.1 (anonymous Viewer).
pub static VIEWER: &[&str] = &[
    "advisor.translations:read",
    "alert.instances.external:read",
    "alert.instances:read",
    "alert.notifications.external:read",
    "alert.notifications.inhibition-rules:read",
    "alert.notifications.receivers:list",
    "alert.notifications.receivers:read",
    "alert.notifications.templates:read",
    "alert.notifications.time-intervals:read",
    "alert.rules.external:read",
    "alert.rules:read",
    "alert.silences:read",
    "annotations:read",
    "datasources.id:read",
    "datasources:query",
    "datasources:read",
    "folders:read",
    "library.panels:read",
    "notebooks:read",
    "notifications.alerting.grafana.app/configs:get",
    "notifications.alerting.grafana.app/routingtrees:get",
    "orgs.quotas:read",
    "orgs:read",
    "playlists:read",
    "plugins.app:access",
    "provisioning.repositories:read",
    "provisioning.settings:read",
    "rules.alerting.grafana.app/configs:get",
    "snapshots:read",
    "variables:read",
];
/// Org role Editor, captured from Grafana 13.3.1 (anonymous Editor).
pub static EDITOR: &[&str] = &[
    "advisor.translations:read",
    "alert.instances.external:read",
    "alert.instances.external:write",
    "alert.instances:create",
    "alert.instances:read",
    "alert.instances:write",
    "alert.notifications.external:read",
    "alert.notifications.external:write",
    "alert.notifications.inhibition-rules:delete",
    "alert.notifications.inhibition-rules:read",
    "alert.notifications.inhibition-rules:write",
    "alert.notifications.receivers.test:create",
    "alert.notifications.receivers:create",
    "alert.notifications.receivers:delete",
    "alert.notifications.receivers:list",
    "alert.notifications.receivers:read",
    "alert.notifications.receivers:test",
    "alert.notifications.receivers:write",
    "alert.notifications.templates.test:write",
    "alert.notifications.templates:delete",
    "alert.notifications.templates:read",
    "alert.notifications.templates:write",
    "alert.notifications.time-intervals:delete",
    "alert.notifications.time-intervals:read",
    "alert.notifications.time-intervals:write",
    "alert.provisioning.provenance:write",
    "alert.rules.external:read",
    "alert.rules.external:write",
    "alert.rules:create",
    "alert.rules:delete",
    "alert.rules:read",
    "alert.rules:write",
    "alert.silences:create",
    "alert.silences:read",
    "alert.silences:write",
    "annotations:create",
    "annotations:delete",
    "annotations:read",
    "annotations:write",
    "dashboards:create",
    "datasources.id:read",
    "datasources:explore",
    "datasources:query",
    "datasources:read",
    "folders:create",
    "folders:read",
    "library.panels:create",
    "library.panels:delete",
    "library.panels:read",
    "library.panels:write",
    "live:push",
    "notebooks:create",
    "notebooks:delete",
    "notebooks:read",
    "notebooks:write",
    "notifications.alerting.grafana.app/configs:get",
    "notifications.alerting.grafana.app/routingtrees:create",
    "notifications.alerting.grafana.app/routingtrees:delete",
    "notifications.alerting.grafana.app/routingtrees:get",
    "notifications.alerting.grafana.app/routingtrees:update",
    "orgs.quotas:read",
    "orgs:read",
    "playlists:read",
    "playlists:write",
    "plugins.app:access",
    "provisioning.jobs:create",
    "provisioning.jobs:delete",
    "provisioning.jobs:read",
    "provisioning.jobs:write",
    "provisioning.repositories:read",
    "provisioning.settings:read",
    "rules.alerting.grafana.app/configs:get",
    "snapshots:create",
    "snapshots:delete",
    "snapshots:read",
    "variables:read",
];
/// Org role Admin, captured from Grafana 13.3.1 (anonymous Admin; equal to the ui-tour-v1 recording).
pub static ADMIN: &[&str] = &[
    "advisor.checks:create",
    "advisor.checks:delete",
    "advisor.checks:read",
    "advisor.checks:write",
    "advisor.checktypes:create",
    "advisor.checktypes:delete",
    "advisor.checktypes:read",
    "advisor.checktypes:write",
    "advisor.register:create",
    "advisor.translations:read",
    "alert.instances.external:read",
    "alert.instances.external:write",
    "alert.instances:create",
    "alert.instances:read",
    "alert.instances:write",
    "alert.notifications.config-history:read",
    "alert.notifications.config-history:write",
    "alert.notifications.external:read",
    "alert.notifications.external:write",
    "alert.notifications.inhibition-rules:delete",
    "alert.notifications.inhibition-rules:read",
    "alert.notifications.inhibition-rules:write",
    "alert.notifications.provisioning:read",
    "alert.notifications.provisioning:write",
    "alert.notifications.receivers.protected:write",
    "alert.notifications.receivers.secrets:read",
    "alert.notifications.receivers.test:create",
    "alert.notifications.receivers:create",
    "alert.notifications.receivers:delete",
    "alert.notifications.receivers:list",
    "alert.notifications.receivers:read",
    "alert.notifications.receivers:test",
    "alert.notifications.receivers:write",
    "alert.notifications.routes:read",
    "alert.notifications.routes:write",
    "alert.notifications.system-status:read",
    "alert.notifications.templates.test:write",
    "alert.notifications.templates:delete",
    "alert.notifications.templates:read",
    "alert.notifications.templates:write",
    "alert.notifications.time-intervals:delete",
    "alert.notifications.time-intervals:read",
    "alert.notifications.time-intervals:write",
    "alert.notifications:read",
    "alert.notifications:write",
    "alert.provisioning.provenance:write",
    "alert.provisioning.secrets:read",
    "alert.provisioning:read",
    "alert.provisioning:write",
    "alert.rules.external:read",
    "alert.rules.external:write",
    "alert.rules.provisioning:read",
    "alert.rules.provisioning:write",
    "alert.rules:create",
    "alert.rules:delete",
    "alert.rules:read",
    "alert.rules:write",
    "alert.silences:create",
    "alert.silences:read",
    "alert.silences:write",
    "annotations:create",
    "annotations:delete",
    "annotations:read",
    "annotations:write",
    "dashboards.permissions:read",
    "dashboards.permissions:write",
    "dashboards.public:write",
    "dashboards:create",
    "dashboards:delete",
    "dashboards:read",
    "dashboards:write",
    "datasources.id:read",
    "datasources:create",
    "datasources:delete",
    "datasources:explore",
    "datasources:query",
    "datasources:read",
    "datasources:write",
    "featuremgmt.read",
    "featuremgmt.write",
    "folders.permissions:read",
    "folders.permissions:write",
    "folders:create",
    "folders:delete",
    "folders:read",
    "folders:write",
    "library.panels:create",
    "library.panels:delete",
    "library.panels:read",
    "library.panels:write",
    "live:push",
    "notebooks:create",
    "notebooks:delete",
    "notebooks:read",
    "notebooks:write",
    "notifications.alerting.grafana.app/alertmanagerimports:create",
    "notifications.alerting.grafana.app/alertmanagerimports:delete",
    "notifications.alerting.grafana.app/alertmanagerimports:get",
    "notifications.alerting.grafana.app/alertmanagerimports:update",
    "notifications.alerting.grafana.app/configs:get",
    "notifications.alerting.grafana.app/configs:update",
    "notifications.alerting.grafana.app/routingtrees:create",
    "notifications.alerting.grafana.app/routingtrees:delete",
    "notifications.alerting.grafana.app/routingtrees:get",
    "notifications.alerting.grafana.app/routingtrees:get_permissions",
    "notifications.alerting.grafana.app/routingtrees:set_permissions",
    "notifications.alerting.grafana.app/routingtrees:update",
    "org.users:add",
    "org.users:read",
    "org.users:remove",
    "org.users:write",
    "orgs.preferences:read",
    "orgs.preferences:write",
    "orgs.quotas:read",
    "orgs:read",
    "orgs:write",
    "playlists:read",
    "playlists:write",
    "plugins.app:access",
    "plugins:write",
    "provisioning.connections:create",
    "provisioning.connections:delete",
    "provisioning.connections:read",
    "provisioning.connections:write",
    "provisioning.historicjobs:read",
    "provisioning.jobs:create",
    "provisioning.jobs:delete",
    "provisioning.jobs:read",
    "provisioning.jobs:write",
    "provisioning.repositories:create",
    "provisioning.repositories:delete",
    "provisioning.repositories:read",
    "provisioning.repositories:write",
    "provisioning.settings:read",
    "provisioning.stats:read",
    "receivers.permissions:read",
    "receivers.permissions:write",
    "rules.alerting.grafana.app/configs:get",
    "rules.alerting.grafana.app/configs:update",
    "secret.keepers:create",
    "secret.keepers:delete",
    "secret.keepers:read",
    "secret.keepers:write",
    "secret.securevalues:create",
    "secret.securevalues:delete",
    "secret.securevalues:read",
    "secret.securevalues:write",
    "serviceaccounts.permissions:read",
    "serviceaccounts.permissions:write",
    "serviceaccounts:create",
    "serviceaccounts:delete",
    "serviceaccounts:read",
    "serviceaccounts:write",
    "settings:read",
    "settings:write",
    "snapshots:create",
    "snapshots:delete",
    "snapshots:read",
    "teams.permissions:read",
    "teams.permissions:write",
    "teams:create",
    "teams:delete",
    "teams:read",
    "teams:write",
    "users.permissions:read",
    "variables:create",
    "variables:delete",
    "variables:read",
    "variables:write",
];
/// Added for a Grafana server admin (the api-flows-v1 recording of admin minus ADMIN).
pub static SERVER_ADMIN: &[&str] = &[
    "ldap.config:reload",
    "ldap.status:read",
    "ldap.user:read",
    "ldap.user:sync",
    "migrationassistant:migrate",
    "orgs.quotas:write",
    "orgs:create",
    "orgs:delete",
    "plugins:install",
    "provisioning:reload",
    "server.stats:read",
    "server.usagestats.report:read",
    "support.bundles:create",
    "support.bundles:delete",
    "support.bundles:read",
    "users.authtoken:read",
    "users.authtoken:write",
    "users.password:write",
    "users.permissions:write",
    "users.quotas:read",
    "users.quotas:write",
    "users:create",
    "users:delete",
    "users:disable",
    "users:enable",
    "users:logout",
    "users:read",
    "users:write",
];
