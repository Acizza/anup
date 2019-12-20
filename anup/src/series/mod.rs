pub mod database;

use crate::config::Config;
use crate::err::{self, Result};
use crate::file::SaveDir;
use anime::local::{EpisodeMap, EpisodeMatcher};
use anime::remote::{RemoteService, SeriesInfo, Status};
use chrono::{Local, NaiveDate};
use database::{Database, Insertable, Selectable};
use snafu::{ensure, OptionExt, ResultExt};
use std::borrow::Cow;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Debug)]
pub struct Series {
    pub info: SeriesInfo,
    pub entry: SeriesEntry,
    pub config: SeriesConfig,
    pub episodes: EpisodeMap,
}

impl Series {
    pub fn from_args_and_remote<S, R>(
        args: &clap::ArgMatches,
        nickname: S,
        config: &Config,
        remote: &R,
    ) -> Result<Series>
    where
        S: Into<String>,
        R: RemoteService + ?Sized,
    {
        let nickname = nickname.into();

        // We should process as much local information as possible before sending requests to
        // the remote service to avoid potentially putting unnecessary load on the service should
        // any errors crop up.
        let path = match args.value_of("path") {
            Some(path) => {
                let path = PathBuf::from(path);
                ensure!(path.is_dir(), err::NotADirectory);
                path
            }
            None => detect::best_matching_folder(&nickname, &config.series_dir)?,
        };

        let title = {
            let path_str = path.file_name().context(err::NoDirName)?.to_string_lossy();
            detect::parse_folder_title(path_str).ok_or(err::Error::FolderTitleParse)?
        };

        let matcher = match args.value_of("matcher") {
            Some(pattern) => episode_matcher_with_pattern(pattern)?,
            None => EpisodeMatcher::new(),
        };

        let episodes = EpisodeMap::parse(&path, &matcher)?;

        // Now we can request all of that juicy data from the remote service.
        let info = best_matching_series_info(remote, title)?;
        let entry = SeriesEntry::from_remote(remote, &info)?;

        let config = SeriesConfig {
            id: info.id,
            nickname,
            path,
            episode_matcher: matcher,
            player_args: Vec::new(),
        };

        let series = Self {
            info,
            entry,
            config,
            episodes,
        };

        Ok(series)
    }

    pub fn save(&self, db: &Database) -> Result<()> {
        db.conn()
            .prepare_cached("BEGIN")
            .and_then(|mut query| query.execute(rusqlite::NO_PARAMS))?;

        self.config.insert_into_db(db, self.info.id)?;
        self.info.insert_into_db(db, ())?;
        self.entry.insert_into_db(db, ())?;

        db.conn()
            .prepare_cached("END")
            .and_then(|mut query| query.execute(rusqlite::NO_PARAMS))?;

        Ok(())
    }

    pub fn load<S>(db: &Database, nickname: S) -> Result<Series>
    where
        S: AsRef<str>,
    {
        let config = SeriesConfig::select_from_db(db, nickname.as_ref())?;
        let info = SeriesInfo::select_from_db(db, config.id)?;
        let entry = SeriesEntry::select_from_db(db, config.id)?;

        let episodes = EpisodeMap::parse(&config.path, &config.episode_matcher)?;

        Ok(Self {
            info,
            entry,
            config,
            episodes,
        })
    }

    pub fn episode_path(&self, episode: u32) -> Option<PathBuf> {
        let episode_filename = self.episodes.get(&episode)?;
        let mut path = self.config.path.clone();
        path.push(episode_filename);
        path.canonicalize().ok()
    }

    pub fn play_episode_cmd(&self, episode: u32, config: &Config) -> Result<Command> {
        let episode_path = self
            .episode_path(episode)
            .context(err::EpisodeNotFound { episode })?;

        let mut cmd = Command::new(&config.episode.player);
        cmd.arg(episode_path);
        cmd.args(&config.episode.player_args);
        cmd.args(&self.config.player_args);
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        cmd.stdin(Stdio::null());

        Ok(cmd)
    }

    pub fn begin_watching<R>(&mut self, remote: &R, config: &Config, db: &Database) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        self.entry.sync_from_remote(remote)?;

        let entry = &mut self.entry;
        let last_status = entry.status();

