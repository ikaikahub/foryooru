CREATE TABLE users (
  id         INTEGER PRIMARY KEY,
  username   TEXT NOT NULL UNIQUE,
  key_hash   TEXT NOT NULL,
  -- bumped once per push so pulls can ask for everything after a version
  version    INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);

CREATE TABLE sessions (
  token_hash TEXT PRIMARY KEY,
  user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  created_at INTEGER NOT NULL
);
CREATE INDEX sessions_by_user ON sessions (user_id);

-- one row per liked post, disliked post, favourite tag, blacklisted tag or saved search.
-- removals stay as present = 0 so other devices learn about them
CREATE TABLE items (
  user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  site       TEXT NOT NULL,
  list       TEXT NOT NULL,
  key        TEXT NOT NULL,
  present    INTEGER NOT NULL CHECK (present IN (0, 1)),
  sort_at    INTEGER NOT NULL,
  changed_at INTEGER NOT NULL,
  version    INTEGER NOT NULL,
  post       TEXT,
  PRIMARY KEY (user_id, site, list, key)
);
CREATE INDEX items_by_version ON items (user_id, site, version);
