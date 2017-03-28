#[macro_use] extern crate lazy_static;
#[macro_use] extern crate error_chain;
#[macro_use] extern crate clap;
extern crate mal;
extern crate chrono;

mod anime;

use std::env;
use std::io;
use std::path::{Path, PathBuf};
use anime::LocalAnime;
use mal::{Auth, Status, AnimeInfo};
use mal::list::Action;
use mal::list::Tag::*;
use chrono::Local;
use clap::ArgMatches;

error_chain! {
    links {
        Anime(anime::Error, anime::ErrorKind);
        MAL(mal::Error, mal::ErrorKind);
        MALList(mal::list::Error, mal::list::ErrorKind);
    }

    foreign_links {
        Io(std::io::Error);
        ParseInt(std::num::ParseIntError);
    }

    errors {
        NoneFound(name: String) {
            description("no anime found")
            display("unable to find [{}] on MAL", name)
        }
    }
}

fn read_line() -> Result<String> {
    let mut buffer = String::new();
    io::stdin().read_line(&mut buffer)?;

    Ok(buffer[..buffer.len() - 1].to_string())
}

fn read_int(min: i32, max: i32) -> Result<i32> {
    let mut input = read_line()?.parse()?;

    while input < min || input > max {
        println!("input must be between {}-{}", min, max);
        input = read_line()?.parse()?;
    }

    Ok(input)
}

fn get_anime_selection(local: &LocalAnime, auth: &Auth) -> Result<AnimeInfo> {
    let found = mal::find(&local.name, &auth)?;

    if found.len() > 1 {
        println!("\nmultiple anime on MAL found");
        println!("input the number corrosponding with the intended anime:");

        for (i, info) in found.iter().enumerate() {
            println!("\t{} [{}]", i + 1, info.name);
        }

        let index = read_int(0, found.len() as i32)? - 1;

        Ok(found[index as usize].clone())
    } else if found.len() == 0 {
        bail!(ErrorKind::NoneFound(local.name.clone()))
    } else {
        Ok(found[0].clone())
    }
}

fn get_from_list(info: &AnimeInfo, auth: &Auth) -> Result<mal::list::Entry> {
    let list  = mal::list::get_entries(auth.username.clone())?;
    let entry = list.iter().find(|a| a.info.id == info.id);

    match entry {
        Some(data) => Ok(data.clone()),
        None => {
            println!("\n[{}] not on anime list\nwould you like to add it? (Y/n)", &info.name);
            let input = read_line()?.to_lowercase();

            if input == "n" {
                bail!("specified anime must be on list to continue")
            } else {
                mal::list::modify(info.id, Action::Add, &auth, &[
                    Episode(0),
                    Status(Status::Watching),
                    StartDate(Local::now().date()),
                ])?;

                Ok(mal::list::Entry {
                    info: info.clone(),
                    watched: 0,
                    status: Status::Watching,
                })
            }
        },
    }
}

fn increment_ep_count(entry: &mal::list::Entry, auth: &Auth) -> Result<Status> {
    let new_ep_count = entry.watched + 1;
    let mut tags     = vec![Episode(new_ep_count)];

    let new_status = if new_ep_count == entry.info.episodes {
        tags.push(FinishDate(Local::now().date()));
        Status::Completed
    } else {
        Status::Watching
    };

    tags.push(Status(new_status));
    mal::list::modify(entry.info.id, Action::Update, &auth, tags.as_slice())?;

    Ok(new_status)
}

fn play_next_episode(path: &Path, auth: Auth) -> Result<()> {
    let local = LocalAnime::find(path)?;
    println!("[{}] identified", &local.name);

    let mal_info   = get_anime_selection(&local, &auth)?;
    let list_entry = get_from_list(&mal_info, &auth)?;
    
    // TODO: Launch video player

    let new_status = increment_ep_count(&list_entry, &auth)?;

    match new_status {
        Status::Completed => {
            println!("[{}] completed!\nwould you like to rate it? (Y/n)", &mal_info.name);
            let input = read_line()?.to_lowercase();

            if input != "n" {
                println!("\nenter a score between 1-10:");
                let score = read_int(1, 10)? as u8;

                mal::list::modify(mal_info.id, Action::Update, &auth, &[
                    Score(score),
                ])?;
            }
        },
        _ => {
            println!("[{}] episode {}/{} completed",
                mal_info.name,
                list_entry.watched + 1,
                mal_info.episodes);
        },
    }

    Ok(())
}

fn run(args: ArgMatches) -> Result<()> {
    let path = if let Some(path) = args.value_of("PATH") {
        PathBuf::from(path)
    } else {
        env::current_dir()?
    };

    // Temporary
    if args.is_present("next") {
        let auth = Auth {
            username: args.value_of("username").unwrap().into(),
            password: args.value_of("password").unwrap().into(),
        };

        play_next_episode(&path, auth)?;
    }

    Ok(())
}

fn main() {
    let args = clap_app!(anitrack =>
        (@arg PATH: "Sets the directory to look for episodes in")
        (@arg username: -u +required +takes_value "Specifies the account name to use for MAL")
        (@arg password: -p +required +takes_value "Specifies the password for the specified account name")
        (@subcommand next =>
            (about: "Plays the next episode from the specified or current directory")
        )
    ).get_matches();

    match run(args) {
        Ok(_) => (),
        Err(e) => panic!("error: {}", e),
    }
}
