pub mod config;
pub mod entry;
pub mod info;

use crate::config::Config;
use crate::database::Database;
use crate::err::{self, Error, Result};
use crate::file;
use crate::file::SaveDir;
use crate::{try_opt_r, SERIES_EPISODE_REP, SERIES_TITLE_REP};
use anime::local::{EpisodeParser, Episodes};
use anime::remote::{RemoteService, Status};
use chrono::{DateTime, Duration, Utc};
use config::SeriesConfig;
use diesel::deserialize::{self, FromSql};
use diesel::prelude::*;
use diesel::serialize::{self, Output, ToSql};
use diesel::sql_types::Text;
use entry::SeriesEntry;
use info::SeriesInfo;
use snafu::{OptionExt, ResultExt};
use std::borrow::Cow;
use std::fs;
use std::io::Write;
use std::path::{self, Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug)]
pub struct SeriesData {
    pub config: SeriesConfig,
    pub info: SeriesInfo,
    pub entry: SeriesEntry,
}

impl SeriesData {
    pub fn from_remote<R>(config: SeriesConfig, info: SeriesInfo, remote: &R) -> Result<Self>
    where
        R: RemoteService + ?Sized,
    {
        let entry = SeriesEntry::from_remote(remote, &info)?;

        Ok(Self {
            config,
            info,
            entry,
        })
    }

    pub fn load_from_config(db: &Database, config: SeriesConfig) -> diesel::QueryResult<Self> {
        use diesel::result::Error as DieselError;

        db.conn().transaction::<_, DieselError, _>(|| {
            let info = SeriesInfo::load(db, config.id)?;
            let entry = SeriesEntry::load(db, config.id)?;

            Ok(Self {
                config,
                info,
                entry,
            })
        })
    }

    pub fn save(&self, db: &Database) -> diesel::QueryResult<()> {
        db.conn()
            .transaction(|| {
                self.config.save(db)?;
                self.info.save(db)?;
                self.entry.save(db)
            })
            .map(|_| ())
    }

    /// Returns the UTC time threshold for an episode should be counted as watched, assuming that the episode was starting to be watched now.
    pub fn next_watch_progress_time(&self, config: &Config) -> DateTime<Utc> {
        let secs_must_watch =
            (self.info.episode_length_mins as f32 * config.episode.pcnt_must_watch) * 60.0;

        Utc::now() + Duration::seconds(secs_must_watch as i64)
    }
}

#[derive(Debug)]
pub struct Series {
    pub data: SeriesData,
    pub episodes: Episodes,
}

impl Series {
    pub fn new(data: SeriesData, config: &Config) -> Result<Self> {
        let episodes = Self::scan_episodes(&data, config)?;
        Ok(Self::with_episodes(data, episodes))
    }

    #[inline(always)]
    pub fn with_episodes(data: SeriesData, episodes: Episodes) -> Self {
        Self { data, episodes }
    }

    /// Sets the specified parameters on the series and reloads any neccessary state.
    pub fn update<R>(
        &mut self,
        params: UpdateParams,
        config: &Config,
        db: &Database,
        remote: &R,
    ) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        let id = params.id;

        if id.is_some() && remote.is_offline() {
            return Err(Error::MustBeOnlineTo {
                reason: "set a new series id",
            });
        }

        self.data.config.update(params, db)?;

        if let Some(id) = id {
            let info = SeriesInfo::from_remote_by_id(id, remote)?;
            let entry = SeriesEntry::from_remote(remote, &info)?;

            self.data.info = info;
            self.data.entry = entry;
        }