        match last_status {
            Status::Watching | Status::Rewatching => {
                // There is an edge case where all episodes have been watched, but the status
                // is still set to watching / rewatching. Here we just start a rewatch
                if entry.watched_eps() >= self.info.episodes {
                    entry.set_status(Status::Rewatching, config);
                    entry.set_watched_eps(0);

                    if last_status == Status::Rewatching {
                        entry.set_times_rewatched(entry.times_rewatched() + 1);
                    }
                }
            }
            Status::Completed => {
                entry.set_status(Status::Rewatching, config);
                entry.set_watched_eps(0);
            }
            Status::PlanToWatch | Status::OnHold => entry.set_status(Status::Watching, config),
            Status::Dropped => {
                entry.set_status(Status::Watching, config);
                entry.set_watched_eps(0);
            }
        }

        self.entry.sync_to_remote(remote)?;
        self.save(db)
    }

    pub fn episode_completed<R>(&mut self, remote: &R, config: &Config, db: &Database) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        let new_progress = self.entry.watched_eps() + 1;

        if new_progress >= self.info.episodes {
            // The watched episode range is inclusive, so it's fine to bump the watched count
            // if we're at exactly at the last episode
            if new_progress == self.info.episodes {
                self.entry.set_watched_eps(new_progress);
            }

            return self.series_complete(remote, config, db);
        }

        self.entry.set_watched_eps(new_progress);
        self.entry.sync_to_remote(remote)?;
        self.save(db)
    }

    pub fn episode_regressed<R>(&mut self, remote: &R, config: &Config, db: &Database) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        let entry = &mut self.entry;
        entry.set_watched_eps(entry.watched_eps().saturating_sub(1));

        let new_status = match entry.status() {
            Status::Completed if entry.times_rewatched() > 0 => Status::Rewatching,
            Status::Rewatching => Status::Rewatching,
            _ => Status::Watching,
        };

        entry.set_status(new_status, config);
        entry.sync_to_remote(remote)?;
        self.save(db)
    }

    pub fn series_complete<R>(&mut self, remote: &R, config: &Config, db: &Database) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        let entry = &mut self.entry;

        // A rewatch is typically only counted once the series is completed again
        if entry.status() == Status::Rewatching {
            entry.set_times_rewatched(entry.times_rewatched() + 1);
        }

        entry.set_status(Status::Completed, config);
        entry.sync_to_remote(remote)?;
        self.save(db)
    }
}

#[derive(Debug)]
pub struct SeriesEntry {
    entry: anime::remote::SeriesEntry,
    needs_sync: bool,
}

impl SeriesEntry {
    pub fn from_remote<R>(remote: &R, info: &SeriesInfo) -> Result<SeriesEntry>
    where
        R: RemoteService + ?Sized,
    {
        match remote.get_list_entry(info.id)? {
            Some(entry) => Ok(SeriesEntry::from(entry)),
            None => Ok(SeriesEntry::from(info.id)),
        }
    }

    pub fn force_sync_to_remote<R>(&mut self, remote: &R) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        if remote.is_offline() {
            return Ok(());
        }

