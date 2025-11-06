PRAGMA foreign_keys=OFF;

CREATE TABLE IF NOT EXISTS libraries (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  last_seen TIMESTAMP NOT NULL,
  selected INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS album_artist (
    album_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    PRIMARY KEY (album_id, artist_id)
);

ALTER TABLE albums ADD COLUMN library_id TEXT REFERENCES libraries(id);
CREATE INDEX IF NOT EXISTS idx_albums_library_id ON albums(library_id);

ALTER TABLE tracks ADD COLUMN library_id TEXT REFERENCES libraries(id);
CREATE INDEX IF NOT EXISTS idx_tracks_library_id ON tracks(library_id);

PRAGMA foreign_keys=ON;
