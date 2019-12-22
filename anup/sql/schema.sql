PRAGMA journal_mode = WAL;

PRAGMA synchronous = NORMAL;

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS SeriesConfig (
    id INTEGER NOT NULL PRIMARY KEY,
    nickname TEXT NOT NULL UNIQUE,
    path TEXT NOT NULL,
    episode_matcher TEXT,
    player_args TEXT
);

CREATE TABLE IF NOT EXISTS AnimeInfo (
    id INTEGER NOT NULL PRIMARY KEY,
    title_preferred TEXT NOT NULL,
    title_romaji TEXT NOT NULL,
    episodes SMALLINT NOT NULL,
    episode_length_mins SMALLINT NOT NULL,
    sequel INT,
    FOREIGN KEY(id) REFERENCES SeriesConfig(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS AnimeEntry (
    id INTEGER NOT NULL PRIMARY KEY,
    watched_episodes SMALLINT NOT NULL,
    score TINYINT,
    status TINYINT NOT NULL,
    times_rewatched SMALLINT NOT NULL,
    start_date TEXT,
    finish_date TEXT,
    needs_sync BIT NOT NULL,
    FOREIGN KEY(id) REFERENCES SeriesConfig(id) ON DELETE CASCADE
);