-- Metadata learned directly from `market.item`. The Market's item namespace
-- can differ from the generic inventory catalog (notably Ball ID 5), so this
-- cache is intentionally isolated from the mirrored game item metadata.
CREATE TABLE IF NOT EXISTS market_item_metadata (
  item_id INTEGER PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  category TEXT,
  asset_path TEXT,
  updated_at INTEGER NOT NULL
);
