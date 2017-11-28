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
use series::Series;

fn run() -> Result<(), Error> {
    // Temporary
    let args = std::env::args().collect::<Vec<String>>();

    let loc_data = Series::from_path(Path::new(&args[1]))?;
    let mal = MAL::new(args[2].clone(), args[3].clone());

    let series = prompt::find_and_select_series(&mal, &loc_data.name)?;

    let anime_list = mal.get_anime_list().context("anime list retrieval failed")?;

    if let Some(list_status) = anime_list.iter().find(|a| a.info.id == series.id) {
        println!("found anime on anime list:\n{:?}", list_status);
    } else {
        println!("anime not found on anime list");
    }

    Ok(())
}

fn main() {
    match run() {
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