        remote.update_list_entry(self.inner())?;
        self.needs_sync = false;
        Ok(())
    }

    pub fn sync_to_remote<R>(&mut self, remote: &R) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        if !self.needs_sync {
            return Ok(());
        }

        self.force_sync_to_remote(remote)
    }

    pub fn force_sync_from_remote<R>(&mut self, remote: &R) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        if remote.is_offline() {
            return Ok(());
        }

        *self = match remote.get_list_entry(self.id())? {
            Some(entry) => SeriesEntry::from(entry),
            None => SeriesEntry::from(self.id()),
        };

        Ok(())
    }

    pub fn sync_from_remote<R>(&mut self, remote: &R) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        if self.needs_sync {
            return Ok(());
        }

        self.force_sync_from_remote(remote)
    }

    #[inline(always)]
    pub fn inner(&self) -> &anime::remote::SeriesEntry {
        &self.entry
    }

    #[inline(always)]
    pub fn needs_sync(&self) -> bool {
        self.needs_sync
    }

    #[inline(always)]
    pub fn id(&self) -> u32 {
        self.entry.id
    }

    #[inline(always)]
    pub fn watched_eps(&self) -> u32 {
        self.entry.watched_eps
    }

    #[inline(always)]
    pub fn set_watched_eps(&mut self, watched_eps: u32) {
        self.entry.watched_eps = watched_eps;
        self.needs_sync = true;
    }

    #[inline(always)]
    pub fn score(&self) -> Option<u8> {
        self.entry.score
    }

    #[inline(always)]
    pub fn set_score(&mut self, score: Option<u8>) {
        self.entry.score = score;
        self.needs_sync = true;
    }

    #[inline(always)]
    pub fn status(&self) -> Status {
        self.entry.status
    }

    #[inline(always)]
    pub fn set_status(&mut self, status: Status, config: &Config) {
        match status {
            Status::Watching if self.start_date().is_none() => {
                self.entry.start_date = Some(Local::today().naive_local());
            }
            Status::Rewatching
                if self.start_date().is_none()
                    || (self.status() == Status::Completed && config.reset_dates_on_rewatch) =>
            {
                self.entry.start_date = Some(Local::today().naive_local());
            }
            Status::Completed
                if self.end_date().is_none()
                    || (self.status() == Status::Rewatching && config.reset_dates_on_rewatch) =>
            {
                self.entry.end_date = Some(Local::today().naive_local());
            }
            Status::Dropped if self.end_date().is_none() => {
                self.entry.end_date = Some(Local::today().naive_local());
            }
            _ => (),
        }

        self.entry.status = status;
        self.needs_sync = true;
    }

    #[inline(always)]
    pub fn times_rewatched(&self) -> u32 {
        self.entry.times_rewatched
    }

    #[inline(always)]
    pub fn set_times_rewatched(&mut self, times_rewatched: u32) {
        self.entry.times_rewatched = times_rewatched;
        self.needs_sync = true;
    }

    #[inline(always)]
    pub fn start_date(&self) -> Option<NaiveDate> {
        self.entry.start_date
    }

    #[inline(always)]
    pub fn end_date(&self) -> Option<NaiveDate> {
        self.entry.end_date
    }
}

impl From<anime::remote::SeriesEntry> for SeriesEntry {
    fn from(entry: anime::remote::SeriesEntry) -> SeriesEntry {
        SeriesEntry {
            entry,
            needs_sync: false,
        }
    }
}

impl From<u32> for SeriesEntry {
    fn from(id: u32) -> SeriesEntry {
        let remote_entry = anime::remote::SeriesEntry::new(id);
        SeriesEntry::from(remote_entry)
    }
}

#[derive(Debug)]
pub struct SeriesConfig {
    pub id: u32,
    pub nickname: String,
    pub path: PathBuf,
    pub episode_matcher: EpisodeMatcher,
    pub player_args: Vec<String>,
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
    pub fn get(&self) -> &Option<String> {
        &self.0
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
        let contents = match &self.0 {
            Some(contents) => contents,
            None => return Ok(()),
        };

        let path = Self::validated_path()?;
        fs::write(&path, contents).context(err::FileIO { path })
    }

    pub fn validated_path() -> Result<PathBuf> {
        let mut path = SaveDir::LocalData.validated_path()?.to_path_buf();
        path.push("last_watched");
        Ok(path)
    }
}

pub fn best_matching_series_info<R, S>(remote: &R, name: S) -> Result<SeriesInfo>
where
    R: RemoteService + ?Sized,
    S: AsRef<str>,
{
    let name = name.as_ref();

    let mut results = remote.search_info_by_name(name)?;
    let index = detect::best_matching_info(name, results.as_slice())
        .context(err::NoMatchingSeries { name })?;

    let info = results.swap_remove(index);
    Ok(info)
}

pub fn episode_matcher_with_pattern<S>(pattern: S) -> Result<EpisodeMatcher>
where
    S: AsRef<str>,
{
    let pattern = pattern
        .as_ref()
        .replace("{title}", "(?P<title>.+)")
        .replace("{episode}", r"(?P<episode>\d+)");

    match EpisodeMatcher::from_pattern(pattern) {
        Ok(matcher) => Ok(matcher),
        // We want to use a more specific error message than the one the anime library
        // provides
        Err(anime::Error::MissingCustomMatcherGroup { group }) => {
            Err(err::Error::MissingEpisodeMatcherGroup { group })
        }
        Err(err) => Err(err.into()),
    }
}