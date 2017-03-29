#[macro_use] extern crate lazy_static;
#[macro_use] extern crate error_chain;
#[macro_use] extern crate clap;
extern crate mal;

mod anime;
mod input;

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use anime::LocalAnime;
use clap::ArgMatches;
use input::DefAnswer;
use mal::{Auth, Status, AnimeInfo};
use mal::list;

error_chain! {
    links {
        Anime(anime::Error, anime::ErrorKind);
        Input(input::Error, input::ErrorKind);
        MAL(mal::Error, mal::ErrorKind);
        MALList(list::Error, list::ErrorKind);
    }

    foreign_links {
        Io(std::io::Error);
    }

    errors {
        NoneFound(name: String) {
            description("no anime found")
            display("unable to find [{}] on MAL", name)
        }

        Exit {
            description("")
            display("")
        }
    }
}

fn get_anime_selection(local: &LocalAnime, auth: &Auth) -> Result<AnimeInfo> {
    let found = mal::find(&local.name, &auth)?;

    if found.len() > 1 {
        println!("\nmultiple anime on MAL found");
        println!("input the number corrosponding with the intended anime:");

        for (i, info) in found.iter().enumerate() {
            println!("\t{} [{}]", i + 1, info.name);
        }

        let index = input::read_int(0, found.len() as i32)? - 1;

        Ok(found[index as usize].clone())
    } else if found.len() == 0 {
        bail!(ErrorKind::NoneFound(local.name.clone()))
    } else {
        Ok(found[0].clone())
    }
}

fn get_from_list(info: &AnimeInfo, auth: &Auth) -> Result<list::Entry> {
    let list  = list::get_entries(auth.username.clone())?;
    let entry = list.iter().find(|a| a.info.id == info.id);

    match entry {
        Some(entry) => {
            match entry.status {
                Status::Completed if !entry.rewatching => {
                    println!("[{}] already completed", info.name);
                    println!("\nwould you like to rewatch it? (Y/n)");
                    println!("(note that you'll need to increase the rewatch count manually)");

                    if input::read_yn(DefAnswer::Yes)? {
                        let mut entry = entry.clone();
                        entry.start_rewatch(&auth)?;

                        Ok(entry)
                    } else {
                        bail!(ErrorKind::Exit)
                    }
                },
                _ => Ok(entry.clone()),
            }
        },
        None => {
            println!("\n[{}] not on anime list\nwould you like to add it? (Y/n)", &info.name);

            if input::read_yn(DefAnswer::Yes)? {
                Ok(list::add_to_watching(&info, &auth)?)
            } else {
                bail!(ErrorKind::Exit)
            }
        },
    }
}

fn set_watched(ep_count: u32, entry: &mut list::Entry, auth: &Auth) -> Result<()> {
    entry.update_watched(ep_count, &auth)?;

    match entry.status {
        Status::Completed => {
            println!("[{}] completed!\nwould you like to rate it? (Y/n)", &entry.info.name);

            if input::read_yn(DefAnswer::Yes)? {
                println!("\nenter a score between 1-10:");
                let score = input::read_int(1, 10)? as u8;

                entry.set_score(score, &auth)?;
            }
        },
        _ => {
            println!("[{}] episode {}/{} completed",
                entry.info.name,
                ep_count,
                entry.info.episodes);
        },
    }

    Ok(())
}

fn play_next_episode(path: &Path, auth: Auth) -> Result<()> {
    let local = LocalAnime::find(path)?;

    println!("[{}] identified", &local.name);

    let mal_info       = get_anime_selection(&local, &auth)?;
    let mut list_entry = get_from_list(&mal_info, &auth)?;

    let next_episode = list_entry.watched + 1;

    let output = Command::new("/usr/bin/xdg-open")
                        .arg(local.get_episode(next_episode)?)
                        .output()?;

    if output.status.success() {
        set_watched(next_episode, &mut list_entry, &auth)?;
    } else {
        println!("video player not exited normally");
        println!("would you still like to count the episode as watched? (y/N)");

        if input::read_yn(DefAnswer::No)? {
            set_watched(next_episode, &mut list_entry, &auth)?;
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
        Err(Error(ErrorKind::Exit, _)) => (),
        Err(e) => println!("error: {}", e),
    }
}
