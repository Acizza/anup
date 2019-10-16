use crate::config::Config;
use crate::detect;
use crate::err::{self, Result};
use crate::file::{FileType, SaveDir, SaveFile};
use anime::local::{EpisodeList, EpisodeMatcher};
use anime::remote::{RemoteService, SeriesInfo};
use anime::SeasonInfoList;
use clap::ArgMatches;
use serde_derive::{Deserialize, Serialize};
use snafu::{ensure, OptionExt, ResultExt};
use std::ffi::OsStr;
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub mod cli;
pub mod tui;

#[derive(Deserialize, Serialize)]
struct CurrentWatchInfo {
    name: String,
    season: usize,
}

impl CurrentWatchInfo {
    fn new<S>(name: S, season: usize) -> CurrentWatchInfo
    where
        S: Into<String>,
    {
        CurrentWatchInfo {
            name: name.into(),
            season,
        }
    }
}

impl SaveFile for CurrentWatchInfo {
    fn filename() -> &'static str {
        ".currently_watching"
    }

    fn save_dir() -> SaveDir {
        SaveDir::LocalData
    }

    fn file_type() -> FileType {
        FileType::MessagePack
    }
}

#[derive(Deserialize, Serialize)]
struct SeriesPath(PathBuf);

impl SeriesPath {
    fn new<P>(path: P) -> Result<SeriesPath>
    where
        P: Into<PathBuf>,
    {
        use std::io::{Error, ErrorKind};

        let path = path.into();

        if !path.exists() {
            Err(Error::from(ErrorKind::NotFound)).context(err::FileIO { path: &path })?;
        }

        Ok(SeriesPath(path))
    }

    fn take(self) -> PathBuf {
        self.0
    }
}

impl SaveFile for SeriesPath {
    fn filename() -> &'static str {
        "path.mpack"
    }

    fn save_dir() -> SaveDir {
        SaveDir::LocalData
    }

    fn file_type() -> FileType {
        FileType::MessagePack
    }
}

#[derive(Deserialize, Serialize)]
struct SeriesPlayerArgs(Vec<String>);

impl SeriesPlayerArgs {
    fn new(args: Vec<String>) -> SeriesPlayerArgs {
        SeriesPlayerArgs(args)
    }

    fn take(self) -> Vec<String> {
        self.0
    }
}

impl SaveFile for SeriesPlayerArgs {
    fn filename() -> &'static str {
        "player_args.mpack"
    }

    fn save_dir() -> SaveDir {
        SaveDir::LocalData
    }

    fn file_type() -> FileType {
        FileType::MessagePack
    }
}

fn get_watch_info(args: &clap::ArgMatches) -> Result<CurrentWatchInfo> {
    match args.value_of("series") {
        Some(name) => {
            let season = args
                .value_of("season")
                .and_then(|num_str| num_str.parse().ok())
                .map(|num: usize| num.saturating_sub(1))
                .unwrap_or(0);

            let watch_info = CurrentWatchInfo::new(name, season);
            watch_info.save(None)?;

            Ok(watch_info)
        }
        None => match CurrentWatchInfo::load(None) {
            Ok(watch_info) => Ok(watch_info),
            Err(ref err) if err.is_file_nonexistant() => Err(err::Error::NoSavedSeriesName),
            Err(err) => Err(err),
        },
    }
}

fn get_config() -> Result<Config> {
    match Config::load(None) {
        Ok(config) => Ok(config),
        Err(ref err) if err.is_file_nonexistant() => {
            // Default base directory: ~/anime/
            let mut dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~/"));
            dir.push("anime");

            let config = Config::new(dir);
            config.save(None)?;
            Ok(config)
        }
        Err(err) => Err(err),
    }
}

fn get_episode_matcher<S>(name: S, matcher: Option<&str>, save_new: bool) -> Result<EpisodeMatcher>
where
    S: AsRef<str>,
{
    let name = name.as_ref();

    match EpisodeMatcher::load(name) {
        Ok(matcher) => Ok(matcher),
        Err(ref err) if err.is_file_nonexistant() => match matcher {
            Some(matcher) if save_new => {
                let matcher = EpisodeMatcher::with_matcher(matcher)?;
                matcher.save(name)?;
                Ok(matcher)
            }
            Some(_) => Ok(EpisodeMatcher::new()),
            None => Ok(EpisodeMatcher::new()),
        },
        Err(err) => Err(err),
    }
}

