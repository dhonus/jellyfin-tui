-- Playlist membership becomes positional: one row per entry, keyed by (playlist_id, position),
-- so duplicates are preserved. SQLite can't change a PK in place, and the old schema allowed
-- several rows of a playlist to share a position, so rebuild and renumber with ROW_NUMBER.
CREATE TABLE playlist_membership_new (
  playlist_id TEXT NOT NULL,
  track_id    TEXT NOT NULL,
  position    INTEGER NOT NULL,
  PRIMARY KEY (playlist_id, position)
);

INSERT INTO playlist_membership_new (playlist_id, track_id, position)
SELECT playlist_id, track_id,
       ROW_NUMBER() OVER (PARTITION BY playlist_id ORDER BY position, rowid) - 1
FROM playlist_membership;

DROP TABLE playlist_membership;
ALTER TABLE playlist_membership_new RENAME TO playlist_membership;
