#[macro_use]
extern crate clap;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;

extern crate base64;
extern crate chrono;
extern crate mal;
extern crate regex;
extern crate serde;
extern crate serde_json;
extern crate toml;

mod config;
mod input;
mod prompt;
mod process;
mod series;

use config::{Config, User};
use chrono::{Local, NaiveDate};
use failure::{Error, ResultExt};
use mal::MAL;
use mal::list::AnimeList;
use series::Series;
use std::path::PathBuf;

fn main() {
    match run() {
        Ok(_) => (),
        Err(e) => {
            eprintln!("fatal error: {}", e.cause());

            for cause in e.causes().skip(1) {
                eprintln!("cause: {}", cause);
            }

            eprintln!("{}", e.backtrace());
        }
    }
}

fn run() -> Result<(), Error> {
    let matches = clap_app!(anitrack =>
        (version: env!("CARGO_PKG_VERSION"))
        (author: env!("CARGO_PKG_AUTHORS"))
        (@arg PATH: "Specifies the directory to look for video files in")
        (@arg CONFIG_PATH: -c --config "Specifies the location of the configuration file")
        (@arg SEASON: -s --season +takes_value "Specifies which season you want to watch")
        (@arg DONT_SAVE_CONFIG: --nosave "Disables saving of your account information")
    ).get_matches();

    let path = match matches.value_of("PATH") {
        Some(p) => PathBuf::from(p),
        None => std::env::current_dir().context("failed to get current directory")?,
    };

    let season = matches
        .value_of("SEASON")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    let mal = init_mal_client(&matches)?;
    let anime_list = AnimeList::new(&mal);

    let mut series = Series::from_path(&path)?;
    series.watch_season(season, &anime_list)
}

pub fn get_today() -> NaiveDate {
    Local::today().naive_utc()
}

fn init_mal_client(args: &clap::ArgMatches) -> Result<MAL, Error> {
    let mut config = load_config(args).context("failed to load config file")?;

    let decoded_password = config
        .user
        .decode_password()
        .context("failed to decode config password")?;

    let mut mal = MAL::new(config.user.name.clone(), decoded_password);
    let mut credentials_changed = false;

    while !mal.verify_credentials()? {
        println!(
            "invalid password for [{}], please try again:",
            config.user.name
        );

        mal.password = input::read_line()?;
        credentials_changed = true;
    }

    if credentials_changed {
        config.user.encode_password(&mal.password);
    }

    if !args.is_present("DONT_SAVE_CONFIG") {
        config.save().context("failed to save config")?;
    }

    Ok(mal)
}

fn load_config(args: &clap::ArgMatches) -> Result<Config, Error> {
    let config_path = match args.value_of("CONFIG_PATH") {
        Some(p) => PathBuf::from(p),
        None => {
            let mut current = std::env::current_exe().context("failed to get executable path")?;
            current.pop();
            current.push("config.toml");
            current
        }
    };

    match Config::from_path(&config_path) {
        Ok(config) => Ok(config),
        Err(_) => {
            println!("please enter your MAL username:");
            let name = input::read_line()?;

            println!("please enter your MAL password:");
            let pass = input::read_line()?;

            let user = User::new(name, &pass);
            let config = Config::new(user, config_path);

            Ok(config)
        }
    }
}
