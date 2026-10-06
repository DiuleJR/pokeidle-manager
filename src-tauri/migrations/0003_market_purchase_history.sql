-- Local audit trail of purchases confirmed to this Manager through
-- batalha.ev[].k = marketComprado. This is deliberately not the Community
-- Market price table, and never infers completed transactions from listings.
CREATE TABLE IF NOT EXISTS market_purchase_history (
  id TEXT PRIMARY KEY NOT NULL,
  purchased_at INTEGER NOT NULL,
  account_id TEXT NOT NULL,
  rule_id TEXT NOT NULL,
  listing_id INTEGER NOT NULL,
  item_id INTEGER NOT NULL,
  description TEXT NOT NULL,
  quantity INTEGER NOT NULL,
  total INTEGER NOT NULL,
  currency TEXT NOT NULL,
  FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_market_purchase_history_account_time
  ON market_purchase_history(account_id, purchased_at DESC);
CREATE INDEX IF NOT EXISTS idx_market_purchase_history_time
  ON market_purchase_history(purchased_at DESC);