        self.episodes = Self::scan_episodes(&self.data, config)?;
        Ok(())
    }

    fn scan_episodes(data: &SeriesData, config: &Config) -> Result<Episodes> {
        let path = data.config.path.absolute(config);
        Episodes::parse(path, &data.config.episode_parser).map_err(Into::into)
    }

    #[inline(always)]
    pub fn save(&self, db: &Database) -> diesel::QueryResult<()> {
        self.data.save(db)
    }

    pub fn load_from_config(sconfig: SeriesConfig, config: &Config, db: &Database) -> Result<Self> {
        let data = SeriesData::load_from_config(db, sconfig)?;
        Self::new(data, config)
    }

    pub fn episode_path(&self, ep_num: u32, config: &Config) -> Option<PathBuf> {
        let episode = self.episodes.get(ep_num)?;
        let mut path = self.data.config.path.absolute(config).into_owned();
        path.push(&episode.filename);
        path.canonicalize().ok()
    }

    pub fn play_episode_cmd(&self, episode: u32, config: &Config) -> Result<Command> {
        let episode_path = self
            .episode_path(episode, config)
            .context(err::EpisodeNotFound { episode })?;

        let mut cmd = Command::new(&config.episode.player);
        cmd.arg(episode_path);
        cmd.args(&config.episode.player_args);
        cmd.args(self.data.config.player_args.as_ref());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        cmd.stdin(Stdio::null());

        Ok(cmd)
    }

    pub fn begin_watching<R>(&mut self, remote: &R, config: &Config, db: &Database) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        self.data.entry.sync_from_remote(remote)?;

        let entry = &mut self.data.entry;
        let last_status = entry.status();

        match last_status {
            Status::Watching | Status::Rewatching => {
                // There is an edge case where all episodes have been watched, but the status
                // is still set to watching / rewatching. Here we just start a rewatch
                if entry.watched_episodes() >= self.data.info.episodes {
                    entry.set_status(Status::Rewatching, config);
                    entry.set_watched_episodes(0);

                    if last_status == Status::Rewatching {
                        entry.set_times_rewatched(entry.times_rewatched() + 1);
                    }
                }
            }
            Status::Completed => {
                entry.set_status(Status::Rewatching, config);
                entry.set_watched_episodes(0);
            }
            Status::PlanToWatch | Status::OnHold => entry.set_status(Status::Watching, config),
            Status::Dropped => {
                entry.set_status(Status::Watching, config);
                entry.set_watched_episodes(0);
            }
        }

        self.data.entry.sync_to_remote(remote)?;
        self.save(db)?;

        Ok(())
    }

    pub fn episode_completed<R>(&mut self, remote: &R, config: &Config, db: &Database) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        let new_progress = self.data.entry.watched_episodes() + 1;

        if new_progress >= self.data.info.episodes {
            // The watched episode range is inclusive, so it's fine to bump the watched count
            // if we're at exactly at the last episode
            if new_progress == self.data.info.episodes {
                self.data.entry.set_watched_episodes(new_progress);
            }

            return self.series_complete(remote, config, db);
        }

        self.data.entry.set_watched_episodes(new_progress);
        self.data.entry.sync_to_remote(remote)?;
        self.save(db)?;

        Ok(())
    }

    pub fn episode_regressed<R>(&mut self, remote: &R, config: &Config, db: &Database) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        let entry = &mut self.data.entry;
        entry.set_watched_episodes(entry.watched_episodes().saturating_sub(1));

        let new_status = match entry.status() {
            Status::Completed if entry.times_rewatched() > 0 => Status::Rewatching,
            Status::Rewatching => Status::Rewatching,
            _ => Status::Watching,
        };

        entry.set_status(new_status, config);
        entry.sync_to_remote(remote)?;
        self.save(db)?;

        Ok(())
    }

    pub fn series_complete<R>(&mut self, remote: &R, config: &Config, db: &Database) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        let entry = &mut self.data.entry;

        // A rewatch is typically only counted once the series is completed again
        if entry.status() == Status::Rewatching {
            entry.set_times_rewatched(entry.times_rewatched() + 1);
        }

        entry.set_status(Status::Completed, config);
        entry.sync_to_remote(remote)?;
        self.save(db)?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct SeriesParams {
    pub name: String,
    pub path: SeriesPath,
    pub parser: EpisodeParser,
}

impl SeriesParams {
    pub fn new<S, P>(name: S, path: P, parser: EpisodeParser) -> Self
    where
        S: Into<String>,
        P: Into<SeriesPath>,
    {
        Self {
            name: name.into(),
            path: path.into(),
            parser,
        }
    }

    pub fn update<'a, S, P, E>(&mut self, name: S, path: P, parser: E)
    where
        S: Into<Cow<'a, str>>,
        P: Into<Cow<'a, SeriesPath>>,
        E: Into<Cow<'a, EpisodeParser>>,
    {
        macro_rules! update_fields {
            ($($field:ident)+) => {
                $(
                let $field = $field.into();

                if self.$field != *$field {
                    self.$field = $field.into_owned();
                }
                )+
            };
        }

        update_fields!(name path parser);
    }
}

#[derive(Clone, Debug)]
pub struct UpdateParams {
    pub id: Option<i32>,
    pub path: Option<SeriesPath>,
    pub parser: Option<EpisodeParser>,
}

