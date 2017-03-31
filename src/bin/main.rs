#[macro_use] extern crate lazy_static;
#[macro_use] extern crate error_chain;
#[macro_use] extern crate clap;
extern crate mal;

mod anime;
mod input;
mod prompt;

use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use anime::LocalAnime;
use clap::ArgMatches;
use input::Answer;
use mal::{Auth, Status, AnimeInfo};
use mal::list;

error_chain! {
    links {
        Anime(anime::Error, anime::ErrorKind);
        Input(input::Error, input::ErrorKind);
        MAL(mal::Error, mal::ErrorKind);
        MALList(list::Error, list::ErrorKind);
        Prompt(prompt::Error, prompt::ErrorKind);
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

fn get_mal_entry(info: &AnimeInfo, auth: &Auth) -> Result<list::Entry> {
    let list  = list::get_entries(auth.username.clone())?;
    let entry = list.iter().find(|a| a.info.id == info.id);

    match entry {
        Some(entry) => {
            match entry.status {
                Status::Completed if !entry.rewatching => {
                    let mut entry = entry.clone();
                    prompt::rewatch(&mut entry, &auth)?;
                    Ok(entry)
                },
                _ => Ok(entry.clone()),
            }
        },
        None => Ok(prompt::add_to_list(&info, &auth)?),
    }
}

fn set_watched(ep_count: u32, entry: &mut list::Entry, auth: &Auth) -> Result<()> {
    entry.set_watched(ep_count, &auth)?;

    match entry.status {
        Status::Completed => prompt::completed(entry, &auth)?,
        _ => {
            println!("[{}] episode {}/{} completed",
                entry.info.name,
                ep_count,
                entry.info.episodes);
        },
    }

    Ok(())
}

fn get_info_path(path: &Path) -> String {
    let mut path = PathBuf::from(path);
    path.push(format!(".{}", env!("CARGO_PKG_NAME")));
    path.to_str().unwrap().to_string()
}

fn save_id(path: &Path, info: &AnimeInfo) -> Result<()> {
    let mut file = File::create(get_info_path(path))?;
    write!(file, "{}", info.id)?;

    Ok(())
}

fn load_id(path: &Path) -> Result<u32> {
    let mut file = File::open(get_info_path(path))?;
    let mut buffer = String::new();

    file.read_to_string(&mut buffer)?;
    Ok(buffer.parse()?)
}

fn play_next_episode(path: &Path, auth: Auth) -> Result<()> {
    let local = LocalAnime::find(path)?;

    let mut list_entry = {
        let info = match load_id(path) {
            Ok(id) => {
                let info = mal::find_by_id(&local.name, id, &auth)?;
                println!("[{}] detected", info.name);

                info
            },
            Err(_) => {
                println!("[{}] identified", local.name);

                let found = mal::find(&local.name, &auth)?;
                let found = prompt::select_found_anime(&found)?;

                save_id(path, &found)?;
                found
            },
        };

        get_mal_entry(&info, &auth)?
    };

    loop {
        let next_episode = list_entry.watched + 1;
        let exit_status  = local.play_episode(next_episode)?;

        if exit_status.success() {
            set_watched(next_episode, &mut list_entry, &auth)?;
        } else {
            println!("video player not exited normally");
            println!("would you still like to count the episode as watched? (y/N)");

            if input::read_yn(Answer::No)? {
                set_watched(next_episode, &mut list_entry, &auth)?;
            }
        }

        println!("\ndo you want to watch the next episode? (Y/n)");

        if !input::read_yn(Answer::Yes)? {
            break
        }
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
        Err(Error(ErrorKind::Prompt(prompt::ErrorKind::Exit), _)) => (),
        Err(e) => println!("error: {}", e),
    }
}
