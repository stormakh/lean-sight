use crate::SqliteStore;
use grafana::store::{PrefStore, Preferences, SResult};
use rusqlite::params;

impl PrefStore for SqliteStore {
    fn prefs_get(&self, org_id: i64, user_id: i64, team_id: i64) -> SResult<Preferences> {
        self.db(|c| {
            c.prepare_cached(
                "SELECT id, org_id, user_id, team_id, version, home_dashboard_id, home_dashboard_uid, timezone, theme, week_start, json_data, created, updated
                 FROM preferences WHERE org_id = ?1 AND user_id = ?2 AND team_id = ?3",
            )?
            .query_row(params![org_id, user_id, team_id], |r| {
                Ok(Preferences {
                    id: r.get(0)?,
                    org_id: r.get(1)?,
                    user_id: r.get(2)?,
                    team_id: r.get(3)?,
                    version: r.get(4)?,
                    home_dashboard_id: r.get(5)?,
                    home_dashboard_uid: r.get(6)?,
                    timezone: r.get(7)?,
                    theme: r.get(8)?,
                    week_start: r.get(9)?,
                    json_data: r.get(10)?,
                    created: r.get(11)?,
                    updated: r.get(12)?,
                })
            })
        })
    }
    fn prefs_save(&self, p: &Preferences) -> SResult<()> {
        self.db(|c| {
            c.execute(
                "INSERT INTO preferences (org_id, user_id, team_id, version, home_dashboard_id, home_dashboard_uid, timezone, theme, week_start, json_data, created, updated)
                 VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT (org_id, user_id, team_id) DO UPDATE SET version = version + 1, home_dashboard_id = excluded.home_dashboard_id,
                   home_dashboard_uid = excluded.home_dashboard_uid, timezone = excluded.timezone, theme = excluded.theme,
                   week_start = excluded.week_start, json_data = excluded.json_data, updated = excluded.updated",
                params![p.org_id, p.user_id, p.team_id, p.home_dashboard_id, p.home_dashboard_uid, p.timezone, p.theme, p.week_start, p.json_data, p.created, p.updated],
            )?;
            Ok(())
        })
    }
}
