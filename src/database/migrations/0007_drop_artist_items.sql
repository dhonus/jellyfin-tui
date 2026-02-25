PRAGMA foreign_keys=OFF;

CREATE TABLE tracks_new (
  id TEXT PRIMARY KEY,
  album_id TEXT NOT NULL,
  download_status TEXT NOT NULL,
  download_size_bytes INTEGER,
  track TEXT NOT NULL,
  last_played TIMESTAMP,
  downloaded_at TIMESTAMP,
  library_id TEXT REFERENCES libraries(id),
  disliked INTEGER NOT NULL DEFAULT 0
);

INSERT INTO tracks_new (
  id,
  album_id,
  download_status,
  download_size_bytes,
  track,
  last_played,
  downloaded_at,
  library_id,
  disliked
)
SELECT
  id,
  album_id,
  download_status,
  download_size_bytes,
  track,
  last_played,
  downloaded_at,
  library_id,
  disliked
FROM tracks;

DROP TABLE tracks;

ALTER TABLE tracks_new RENAME TO tracks;

CREATE TRIGGER update_json_download_status
AFTER UPDATE OF download_status ON tracks
FOR EACH ROW
BEGIN
  UPDATE tracks
  SET track = json_set(track, '$.download_status', NEW.download_status)
  WHERE id = NEW.id;
END;

CREATE INDEX idx_tracks_library_id ON tracks(library_id);

PRAGMA foreign_keys=ON;
