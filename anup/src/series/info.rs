use super::SeriesParams;
use crate::database::schema::series_info;
use crate::database::Database;
use crate::err::Result;
use anime::remote::RemoteService;
use diesel::prelude::*;
use std::borrow::Cow;

#[derive(Clone, Debug, Queryable, Insertable)]
#[table_name = "series_info"]
pub struct SeriesInfo {
    pub id: i32,
    pub title_preferred: String,
    pub title_romaji: String,
    pub episodes: i16,
    pub episode_length_mins: i16,
    pub sequel: Option<i32>,
}

impl SeriesInfo {
    pub fn load(db: &Database, info_id: i32) -> diesel::QueryResult<Self> {
        use crate::database::schema::series_info::dsl::*;

        series_info.filter(id.eq(info_id)).get_result(db.conn())
    }

    pub fn save(&self, db: &Database) -> diesel::QueryResult<usize> {
        use crate::database::schema::series_info::dsl::*;

        diesel::replace_into(series_info)
            .values(self)
            .execute(db.conn())
    }

    pub fn from_remote<R>(sel: InfoSelector, remote: &R) -> Result<InfoResult>
    where
        R: RemoteService + ?Sized,
    {
        match sel {
            InfoSelector::ID(id) => Self::from_remote_by_id(id, remote).map(InfoResult::Confident),
            InfoSelector::Name(name) => Self::from_remote_by_name(name, remote),
        }
    }

    pub fn from_remote_by_id<R>(id: i32, remote: &R) -> Result<Self>
    where
        R: RemoteService + ?Sized,
    {
        remote
            .search_info_by_id(id as u32)
            .map(Into::into)
            .map_err(Into::into)
    }

    pub fn from_remote_by_name<S, R>(name: S, remote: &R) -> Result<InfoResult>
    where
        S: Into<String>,
        R: RemoteService + ?Sized,
    {
        let name = name.into();

        let mut results = remote.search_info_by_name(&name)?.collect::<Vec<_>>();
        let found = detect::series_info::closest_match(results.iter().map(Cow::Borrowed), name);

        match found {
            Some((best_match, _)) => {
                let info = results.swap_remove(best_match).into();
                Ok(InfoResult::Confident(info))
            }
            None => Ok(InfoResult::Unconfident(
                results.into_iter().map(Into::into).collect(),
            )),
        }
    }
}

impl From<anime::remote::SeriesInfo> for SeriesInfo {
    fn from(value: anime::remote::SeriesInfo) -> Self {
        Self {
            id: value.id as i32,
            title_preferred: value.title.preferred,
            title_romaji: value.title.romaji,
            episodes: value.episodes as i16,
            episode_length_mins: value.episode_length as i16,
            sequel: value.sequel.map(|sequel| sequel as i32),
        }
    }
}

pub enum InfoSelector {
    Name(String),
    ID(i32),
}

impl InfoSelector {
    pub fn from_params_or_name<S>(params: &SeriesParams, nickname: S) -> Self
    where
        S: Into<String>,
    {
        params
            .id
            .map_or_else(|| Self::Name(nickname.into()), Self::ID)
    }
}

#[derive(Debug)]
pub enum InfoResult {
    Confident(SeriesInfo),
    Unconfident(Vec<SeriesInfo>),
}
