#[macro_use] extern crate lazy_static;
#[macro_use] extern crate error_chain;

mod anime;

use std::env;
use std::io;
use anime::local::LocalAnime;
use anime::mal::AnimeInfo;

fn main() {
    // All temporary
    let path = env::current_dir().unwrap();

    let mut args = env::args().skip(1);
    let username = args.next().unwrap();
    let password = args.next().unwrap();

    let local = LocalAnime::new(&path).unwrap();
    println!("{:?}", local);

    let found = AnimeInfo::request(&local.name, username.clone(), password);
    println!("{:?}", found);

    match found {
        Ok(ref found) if found.len() > 0 => {
            let selected = if found.len() > 1 {
                for (i, anime) in found.iter().enumerate() {
                    println!("{} [{}]", i, anime.name);
                }

                let mut buffer = String::new();
                io::stdin().read_line(&mut buffer).unwrap();

                &found[buffer[..buffer.len()-1].parse::<usize>().unwrap()]
            } else {
                &found[0]
            };

            println!("Watched episodes of [{}]: {:?}", selected.name, selected.get_watched_episodes(username.clone()));
        },
        _ => (),
    }
}
