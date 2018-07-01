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
#[macro_use]
extern crate serde_json;

extern crate base64;
extern crate chrono;
extern crate directories;
extern crate regex;
extern crate reqwest;
extern crate serde;
extern crate toml;

mod backend;
mod config;
mod error;
mod input;
mod process;
mod series;

use backend::{anilist::AniList, SyncBackend};
use config::Config;
use error::{Error, SeriesError};
use series::{EpisodeData, SaveData, Series};
use std::path::PathBuf;

fn main() {
    match run() {
        Ok(_) => (),
        Err(Error::Series(SeriesError::RequestExit)) => (),
        Err(e) => {
            let e: failure::Error = e.into();
            eprintln!("fatal error: {}", e.cause());

            for cause in e.causes().skip(1) {
                eprintln!("cause: {}", cause);
            }

            eprintln!("{}", e.backtrace());
            std::process::exit(1);
        }
    }
}

fn run() -> Result<(), Error> {
    let args = clap_app!(anup =>
        (version: env!("CARGO_PKG_VERSION"))
        (author: env!("CARGO_PKG_AUTHORS"))
        (@arg NAME: "The name of the series to watch")
        (@arg PATH: -p --path +takes_value "Specifies the directory to look for video files in")
        (@arg SEASON: -s --season +takes_value "Specifies which season you want to watch")
        (@arg INFO: -i --info "Displays saved series information")
    ).get_matches();

    if args.is_present("INFO") {
        print_saved_series_info()
    } else {
        watch_series(&args)
    }
}

fn watch_series(args: &clap::ArgMatches) -> Result<(), Error> {
    let mut config = Config::load()?;
    config.remove_invalid_series();

    let path = get_series_path(&mut config, args)?;
    let sync_backend = AniList::init(&mut config)?;

    config.save()?;

    let season = {
        let value: u32 = args
            .value_of("SEASON")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        value.saturating_sub(1)
    };

    Series::load(&path, sync_backend)?
        .load_season(season)?
        .play_all_episodes()?;

    Ok(())
}

fn print_saved_series_info() -> Result<(), Error> {
    let mut config = Config::load()?;
    config.remove_invalid_series();

    println!("found [{}] series:\n", config.series.len());

    for (name, path) in config.series {
        let ep_matcher = match SaveData::from_path(&path) {
            Ok(save_data) => save_data.episode_matcher,
            _ => None,
        };

        let ep_data = match EpisodeData::parse_dir(&path, ep_matcher) {
            Ok(ep_data) => ep_data,
            Err(err) => {
                eprintln!("failed to parse episode data for [{}]: {}", name, err);
                continue;
            }
        };

        let mut ep_nums = ep_data.episodes.keys().cloned().collect::<Vec<_>>();
        ep_nums.sort_unstable();

        let ep_list =
            concat_sequential_values(&ep_nums, "..", " | ").unwrap_or_else(|| "none".into());

        println!("[{}]:", name);
        println!(
            "  path: {}\n  episodes on disk: {}",
            path.to_string_lossy(),
            ep_list
        );
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

fn concat_sequential_values(list: &[u32], group_delim: &str, space_delim: &str) -> Option<String> {
    match list.len() {
        0 => return None,
        1 => return Some(list[0].to_string()),
        _ => (),
    }

    let mut concat_str = list[0].to_string();
    let mut group_start_val = list[0];
    let mut prev_value = list[0];

    for &value in list {
        // Check for a nonsequential jump
        if (value as i32 - prev_value as i32).abs() > 1 {
            // Extend the current group with a range if there's a big enough gap between the current value
            // and start value of the group
            if (value as i32 - group_start_val as i32).abs() > 2 {
                concat_str.push_str(group_delim);
                concat_str.push_str(&prev_value.to_string());
            }

            // Form a new group
            concat_str.push_str(space_delim);
            concat_str.push_str(&value.to_string());

            group_start_val = value;
        }

        prev_value = value;
    }

    let last_item = list[list.len() - 1];

    // Finish off the last list item with a range if it extends beyond the start value of the current group
    if group_start_val != last_item {
        concat_str.push_str(group_delim);
        concat_str.push_str(&last_item.to_string());
    }

    Some(concat_str)
}
