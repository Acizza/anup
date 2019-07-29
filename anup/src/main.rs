mod config;
mod detect;
mod err;
mod file;
mod interface;
mod track;
mod util;

use crate::config::Config;
use crate::err::Result;
use crate::file::{FileType, SaveDir, SaveFile};
use crate::track::EntryState;
use anime::local::{EpisodeList, EpisodeMatcher};
use anime::remote::{RemoteService, SeriesInfo};
use anime::{SeasonInfoList, Series};
use clap::clap_app;
use clap::ArgMatches;
use interface::cli;
use serde_derive::{Deserialize, Serialize};
use snafu::{ensure, OptionExt, ResultExt};
use std::io;
use std::path::PathBuf;

fn main() {
    let args = clap_app!(anup =>
        (version: env!("CARGO_PKG_VERSION"))
        (author: env!("CARGO_PKG_AUTHORS"))
        (@arg series: +takes_value "The name of the series to watch")
        (@arg season: -s --season +takes_value "The season to watch. Meant to be used when playing from a folder that has multiple seasons merged together under one name")
        (@arg matcher: -m --matcher +takes_value "The custom pattern to match episode files with")
        (@arg offline: -o --offline "Run in offline mode")
        (@arg prefetch: --prefetch "Fetch series info from AniList (for use with offline mode)")
        (@arg sync: --sync "Syncronize changes made while offline to AniList")
        (@arg oneshot: --oneshot "Play the next episode and exit")
        (@arg quiet: -q --quiet "Don't print series information")
        (@arg rate: -r --rate +takes_value "Rate a series")
        (@arg drop: -d --drop "Drop a series")
        (@arg hold: -h --hold "Put a series on hold")
        (@arg path: -p --path +takes_value "Manually specify a path to a series")
        (@arg clean: -c --clean "Remove series data that is no longer needed")
    )
    .get_matches();

    if let Err(err) = cli::run(&args) {
        err::display_error(err);
        std::process::exit(1);
    }
}

#[derive(Deserialize, Serialize)]
struct LastWatched(String);

impl LastWatched {
    fn new<S>(name: S) -> LastWatched
    where
        S: Into<String>,
    {
        LastWatched(name.into())
    }

    #[inline(always)]
    fn take(self) -> String {
        self.0
    }
}

impl SaveFile for LastWatched {
    fn filename() -> &'static str {
        ".last_watched"
    }

    fn save_dir() -> SaveDir {
        SaveDir::LocalData
    }

    fn file_type() -> FileType {
        FileType::MessagePack
    }
}

