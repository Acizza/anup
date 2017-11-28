#[macro_use]
extern crate failure;
#[macro_use]
extern crate failure_derive;
#[macro_use]
extern crate lazy_static;

extern crate chrono;
extern crate mal;
extern crate regex;

mod input;
mod prompt;
mod process;
mod series;

use failure::{Error, ResultExt};
use input::Answer;
use mal::MAL;
use mal::list::{AnimeEntry, Status};
use series::{EpisodeData, Series};
use std::path::Path;

fn main() {
    // Temporary
    let args = std::env::args().collect::<Vec<String>>();

    match run(args) {
        Ok(_) => (),
        Err(e) => {
            let mut fail: &failure::Fail = e.cause();
            eprintln!("fatal error: {}", fail);

            while let Some(cause) = fail.cause() {
                eprintln!("cause: {}", cause);
                fail = cause;
            }

            eprintln!("{}", e.backtrace());
        }
    }
}

fn run(args: Vec<String>) -> Result<(), Error> {
    let mal = MAL::new(args[2].clone(), args[3].clone());
    let series = get_series_data(&mal, Path::new(&args[1]))?;

    let mut entry = get_mal_list_entry(&mal, &series)?;

    loop {
        entry.watched_episodes += 1;

        if series.play_episode(entry.watched_episodes)?.success() {
            prompt::update_watched(&mal, &mut entry)?;
        } else {
            prompt::abnormal_player_exit(&mal, &mut entry)?;
        }

        if entry.status == Status::Completed {
            break;
        }

        println!("do you want to watch the next episode? (Y/n)");

        if !input::read_yn(Answer::Yes)? {
            break;
        }
    }

    Ok(())
}

fn get_mal_list_entry(mal: &MAL, series: &Series) -> Result<AnimeEntry, Error> {
    let list = mal.get_anime_list().context("anime list retrieval failed")?;

    match list.into_iter().find(|e| e.info.id == series.info.id) {
        Some(mut entry) => {
            if entry.status == Status::Completed && !entry.rewatching {
                prompt::rewatch(mal, &mut entry)?;
            }

            Ok(entry)
        },
        None => prompt::add_to_anime_list(mal, series),
    }
}

fn get_series_data(mal: &MAL, path: &Path) -> Result<Series, Error> {
    let loc_data = EpisodeData::parse(path)?;
    let info = prompt::find_and_select_series(mal, &loc_data.series_name)?;

    Ok(Series::new(info, loc_data))
}
