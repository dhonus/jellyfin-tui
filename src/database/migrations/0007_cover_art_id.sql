PRAGMA foreign_keys = OFF;

-- Stores the resolved cover art item ID for offline track downloads.
-- Set to the song's own ID when song_cover_art preference is enabled,
-- or the album/parent ID otherwise.
ALTER TABLE tracks ADD COLUMN cover_art_id TEXT;

PRAGMA foreign_keys = ON;
