#[macro_use] extern crate error_chain;
#[macro_use] extern crate lazy_static;
extern crate mal;
extern crate regex;

mod series;

use std::path::Path;
use mal::MAL;
use series::Series;

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
    println!("MAL data:\n{:?}", mal.search(&series.name));
}