mod config;
mod err;
mod file;
mod process;
mod series;

use crate::config::Config;
use crate::err::Result;
use crate::file::{SaveFile, SaveFileInDir};
use crate::series::local::{EpisodeList, EpisodeMatcher};
use crate::series::remote::anilist::{self, AniList, AniListConfig};
use crate::series::remote::offline::Offline;
use crate::series::remote::{RemoteService, SeriesInfo};
use crate::series::{detect, SeasonInfoList};
use clap::clap_app;
use snafu::{ensure, OptionExt, ResultExt};
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
    let is_offline = args.is_present("offline");

    let config = load_config()?;

    // TODO: use -p argument if specified
    let dir = detect::best_matching_folder(keyword, &config.series_dir)?;
    println!("detected dir: {:?}\n\n", dir);

    let episodes = {
        let matcher = load_episode_matcher(keyword, args.value_of("matcher"))?;
        EpisodeList::parse(&dir, &matcher)?
    };

    let remote: Box<RemoteService> = if is_offline {
        Box::new(Offline::new())
    } else {
        Box::new(init_anilist()?)
    };

    let season_num = args
        .value_of("season")
        .and_then(|num_str| num_str.parse().ok())
        .map(|num: usize| num.saturating_sub(1))
        .unwrap_or(0);

    let info = get_series_info(&remote, keyword, season_num, &dir, is_offline)?;

    let entry = series::remote::SeriesEntry::new(info.id);
    entry.save(keyword)?;

    let series = series::Series {
        info,
        episodes,
        episode_range: None,
    };

    println!("series:\n{:?}\n\n", series);

    let episode = series.get_episode(1).expect("no episode");
    println!("episode:\n{:?}\n\n", episode);

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

fn load_episode_matcher<S>(keyword: S, matcher: Option<S>) -> Result<EpisodeMatcher>
where
    S: AsRef<str> + Into<String>,
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

fn get_series_info<R, S>(
    remote: R,
    keyword: S,
    season_num: usize,
    dir: &PathBuf,
    offline: bool,
) -> Result<SeriesInfo>
where
    R: AsRef<RemoteService>,
    S: AsRef<str>,
{
    let keyword = keyword.as_ref();

    let seasons = match SeasonInfoList::load(keyword) {
        Ok(mut seasons) => {
            if offline {
                ensure!(seasons.has(season_num), err::RunWithPrefetch);
            }

            if seasons.add_from_remote_upto(&remote, season_num)? {
                seasons.save(keyword)?;
            }

            seasons
        }
        Err(ref err) if err.is_file_nonexistant() => {
            ensure!(!offline, err::RunWithPrefetch);

            // The directory is more likely to have a complete name, which will likely match
            // better than just a keyword, which could be an abstract identifier
            let dir_name = dir
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| keyword.into());

            let info = SeriesInfo::best_matching_from_remote(&remote, dir_name)?;

            let seasons = SeasonInfoList::from_info_and_remote(info, &remote, Some(season_num))?;
            seasons.save(keyword)?;
            seasons
        }
        Err(err) => return Err(err),
    };

    seasons.take(season_num).context(err::NoSeason {
        season: 1 + season_num,
    })
}
