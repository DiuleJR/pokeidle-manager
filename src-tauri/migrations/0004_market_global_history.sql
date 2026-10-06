-- Public, completed Community Market transactions. This is deliberately
-- separate from market_observations (open offers) and from this Manager's own
-- confirmed purchases.
CREATE TABLE IF NOT EXISTS market_global_history (
  id INTEGER PRIMARY KEY NOT NULL,
  occurred_at INTEGER NOT NULL,
  kind TEXT NOT NULL,
  currency TEXT NOT NULL,
  description TEXT NOT NULL,
  total INTEGER NOT NULL,
  seller TEXT NOT NULL,
  buyer TEXT NOT NULL,
  item_name TEXT,
  quantity INTEGER,
  pokemon_name TEXT,
  pokemon_level INTEGER,
  pokemon_looktype INTEGER,
  pokemon_shiny INTEGER NOT NULL DEFAULT 0,
  pokemon_look_shiny INTEGER
);

CREATE INDEX IF NOT EXISTS idx_market_global_history_time
  ON market_global_history(occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_market_global_history_item_time
  ON market_global_history(kind, item_name, occurred_at DESC);
