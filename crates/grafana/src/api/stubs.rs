//! Out-of-scope routes answered with an empty or disabled result of the recorded shape (alerting, live, plugin catalog, advisor, provisioning, quotas, OFREP, ...).
use crate::ctx::Routes;

pub fn routes(_r: &mut Routes) {}
