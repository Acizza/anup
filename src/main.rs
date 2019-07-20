mod config;
mod err;
mod file;
mod process;
mod series;
mod track;

use crate::config::Config;
use crate::err::Result;
use crate::file::{SaveFile, SaveFileInDir};
use crate::series::local::{EpisodeList, EpisodeMatcher};
use crate::series::remote::anilist::{self, AniList, AniListConfig};
use crate::series::remote::offline::Offline;
use crate::series::remote::{RemoteService, SeriesInfo, Status};
use crate::series::{detect, SeasonInfoList, Series};
use crate::track::SeriesTracker;
use clap::clap_app;
use snafu::ResultExt;
use std::io;
use std::path::PathBuf;

fn main() {
    let args = clap_app!(anup =>
        (version: env!("CARGO_PKG_VERSION"))
        (author: env!("CARGO_PKG_AUTHORS"))
        (@arg SERIES: +takes_value +required "The name of the series to watch")
        (@arg season: -s --season +takes_value "The season to watch. Meant to be used when playing from a folder that has multiple seasons merged together under one name")
        (@arg matcher: -m --matcher +takes_value "The custom pattern to match episode files with")
        (@arg offline: -o --offline "Run in offline mode")
        (@arg prefetch: --prefetch "Fetch series info from AniList. For use with offline mode")
    )
    .get_matches();

    if let Err(err) = run(&args) {
        err::display_error(err);
        std::process::exit(1);
    }
}

fn run(args: &clap::ArgMatches) -> Result<()> {
    let keyword = args.value_of("SERIES").unwrap();
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

    let series = get_series(
        keyword,
        &remote,
        &config,
        season_num,
        args.value_of("matcher"),
    )?;

    let mut tracker = SeriesTracker::init(&remote, &series.info, keyword)?;
    tracker.begin_watching(&remote, &config)?;

    play_episode_loop(remote, &config, &series, &mut tracker)?;
    Ok(())
}

fn load_config() -> Result<Config> {
    match Config::load() {
        Ok(config) => Ok(config),
        Err(ref err) if err.is_file_nonexistant() => {
            // Default base directory: ~/anime/
            let mut dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~/"));
            dir.push("anime");

            let config = Config::new(dir);
            config.save()?;
            Ok(config)
        }
        Err(err) => Err(err),
    }
}

fn load_episode_matcher<S>(keyword: S, matcher: Option<&str>) -> Result<EpisodeMatcher>
where
    S: AsRef<str>,
{
    match EpisodeMatcher::load(&keyword) {
        Ok(matcher) => Ok(matcher),
        Err(ref err) if err.is_file_nonexistant() => match matcher {
            Some(matcher) => {
                let matcher = EpisodeMatcher::with_matcher(matcher)?;
                matcher.save(keyword)?;
                Ok(matcher)
            }
            None => Ok(EpisodeMatcher::new()),
        },
        Err(err) => Err(err),
    }
}

fn init_anilist() -> Result<AniList> {
    use crate::series::remote::anilist::AccessToken;

    let config = match AniListConfig::load() {
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
            config.save()?;
            config
        }
        Err(err) => return Err(err),
    };

    AniList::login(config)
}

fn get_season_list<R, S>(
    remote: R,
    keyword: S,
    season_num: usize,
    dir: &PathBuf,
) -> Result<SeasonInfoList>
where
    R: AsRef<RemoteService>,
    S: AsRef<str>,
{
    let keyword = keyword.as_ref();

    match SeasonInfoList::load(keyword) {
        Ok(mut seasons) => {
            if seasons.add_from_remote_upto(&remote, season_num)? {
                seasons.save(keyword)?;
            }

            Ok(seasons)
        }
        Err(ref err) if err.is_file_nonexistant() => {
            // The directory is more likely to have a complete name, which will likely match
            // better than just a keyword, which could be an abstract identifier
            let dir_name = dir
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| keyword.into());

            let info = SeriesInfo::best_matching_from_remote(&remote, dir_name)?;

            let seasons = SeasonInfoList::from_info_and_remote(info, &remote, Some(season_num))?;
            seasons.save(keyword)?;
            Ok(seasons)
        }
        Err(err) => Err(err),
    }
}

fn get_series<R, S>(
    name: S,
    remote: R,
    config: &Config,
    season_num: usize,
    matcher: Option<&str>,
) -> Result<Series>
where
    R: AsRef<RemoteService>,
    S: AsRef<str>,
{
    // TODO: allow overriding with argument
    let dir = detect::best_matching_folder(&name, &config.series_dir)?;

    let episodes = {
        let matcher = load_episode_matcher(&name, matcher)?;
        EpisodeList::parse(&dir, &matcher)?
    };

    let seasons = get_season_list(&remote, &name, season_num, &dir)?;
    Series::from_season_list(seasons, season_num, episodes)
}

fn play_episode<R>(
    remote: R,
    config: &Config,
    series: &Series,
    tracker: &mut SeriesTracker,
) -> Result<()>
where
    R: AsRef<RemoteService>,
{
    let ep_num = tracker.state.watched_eps() + 1;

    series.play_episode(ep_num)?;
    tracker.episode_completed(&remote, config)?;

    match tracker.state.status() {
        Status::Completed => {
            println!("[{}] completed!", series.info.title);
        }
        _ => println!(
            "[{}] episode {}/{} completed",
            series.info.title, ep_num, series.info.episodes
        ),
    }

    Ok(())
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
    loop {
        play_episode(&remote, config, series, tracker)?;

        if tracker.state.status() == Status::Completed {
            break Ok(());
        }
    }
}
