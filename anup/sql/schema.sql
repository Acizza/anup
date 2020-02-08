PRAGMA locking_mode = EXCLUSIVE;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS series_configs (
    id INTEGER NOT NULL PRIMARY KEY,
    nickname TEXT NOT NULL UNIQUE,
    path TEXT NOT NULL,
    episode_matcher TEXT,
    player_args TEXT
);

CREATE TABLE IF NOT EXISTS series_info (
    id INTEGER NOT NULL PRIMARY KEY,
    title_preferred TEXT NOT NULL,
    title_romaji TEXT NOT NULL,
    episodes SMALLINT NOT NULL,
    episode_length_mins SMALLINT NOT NULL,
    sequel INTEGER,
    FOREIGN KEY(id) REFERENCES series_configs(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS series_entries (
    id INTEGER NOT NULL PRIMARY KEY,
    watched_episodes SMALLINT NOT NULL,
    score SMALLINT,
    status SMALLINT NOT NULL,
    times_rewatched SMALLINT NOT NULL,
    start_date DATE,
    end_date DATE,
    needs_sync BIT NOT NULL,
    FOREIGN KEY(id) REFERENCES series_configs(id) ON DELETE CASCADE
);