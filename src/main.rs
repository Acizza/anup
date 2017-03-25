#[macro_use]
extern crate lazy_static;

mod anime;

use std::env;
use anime::Anime;

fn main() {
    let path = env::current_dir().unwrap();
    println!("{:?}", Anime::new(&path));
}
