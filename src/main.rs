#[macro_use] extern crate lazy_static;
#[macro_use] extern crate error_chain;

mod anime;

use std::env;
use anime::local::LocalAnime;
use anime::mal::AnimeInfo;

fn main() {
    let path = env::current_dir().unwrap();

    let mut args = env::args().skip(1);
    let username = args.next().unwrap();
    let password = args.next().unwrap();

    println!("{:?}", LocalAnime::new(&path));
    println!("{:?}", AnimeInfo::request("Full Metal", username, password));
}
