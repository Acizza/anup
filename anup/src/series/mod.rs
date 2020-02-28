pub mod config;
pub mod entry;
pub mod info;

use crate::config::Config;
use crate::database::Database;
use crate::err::{self, Result};
use crate::file::SaveDir;
use anime::local::{EpisodeMap, EpisodeMatcher};
use anime::remote::{RemoteService, Status};
use config::SeriesConfig;
use diesel::prelude::*;
use entry::SeriesEntry;
use info::SeriesInfo;
use smallvec::SmallVec;
use snafu::{ensure, OptionExt, ResultExt};
use std::borrow::Cow;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Debug)]
pub struct Series {
    pub config: SeriesConfig,
    pub info: SeriesInfo,
    pub entry: SeriesEntry,
    pub episodes: EpisodeMap,
}

impl Series {
    pub fn from_remote<'a, C, R>(
        sconfig: C,
        info: SeriesInfo,
        config: &Config,
        remote: &R,
    ) -> Result<Self>
    where
        C: Into<Cow<'a, SeriesConfig>>,
        R: RemoteService + ?Sized,
    {
        let sconfig = sconfig.into();

        let ep_path = sconfig.full_path(config);
        let episodes = EpisodeMap::parse(ep_path.as_ref(), &sconfig.episode_matcher)?;
        let entry = SeriesEntry::from_remote(remote, &info)?;

        let series = Self {
            config: sconfig.into_owned(),
            info,
            entry,
            episodes,
        };

        Ok(series)
    }

    /// Sets the specified parameters on the series and reloads any neccessary state.
    pub fn apply_params<R>(
        &mut self,
        params: SeriesParams,
        config: &Config,
        db: &Database,
        remote: &R,
    ) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        let any_changed = self.config.apply_params(&params, config, db)?;

        if !any_changed {
            return Ok(());
        }

        if let Some(id) = params.id {
            self.info = SeriesInfo::from_remote_by_id(id, remote)?;
            self.entry = SeriesEntry::from_remote(remote, &self.info)?;
        }

        let path = self.config.full_path(config);
        self.episodes = EpisodeMap::parse(path.as_ref(), &self.config.episode_matcher)?;

        Ok(())
    }

    pub fn save(&self, db: &Database) -> Result<()> {
        db.conn().transaction(|| {
            self.config.save(db)?;
            self.info.save(db)?;
            self.entry.save(db)
        })?;

        Ok(())
    }

    pub fn load(sconfig: SeriesConfig, config: &Config, db: &Database) -> Result<Self> {
        use diesel::result::Error as DieselError;

        let (info, entry) = db.conn().transaction::<_, DieselError, _>(|| {
            let info = SeriesInfo::load(db, sconfig.id)?;
            let entry = SeriesEntry::load(db, sconfig.id)?;
            Ok((info, entry))
        })?;

        let path = sconfig.full_path(config);
        let episodes = EpisodeMap::parse(path.as_ref(), &sconfig.episode_matcher)?;

        Ok(Self {
            config: sconfig,
            info,
            entry,
            episodes,
        })
    }

    pub fn delete_by_name<S>(db: &Database, nickname: S) -> diesel::QueryResult<usize>
    where
        S: AsRef<str>,
    {
        // The database is set up to remove all associated series data when we remove its configuration
        SeriesConfig::delete_by_name(db, nickname)
    }

    pub fn episode_path(&self, episode: u32, config: &Config) -> Option<PathBuf> {
        let episode_filename = self.episodes.get(&episode)?;
        let mut path = self.config.full_path(config).into_owned();
        path.push(episode_filename);
        path.canonicalize().ok()
    }

    pub fn play_episode_cmd(&self, episode: u32, config: &Config) -> Result<Command> {
        let episode_path = self
            .episode_path(episode, config)
            .context(err::EpisodeNotFound { episode })?;

        let mut cmd = Command::new(&config.episode.player);
        cmd.arg(episode_path);
        cmd.args(&config.episode.player_args);
        cmd.args(self.config.player_args.as_ref());
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
                if entry.watched_episodes() >= self.info.episodes {
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

        self.entry.sync_to_remote(remote)?;
        self.save(db)
    }

    pub fn episode_completed<R>(&mut self, remote: &R, config: &Config, db: &Database) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        let new_progress = self.entry.watched_episodes() + 1;

        if new_progress >= self.info.episodes {
            // The watched episode range is inclusive, so it's fine to bump the watched count
            // if we're at exactly at the last episode
            if new_progress == self.info.episodes {
                self.entry.set_watched_episodes(new_progress);
            }

            return self.series_complete(remote, config, db);
        }

        self.entry.set_watched_episodes(new_progress);
        self.entry.sync_to_remote(remote)?;
        self.save(db)
    }

    pub fn episode_regressed<R>(&mut self, remote: &R, config: &Config, db: &Database) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        let entry = &mut self.entry;
        entry.set_watched_episodes(entry.watched_episodes().saturating_sub(1));

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

#[derive(Clone, Debug)]
pub struct SeriesParams {
    pub id: Option<i32>,
    pub path: Option<PathBuf>,
    pub matcher: Option<String>,
}

impl SeriesParams {
    pub fn from_name_value_pairs(pairs: &[(&str, &str)]) -> Result<Self> {
        let mut params = Self::default();

        for &(name, value) in pairs {
            match name.to_ascii_lowercase().as_ref() {
                "id" => params.id = Some(value.parse()?),
                "path" => {
                    let path = PathBuf::from(value).canonicalize().context(err::IO)?;
                    ensure!(path.is_dir(), err::NotADirectory);
                    params.path = Some(path);
                }
                "matcher" => params.matcher = Some(value.to_string()),
                _ => (),
            }
        }

        Ok(params)
    }

    pub fn from_name_value_list<'a, I>(pairs: I) -> Result<Self>
    where
        I: IntoIterator<Item = &'a &'a str>,
    {
        let char_is_quote = |c| c == '\"' || c == '\'';

        let pairs = pairs
            .into_iter()
            .filter_map(|pair| {
                let idx = pair.find('=')?;
                let (name, value) = pair.split_at(idx);
                let value = value[1..].trim_matches(char_is_quote);
                Some((name, value))
            })
            .collect::<SmallVec<[_; 1]>>();

        Self::from_name_value_pairs(&pairs)
    }
}

impl Default for SeriesParams {
    fn default() -> Self {
        Self {
            id: None,
            path: None,
            matcher: None,
        }
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
        let contents = match &self.0 {
            Some(contents) => contents,
            None => return Ok(()),
        };

        let path = Self::validated_path()?;
        fs::write(&path, contents).context(err::FileIO { path })
    }

    pub fn validated_path() -> Result<PathBuf> {
        let mut path = SaveDir::LocalData.validated_dir_path()?.to_path_buf();
        path.push("last_watched");
        Ok(path)
    }
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
