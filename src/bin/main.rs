#[macro_use]
extern crate clap;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;

extern crate base64;
extern crate chrono;
extern crate mal;
extern crate regex;
extern crate serde;
extern crate serde_json;
extern crate toml;

mod config;
mod input;
mod prompt;
mod process;
mod series;

use config::Config;
use failure::{Error, ResultExt};
use mal::MAL;
use mal::list::{AnimeList, ListEntry, Status};
use prompt::SearchResult;
use series::{SeasonInfo, Series};
use std::path::PathBuf;

fn main() {
    match run() {
        Ok(_) => (),
        Err(e) => {
            eprintln!("fatal error: {}", e.cause());

            for cause in e.causes().skip(1) {
                eprintln!("cause: {}", cause);
            }

            eprintln!("{}", e.backtrace());
        }
    }
}

fn run() -> Result<(), Error> {
    let matches = clap_app!(anitrack =>
        (version: env!("CARGO_PKG_VERSION"))
        (author: env!("CARGO_PKG_AUTHORS"))
        (@arg PATH: "Specifies the directory to look for video files in")
        (@arg CONFIG_PATH: -c --config "Specifies the location of the configuration file")
        (@arg USERNAME: -u --username +takes_value "Your MyAnimeList username")
        (@arg SEASON: -s --season +takes_value "Specifies which season you want to watch")
        (@arg DONT_SAVE_CONFIG: --nosave "Disables saving of your account information")
    ).get_matches();

    let path = match matches.value_of("PATH") {
        Some(p) => PathBuf::from(p),
        None => std::env::current_dir().context("failed to get current directory")?,
    };

    let mal = init_mal(&matches)?;

    let season = matches
        .value_of("SEASON")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    let mut series = Series::from_path(&path)?;
    watch_season(&mal, season, &mut series)
}

fn init_mal(args: &clap::ArgMatches) -> Result<MAL, Error> {
    let username = args.value_of("USERNAME").map(|u| u.to_string());

    let mut config = load_config(args).context("failed to load config file")?;

    let user = config
        .load_user_prompt(username)
        .context("failed to get config user information")?;

    if !args.is_present("DONT_SAVE_CONFIG") {
        config.save().context("failed to save config")?;
    }

    let password = user.decode_password()
        .context("failed to decode config password")?;

    let mal = MAL::new(user.name, password);
    Ok(mal)
}

fn load_config(args: &clap::ArgMatches) -> Result<Config, Error> {
    let config_path = match args.value_of("CONFIG_PATH") {
        Some(p) => PathBuf::from(p),
        None => {
            let mut current = std::env::current_exe().context("failed to get current directory")?;
            current.pop();
            current.push("config.toml");
            current
        }
    };

    let config = if !config_path.exists() {
        Config::new(config_path)
    } else {
        Config::from_path(&config_path)?
    };

    Ok(config)
}

fn watch_season(mal: &MAL, season: u32, series: &mut Series) -> Result<(), Error> {
    let find_result = find_season_series_info(mal, season, series)?;
    let series_info = find_result.info;

    if !series.has_season_data(season) {
        let info = SeasonInfo::create_basic(
            series_info.id,
            series_info.episodes,
            find_result.search_term,
        );

        series.set_season_data(season, info);
        series.save_data()?;
    }

    let anime_list = AnimeList::new(mal);
    let mut list_entry = find_list_entry(&anime_list, &series_info)?;

    play_episode_loop(season, series, &anime_list, &mut list_entry)
}

fn get_season_ep_offset(season: u32, series: &Series) -> Result<u32, Error> {
    let mut ep_offset = 0;

    for cur_season in 1..season {
        // TODO: handle case where previous season info doesn't exist?
        let season = series.get_season_data(cur_season)?;
        ep_offset += season.episodes;
    }

    Ok(ep_offset)
}

fn play_episode_loop(
    season: u32,
    series: &Series,
    list: &AnimeList,
    entry: &mut ListEntry,
) -> Result<(), Error> {
    let season_offset = get_season_ep_offset(season, series)?;

    loop {
        let watched = entry.watched_episodes() + 1;
        entry.set_watched_episodes(watched);
        let real_ep_num = watched + season_offset;

        if series.play_episode(real_ep_num)?.success() {
            prompt::update_watched(list, entry)?;
        } else {
            prompt::abnormal_player_exit(list, entry)?;
        }

        list.update(entry)?;
        prompt::next_episode_options(list, entry)?;
    }
}

fn find_list_entry(list: &AnimeList, info: &mal::SeriesInfo) -> Result<ListEntry, Error> {
    let entries = list.read_entries().context("MAL list retrieval failed")?;
    let found = entries.into_iter().find(|e| e.series_info == *info);

    match found {
        Some(mut entry) => {
            if entry.status() == Status::Completed && !entry.rewatching() {
                prompt::rewatch(list, &mut entry)?;
            }

            Ok(entry)
        }
        None => prompt::add_to_anime_list(list, info),
    }
}

fn find_season_series_info(mal: &MAL, season: u32, series: &Series) -> Result<SearchResult, Error> {
    match series.get_season_data(season) {
        Ok(season) => {
            let info = season.request_mal_info(mal)?;
            let name = series.name.clone();

            Ok(SearchResult::new(info, name))
        }
        Err(_) => prompt::find_and_select_series_info(mal, &series.name),
    }
}
