CREATE TABLE IF NOT EXISTS accounts (
  id TEXT PRIMARY KEY NOT NULL,
  nick TEXT NOT NULL UNIQUE,
  local_alias TEXT,
  card_color TEXT NOT NULL,
  avatar_key TEXT,
  last_active_at INTEGER,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS automation_settings (
  account_id TEXT NOT NULL,
  automation_key TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 0,
  config_json TEXT NOT NULL DEFAULT '{}',
  PRIMARY KEY (account_id, automation_key),
  FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS account_history (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id TEXT NOT NULL,
  started_at INTEGER NOT NULL,
  ended_at INTEGER,
  duration_seconds INTEGER,
  kills INTEGER NOT NULL DEFAULT 0,
  captures INTEGER NOT NULL DEFAULT 0,
  xp_gained INTEGER NOT NULL DEFAULT 0,
  gold_gained INTEGER NOT NULL DEFAULT 0,
  auto_sale_gold INTEGER NOT NULL DEFAULT 0,
  potions_used_json TEXT NOT NULL DEFAULT '{}',
  balls_used_json TEXT NOT NULL DEFAULT '{}',
  disconnects INTEGER NOT NULL DEFAULT 0,
  error_summary TEXT,
  FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS app_settings (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY NOT NULL, applied_at INTEGER NOT NULL);
