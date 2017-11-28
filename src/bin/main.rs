#[macro_use]
extern crate failure;
#[macro_use]
extern crate failure_derive;
#[macro_use]
extern crate lazy_static;

extern crate mal;
extern crate regex;

mod prompt;
mod process;
mod series;

use std::path::Path;
use failure::{Error, ResultExt};
use mal::MAL;
use series::{EpisodeData, Series};

fn get_series_data(mal: &MAL, path: &Path) -> Result<Series, Error> {
    let loc_data = EpisodeData::parse(path)?;
    let info = prompt::find_and_select_series(mal, &loc_data.series_name)?;

    Ok(Series::new(info, loc_data))
}

fn run(args: Vec<String>) -> Result<(), Error> {
    let mal = MAL::new(args[2].clone(), args[3].clone());
    let series = get_series_data(&mal, Path::new(&args[1]))?;

    let anime_list = mal.get_anime_list().context("anime list retrieval failed")?;

    if let Some(list_status) = anime_list.iter().find(|a| a.info.id == series.info.id) {
        println!("found anime on anime list:\n{:?}", list_status);
    } else {
        println!("anime not found on anime list");
    }

    Ok(())
}

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