fn get_episodes<S>(
    args: &ArgMatches,
    name: S,
    config: &Config,
    save_new: bool,
) -> Result<EpisodeList>
where
    S: AsRef<str>,
{
    let name = name.as_ref();

    let dir = match args.value_of("path") {
        Some(path) if save_new => {
            let path = SeriesPath::new(path)?;
            path.save(name)?;
            path.take()
        }
        _ => get_series_path(name, config)?,
    };

    let matcher = get_episode_matcher(name, args.value_of("matcher"), save_new)?;
    let episodes = EpisodeList::parse(dir, &matcher)?;

    Ok(episodes)
}

fn get_series_path<S>(name: S, config: &Config) -> Result<PathBuf>
where
    S: AsRef<str>,
{
    match SeriesPath::load(name.as_ref()) {
        Ok(path) => Ok(path.take()),
        Err(ref err) if err.is_file_nonexistant() => {
            Ok(detect::best_matching_folder(&name, &config.series_dir)?)
        }
        Err(err) => Err(err),
    }
}

fn get_remote(args: &ArgMatches, can_use_offline: bool) -> Result<Box<dyn RemoteService>> {
    use anime::remote::anilist::{self, AccessToken, AniList};
    use anime::remote::offline::Offline;

    if args.is_present("offline") {
        ensure!(can_use_offline, err::MustRunOnline);
        Ok(Box::new(Offline::new()))
    } else {
        let token = match AccessToken::load(None) {
            Ok(config) => config,
            Err(ref err) if err.is_file_nonexistant() => {
                ensure!(!args.is_present("interactive"), err::GetAniListTokenFromCLI);

                println!(
                    "need AniList login token\ngo to {}\n\npaste your token:",
                    anilist::auth_url(super::ANILIST_CLIENT_ID)
                );

                let token = {
                    let mut buffer = String::new();
                    io::stdin().read_line(&mut buffer).context(err::IO)?;
                    let buffer = buffer.trim_end();

                    AccessToken::new(buffer)
                };

                token.save(None)?;
                token
            }
            Err(err) => return Err(err),
        };

        let anilist = AniList::login(token)?;
        Ok(Box::new(anilist))
    }
}

fn get_best_info_from_remote<R, S>(remote: &R, name: S) -> Result<SeriesInfo>
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

fn get_season_list<R, S>(name: S, remote: &R, episodes: &EpisodeList) -> Result<SeasonInfoList>
where
    R: RemoteService + ?Sized,
    S: AsRef<str>,
{
    let name = name.as_ref();

    match SeasonInfoList::load(name) {
        Ok(mut seasons) => {
            if seasons.add_from_remote(remote)? {
                seasons.save(name)?;
            }

            Ok(seasons)
        }
        Err(ref err) if err.is_file_nonexistant() => {
            let info = get_best_info_from_remote(remote, &episodes.title)?;
            let seasons = SeasonInfoList::from_info_and_remote(info, remote)?;
            seasons.save(name)?;
            Ok(seasons)
        }
        Err(err) => Err(err),
    }
}

fn prepare_episode_cmd<S, P>(name: S, config: &Config, ep_path: P) -> Result<Command>
where
    S: AsRef<str>,
    P: AsRef<OsStr>,
{
    let name = name.as_ref();

    let extra_args = match SeriesPlayerArgs::load(name) {
        Ok(args) => args.take(),
        Err(ref err) if err.is_file_nonexistant() => Vec::new(),
        Err(err) => return Err(err),
    };

    let mut cmd = Command::new(&config.episode.player);
    cmd.arg(ep_path);
    cmd.args(&config.episode.player_args);
    cmd.args(extra_args);
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    cmd.stdin(Stdio::null());

    Ok(cmd)
}

fn remove_orphaned_data<F>(config: &Config, mut on_removed: F) -> Result<()>
where
    F: FnMut(&str),
{
    let series_data = SaveDir::LocalData.get_subdirs()?;

    for series in series_data {
        let exists = match get_series_path(&series, &config) {
            Ok(dir) => dir.exists(),
            Err(err::Error::NoMatchingSeries { .. }) => false,
            Err(err) => return Err(err),
        };

        if exists {
            continue;
        }

        on_removed(&series);
        SaveDir::LocalData.remove_subdir(&series)?;
    }

    Ok(())
}
