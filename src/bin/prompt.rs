use chrono::{Local, NaiveDate};
use failure::{Error, ResultExt};
use input::{self, Answer};
use mal::{SeriesInfo, MAL};
use mal::list::{AnimeEntry, EntryTag, Status};
use std;

fn get_today_naive() -> NaiveDate {
    Local::today().naive_utc()
}

pub fn find_and_select_series_info(mal: &MAL, name: &str) -> Result<SeriesInfo, Error> {
    let mut series = mal.search(name).context("MAL search failed")?;

    if series.len() == 0 {
        bail!("no anime named [{}] found", name);
    } else if series.len() > 1 {
        println!("found multiple anime named [{}] on MAL", name);
        println!("input the number corrosponding with the intended anime:\n");

        for (i, s) in series.iter().enumerate() {
            println!("{} [{}]", 1 + i, s.title);
        }

        let idx = input::read_usize_range(1, series.len())? - 1;
        Ok(series.swap_remove(idx))
    } else {
        Ok(series.swap_remove(0))
    }
}

pub fn add_to_anime_list(mal: &MAL, info: &SeriesInfo) -> Result<AnimeEntry, Error> {
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

        Ok(AnimeEntry {
            info: info.clone(),
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
    entry: &AnimeEntry,
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

fn completed(mal: &MAL, entry: &mut AnimeEntry, mut tags: Vec<EntryTag>) -> Result<(), Error> {
    let today = get_today_naive();

    tags.push(EntryTag::Status(Status::Completed));

    println!(
        "[{}] completed!\ndo you want to rate it? (Y/n)",
        entry.info.title
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

    mal.update_anime(entry.info.id, &tags)?;
    // Nothing to do now
    std::process::exit(0);
}

pub fn update_watched(mal: &MAL, entry: &mut AnimeEntry) -> Result<(), Error> {
    let mut tags = vec![EntryTag::Episode(entry.watched_episodes)];

    if entry.watched_episodes >= entry.info.episodes {
        completed(mal, entry, tags)?;
    } else {
        println!(
            "[{}] episode {}/{} completed",
            entry.info.title,
            entry.watched_episodes,
            entry.info.episodes
        );

        if !entry.rewatching {
            tags.push(EntryTag::Status(Status::Watching));

            if entry.watched_episodes <= 1 {
                tags.push(EntryTag::StartDate(Some(get_today_naive())));
            }
        }

        entry.sync_tags(&tags);
        mal.update_anime(entry.info.id, &tags)?;
    }

    Ok(())
}

pub fn next_episode_options(mal: &MAL, entry: &AnimeEntry) -> Result<(), Error> {
    println!("\t[d] drop series\n\t[h] put series on hold\n\t[x] exit\n\t[n] watch next episode (default)");

    let input = input::read_line()?.to_lowercase();

    match input.as_str() {
        "d" => {
            let mut tags = vec![EntryTag::Status(Status::Dropped)];
            add_finish_date(entry, get_today_naive(), &mut tags)?;

            mal.update_anime(entry.info.id, &tags)?;
            std::process::exit(0);
        },
        "h" => {
            mal.update_anime(entry.info.id, &[EntryTag::Status(Status::OnHold)])?;
            std::process::exit(0);
        },
        "x" => std::process::exit(0),
        _ => (),
    }

    Ok(())
}

pub fn abnormal_player_exit(mal: &MAL, entry: &mut AnimeEntry) -> Result<(), Error> {
    println!("video player not exited normally");
    println!("do you still want to count the episode as watched? (y/N)");

    if input::read_yn(Answer::No)? {
        update_watched(mal, entry)?;
    }

    Ok(())
}

pub fn rewatch(mal: &MAL, entry: &mut AnimeEntry) -> Result<(), Error> {
    println!("[{}] already completed", entry.info.title);
    println!("do you want to rewatch it? (Y/n)");
    println!("(note that you have to increase the rewatch count manually)");

    if input::read_yn(Answer::Yes)? {
        let mut tags = vec![EntryTag::Rewatching(true), EntryTag::Episode(0)];

        println!("do you want to reset the start and end date? (Y/n)");

        if input::read_yn(Answer::Yes)? {
            tags.push(EntryTag::StartDate(Some(get_today_naive())));
            tags.push(EntryTag::FinishDate(None));
        }

        entry.sync_tags(&tags);
        mal.update_anime(entry.info.id, &tags)?;
    } else {
        // No point in continuing in this case
        std::process::exit(0);
    }

    Ok(())
}