fn get_series_name(args: &clap::ArgMatches) -> Result<String> {
    if let Some(name) = args.value_of("series") {
        let name = LastWatched::new(name);
        name.save(None)?;

        return Ok(name.take());
    }

    match LastWatched::load(None) {
        Ok(sname) => Ok(sname.take()),
        Err(ref err) if err.is_file_nonexistant() => Err(err::Error::NoSavedSeriesName),
        Err(err) => Err(err),
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

    #[inline(always)]
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

fn get_episode_matcher<S>(name: S, matcher: Option<&str>) -> Result<EpisodeMatcher>
where
    S: AsRef<str>,
{
    let name = name.as_ref();

    match EpisodeMatcher::load(name) {
        Ok(matcher) => Ok(matcher),
        Err(ref err) if err.is_file_nonexistant() => match matcher {
            Some(matcher) => {
                let matcher = EpisodeMatcher::with_matcher(matcher)?;
                matcher.save(name)?;
                Ok(matcher)
            }
            None => Ok(EpisodeMatcher::new()),
        },
        Err(err) => Err(err),
    }
}

fn get_episodes<S>(args: &ArgMatches, name: S, config: &Config) -> Result<EpisodeList>
where
    S: AsRef<str>,
{
    let name = name.as_ref();

    let dir = if let Some(path) = args.value_of("path") {
        let path = SeriesPath::new(path)?;
        path.save(name)?;
        path.take()
    } else {
        get_series_path(name, config)?
    };

    let matcher = get_episode_matcher(name, args.value_of("matcher"))?;
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

fn get_remote(args: &ArgMatches, can_use_offline: bool) -> Result<Box<RemoteService>> {
    use anime::remote::anilist::{self, AccessToken, AniList, AniListConfig};
    use anime::remote::offline::Offline;

    if args.is_present("offline") {
        ensure!(can_use_offline, err::MustRunOnline);
        Ok(Box::new(Offline::new()))
    } else {
        let config = match AniListConfig::load(None) {
            Ok(config) => config,
            Err(ref err) if err.is_file_nonexistant() => {
                println!(
                    "need AniList login token\ngo to {}\n\npaste your token:",
                    anilist::LOGIN_URL
                );

                let token = {
                    let mut buffer = String::new();
                    io::stdin().read_line(&mut buffer).context(err::IO)?;
                    let buffer = buffer.trim_end();

                    AccessToken::new(buffer)
                };

                let config = AniListConfig::new(token);
                config.save(None)?;
                config
            }
            Err(err) => return Err(err),
        };

        let anilist = AniList::login(config)?;
        Ok(Box::new(anilist))
    }
}

fn get_best_info_from_remote<R, S>(remote: R, name: S) -> Result<SeriesInfo>
where
    R: AsRef<RemoteService>,
    S: AsRef<str>,
{
    let remote = remote.as_ref();
    let name = name.as_ref();

    let mut results = remote.search_info_by_name(name)?;
    let index = detect::best_matching_info(name, results.as_slice())
        .context(err::NoMatchingSeries { name })?;

    let info = results.swap_remove(index);
    Ok(info)
}

fn get_season_num(args: &ArgMatches) -> usize {
    args.value_of("season")
        .and_then(|num_str| num_str.parse().ok())
        .map(|num: usize| num.saturating_sub(1))
        .unwrap_or(0)
}

fn get_series<R, S>(name: S, remote: R, episodes: EpisodeList, season_num: usize) -> Result<Series>
where
    R: AsRef<RemoteService>,
    S: AsRef<str>,
{
    let name = name.as_ref();

    let seasons = match SeasonInfoList::load(name) {
        Ok(mut seasons) => {
            if seasons.add_from_remote_upto(&remote, season_num)? {
                seasons.save(name)?;
            }

            seasons
        }
        Err(ref err) if err.is_file_nonexistant() => {
            let info = get_best_info_from_remote(&remote, &episodes.title)?;
            let seasons = SeasonInfoList::from_info_and_remote(info, &remote, Some(season_num))?;
            seasons.save(name)?;
            seasons
        }
        Err(err) => return Err(err),
    };

    let series = Series::from_season_list(seasons, season_num, episodes)?;
    Ok(series)
}

fn print_info<R>(remote: R, config: &Config, series: &Series, state: &EntryState)
where
    R: AsRef<RemoteService>,
{
    if !util::is_running_in_terminal() {
        return;
    }

    let repeater = "-".repeat(series.info.title.len() + 2);

    println!("+{}+\n@ {} @\n+{}+", repeater, series.info.title, repeater);
    println!();

    println!("watched: {}/{}", state.watched_eps(), series.info.episodes);
    println!(
        "score: {}",
        state
            .score()
            .map(|s| remote.as_ref().score_to_str(s))
            .unwrap_or_else(|| "none".into())
    );

    println!();

    let watch_time = series.info.episode_length * (series.info.episodes - state.watched_eps());
    let minutes_must_watch = series.info.episode_length as f32 * config.episode.pcnt_must_watch;

    println!("time to finish: {}", util::hms_from_mins(watch_time as f32));
    println!("progress time: {}", util::ms_from_mins(minutes_must_watch));

    println!();
    println!("+{}+", repeater);
    println!();
}
