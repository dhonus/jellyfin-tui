CREATE TABLE IF NOT EXISTS missing_counters (
  entity_type TEXT NOT NULL, -- 'album' | 'artist' | 'playlist'
  id          TEXT NOT NULL,
  missing_seen_count INTEGER NOT NULL DEFAULT 1,
  last_checked_at   INTEGER NOT NULL,  -- unix seconds
  PRIMARY KEY (entity_type, id)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_missing_album ON missing_counters(entity_type, missing_seen_count);
