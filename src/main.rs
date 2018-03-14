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
mod error;
mod input;
mod prompt;
mod process;
mod series;

use chrono::{Local, NaiveDate};
use error::Error;
use mal::MAL;
use series::Series;
use std::path::Path;
use std::path::PathBuf;

fn main() {
    match run() {
        Ok(_) => (),
        Err(e) => {
            let e: failure::Error = e.into();
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
        None => std::env::current_dir().map_err(Error::FailedToGetCWD)?,
    };

    let season = matches
        .value_of("SEASON")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    let mal = init_mal_client(&matches)?;

    let mut series = Series::from_path(&path)?;
    series.watch_season(&mal, season)?;

    Ok(())
}

pub fn get_today() -> NaiveDate {
    Local::today().naive_utc()
}

fn init_mal_client<'a>(args: &clap::ArgMatches) -> Result<MAL<'a>, Error> {
    let mut config = {
        let path = args.value_of("CONFIG_PATH").map(Path::new);
        config::load(path)?
    };

    let decoded_password = config.user.decode_password()?;

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
        config.save()?;
    }

    Ok(mal)
}
