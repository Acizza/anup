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
use std::path::Path;

fn main() {
    // Temporary
    let args = std::env::args().collect::<Vec<String>>();

    match run(args) {
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

fn run(args: Vec<String>) -> Result<(), Error> {
    let mal = MAL::new(args[2].clone(), args[3].clone());

    let mut series = Series::from_path(Path::new(&args[1]))?;
    let info = request_series_info(&mal, &mut series)?;

    let mut entry = get_mal_list_entry(&mal, &info)?;

    loop {
        entry.watched_episodes += 1;

        if series.play_episode(entry.watched_episodes)?.success() {
            prompt::update_watched(&mal, &mut entry)?;
        } else {
            prompt::abnormal_player_exit(&mal, &mut entry)?;
        }

        prompt::next_episode_options(&mal, &entry)?;
    }
}

fn get_mal_list_entry(mal: &MAL, info: &mal::SeriesInfo) -> Result<AnimeEntry, Error> {
    let list = mal.get_anime_list().context("anime list retrieval failed")?;

    match list.into_iter().find(|e| e.info == *info) {
        Some(mut entry) => {
            if entry.status == Status::Completed && !entry.rewatching {
                prompt::rewatch(mal, &mut entry)?;
            }

            Ok(entry)
        }
        None => prompt::add_to_anime_list(mal, info),
    }
}

fn request_series_info(mal: &MAL, series: &mut Series) -> Result<mal::SeriesInfo, Error> {
    if series.data_exists() {
        let season = series.get_season_data(1)?;

        mal.search(&series.name)?
            .into_iter()
            .find(|a| a.id == season.series_id)
            .ok_or(format_err!( // TODO: use custom error type & context
                "no anime with id {} found on MAL",
                season.series_id,
            ))
    } else {
        let selected = prompt::find_and_select_series_info(mal, &series.name)?;

        series.set_season_data(1, SeasonInfo::with_series_id(selected.id));
        series.save()?;

        Ok(selected)
    }
}
