use super::{SeriesConfig, SeriesEntry};
use crate::err::Result;
use crate::file::SaveDir;
use anime::remote::SeriesInfo;
use rusqlite::{named_params, params, Connection, Row, NO_PARAMS};
use std::path::PathBuf;

pub struct Database(Connection);

impl Database {
    pub fn open() -> Result<Self> {
        let path = Self::validated_path()?;
        let conn = Connection::open(path)?;
        conn.execute_batch(include_str!("../../sql/schema.sql"))?;
        Ok(Self(conn))
    }

    pub fn validated_path() -> Result<PathBuf> {
        let mut path = SaveDir::LocalData.validated_dir_path()?.to_path_buf();
        path.push("data.sqlite");
        Ok(path)
    }

    #[inline(always)]
    pub fn conn(&self) -> &Connection {
        &self.0
    }
}

macro_rules! query {
    ($type:expr, $name:expr) => {
        include_str!(concat!("../../sql/", $type, "/", $name, ".sql"))
    };
}

macro_rules! select {
    ($name:expr) => {
        query!("select", $name)
    };
}

macro_rules! insert {
    ($name:expr) => {
        query!("insert", $name)
    };
}

macro_rules! delete {
    ($name:expr) => {
        query!("delete", $name)
    };
}

pub fn get_series_names(db: &Database) -> Result<Vec<String>> {
    let mut query = db.conn().prepare(select!("series_names"))?;

    let results = query
        .query_map(NO_PARAMS, |row| Ok(row.get(0)?))?
        .flatten()
        .collect();

    Ok(results)
}

pub fn get_series_entries_need_sync(db: &Database) -> Result<Vec<SeriesEntry>> {
    let mut query = db.conn().prepare(select!("anime_entries_need_sync"))?;

    let results = query
        .query_map(NO_PARAMS, SeriesEntry::from_db_row)?
        .flatten()
        .collect();

    Ok(results)
}

pub trait Insertable {
    type ExtraData;

    fn insert_into_db(&self, db: &Database, data: Self::ExtraData) -> Result<()>;
}

impl Insertable for SeriesConfig {
    type ExtraData = anime::remote::SeriesID;

    fn insert_into_db(&self, db: &Database, id: Self::ExtraData) -> Result<()> {
        let mut query = db.conn().prepare_cached(insert!("series_config"))?;

        query.execute_named(named_params! {
            ":id": id,
            ":nickname": self.nickname,
            ":path": self.path.to_string_lossy(),
            ":episode_matcher": self.episode_matcher,
            ":player_args": self.player_args.join(" ")
        })?;

        Ok(())
    }
}

impl Insertable for SeriesInfo {
    type ExtraData = ();

    fn insert_into_db(&self, db: &Database, _: Self::ExtraData) -> Result<()> {
        let mut query = db.conn().prepare_cached(insert!("anime_info"))?;

        query.execute_named(named_params! {
            ":id": self.id,
            ":title_preferred": self.title.preferred,
            ":title_romaji": self.title.romaji,
            ":episodes": self.episodes,
            ":episode_length": self.episode_length,
            ":sequel": self.sequel
        })?;

        Ok(())
    }
}

impl Insertable for SeriesEntry {
    type ExtraData = ();

    fn insert_into_db(&self, db: &Database, _: Self::ExtraData) -> Result<()> {
        let mut query = db.conn().prepare_cached(insert!("anime_entry"))?;

        query.execute_named(named_params! {
            ":id": self.id(),
            ":watched_episodes": self.watched_eps(),
            ":score": self.score(),
            ":status": self.status(),
            ":times_rewatched": self.times_rewatched(),
            ":start_date": self.start_date(),
            ":finish_date": self.end_date(),
            ":needs_sync": self.needs_sync
        })?;

        Ok(())
    }
}

pub trait Selectable<Sel>: Sized {
    fn from_db_row(row: &Row) -> rusqlite::Result<Self>;
    fn select_from_db(db: &Database, selector: Sel) -> Result<Self>;
}

impl<'a> Selectable<&'a str> for SeriesConfig {
    fn from_db_row(row: &Row) -> rusqlite::Result<Self> {
        let config = Self {
            id: row.get(0)?,
            nickname: row.get(1)?,
            path: {
                let path: String = row.get(2)?;
                PathBuf::from(path)
            },
            episode_matcher: row.get(3)?,
            player_args: row
                .get(4)
                .map(|args: String| args.split_whitespace().map(str::to_string).collect())
                .unwrap_or_else(|_| Vec::new()),
        };

        Ok(config)
    }

    fn select_from_db(db: &Database, nickname: &'a str) -> Result<Self> {
        let mut query = db
            .conn()
            .prepare_cached(select!("series_config_by_nickname"))?;

        let result = query.query_row(params![nickname], Self::from_db_row)?;
        Ok(result)
    }
}

impl Selectable<anime::remote::SeriesID> for SeriesInfo {
    fn from_db_row(row: &Row) -> rusqlite::Result<Self> {
        use anime::remote::SeriesTitle;

        let info = SeriesInfo {
            id: row.get(0)?,
            title: SeriesTitle {
                preferred: row.get(1)?,
                romaji: row.get(2)?,
            },
            episodes: row.get(3)?,
            episode_length: row.get(4)?,
            sequel: row.get(5)?,
        };

        Ok(info)
    }

    fn select_from_db(db: &Database, id: anime::remote::SeriesID) -> Result<Self> {
        let mut query = db.conn().prepare_cached(select!("anime_info"))?;
        let info = query.query_row(params![id], Self::from_db_row)?;
        Ok(info)
    }
}

impl Selectable<anime::remote::SeriesID> for SeriesEntry {
    fn from_db_row(row: &Row) -> rusqlite::Result<Self> {
        let entry = anime::remote::SeriesEntry {
            id: row.get(0)?,
            watched_eps: row.get(1)?,
            score: row.get(2)?,
            status: row.get(3)?,
            times_rewatched: row.get(4)?,
            start_date: row.get(5)?,
            end_date: row.get(6)?,
        };

        Ok(SeriesEntry {
            entry,
            needs_sync: row.get(7)?,
        })
    }

    fn select_from_db(db: &Database, id: anime::remote::SeriesID) -> Result<Self> {
        let mut query = db.conn().prepare_cached(select!("anime_entry"))?;
        let entry = query.query_row(params![id], Self::from_db_row)?;
        Ok(entry)
    }
}

pub trait Deletable<Fil> {
    fn delete_from_db(db: &Database, filter: Fil) -> Result<()>;
}

impl<'a> Deletable<&'a str> for SeriesConfig {
    fn delete_from_db(db: &Database, nickname: &'a str) -> Result<()> {
        let mut query = db.conn().prepare_cached(delete!("series_config"))?;
        query.execute(params![nickname])?;
        Ok(())
    }
}
