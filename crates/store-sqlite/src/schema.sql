-- grafana-rs schema v1. Table and column names follow Grafana's (pkg/services/sqlstore/migrations); times are
-- unix seconds (annotation epochs: milliseconds); text columns are NOT NULL with '' for "none".
CREATE TABLE IF NOT EXISTS org (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  version INTEGER NOT NULL DEFAULT 0,
  name TEXT NOT NULL UNIQUE,
  created INTEGER NOT NULL,
  updated INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS org_user (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  org_id INTEGER NOT NULL,
  user_id INTEGER NOT NULL,
  role TEXT NOT NULL,
  created INTEGER NOT NULL,
  updated INTEGER NOT NULL,
  UNIQUE (org_id, user_id)
);
CREATE INDEX IF NOT EXISTS idx_org_user_user ON org_user (user_id);
CREATE TABLE IF NOT EXISTS user (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  uid TEXT NOT NULL UNIQUE,
  version INTEGER NOT NULL DEFAULT 0,
  login TEXT NOT NULL UNIQUE COLLATE NOCASE,
  email TEXT NOT NULL UNIQUE COLLATE NOCASE,
  name TEXT NOT NULL DEFAULT '',
  password TEXT NOT NULL DEFAULT '',
  salt TEXT NOT NULL DEFAULT '',
  rands TEXT NOT NULL DEFAULT '',
  company TEXT NOT NULL DEFAULT '',
  org_id INTEGER NOT NULL,
  is_admin INTEGER NOT NULL DEFAULT 0,
  email_verified INTEGER NOT NULL DEFAULT 0,
  theme TEXT NOT NULL DEFAULT '',
  help_flags1 INTEGER NOT NULL DEFAULT 0,
  is_disabled INTEGER NOT NULL DEFAULT 0,
  is_service_account INTEGER NOT NULL DEFAULT 0,
  is_provisioned INTEGER NOT NULL DEFAULT 0,
  created INTEGER NOT NULL,
  updated INTEGER NOT NULL,
  last_seen_at INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS user_auth_token (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id INTEGER NOT NULL,
  auth_token TEXT NOT NULL UNIQUE,
  prev_auth_token TEXT NOT NULL,
  user_agent TEXT NOT NULL,
  client_ip TEXT NOT NULL,
  auth_token_seen INTEGER NOT NULL,
  seen_at INTEGER NOT NULL DEFAULT 0,
  rotated_at INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  revoked_at INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_user_auth_token_prev ON user_auth_token (prev_auth_token);
CREATE INDEX IF NOT EXISTS idx_user_auth_token_user ON user_auth_token (user_id);
CREATE TABLE IF NOT EXISTS api_key (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  org_id INTEGER NOT NULL,
  name TEXT NOT NULL,
  key TEXT NOT NULL UNIQUE,
  role TEXT NOT NULL,
  created INTEGER NOT NULL,
  updated INTEGER NOT NULL,
  expires INTEGER NOT NULL DEFAULT 0,
  service_account_id INTEGER NOT NULL DEFAULT 0,
  last_used_at INTEGER NOT NULL DEFAULT 0,
  is_revoked INTEGER NOT NULL DEFAULT 0,
  UNIQUE (org_id, name)
);
CREATE TABLE IF NOT EXISTS preferences (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  org_id INTEGER NOT NULL,
  user_id INTEGER NOT NULL,
  team_id INTEGER NOT NULL,
  version INTEGER NOT NULL DEFAULT 0,
  home_dashboard_id INTEGER NOT NULL DEFAULT 0,
  home_dashboard_uid TEXT NOT NULL DEFAULT '',
  timezone TEXT NOT NULL DEFAULT '',
  theme TEXT NOT NULL DEFAULT '',
  week_start TEXT NOT NULL DEFAULT '',
  json_data TEXT NOT NULL DEFAULT '',
  created INTEGER NOT NULL,
  updated INTEGER NOT NULL,
  UNIQUE (org_id, user_id, team_id)
);
CREATE TABLE IF NOT EXISTS star (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  org_id INTEGER NOT NULL,
  user_id INTEGER NOT NULL,
  dashboard_uid TEXT NOT NULL,
  updated INTEGER NOT NULL,
  UNIQUE (org_id, user_id, dashboard_uid)
);
CREATE TABLE IF NOT EXISTS folder (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  uid TEXT NOT NULL,
  org_id INTEGER NOT NULL,
  title TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  parent_uid TEXT NOT NULL DEFAULT '',
  version INTEGER NOT NULL DEFAULT 1,
  created INTEGER NOT NULL,
  updated INTEGER NOT NULL,
  created_by INTEGER NOT NULL DEFAULT 0,
  updated_by INTEGER NOT NULL DEFAULT 0,
  UNIQUE (org_id, uid),
  UNIQUE (org_id, parent_uid, title)
);
CREATE TABLE IF NOT EXISTS dashboard (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  uid TEXT NOT NULL,
  org_id INTEGER NOT NULL,
  version INTEGER NOT NULL,
  slug TEXT NOT NULL,
  title TEXT NOT NULL,
  data TEXT NOT NULL,
  folder_uid TEXT NOT NULL DEFAULT '',
  created INTEGER NOT NULL,
  updated INTEGER NOT NULL,
  created_by INTEGER NOT NULL DEFAULT 0,
  updated_by INTEGER NOT NULL DEFAULT 0,
  gnet_id INTEGER NOT NULL DEFAULT 0,
  plugin_id TEXT NOT NULL DEFAULT '',
  api_version TEXT NOT NULL DEFAULT '',
  UNIQUE (org_id, uid)
);
CREATE INDEX IF NOT EXISTS idx_dashboard_folder ON dashboard (org_id, folder_uid);
CREATE TABLE IF NOT EXISTS dashboard_version (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  dashboard_id INTEGER NOT NULL,
  dashboard_uid TEXT NOT NULL,
  parent_version INTEGER NOT NULL,
  restored_from INTEGER NOT NULL DEFAULT 0,
  version INTEGER NOT NULL,
  created INTEGER NOT NULL,
  created_by INTEGER NOT NULL,
  message TEXT NOT NULL DEFAULT '',
  data TEXT NOT NULL,
  api_version TEXT NOT NULL DEFAULT '',
  UNIQUE (dashboard_id, version)
);
CREATE TABLE IF NOT EXISTS dashboard_tag (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  dashboard_id INTEGER NOT NULL,
  org_id INTEGER NOT NULL,
  term TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_dashboard_tag_dash ON dashboard_tag (dashboard_id);
CREATE TABLE IF NOT EXISTS data_source (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  uid TEXT NOT NULL,
  org_id INTEGER NOT NULL,
  version INTEGER NOT NULL DEFAULT 1,
  type TEXT NOT NULL,
  name TEXT NOT NULL,
  access TEXT NOT NULL DEFAULT 'proxy',
  url TEXT NOT NULL DEFAULT '',
  user TEXT NOT NULL DEFAULT '',
  database TEXT NOT NULL DEFAULT '',
  basic_auth INTEGER NOT NULL DEFAULT 0,
  basic_auth_user TEXT NOT NULL DEFAULT '',
  with_credentials INTEGER NOT NULL DEFAULT 0,
  is_default INTEGER NOT NULL DEFAULT 0,
  json_data TEXT NOT NULL DEFAULT '',
  secure_json_data TEXT NOT NULL DEFAULT '',
  read_only INTEGER NOT NULL DEFAULT 0,
  api_version TEXT NOT NULL DEFAULT '',
  created INTEGER NOT NULL,
  updated INTEGER NOT NULL,
  UNIQUE (org_id, uid),
  UNIQUE (org_id, name)
);
CREATE TABLE IF NOT EXISTS annotation (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  org_id INTEGER NOT NULL,
  alert_id INTEGER NOT NULL DEFAULT 0,
  user_id INTEGER NOT NULL DEFAULT 0,
  dashboard_id INTEGER NOT NULL DEFAULT 0,
  dashboard_uid TEXT NOT NULL DEFAULT '',
  panel_id INTEGER NOT NULL DEFAULT 0,
  text TEXT NOT NULL DEFAULT '',
  prev_state TEXT NOT NULL DEFAULT '',
  new_state TEXT NOT NULL DEFAULT '',
  data TEXT NOT NULL DEFAULT '',
  epoch INTEGER NOT NULL,
  epoch_end INTEGER NOT NULL,
  tags TEXT NOT NULL DEFAULT '',
  created INTEGER NOT NULL,
  updated INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_annotation_org_epoch ON annotation (org_id, epoch_end, epoch);
CREATE INDEX IF NOT EXISTS idx_annotation_dash ON annotation (org_id, dashboard_uid);
CREATE TABLE IF NOT EXISTS query_history (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  uid TEXT NOT NULL UNIQUE,
  org_id INTEGER NOT NULL,
  datasource_uid TEXT NOT NULL,
  created_by INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  comment TEXT NOT NULL DEFAULT '',
  queries TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_query_history_user ON query_history (org_id, created_by, created_at);
CREATE TABLE IF NOT EXISTS query_history_star (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  query_uid TEXT NOT NULL,
  user_id INTEGER NOT NULL,
  org_id INTEGER NOT NULL,
  UNIQUE (user_id, query_uid)
);
CREATE TABLE IF NOT EXISTS kv_store (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  org_id INTEGER NOT NULL,
  namespace TEXT NOT NULL,
  key TEXT NOT NULL,
  value TEXT NOT NULL,
  created INTEGER NOT NULL,
  updated INTEGER NOT NULL,
  UNIQUE (org_id, namespace, key)
);
CREATE TABLE IF NOT EXISTS team (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  uid TEXT NOT NULL DEFAULT '',
  org_id INTEGER NOT NULL,
  name TEXT NOT NULL,
  email TEXT NOT NULL DEFAULT '',
  created INTEGER NOT NULL,
  updated INTEGER NOT NULL,
  UNIQUE (org_id, name)
);
CREATE TABLE IF NOT EXISTS team_member (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  org_id INTEGER NOT NULL,
  team_id INTEGER NOT NULL,
  user_id INTEGER NOT NULL,
  permission INTEGER NOT NULL DEFAULT 0,
  created INTEGER NOT NULL,
  updated INTEGER NOT NULL,
  UNIQUE (team_id, user_id)
);
