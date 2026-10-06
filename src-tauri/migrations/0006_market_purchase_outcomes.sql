-- Store completed purchases and lost Market lottery attempts in one local feed.
-- Existing rows are confirmed purchases, so SQLite applies this default.
ALTER TABLE market_purchase_history
  ADD COLUMN outcome TEXT NOT NULL DEFAULT 'purchased';
