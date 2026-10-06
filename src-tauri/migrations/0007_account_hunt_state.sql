CREATE TABLE IF NOT EXISTS account_hunt_state (
  account_id TEXT PRIMARY KEY NOT NULL,
  hunt_slug TEXT NOT NULL,
  started_at_ms INTEGER NOT NULL,
  revision INTEGER NOT NULL CHECK (revision > 0),
  FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
);
