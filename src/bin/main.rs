#[macro_use] extern crate failure;
#[macro_use] extern crate failure_derive;
#[macro_use] extern crate lazy_static;
extern crate mal;
extern crate regex;

mod input;
mod series;

use std::path::Path;
use failure::{Error, ResultExt};
use mal::MAL;
use series::Series;

fn run() -> Result<(), Error> {
    // Temporary
    let args = std::env::args().collect::<Vec<String>>();

    let series = match Series::from_path(Path::new(&args[1])) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("{}", err);
            panic!("{:?}", err);
        },
    };

    println!("{:?}", series);

    let mal = MAL::new(args[2].clone(), args[3].clone());
    let selected = find_and_select_series(&mal, &series.name)?;

    println!("selected:\n{:?}", selected);
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
        },
    }
}

fn find_and_select_series(mal: &MAL, name: &str) -> Result<mal::AnimeEntry, Error> {
    let mut series = mal.search(name).context("MAL search failed")?;

    if series.len() == 0 {
        return Err(format_err!("no anime named [{}] found", name));
    } else if series.len() > 1 {
        println!("found multiple anime named [{}] on MAL", name);
        println!("input the number corrosponding with the intended anime:\n");

        for (i, s) in series.iter().enumerate() {
            println!("{} [{}]", 1 + i, s.title);
        }

        let idx = input::read_usize_range(1, series.len())? - 1;
        Ok(series.swap_remove(idx))
    } else {
        Ok(series.swap_remove(0))
    }
}