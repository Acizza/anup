use chrono::NaiveDate;
use failure::{Error, ResultExt};
use get_today;
use input::{self, Answer};
use mal::{AnimeInfo, MAL};
use mal::list::List;
use mal::list::anime::{AnimeEntry, WatchStatus};
use std;

pub struct SearchResult {
    pub info: AnimeInfo,
    pub search_term: String,
}

impl SearchResult {
    pub fn new<S: Into<String>>(info: AnimeInfo, search_term: S) -> SearchResult {
        SearchResult {
            info,
            search_term: search_term.into(),
        }
    }
}

pub fn select_series_info(mal: &MAL, name: &str) -> Result<SearchResult, Error> {
    let mut series = mal.search_anime(name).context("MAL search failed")?;

    if !series.is_empty() {
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

            select_series_info(mal, &name)
        } else {
            Ok(SearchResult::new(series.swap_remove(index - 1), name))
        }
    } else {
        bail!("no anime named [{}] found", name);
    }
}

fn prompt_to_add_finish_date(entry: &mut AnimeEntry, date: NaiveDate) -> Result<(), Error> {
    // Someone may want to keep the original start / finish date for an
    // anime they're rewatching
    if entry.values.rewatching() && entry.values.finish_date().is_some() {
        println!("do you want to override the finish date? (Y/n)");

        if input::read_yn(Answer::Yes)? {
            entry.values.set_finish_date(Some(date));
        }
    } else {
        entry.values.set_finish_date(Some(date));
    }

    Ok(())
}

fn series_completed(list: &List<AnimeEntry>, entry: &mut AnimeEntry) -> Result<(), Error> {
    let today = get_today();
    entry.values.set_status(WatchStatus::Completed);

    println!(
        "[{}] completed!\ndo you want to rate it? (Y/n)",
        entry.series_info.title
    );

    if input::read_yn(Answer::Yes)? {
        println!("enter your score between 1-10:");
        let score = input::read_usize_range(1, 10)? as u8;

        entry.values.set_score(score);
    }

    if entry.values.rewatching() {
        entry.values.set_rewatching(false);
    }

    prompt_to_add_finish_date(entry, today)?;
    list.update(entry)?;

    // Nothing to do now
    std::process::exit(0);
}

pub fn update_watched_eps(list: &List<AnimeEntry>, entry: &mut AnimeEntry) -> Result<(), Error> {
    let watched = entry.values.watched_episodes();
    entry.values.set_watched_episodes(watched);

    if entry.values.watched_episodes() >= entry.series_info.episodes {
        series_completed(list, entry)?;
    } else {
        println!(
            "[{}] episode {}/{} completed",
            entry.series_info.title,
            entry.values.watched_episodes(),
            entry.series_info.episodes
        );

        if !entry.values.rewatching() {
            entry.values.set_status(WatchStatus::Watching);

            if entry.values.watched_episodes() <= 1 {
                entry.values.set_start_date(Some(get_today()));
            }
        }
    }

    Ok(())
}

pub fn next_episode_options(list: &List<AnimeEntry>, entry: &mut AnimeEntry) -> Result<(), Error> {
    println!("options:");
    println!("\t[d] drop series\n\t[h] put series on hold\n\t[r] rate series\n\t[x] exit\n\t[n] watch next episode (default)");

    let input = input::read_line()?.to_lowercase();

    match input.as_str() {
        "d" => {
            entry.values.set_status(WatchStatus::Dropped);
            prompt_to_add_finish_date(entry, get_today())?;

            list.update(entry)?;

            std::process::exit(0);
        }
        "h" => {
            entry.values.set_status(WatchStatus::OnHold);
            list.update(entry)?;

            std::process::exit(0);
        }
        "r" => {
            println!("enter your score between 1-10:");

            let score = input::read_usize_range(1, 10)? as u8;
            entry.values.set_score(score);

            list.update(entry)?;
            next_episode_options(list, entry)?;
        }
        "x" => std::process::exit(0),
        _ => (),
    }

    Ok(())
}

pub fn abnormal_player_exit(list: &List<AnimeEntry>, entry: &mut AnimeEntry) -> Result<(), Error> {
    println!("video player not exited normally");
    println!("do you still want to count the episode as watched? (y/N)");

    if input::read_yn(Answer::No)? {
        update_watched_eps(list, entry)?;
    }

    Ok(())
}

pub fn rewatch_series(list: &List<AnimeEntry>, entry: &mut AnimeEntry) -> Result<(), Error> {
    println!("[{}] already completed", entry.series_info.title);
    println!("do you want to rewatch it? (Y/n)");
    println!("(note that you have to increase the rewatch count manually)");

    if input::read_yn(Answer::Yes)? {
        entry.values.set_rewatching(true).set_watched_episodes(0);

        println!("do you want to reset the start and end date? (Y/n)");

        if input::read_yn(Answer::Yes)? {
            entry.values
                 .set_start_date(Some(get_today()))
                 .set_finish_date(None);
        }

        list.update(entry)?;
    } else {
        // No point in continuing in this case
        std::process::exit(0);
    }

    Ok(())
}
