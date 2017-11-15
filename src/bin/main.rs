#[macro_use] extern crate error_chain;
#[macro_use] extern crate lazy_static;
extern crate mal;
extern crate regex;

mod input;
mod series;

use std::path::Path;
use mal::MAL;
use series::Series;

error_chain! {
    links {
        MAL(mal::Error, mal::ErrorKind);
        Input(input::Error, input::ErrorKind);
    }

    errors {
        NoAnimeFound(name: String) {
            display("no anime named [{}] found", name)
        }
    }
}

fn main() {
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
    let selected = find_and_select_series(&mal, &series.name).unwrap();

    println!("selected:\n{:?}", selected);
}

fn find_and_select_series(mal: &MAL, name: &str) -> Result<mal::AnimeEntry> {
    let mut series = mal.search(name)?;

    if series.len() == 0 {
        bail!(ErrorKind::NoAnimeFound(name.into()))
    } else if series.len() > 1 {
        println!("multiple anime named [{}] found", name);
        println!("enter the number next to the anime name for the one you want\n");

        for (i, s) in series.iter().enumerate() {
            println!("{} [{}]", 1 + i, s.title);
        }

        let idx = input::read_usize_range(1, series.len())? - 1;
        Ok(series.swap_remove(idx))
    } else {
        Ok(series.swap_remove(0))
    }
}