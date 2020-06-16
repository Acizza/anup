CREATE TEMPORARY TABLE series_info_backup (
    id INTEGER NOT NULL PRIMARY KEY,
    title_preferred TEXT NOT NULL,
    title_romaji TEXT NOT NULL,
    episodes SMALLINT NOT NULL,
    episode_length_mins SMALLINT NOT NULL
);

INSERT INTO series_info_backup SELECT id, title_preferred, title_romaji, episodes, episode_length_mins FROM series_info;
DROP TABLE series_info;

CREATE TABLE series_info (
    id INTEGER NOT NULL PRIMARY KEY,
    title_preferred TEXT NOT NULL,
    title_romaji TEXT NOT NULL,
    episodes SMALLINT NOT NULL,
    episode_length_mins SMALLINT NOT NULL,
    FOREIGN KEY(id) REFERENCES series_configs(id) ON DELETE CASCADE
);

INSERT INTO series_info SELECT * FROM series_info_backup;
DROP TABLE series_info_backup;