pub mod config;
pub mod entry;
pub mod info;

use crate::config::Config;
use crate::database::Database;
use crate::file;
use crate::file::SaveDir;
use crate::try_opt_r;
use anime::local::{CategorizedEpisodes, EpisodeParser, SortedEpisodes};
use anime::remote::{Remote, RemoteService, Status};
use anyhow::{anyhow, Context, Error, Result};
use chrono::{DateTime, Duration, Utc};
use config::SeriesConfig;
use diesel::deserialize::{self, FromSql};
use diesel::prelude::*;
use diesel::serialize::{self, Output, ToSql};
use diesel::sql_types::Text;
use entry::SeriesEntry;
use info::SeriesInfo;
use std::borrow::Cow;
use std::fs;
use std::io::Write;
use std::path::{self, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::result;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EpisodeScanError {
    #[error("failed to parse episodes at {path}: {source}")]
    EpisodeParseFailed { source: anime::Error, path: PathBuf },

    #[error("no episodes found")]
    NoEpisodes,

    #[error("multiple OVA / ONA / special / movie episode categories found without season episodes\nplease isolate each episode set into its own folder")]
    SeriesNeedsSplitting,
}

pub struct SeriesData {
    pub config: SeriesConfig,
    pub info: SeriesInfo,
    pub entry: SeriesEntry,
}

impl SeriesData {
    pub fn from_remote(config: SeriesConfig, info: SeriesInfo, remote: &Remote) -> Result<Self> {
        let entry = SeriesEntry::from_remote(remote, &info)?;

        Ok(Self {
            config,
            info,
            entry,
        })
    }

    pub fn load_from_config(db: &Database, config: Cow<SeriesConfig>) -> diesel::QueryResult<Self> {
        use diesel::result::Error as DieselError;

        db.conn().transaction::<_, DieselError, _>(|| {
            let info = SeriesInfo::load(db, config.id)?;
            let entry = SeriesEntry::load(db, config.id)?;

            Ok(Self {
                config: config.into_owned(),
                info,
                entry,
            })
        })
    }

    pub fn update(&mut self, params: UpdateParams, db: &Database, remote: &Remote) -> Result<()> {
        let id = params.id;

        if id.is_some() && remote.is_offline() {
            return Err(anyhow!("must be online to set a new series id"));
        }

        self.config.update(params, db)?;

        if let Some(id) = id {
            let info = SeriesInfo::from_remote_by_id(id, remote).context("getting series info")?;
            let entry = SeriesEntry::from_remote(remote, &info).context("getting series entry")?;

            self.info = info;
            self.entry = entry;
        }

        Ok(())
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

pub struct Series {
    pub data: SeriesData,
    pub episodes: SortedEpisodes,
}

impl Series {
    pub fn init(data: SeriesData, config: &Config) -> LoadedSeries {
        match Self::scan_episodes(&data, config) {
            Ok(eps) => LoadedSeries::Complete(Self::with_episodes(data, eps)),
            Err(err) => LoadedSeries::Partial(data, err),
        }
    }

    #[inline(always)]
    pub fn with_episodes(data: SeriesData, episodes: SortedEpisodes) -> Self {
        Self { data, episodes }
    }

    /// Sets the specified parameters on the series and reloads any neccessary state.
    pub fn update(
        &mut self,
        params: UpdateParams,
        config: &Config,
        db: &Database,
        remote: &Remote,
    ) -> Result<()> {
        self.data.update(params, db, remote)?;
        self.episodes = Self::scan_episodes(&self.data, config)?;
        Ok(())
    }

    fn scan_episodes(
        data: &SeriesData,
        config: &Config,
    ) -> result::Result<SortedEpisodes, EpisodeScanError> {
        let path = data.config.path.absolute(config);

        let episodes =
            CategorizedEpisodes::parse(&path, &data.config.episode_parser).map_err(|source| {
                EpisodeScanError::EpisodeParseFailed {
                    source,
                    path: path.into(),
                }
            })?;

        if episodes.is_empty() {
            return Err(EpisodeScanError::NoEpisodes);
        }

        episodes
            .take_season_episodes_or_present()
            .ok_or(EpisodeScanError::SeriesNeedsSplitting)
    }

    #[inline(always)]
    pub fn save(&self, db: &Database) -> diesel::QueryResult<()> {
        self.data.save(db)
    }

    pub fn load_from_config<'a, C>(sconfig: C, config: &Config, db: &Database) -> LoadedSeries
    where
        C: Into<Cow<'a, SeriesConfig>>,
    {
        let sconfig = sconfig.into();

        let data = match SeriesData::load_from_config(db, sconfig.clone()) {
            Ok(data) => data,
            Err(err) => return LoadedSeries::None(sconfig.into_owned(), err.into()),
        };

        Self::init(data, config)
    }

    pub fn episode_path(&self, ep_num: u32, config: &Config) -> Option<PathBuf> {
        let episode = self.episodes.find(ep_num)?;
        let mut path = self.data.config.path.absolute(config).into_owned();
        path.push(&episode.filename);
        path.canonicalize().ok()
    }

    pub fn play_episode(&self, episode: u32, config: &Config) -> Result<Child> {
        let episode_path = self
            .episode_path(episode, config)
            .with_context(|| anyhow!("episode {} not found", episode))?;

        let mut cmd = Command::new(&config.episode.player);
        cmd.arg(episode_path);
        cmd.args(&config.episode.player_args);
        cmd.args(self.data.config.player_args.as_ref());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        cmd.stdin(Stdio::null());

        cmd.spawn()
            .with_context(|| anyhow!("failed to play episode {}", episode))
    }

    pub fn begin_watching(
        &mut self,
        remote: &Remote,
        config: &Config,
        db: &Database,
    ) -> Result<()> {
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

    pub fn episode_completed(
        &mut self,
        remote: &Remote,
        config: &Config,
        db: &Database,
    ) -> Result<()> {
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

    pub fn episode_regressed(
        &mut self,
        remote: &Remote,
        config: &Config,
        db: &Database,
    ) -> Result<()> {
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

    pub fn series_complete(
        &mut self,
        remote: &Remote,
        config: &Config,
        db: &Database,
    ) -> Result<()> {
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

pub enum LoadedSeries {
    Complete(Series),
    Partial(SeriesData, EpisodeScanError),
    None(SeriesConfig, Error),
}

impl LoadedSeries {
    pub fn try_load(&mut self, config: &Config, db: &Database) {
        match self {
            Self::Complete(_) => (),
            Self::Partial(data, _) => *self = Series::load_from_config(&data.config, config, db),
            Self::None(cfg, _) => *self = Series::load_from_config(cfg.clone(), config, db),
        }
    }

    pub fn save(&self, db: &Database) -> diesel::QueryResult<()> {
        match self {
            Self::Complete(series) => series.save(db),
            Self::Partial(data, _) => data.save(db),
            Self::None(_, _) => Ok(()),
        }
    }

    pub fn config(&self) -> &SeriesConfig {
        match self {
            Self::Complete(series) => &series.data.config,
            Self::Partial(data, _) => &data.config,
            Self::None(cfg, _) => cfg,
        }
    }

    pub fn complete_mut(&mut self) -> Option<&mut Series> {
        match self {
            Self::Complete(series) => Some(series),
            Self::Partial(_, _) | Self::None(_, _) => None,
        }
    }

    pub fn nickname(&self) -> &str {
        match self {
            Self::Complete(series) => series.data.config.nickname.as_ref(),
            Self::Partial(data, _) => data.config.nickname.as_ref(),
            Self::None(cfg, _) => cfg.nickname.as_ref(),
        }
    }
}

#[derive(Clone)]
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

#[derive(Clone)]
#[cfg_attr(test, derive(Debug))]
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
                    EpisodeParser::custom(pattern)
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

        let last_watched = fs::read_to_string(&path).context("reading file")?;
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
        let path = Self::validated_path().context("getting path")?;
        fs::write(&path, contents).context("writing file")
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
    #[inline(always)]
    pub fn new<'a, P>(path: P, config: &Config) -> Self
    where
        P: Into<Cow<'a, Path>>,
    {
        Self::with_base(&config.series_dir, path)
    }

    pub fn with_base<'a, B, P>(base: B, path: P) -> Self
    where
        B: AsRef<Path>,
        P: Into<Cow<'a, Path>>,
    {
        let path = Self::stripped_path(base, path);
        Self(path)
    }

    #[inline(always)]
    pub fn absolute(&self, config: &Config) -> Cow<Path> {
        self.absolute_base(&config.series_dir)
    }

    #[inline(always)]
    pub fn absolute_base<B>(&self, base: B) -> Cow<Path>
    where
        B: AsRef<Path>,
    {
        Self::absolute_from_path_base(&self.0, base)
    }

    #[inline(always)]
    pub fn absolute_from_path<'a>(path: &'a Path, config: &Config) -> Cow<'a, Path> {
        Self::absolute_from_path_base(path, &config.series_dir)
    }

    pub fn absolute_from_path_base<B>(path: &Path, base: B) -> Cow<Path>
    where
        B: AsRef<Path>,
    {
        if path.is_relative() {
            Cow::Owned(base.as_ref().join(path))
        } else {
            Cow::Borrowed(path)
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
            || Err(anyhow!("no series found on disk matching {}", name)),
            |dir| Ok(Self::new(dir.path(), config)),
        )
    }

    #[inline(always)]
    pub fn inner(&self) -> &PathBuf {
        &self.0
    }

    #[inline(always)]
    pub fn exists_base<B>(&self, base: B) -> bool
    where
        B: AsRef<Path>,
    {
        self.absolute_base(base).as_ref().exists()
    }

    #[inline(always)]
    pub fn set<'a, P>(&mut self, path: P, config: &Config)
    where
        P: Into<Cow<'a, Path>>,
    {
        self.set_with_base(&config.series_dir, path);
    }

    #[inline(always)]
    pub fn set_with_base<'a, B, P>(&mut self, base: B, path: P)
    where
        B: AsRef<Path>,
        P: Into<Cow<'a, Path>>,
    {
        self.0 = Self::stripped_path(base, path);
    }

    fn stripped_path<'a, B, P>(base: B, path: P) -> PathBuf
    where
        B: AsRef<Path>,
        P: Into<Cow<'a, Path>>,
    {
        let path = path.into();

        match path.strip_prefix(base) {
            Ok(stripped) => stripped.into(),
            Err(_) => path.into(),
        }
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
