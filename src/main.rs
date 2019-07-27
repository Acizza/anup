mod config;
mod err;
mod file;
mod process;
mod series;
mod track;

use crate::config::Config;
use crate::err::Result;
use crate::file::{FileType, SaveDir, SaveFile};
use crate::series::local::{EpisodeList, EpisodeMatcher};
use crate::series::remote::anilist::{self, AniList, AniListConfig};
use crate::series::remote::offline::Offline;
use crate::series::remote::{RemoteService, SeriesInfo, Status};
use crate::series::{detect, SeasonInfoList, Series};
use crate::track::{EntryState, SeriesTracker};
use chrono::Utc;
use clap::clap_app;
use clap::ArgMatches;
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

    if let Err(err) = run(&args) {
        err::display_error(err);
        std::process::exit(1);
    }
}

fn run(args: &clap::ArgMatches) -> Result<()> {
    let name = get_series_name(args)?;
    let config = load_config()?;

    let episodes = {
        let dir = if let Some(path) = args.value_of("path") {
            let path = SeriesPath::new(path)?;
            path.save(name.as_ref())?;
            path.take()
        } else {
            get_series_path(&name, &config)?
        };

        let matcher = load_episode_matcher(&name, args.value_of("matcher"))?;
        EpisodeList::parse(&dir, &matcher)?
    };

    if args.is_present("prefetch") {
        prefetch(args, name, episodes)
    } else if args.is_present("sync") {
        sync(args, name)
    } else if args.is_present("rate") || args.is_present("drop") || args.is_present("hold") {
        modify_series(args, name)
    } else if args.is_present("clean") {
        remove_orphaned_data(config)
    } else {
        play(args, config, name, episodes)
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

fn prefetch(args: &ArgMatches, name: String, episodes: EpisodeList) -> Result<()> {
    ensure!(
        !args.is_present("offline"),
        err::MustRunOnline {
            command: "prefetch"
        }
    );

    let remote: Box<RemoteService> = Box::new(init_anilist()?);
    let info = SeriesInfo::best_matching_from_remote(&remote, &episodes.title)?;
    let seasons = SeasonInfoList::from_info_and_remote(info, &remote, None)?;

    seasons.save(name.as_ref())?;

    for (season_num, season) in seasons.inner().iter().enumerate() {
        if let Some(entry) = remote.get_list_entry(season.id)? {
            let state = EntryState::new(entry);
            state.save_with_id(season.id, name.as_ref())?;
        }

        println!("season {} -> {}", 1 + season_num, season.title);
    }

    println!("\nprefetch complete\nyou can now fully watch this series offline");
    Ok(())
}

fn sync(args: &ArgMatches, name: String) -> Result<()> {
    ensure!(
        !args.is_present("offline"),
        err::MustRunOnline { command: "sync" }
    );

    let remote: Box<RemoteService> = Box::new(init_anilist()?);
    let seasons = SeasonInfoList::load(name.as_ref())?;

    for (season_num, season) in seasons.inner().iter().enumerate() {
        let mut state = match EntryState::load_with_id(season.id, name.as_ref()) {
            Ok(state) => state,
            Err(ref err) if err.is_file_nonexistant() => continue,
            Err(err) => return Err(err),
        };

        if !state.needs_sync() {
            continue;
        }

        println!("syncing season {}: {}", 1 + season_num, season.title);
        state.sync_changes_to_remote(&remote, &name)?;
    }

    Ok(())
}

fn modify_series(args: &ArgMatches, name: String) -> Result<()> {
    let config = load_config()?;

    let remote: Box<RemoteService> = if args.is_present("offline") {
        Box::new(Offline::new())
    } else {
        Box::new(init_anilist()?)
    };

    let season_num = args
        .value_of("season")
        .and_then(|num_str| num_str.parse().ok())
        .map(|num: usize| num.saturating_sub(1))
        .unwrap_or(0);

    let season = {
        let seasons = SeasonInfoList::load(name.as_ref())?;
        seasons.take_unchecked(season_num)
    };

    let mut state = EntryState::load_with_id(season.id, name.as_ref())?;
    state.sync_changes_from_remote(&remote, &name)?;

    if let Some(score) = args.value_of("rate") {
        let score = remote.parse_score(score).context(err::ScoreParseFailed)?;
        state.set_score(Some(score));
    }

    match (args.is_present("drop"), args.is_present("hold")) {
        (true, true) => return Err(err::Error::CantDropAndHold),
        (true, false) => state.mark_as_dropped(&config),
        (false, true) => state.mark_as_on_hold(),
        (false, false) => (),
    }

    state.sync_changes_to_remote(&remote, &name)
}

fn remove_orphaned_data(config: Config) -> Result<()> {
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

        println!("{} will be purged", series);
        SaveDir::LocalData.remove_subdir(&series)?;
    }

    Ok(())
}

fn get_series_path<S>(name: S, config: &Config) -> Result<PathBuf>
where
    S: AsRef<str>,
{
    match SeriesPath::load(name.as_ref()) {
        Ok(path) => Ok(path.take()),
        Err(ref err) if err.is_file_nonexistant() => {
            detect::best_matching_folder(&name, &config.series_dir)
        }
        Err(err) => Err(err),
    }
}

fn play(args: &ArgMatches, config: Config, name: String, episodes: EpisodeList) -> Result<()> {
    let remote: Box<RemoteService> = if args.is_present("offline") {
        Box::new(Offline::new())
    } else {
        Box::new(init_anilist()?)
    };

    let season_num = args
        .value_of("season")
        .and_then(|num_str| num_str.parse().ok())
        .map(|num: usize| num.saturating_sub(1))
        .unwrap_or(0);

    let series = get_series(&name, &remote, episodes, season_num)?;

    let mut tracker = SeriesTracker::init(&remote, &series.info, &name)?;
    tracker.begin_watching(&remote, &config)?;

    if !args.is_present("quiet") {
        print_info(&remote, &config, &series, &tracker);
    }

    if args.is_present("oneshot") {
        play_episode(remote, &config, &series, &mut tracker)?;
    } else {
        play_episode_loop(remote, &config, &series, &mut tracker)?;
    }

    Ok(())
}

#[derive(Deserialize, Serialize)]
struct SeriesName(String);

impl SeriesName {
    fn new<S>(name: S) -> SeriesName
    where
        S: Into<String>,
    {
        SeriesName(name.into())
    }

    #[inline(always)]
    fn take(self) -> String {
        self.0
    }
}

impl SaveFile for SeriesName {
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
        let sname = SeriesName::new(name);
        sname.save(None)?;

        return Ok(sname.take());
    }

    match SeriesName::load(None) {
        Ok(sname) => Ok(sname.take()),
        Err(ref err) if err.is_file_nonexistant() => Err(err::Error::NoSavedSeriesName),
        Err(err) => Err(err),
    }
}

