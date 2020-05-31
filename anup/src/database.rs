use crate::err::Result;
use crate::file::SaveDir;
use diesel::connection::SimpleConnection;
use diesel::deserialize::{self, FromSql};
use diesel::prelude::*;
use diesel::serialize::{self, Output, ToSql};
use diesel::sql_types::{Nullable, Text};
use smallvec::SmallVec;
use std::io::Write;
use std::ops::Deref;
use std::path::PathBuf;

pub mod schema {
    table! {
        series_configs {
            id -> Integer,
            nickname -> Text,
            path -> Text,
            // TODO: this should be migrated
            #[sql_name = "episode_matcher"]
            episode_parser -> Nullable<Text>,
            player_args -> Nullable<Text>,
        }
    }

    table! {
        series_info {
            id -> Integer,
            title_preferred -> Text,
            title_romaji -> Text,
            episodes -> SmallInt,
            episode_length_mins -> SmallInt,
            sequel -> Nullable<Integer>,
        }
    }

    table! {
        series_entries {
            id -> Integer,
            watched_episodes -> SmallInt,
            score -> Nullable<SmallInt>,
            status -> SmallInt,
            times_rewatched -> SmallInt,
            start_date -> Nullable<Date>,
            end_date -> Nullable<Date>,
            needs_sync -> Bool,
        }
    }
}

pub struct Database(SqliteConnection);

impl Database {
    pub fn open() -> Result<Self> {
        let path = Self::validated_path()?;
        let conn = SqliteConnection::establish(&path.to_string_lossy())?;

        conn.batch_execute(include_str!("../sql/schema.sql"))?;

        Ok(Self(conn))
    }

    pub fn validated_path() -> Result<PathBuf> {
        let mut path = SaveDir::LocalData.validated_dir_path()?.to_path_buf();
        path.push("data.sqlite");
        Ok(path)
    }

    #[inline(always)]
    pub fn conn(&self) -> &SqliteConnection {
        &self.0
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        self.conn().execute("PRAGMA optimize").ok();
    }
}

#[derive(Clone, Debug, Default, AsExpression, FromSqlRow)]
#[sql_type = "Text"]
pub struct PlayerArgs(SmallVec<[String; 3]>);

impl PlayerArgs {
    #[inline(always)]
    pub fn new() -> Self {
        Self(SmallVec::new())
    }
}

impl<DB> FromSql<Nullable<Text>, DB> for PlayerArgs
where
    DB: diesel::backend::Backend,
    String: FromSql<Text, DB>,
{
    fn from_sql(bytes: Option<&DB::RawValue>) -> deserialize::Result<Self> {
        match bytes {
            Some(_) => {
                let args = String::from_sql(bytes)?
                    .split(";;")
                    .map(Into::into)
                    .collect();

                Ok(Self(args))
            }
            None => Ok(Self::new()),
        }
    }
}

impl<DB> ToSql<Text, DB> for PlayerArgs
where
    DB: diesel::backend::Backend,
    String: ToSql<Text, DB>,
{
    fn to_sql<W: Write>(&self, out: &mut Output<W, DB>) -> serialize::Result {
        let value = self.0.join(";;");
        value.to_sql(out)
    }
}

impl AsRef<SmallVec<[String; 3]>> for PlayerArgs {
    fn as_ref(&self) -> &SmallVec<[String; 3]> {
        &self.0
    }
}

impl From<SmallVec<[String; 3]>> for PlayerArgs {
    fn from(value: SmallVec<[String; 3]>) -> Self {
        Self(value)
    }
}

impl Deref for PlayerArgs {
    type Target = SmallVec<[String; 3]>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