impl UpdateParams {
    pub fn from_strings(
        id: Option<String>,
        path: Option<String>,
        parser: Option<String>,
        config: &Config,
    ) -> Result<Self> {
        let id = match id {
            Some(id) => Some(id.parse::<i32>()?),
            None => None,
        };

        let parser = match parser {
            Some(pattern) => {
                let parser = if pattern.is_empty() {
                    EpisodeParser::default()
                } else {
                    EpisodeParser::custom_with_replacements(
                        pattern,
                        SERIES_TITLE_REP,
                        SERIES_EPISODE_REP,
                    )?
                };

                Some(parser)
            }
            None => None,
        };

        let path = path.map(|path| SeriesPath::new(PathBuf::from(path), config));

        Ok(Self { id, path, parser })
    }
}

pub struct LastWatched(Option<String>);

impl LastWatched {
    pub fn load() -> Result<Self> {
        let path = Self::validated_path()?;

        if !path.exists() {
            return Ok(Self(None));
        }

        let last_watched = fs::read_to_string(&path).context(err::FileIO { path })?;
        Ok(Self(Some(last_watched)))
    }

    #[inline(always)]
    pub fn get(&self) -> Option<&String> {
        self.0.as_ref()
    }

    pub fn set<'a, S>(&mut self, nickname: S) -> bool
    where
        S: Into<Cow<'a, str>>,
    {
        let nickname = nickname.into();

        let is_different = self
            .0
            .as_ref()
            .map(|existing| existing != nickname.as_ref())
            .unwrap_or(true);

        if is_different {
            self.0 = Some(nickname.into_owned());
        }

        is_different
    }

    pub fn save(&self) -> Result<()> {
        let contents = try_opt_r!(&self.0);
        let path = Self::validated_path()?;
        fs::write(&path, contents).context(err::FileIO { path })
    }

    pub fn validated_path() -> Result<PathBuf> {
        let mut path = SaveDir::LocalData.validated_dir_path()?.to_path_buf();
        path.push("last_watched");
        Ok(path)
    }
}

#[derive(Clone, Debug, AsExpression, FromSqlRow)]
#[sql_type = "Text"]
pub struct SeriesPath(PathBuf);

impl SeriesPath {
    pub fn new<'a, P>(path: P, config: &Config) -> Self
    where
        P: Into<Cow<'a, Path>>,
    {
        let path = config.stripped_path(path);
        Self(path)
    }

    pub fn absolute(&self, config: &Config) -> Cow<Path> {
        if self.0.is_relative() {
            Cow::Owned(config.series_dir.join(&self.0))
        } else {
            Cow::Borrowed(&self.0)
        }
    }

    pub fn closest_matching<S>(name: S, config: &Config) -> Result<Self>
    where
        S: AsRef<str>,
    {
        use anime::local::detect::dir;

        const MIN_CONFIDENCE: f32 = 0.6;

        let name = name.as_ref();
        let files = file::read_dir(&config.series_dir)?;

        dir::closest_match(name, MIN_CONFIDENCE, files.into_iter()).map_or_else(
            || Err(Error::NoMatchingSeriesOnDisk { name: name.into() }),
            |dir| Ok(Self::new(dir.path(), config)),
        )
    }

    #[inline(always)]
    pub fn get(&self) -> &PathBuf {
        &self.0
    }

    #[inline(always)]
    pub fn set<'a, P>(&mut self, path: P, config: &Config)
    where
        P: Into<Cow<'a, Path>>,
    {
        self.0 = config.stripped_path(path);
    }

    #[inline(always)]
    pub fn display(&self) -> path::Display {
        self.0.display()
    }
}

impl<'a> Into<Cow<'a, Self>> for SeriesPath {
    fn into(self) -> Cow<'a, Self> {
        Cow::Owned(self)
    }
}

impl<'a> Into<Cow<'a, SeriesPath>> for &'a SeriesPath {
    fn into(self) -> Cow<'a, SeriesPath> {
        Cow::Borrowed(self)
    }
}

impl PartialEq for SeriesPath {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<DB> FromSql<Text, DB> for SeriesPath
where
    DB: diesel::backend::Backend,
    String: FromSql<Text, DB>,
{
    fn from_sql(bytes: Option<&DB::RawValue>) -> deserialize::Result<Self> {
        let path = String::from_sql(bytes)?.into();
        Ok(Self(path))
    }
}

impl<DB> ToSql<Text, DB> for SeriesPath
where
    DB: diesel::backend::Backend,
    str: ToSql<Text, DB>,
{
    fn to_sql<W: Write>(&self, out: &mut Output<W, DB>) -> serialize::Result {
        let value = self.0.to_string_lossy();
        value.to_sql(out)
    }
}
