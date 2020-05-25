use super::SeriesParams;
use crate::config::Config;
use crate::database::schema::series_configs;
use crate::database::{self, Database};
use crate::err::{Error, Result};
use crate::{SERIES_EPISODE_REP, SERIES_TITLE_REP};
use anime::local::EpisodeParser;
use diesel::prelude::*;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Queryable, Insertable)]
pub struct SeriesConfig {
    pub id: i32,
    pub nickname: String,
    path: database::Path,
    pub episode_parser: EpisodeParser,
    pub player_args: database::PlayerArgs,
}

impl SeriesConfig {
    pub fn from_params<S, C>(
        nickname: S,
        id: i32,
        path: C,
        params: SeriesParams,
        config: &Config,
        db: &Database,
    ) -> Result<Self>
    where
        S: Into<String>,
        C: Into<PathBuf>,
    {
        if let Some(existing) = Self::exists(db, id) {
            return Err(Error::SeriesAlreadyExists { name: existing });
        }

        let nickname = nickname.into();

        let path = {
            let source = params.path.unwrap_or_else(|| path.into());
            config.stripped_path(source)
        };

        let episode_parser = match params.episode_parser {
            Some(pattern) => EpisodeParser::custom_with_replacements(
                pattern,
                SERIES_TITLE_REP,
                SERIES_EPISODE_REP,
            )?,
            None => EpisodeParser::default(),
        };

        Ok(Self {
            id,
            nickname,
            path: path.into(),
            episode_parser,
            player_args: database::PlayerArgs::new(),
        })
    }

    pub fn save(&self, db: &Database) -> diesel::QueryResult<usize> {
        use crate::database::schema::series_configs::dsl::*;

        diesel::replace_into(series_configs)
            .values(self)
            .execute(db.conn())
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

    /// Delete the series configuration from the database.
    ///
    /// This will also remove the series info and entry, if it exists.
    pub fn delete(&self, db: &Database) -> diesel::QueryResult<usize> {
        use crate::database::schema::series_configs::dsl::*;

        diesel::delete(series_configs.filter(id.eq(self.id))).execute(db.conn())
    }

    pub fn exists(db: &Database, config_id: i32) -> Option<String> {
        use crate::database::schema::series_configs::dsl::*;

        series_configs
            .filter(id.eq(config_id))
            .select(nickname)
            .get_result(db.conn())
            .ok()
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
    pub fn apply_params(
        &mut self,
        params: &SeriesParams,
        config: &Config,
        db: &Database,
    ) -> Result<bool> {
        let mut any_changed = false;

        if let Some(id) = params.id {
            if let Some(existing) = Self::exists(db, id) {
                return Err(Error::SeriesAlreadyExists { name: existing });
            }

            self.id = id;
            any_changed = true;
        }

        if let Some(path) = &params.path {
            self.set_path(path, config);
            any_changed = true;
        }

        if let Some(pattern) = &params.episode_parser {
            self.episode_parser = if !pattern.is_empty() {
                EpisodeParser::custom_with_replacements(
                    pattern,
                    SERIES_TITLE_REP,
                    SERIES_EPISODE_REP,
                )?
            } else {
                EpisodeParser::default()
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
