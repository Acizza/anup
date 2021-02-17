pub mod config;
pub mod entry;
pub mod info;

use crate::config::Config;
use crate::database::Database;
use crate::file;
use crate::file::SaveDir;
use crate::try_opt_r;
use anime::local::{CategorizedEpisodes, EpisodeParser, SortedEpisodes};
use anime::remote::{Remote, SeriesID, Status};
use anyhow::{anyhow, Context, Error, Result};
use chrono::{DateTime, Duration, Utc};
use config::SeriesConfig;
use diesel::deserialize::{self, FromSql};
use diesel::prelude::*;
use diesel::serialize::{self, Output, ToSql};
use diesel::sql_types::Text;
use entry::SeriesEntry;
use info::SeriesInfo;
use smallvec::SmallVec;
use std::cmp::{Ordering, PartialOrd};
use std::fs;
use std::io::Write;
use std::mem;
use std::path::{self, Path, PathBuf};
use std::result;
use std::{borrow::Cow, process::Stdio};
use thiserror::Error;
use tokio::process::{Child, Command};

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
        let id_changed = self.config.update(params, db, remote)?;

        if id_changed {
            let info = SeriesInfo::from_remote_by_id(self.config.id as SeriesID, remote)
                .context("getting series info")?;

            let entry = SeriesEntry::from_remote(remote, &info).context("getting series entry")?;

            self.info = info;
            self.entry = entry;
        }

        Ok(())
    }

    pub fn force_sync_from_remote(&mut self, remote: &Remote) -> Result<()> {
        // We don't want to set the new info now in case the entry sync fails
        let info = SeriesInfo::from_remote_by_id(self.info.id as SeriesID, remote)?;

        self.entry.force_sync_from_remote(remote)?;
        self.info = info;

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
            (f32::from(self.info.episode_length_mins) * config.episode.pcnt_must_watch) * 60.0;

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
        mut params: UpdateParams,
        config: &Config,
        db: &Database,
        remote: &Remote,
    ) -> Result<()> {
        let episodes = mem::take(&mut params.episodes);

        self.data.update(params, db, remote)?;

        self.episodes = match episodes {
            Some(episodes) => episodes,
            None => Self::scan_episodes(&self.data, config)?,
        };

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

    pub fn load_from_config<'a, C>(series_config: C, config: &Config, db: &Database) -> LoadedSeries
    where
        C: Into<Cow<'a, SeriesConfig>>,
    {
        let series_config = series_config.into();

        let data = match SeriesData::load_from_config(db, series_config.clone()) {
            Ok(data) => data,
            Err(err) => return LoadedSeries::None(series_config.into_owned(), err.into()),
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

    pub fn info(&self) -> Option<&SeriesInfo> {
        match self {
            Self::Complete(series) => Some(&series.data.info),
            Self::Partial(data, _) => Some(&data.info),
            Self::None(_, _) => None,
        }
    }

    pub fn complete_mut(&mut self) -> Option<&mut Series> {
        match self {
            Self::Complete(series) => Some(series),
            Self::Partial(_, _) | Self::None(_, _) => None,
        }
    }

    #[inline(always)]
    pub fn id(&self) -> Option<i32> {
        self.info().map(|info| info.id)
    }

    #[inline(always)]
    pub fn nickname(&self) -> &str {
        self.config().nickname.as_ref()
    }

    #[inline(always)]
    pub fn path(&self) -> &SeriesPath {
        &self.config().path
    }

    #[inline(always)]
    pub fn parser(&self) -> &EpisodeParser {
        &self.config().episode_parser
    }

    pub fn update(
        &mut self,
        params: UpdateParams,
        config: &Config,
        db: &Database,
        remote: &Remote,
    ) -> Result<()> {
        match self {
            Self::Complete(series) => {
                series.update(params, config, db, remote)?;
                series.save(db)?;
            }
            Self::Partial(data, _) => {
                data.update(params, db, remote)?;
                data.save(db)?;
            }
            Self::None(cfg, _) => {
                cfg.update(params, db, remote)?;
                cfg.save(db)?;
            }
        }

        Ok(())
    }
}

impl PartialEq for LoadedSeries {
    fn eq(&self, other: &Self) -> bool {
        self.nickname() == other.nickname()
    }
}

impl Eq for LoadedSeries {}

impl PartialOrd for LoadedSeries {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LoadedSeries {
    fn cmp(&self, other: &Self) -> Ordering {
        self.nickname().cmp(other.nickname())
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

#[cfg_attr(test, derive(Debug))]
pub struct UpdateParams {
    pub id: Option<SeriesID>,
    pub path: Option<SeriesPath>,
    pub parser: Option<EpisodeParser>,
    pub episodes: Option<SortedEpisodes>,
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
            .map_or(true, |existing| existing != nickname.as_ref());

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

    pub fn closest_matching(name: &str, config: &Config) -> Result<Self> {
        use anime::local::detect::dir;

        const MIN_CONFIDENCE: f32 = 0.6;

        let dirs = file::subdirectories(&config.series_dir)?;

        dir::closest_match(name, MIN_CONFIDENCE, dirs.into_iter()).map_or_else(
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

/// Attempts to generate a short and readable nickname for the given `title`.
pub fn generate_nickname<S>(title: S) -> Option<String>
where
    S: Into<String>,
{
    const SPACER: &str = "_";
    const TITLE_WHITESPACE: [u8; 4] = [b' ', b'_', b'.', b'-'];
    const SKIP_WORDS: [&str; 1] = ["the"];
    const SPECIAL_WORDS: [&str; 4] = ["special", "ova", "ona", "movie"];

    let is_special_word = |word: &str| {
        SPECIAL_WORDS
            .iter()
            .any(|special| word.starts_with(special))
    };

    let title = {
        let mut title = title.into();
        title.make_ascii_lowercase();
        title
    };

    let fragments = title
        .split(|ch| TITLE_WHITESPACE.contains(&(ch as u8)))
        .collect::<SmallVec<[_; 8]>>();

    let mut nickname: SmallVec<[&str; 4]> = SmallVec::new();

    let (fragments, end_fragment) = match fragments.last() {
        Some(last) if is_special_word(last) => (&fragments[..fragments.len() - 1], Some(*last)),
        Some(last) => {
            let end_fragment = parse_season_number(last);

            let fragments = if end_fragment.is_some() {
                &fragments[..fragments.len() - 1]
            } else {
                &fragments
            };

            (fragments, end_fragment)
        }
        None => return None,
    };

    let mut used_frags = 0;

    for fragment in fragments {
        let len = fragment.len();

        if len <= 2 || SKIP_WORDS.contains(fragment) {
            continue;
        }

        nickname.push(fragment);

        if len > 8 {
            break;
        }

        used_frags += 1;

        if used_frags >= 2 {
            break;
        }
    }

    if nickname.is_empty() {
        return None;
    }

    if let Some(end) = end_fragment {
        nickname.push(end);
    }

    Some(nickname.join(SPACER))
}

fn parse_season_number(slice: &str) -> Option<&str> {
    let is_digits = |digits: &[u8]| digits.iter().all(u8::is_ascii_digit);

    let offset = match slice.as_bytes() {
        [b's', b'0', b'1', ..] | [b's', b'1', ..] | [] => None,
        [b's', b'0', ..] => Some(2),
        [b's', rest @ ..] if is_digits(rest) => Some(1),
        rest if is_digits(rest) => Some(0),
        _ => None,
    };

    offset.map(|offset| &slice[offset..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nickname_generation() {
        let titles = vec![
            ("series title", Some("series_title")),
            ("longer series title test", Some("longer_series")),
            ("the series title", Some("series_title")),
            ("title of series", Some("title_series")),
            ("longfirstword of series", Some("longfirstword")),
            ("longfirstword S02", Some("longfirstword_2")),
            ("title longsecondword test", Some("title_longsecondword")),
            ("title test longthirdword", Some("title_test")),
            ("series title 2", Some("series_title_2")),
            ("longer series title 2", Some("longer_series_2")),
            ("longer series title s02", Some("longer_series_2")),
            ("series title s01", Some("series_title")),
            ("Yuragi-sou no Yuuna-san OVA", Some("yuragi_sou_ova")),
            ("Kaguya-sama wa wa Kokurasetai S2", Some("kaguya_sama_2")),
            ("series title OVAs", Some("series_title_ovas")),
            ("test s02", Some("test_2")),
            ("test s2", Some("test_2")),
            ("s.m.o.l S02", None),
            ("s.m.o.l OVA", None),
            ("s2", None),
        ];

        for (title, expected) in titles {
            assert_eq!(
                generate_nickname(title).as_deref(),
                expected,
                "nickname mismatch for title: {}",
                title
            );
        }
    }
}
