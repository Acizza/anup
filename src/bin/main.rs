#[macro_use]
extern crate clap;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;

extern crate chrono;
extern crate mal;
extern crate regex;
extern crate serde;
extern crate serde_json;

mod input;
mod prompt;
mod process;
mod series;

use failure::{Error, ResultExt};
use mal::MAL;
use mal::list::{AnimeEntry, Status};
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
        (@arg USERNAME: -u --username +takes_value +required "Your MyAnimeList username")
        (@arg PASSWORD: -p --password +takes_value +required "Your MyAnimeList password")
        (@arg SEASON: -s --season +takes_value "Specifies which season you want to watch")
    ).get_matches();

    let path = match matches.value_of("PATH") {
        Some(p) => PathBuf::from(p),
        None => std::env::current_dir().context("failed to get current directory")?,
    };

    let mal = {
        let username = matches.value_of("USERNAME").unwrap();
        let password = matches.value_of("PASSWORD").unwrap();
        MAL::new(username, password)
    };

    let season = matches
        .value_of("SEASON")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    let mut series = Series::from_path(&path)?;
    watch_season(&mal, season, &mut series)
}

fn watch_season(mal: &MAL, season: u32, series: &mut Series) -> Result<(), Error> {
    let series_info = find_season_series_info(mal, season, series)?;

    if !series.has_season_data(season) {
        let info = SeasonInfo::create_basic(series_info.id, series_info.episodes);
        series.set_season_data(season, info);
        series.save_data()?;
    }

    let anime_list = mal.get_anime_list().context("MAL list retrieval failed")?;
    let mut list_entry = find_list_entry(mal, &series_info, &anime_list)?;

    play_episode_loop(mal, season, series, &mut list_entry)
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
    mal: &MAL,
    season: u32,
    series: &Series,
    list_entry: &mut AnimeEntry,
) -> Result<(), Error> {
    let season_offset = get_season_ep_offset(season, series)?;

    loop {
        list_entry.watched_episodes += 1;
        let real_ep_num = list_entry.watched_episodes + season_offset;

        if series.play_episode(real_ep_num)?.success() {
            prompt::update_watched(mal, list_entry)?;
        } else {
            prompt::abnormal_player_exit(mal, list_entry)?;
        }

        prompt::next_episode_options(mal, list_entry)?;
    }
}

fn find_list_entry(
    mal: &MAL,
    info: &mal::SeriesInfo,
    list: &[AnimeEntry],
) -> Result<AnimeEntry, Error> {
    let found = list.iter().find(|e| e.info == *info);

    match found {
        Some(entry) => {
            let mut entry = entry.clone();

            if entry.status == Status::Completed && !entry.rewatching {
                prompt::rewatch(mal, &mut entry)?;
            }

            Ok(entry)
        }
        None => prompt::add_to_anime_list(mal, info),
    }
}

fn find_season_series_info(
    mal: &MAL,
    season: u32,
    series: &Series,
) -> Result<mal::SeriesInfo, Error> {
    match series.get_season_data(season) {
        Ok(season) => find_series_info_by_id(mal, &series.name, season.series_id),
        Err(_) => prompt::find_and_select_series_info(mal, &series.name),
    }
}

#[derive(Fail, Debug)]
#[fail(display = "no anime with id {} found with name [{}] on MAL", _0, _1)]
struct UnknownAnimeID(u32, String);

fn find_series_info_by_id(mal: &MAL, name: &str, id: u32) -> Result<mal::SeriesInfo, Error> {
    mal.search(name)
        .context("MAL search failed")?
        .into_iter()
        .find(|i| i.id == id)
        .ok_or(UnknownAnimeID(id, name.into()).into())
}
