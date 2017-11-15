#[macro_use] extern crate error_chain;
#[macro_use] extern crate lazy_static;
extern crate regex;

mod series;

use std::path::Path;
use series::Series;

fn main() {
    let series = match Series::from_path(Path::new("/home/jonathan/anime/Boku no Hero Academia")) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("{}", err);
            panic!("{:?}", err);
        },
    };

    println!("{:?}", series);
}