PRAGMA foreign_keys=ON;

CREATE TABLE IF NOT EXISTS tracks (
  id TEXT PRIMARY KEY,
  album_id TEXT NOT NULL,
  artist_items TEXT NOT NULL,
  download_status TEXT NOT NULL,
  download_size_bytes INTEGER,
  track TEXT NOT NULL,
  last_played TIMESTAMP,
  downloaded_at TIMESTAMP
);

-- this client uses DiscographySong structs everywhere (track)
-- to avoid dealing with json_set in every GET function, we update the JSON download_status
-- at every change, avoiding inconsistent data
CREATE TRIGGER IF NOT EXISTS update_json_download_status
AFTER UPDATE OF download_status ON tracks
FOR EACH ROW
BEGIN
  UPDATE tracks
  SET track = json_set(track, '$.download_status', NEW.download_status)
  WHERE id = NEW.id;
END;

CREATE TABLE IF NOT EXISTS artists (
  id TEXT PRIMARY KEY,
  artist TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS albums (
  id TEXT PRIMARY KEY,
  album TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS playlists (
  id TEXT PRIMARY KEY,
  playlist TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS artist_membership (
  artist_id TEXT NOT NULL,
  track_id  TEXT NOT NULL,
  PRIMARY KEY (artist_id, track_id)
);

CREATE TABLE IF NOT EXISTS playlist_membership (
  playlist_id TEXT NOT NULL,
  track_id    TEXT NOT NULL,
  position    INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (playlist_id, track_id)
);

CREATE TABLE IF NOT EXISTS lyrics (
  id TEXT PRIMARY KEY,
  lyric TEXT NOT NULL
);
