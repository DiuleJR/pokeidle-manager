CREATE TABLE IF NOT EXISTS market_sniper_rules (
  id TEXT PRIMARY KEY NOT NULL,
  account_id TEXT NOT NULL,
  config_json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

-- These are changing public offer summaries, not sale transactions. They are
-- retained briefly for diagnostics and future confirmed analysis only.
CREATE TABLE IF NOT EXISTS market_observations (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  observed_at INTEGER NOT NULL,
  item_id INTEGER NOT NULL,
  listings INTEGER NOT NULL,
  units INTEGER NOT NULL,
  min_gold INTEGER,
  min_orb INTEGER
);
CREATE INDEX IF NOT EXISTS idx_market_observations_observed_at
  ON market_observations(observed_at);
CREATE INDEX IF NOT EXISTS idx_market_observations_item_observed
  ON market_observations(item_id, observed_at);
