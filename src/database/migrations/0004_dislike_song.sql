PRAGMA foreign_keys = OFF;

ALTER TABLE tracks ADD COLUMN disliked INTEGER NOT NULL DEFAULT 0;

PRAGMA foreign_keys = ON;
