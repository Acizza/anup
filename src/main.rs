#[cfg(windows)]
extern crate winapi;

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
extern crate toml;

mod config;
mod error;
mod input;
mod process;
mod series;

use config::Config;
use error::Error;
use mal::MAL;
use series::Series;
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
    let args = clap_app!(anitrack =>
        (version: env!("CARGO_PKG_VERSION"))
        (author: env!("CARGO_PKG_AUTHORS"))
        (@arg NAME: "The name of the series to watch")
        (@arg PATH: -p --path +takes_value "Specifies the directory to look for video files in")
        (@arg SEASON: -s --season +takes_value "Specifies which season you want to watch")
        (@arg LIST: -l --list "Displays all saved series")
        (@arg DONT_SAVE_PASS: --dontsavepass "Disables saving of your account password")
    ).get_matches();

    if args.is_present("LIST") {
        return print_series_list();
    }

    watch_series(&args)
}

fn watch_series(args: &clap::ArgMatches) -> Result<(), Error> {
    let mut config = config::load()?;
    config.remove_invalid_series();

    let path = get_series_path(&mut config, args)?;
    let mal = init_mal_client(args, &mut config)?;

    config.save(!args.is_present("DONT_SAVE_PASS"))?;

    let season = args.value_of("SEASON")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    let mut series = Series::from_dir(&path, &mal)?;
    series.load_season(season)?.play_all_episodes()?;

    Ok(())
}

fn print_series_list() -> Result<(), Error> {
    let config = config::load()?;

    println!("{} series found", config.series.len());
    println!(
        "note that any series marked as invalid will be removed the next time you watch a series"
    );
    println!();

    for (name, path) in config.series {
        let status_str = if path.exists() { "valid" } else { "invalid" };
        println!("[{}] {}: {}", status_str, name, path.to_string_lossy());
    }

    Ok(())
}

fn get_series_path(config: &mut Config, args: &clap::ArgMatches) -> Result<PathBuf, Error> {
    match args.value_of("PATH") {
        Some(path) => {
            if let Some(series_name) = args.value_of("NAME") {
                config.series.insert(series_name.into(), path.into());
            }

            Ok(path.into())
        }
        None => {
            let name = args.value_of("NAME").ok_or(Error::NoSeriesInfoProvided)?;

            config
                .series
                .get(name)
                .ok_or_else(|| Error::SeriesNotFound(name.into()))
                .map(|path| path.into())
        }
    }
}

fn init_mal_client<'a>(args: &clap::ArgMatches, config: &mut Config) -> Result<MAL<'a>, Error> {
    let mut mal = {
        let decoded_password = config.user.decode_password()?;
        MAL::new(config.user.name.clone(), decoded_password)
    };

    let mut password_changed = false;

    while !mal.verify_credentials()? {
        println!(
            "invalid password for [{}], please try again:",
            config.user.name
        );

        mal.password = input::read_line()?;
        password_changed = true;
    }

    if !args.is_present("DONT_SAVE_CONFIG") && password_changed {
        config.user.encode_password(&mal.password);
    }

    Ok(mal)
}
