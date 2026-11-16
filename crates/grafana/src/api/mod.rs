//! HTTP API, one module per area. Each module has `pub fn routes(r: &mut Routes)`; `register` calls them all.
//! Area owners: boot + auth + users (agent A), dashboards + folders + search (agent B),
//! datasources + prometheus (agent C), annotations + stubs (shared, small).
pub mod annotations;
pub mod auth;
pub mod boot;
pub mod dashboards;
pub mod datasources;
pub mod folders;
pub mod health;
pub mod prometheus;
pub mod public;
pub mod query_history;
pub mod search;
pub mod stubs;
pub mod users;

use crate::ctx::Routes;

pub fn register(r: &mut Routes) {
    health::routes(r);
    public::routes(r);
    boot::routes(r);
    auth::routes(r);
    users::routes(r);
    dashboards::routes(r);
    folders::routes(r);
    search::routes(r);
    datasources::routes(r);
    prometheus::routes(r);
    query_history::routes(r);
    annotations::routes(r);
    stubs::routes(r);
}