fn load_config() -> Result<Config> {
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

fn load_episode_matcher<S>(name: S, matcher: Option<&str>) -> Result<EpisodeMatcher>
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

fn init_anilist() -> Result<AniList> {
    use crate::series::remote::anilist::AccessToken;

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

    AniList::login(config)
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
            let info = SeriesInfo::best_matching_from_remote(&remote, &episodes.title)?;
            let seasons = SeasonInfoList::from_info_and_remote(info, &remote, Some(season_num))?;
            seasons.save(name)?;
            seasons
        }
        Err(err) => return Err(err),
    };

    Series::from_season_list(seasons, season_num, episodes)
}

fn is_running_in_terminal() -> bool {
    unsafe { libc::isatty(libc::STDOUT_FILENO) != 0 }
}

fn print_info<R>(remote: R, config: &Config, series: &Series, tracker: &SeriesTracker)
where
    R: AsRef<RemoteService>,
{
    if !is_running_in_terminal() {
        return;
    }

    let repeater = "-".repeat(series.info.title.len() + 2);

    println!("+{}+\n@ {} @\n+{}+", repeater, series.info.title, repeater);
    println!();

    println!(
        "watched: {}/{}",
        tracker.state.watched_eps(),
        series.info.episodes
    );
    println!(
        "score: {}",
        tracker
            .state
            .score()
            .map(|s| remote.as_ref().score_to_str(s))
            .unwrap_or_else(|| "none".into())
    );

    println!();

    let watch_time =
        series.info.episode_length * (series.info.episodes - tracker.state.watched_eps());
    let minutes_must_watch = series.info.episode_length as f32 * config.episode.pcnt_must_watch;

    println!("time to finish: {}", hms_from_mins(watch_time as f32));
    println!("progress time: {}", ms_from_mins(minutes_must_watch as f32));

    println!();
    println!("+{}+", repeater);
    println!();
}

fn ms_from_mins(mins: f32) -> String {
    let m = mins.floor() as u32;
    let s = (mins * 60.0 % 60.0).floor() as u32;

    format!("{:02}:{:02}", m, s)
}

fn hms_from_mins(mins: f32) -> String {
    let h = (mins / 60.0).floor() as u32;
    let m = (mins % 60.0).floor() as u32;
    let s = m * 60 % 60;

    format!("{:02}:{:02}:{:02}", h, m, s)
}

#[derive(PartialEq)]
enum PlayResult {
    Continue,
    Finished,
}

fn play_episode<R>(
    remote: R,
    config: &Config,
    series: &Series,
    tracker: &mut SeriesTracker,
) -> Result<PlayResult>
where
    R: AsRef<RemoteService>,
{
    let ep_num = tracker.state.watched_eps() + 1;
    let start_time = Utc::now();

    series.play_episode(ep_num)?;

    let end_time = Utc::now();

    let mins_watched = {
        let watch_time = end_time - start_time;
        watch_time.num_seconds() as f32 / 60.0
    };

    let mins_must_watch = series.info.episode_length as f32 * config.episode.pcnt_must_watch;

    if mins_watched < mins_must_watch {
        println!("did not watch episode long enough");
        return Ok(PlayResult::Finished);
    }

    tracker.episode_completed(&remote, config)?;

    match tracker.state.status() {
        Status::Completed => {
            println!("completed!");
            Ok(PlayResult::Finished)
        }
        _ => {
            println!("{}/{} completed", ep_num, series.info.episodes);
            Ok(PlayResult::Continue)
        }
    }
}

fn play_episode_loop<R>(
    remote: R,
    config: &Config,
    series: &Series,
    tracker: &mut SeriesTracker,
) -> Result<()>
where
    R: AsRef<RemoteService>,
{
    use std::thread;
    use std::time::Duration;

    loop {
        if let PlayResult::Finished = play_episode(&remote, config, series, tracker)? {
            break Ok(());
        }

        if config.episode.seconds_before_next > 0.0 {
            let millis = (config.episode.seconds_before_next * 1000.0) as u64;
            let duration = Duration::from_millis(millis);
            thread::sleep(duration);
        }
    }
}
