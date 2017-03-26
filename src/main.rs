#[macro_use] extern crate lazy_static;
#[macro_use] extern crate error_chain;
#[macro_use] extern crate clap;

mod anime;

use std::env;
use std::io;
use std::path::PathBuf;
use anime::local::LocalAnime;
use anime::mal::AnimeInfo;

fn main() {
    let args = clap_app!(anitrack =>
        (@arg PATH: "Sets the directory to look for episodes in")
        (@arg username: -u +required +takes_value "Specifies the account name to use for MAL")
        (@arg password: -p +required +takes_value "Specifies the password for the specified account name")
        (@subcommand next =>
            (about: "Plays the next episode from the specified or current directory")
        )
    ).get_matches();

    let path = if let Some(path) = args.value_of("PATH") {
        PathBuf::from(path)
    } else {
        env::current_dir().unwrap()
    };

    // Temporary
    if args.is_present("next") {
        let username = args.value_of("username").unwrap();
        let password = args.value_of("password").unwrap();

        let local = LocalAnime::find(&path).unwrap();
        let found = AnimeInfo::request(&local.name, username.into(), password.into()).unwrap();

        if found.len() > 0 {
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

            let watched = selected.request_watched(username.into()).unwrap();
            println!("watched episodes of [{}]: {}", selected.name, watched);
        }
    }
}
