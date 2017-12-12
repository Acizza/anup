use chrono::{Local, NaiveDate};
use failure::{Error, ResultExt};
use input::{self, Answer};
use mal::{SeriesInfo, MAL};
use mal::list::{EntryInfo, EntryUpdate, EntryTag, Status};
use std;

// This code will be refactored and cleaned up soon™

fn get_today_naive() -> NaiveDate {
    Local::today().naive_utc()
}

#[derive(Debug)]
pub struct SearchResult {
    pub info: SeriesInfo,
    pub search_term: String,
}

impl SearchResult {
    pub fn new<S: Into<String>>(info: SeriesInfo, search_term: S) -> SearchResult {
        SearchResult {
            info,
            search_term: search_term.into(),
        }
    }
}

pub fn find_and_select_series_info(mal: &MAL, name: &str) -> Result<SearchResult, Error> {
    let mut series = mal.search(name).context("MAL search failed")?;

    if series.len() > 0 {
        println!("MAL results for [{}]:", name);
        println!("enter the number next to the desired series:\n");

        println!("0 [custom search]");

        for (i, s) in series.iter().enumerate() {
            println!("{} [{}]", 1 + i, s.title);
        }

        let index = input::read_usize_range(0, series.len())?;

        if index == 0 {
            println!("enter the name you want to search for:");
            let name = input::read_line()?;

            find_and_select_series_info(mal, &name)
        } else {
            Ok(SearchResult::new(series.swap_remove(index - 1), name))
        }
    } else {
        bail!("no anime named [{}] found", name);
    }
}

pub fn add_to_anime_list(mal: &MAL, info: &SeriesInfo) -> Result<EntryInfo, Error> {
    println!(
        "[{}] is not on your anime list\ndo you want to add it? (Y/n)",
        info.title
    );

    if input::read_yn(Answer::Yes)? {
        let today = get_today_naive();

        mal.add_anime(
            info.id,
            &[
                EntryTag::Status(Status::Watching),
                EntryTag::StartDate(Some(today)),
            ],
        )?;

        Ok(EntryInfo {
            series: info.clone(),
            watched_episodes: 0,
            start_date: Some(today),
            end_date: None,
            status: Status::Watching,
            score: 0,
            rewatching: false,
        })
    } else {
        // No point in continuing in this case
        std::process::exit(0);
    }
}

/// Adds the `FinishDate` tag to the `tags` parameter.
/// If the `entry` is being rewatched, it will ask the user before adding the tag.
fn add_finish_date(
    entry: &EntryInfo,
    date: NaiveDate,
    tags: &mut Vec<EntryTag>,
) -> Result<(), Error> {
    // Someone may want to keep the original start / finish date for an
    // anime they're rewatching
    if entry.rewatching && entry.end_date.is_some() {
        println!("do you want to override the finish date? (Y/n)");

        if input::read_yn(Answer::Yes)? {
            tags.push(EntryTag::FinishDate(Some(date)));
        }
    } else {
        tags.push(EntryTag::FinishDate(Some(date)));
    }

    Ok(())
}

fn completed(mal: &MAL, entry: &mut EntryInfo, mut tags: Vec<EntryTag>) -> Result<(), Error> {
    let today = get_today_naive();

    tags.push(EntryTag::Status(Status::Completed));

    println!(
        "[{}] completed!\ndo you want to rate it? (Y/n)",
        entry.series.title
    );

    if input::read_yn(Answer::Yes)? {
        println!("enter your score between 1-10:");
        let score = input::read_usize_range(1, 10)? as u8;

        tags.push(EntryTag::Score(score));
    }

    if entry.rewatching {
        tags.push(EntryTag::Rewatching(false));
    }

    add_finish_date(entry, today, &mut tags)?;

    mal.update_anime(entry.series.id, &tags)?;
    // Nothing to do now
    std::process::exit(0);
}

pub fn update_watched(mal: &MAL, entry: &mut EntryInfo) -> Result<(), Error> {
    let mut tags = vec![EntryTag::Episode(entry.watched_episodes)];

    if entry.watched_episodes >= entry.series.episodes {
        completed(mal, entry, tags)?;
    } else {
        println!(
            "[{}] episode {}/{} completed",
            entry.series.title,
            entry.watched_episodes,
            entry.series.episodes
        );

        if !entry.rewatching {
            tags.push(EntryTag::Status(Status::Watching));

            if entry.watched_episodes <= 1 {
                tags.push(EntryTag::StartDate(Some(get_today_naive())));
            }
        }

        entry.sync_from_tags(&tags);
        mal.update_anime(entry.series.id, &tags)?;
    }

    Ok(())
}

pub fn next_episode_options(mal: &MAL, entry: &mut EntryInfo) -> Result<(), Error> {
    println!("options:");
    println!("\t[d] drop series\n\t[h] put series on hold\n\t[r] rate series\n\t[x] exit\n\t[n] watch next episode (default)");

    let input = input::read_line()?.to_lowercase();

    match input.as_str() {
        "d" => {
            let mut tags = vec![EntryTag::Status(Status::Dropped)];
            add_finish_date(entry, get_today_naive(), &mut tags)?;

            mal.update_anime(entry.series.id, &tags)?;
            std::process::exit(0);
        },
        "h" => {
            mal.update_anime(entry.series.id, &[EntryTag::Status(Status::OnHold)])?;
            std::process::exit(0);
        },
        "r" => {
            println!("enter your score between 1-10:");

            let score = input::read_usize_range(1, 10)? as u8;
            let tags = vec![EntryTag::Score(score)];

            entry.sync_from_tags(&tags);
            mal.update_anime(entry.series.id, &tags)?;

            next_episode_options(mal, entry)?;
        },
        "x" => std::process::exit(0),
        _ => (),
    }

    Ok(())
}

pub fn abnormal_player_exit(mal: &MAL, entry: &mut EntryInfo) -> Result<(), Error> {
    println!("video player not exited normally");
    println!("do you still want to count the episode as watched? (y/N)");

    if input::read_yn(Answer::No)? {
        update_watched(mal, entry)?;
    }

    Ok(())
}

pub fn rewatch(mal: &MAL, entry: &mut EntryInfo) -> Result<(), Error> {
    println!("[{}] already completed", entry.series.title);
    println!("do you want to rewatch it? (Y/n)");
    println!("(note that you have to increase the rewatch count manually)");

    if input::read_yn(Answer::Yes)? {
        let mut tags = vec![EntryTag::Rewatching(true), EntryTag::Episode(0)];

        println!("do you want to reset the start and end date? (Y/n)");

        if input::read_yn(Answer::Yes)? {
            tags.push(EntryTag::StartDate(Some(get_today_naive())));
            tags.push(EntryTag::FinishDate(None));
        }

        entry.sync_from_tags(&tags);
        mal.update_anime(entry.series.id, &tags)?;
    } else {
        // No point in continuing in this case
        std::process::exit(0);
    }

    Ok(())
}
