use super::SeriesParams;
use crate::config::Config;
use crate::database::schema::series_configs;
use crate::database::{self, Database};
use crate::err::Result;
use anime::local::EpisodeMatcher;
use diesel::prelude::*;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Queryable, Insertable)]
pub struct SeriesConfig {
    pub id: i32,
    pub nickname: String,
    path: database::Path,
    pub episode_matcher: EpisodeMatcher,
    pub player_args: database::PlayerArgs,
}

impl SeriesConfig {
    pub fn new<'a, S, P>(
        id: i32,
        nickname: S,
        path: P,
        episode_matcher: EpisodeMatcher,
        config: &Config,
    ) -> Self
    where
        S: Into<String>,
        P: Into<Cow<'a, Path>>,
    {
        Self {
            id,
            nickname: nickname.into(),
            path: config.stripped_path(path).into(),
            episode_matcher,
            player_args: database::PlayerArgs::new(),
        }
    }

    pub fn from_params<S, C>(
        nickname: S,
        id: i32,
        path: C,
        params: SeriesParams,
        config: &Config,
    ) -> Result<Self>
    where
        S: Into<String>,
        C: Into<PathBuf>,
    {
        let nickname = nickname.into();

        let path = {
            let source = params.path.unwrap_or_else(|| path.into());
            config.stripped_path(source)
        };

        let episode_matcher = match params.matcher {
            Some(pattern) => super::episode_matcher_with_pattern(pattern)?,
            None => EpisodeMatcher::default(),
        };

        Ok(Self {
            id,
            nickname,
            path: path.into(),
            episode_matcher,
            player_args: database::PlayerArgs::new(),
        })
    }

    pub fn save(&self, db: &Database) -> diesel::QueryResult<usize> {
        use crate::database::schema::series_configs::dsl::*;

        diesel::replace_into(series_configs)
            .values(self)
            .execute(db.conn())
    }

    pub fn delete_by_name<S>(db: &Database, name: S) -> diesel::QueryResult<usize>
    where
        S: AsRef<str>,
    {
        use crate::database::schema::series_configs::dsl::*;

        let name = name.as_ref();

        diesel::delete(series_configs.filter(nickname.eq(name))).execute(db.conn())
    }

    pub fn load_all(db: &Database) -> diesel::QueryResult<Vec<Self>> {
        use crate::database::schema::series_configs::dsl::*;

        series_configs.load(db.conn())
    }

    pub fn load_by_name<S>(db: &Database, name: S) -> diesel::QueryResult<Self>
    where
        S: AsRef<str>,
    {
        use crate::database::schema::series_configs::dsl::*;

        series_configs
            .filter(nickname.eq(name.as_ref()))
            .get_result(db.conn())
    }

    pub fn full_path(&self, config: &Config) -> Cow<PathBuf> {
        if self.path.is_relative() {
            Cow::Owned(config.series_dir.join(self.path.as_ref()))
        } else {
            Cow::Borrowed(&self.path)
        }
    }

    pub fn set_path<'a, P>(&mut self, path: P, config: &Config)
    where
        P: Into<Cow<'a, Path>>,
    {
        self.path = config.stripped_path(path).into();
    }

    /// Applies the supplied `SeriesParams` to the `SeriesConfig`.
    /// Returns a bool indicating whether or not anything was changed.
    pub fn apply_params(&mut self, params: &SeriesParams, config: &Config) -> Result<bool> {
        let mut any_changed = false;

        if let Some(id) = params.id {
            self.id = id;
            any_changed = true;
        }

        if let Some(path) = &params.path {
            self.set_path(path, config);
            any_changed = true;
        }

        if let Some(pattern) = &params.matcher {
            self.episode_matcher = if !pattern.is_empty() {
                super::episode_matcher_with_pattern(pattern)?
            } else {
                EpisodeMatcher::default()
            };

            any_changed = true;
        }

        Ok(any_changed)
    }
}

impl PartialEq<String> for SeriesConfig {
    fn eq(&self, nickname: &String) -> bool {
        self.nickname == *nickname
    }
}

impl<'a> Into<Cow<'a, Self>> for SeriesConfig {
    fn into(self) -> Cow<'a, Self> {
        Cow::Owned(self)
    }
}

impl<'a> Into<Cow<'a, SeriesConfig>> for &'a SeriesConfig {
    fn into(self) -> Cow<'a, SeriesConfig> {
        Cow::Borrowed(self)
    }
}
