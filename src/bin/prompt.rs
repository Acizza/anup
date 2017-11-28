use chrono::{Local, NaiveDate};
use failure::{Error, ResultExt};
use input::{self, Answer};
use mal::{SeriesInfo, MAL};
use mal::list::{AnimeEntry, EntryTag, Status};
use Series;
use std;

fn get_today_naive() -> NaiveDate {
    Local::today().naive_utc()
}

pub fn find_and_select_series(mal: &MAL, name: &str) -> Result<SeriesInfo, Error> {
    let mut series = mal.search(name).context("MAL search failed")?;

    if series.len() == 0 {
        return Err(format_err!("no anime named [{}] found", name));
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

pub fn add_to_anime_list(mal: &MAL, series: &Series) -> Result<AnimeEntry, Error> {
    println!(
        "[{}] is not on your anime list\ndo you want to add it? (Y/n)",
        series.info.title
    );

    if input::read_yn(Answer::Yes)? {
        let today = get_today_naive();

        mal.add_anime(
            series.info.id,
            &[
                EntryTag::Status(Status::Watching),
                EntryTag::StartDate(today),
            ],
        )?;

        Ok(AnimeEntry {
            info: series.info.clone(),
            watched_episodes: 0,
            start_date: Some(today),
            end_date: None,
            status: Status::Watching,
        })
    } else {
        // No point in continuing in this case
        std::process::exit(0);
    }
}

pub fn update_watched(mal: &MAL, entry: &mut AnimeEntry) -> Result<(), Error> {
    let mut tags = vec![EntryTag::Episode(entry.watched_episodes)];

    if entry.watched_episodes >= entry.info.episodes {
        let today = get_today_naive();

        tags.push(EntryTag::Status(Status::Completed));
        tags.push(EntryTag::FinishDate(today));

        entry.status = Status::Completed;
        entry.end_date = Some(today);

        println!(
            "[{}] completed!\ndo you want to rate it? (Y/n)",
            entry.info.title
        );

        if input::read_yn(Answer::Yes)? {
            println!("enter your score between 1-10:");
            let score = input::read_usize_range(1, 10)? as u8;

            tags.push(EntryTag::Score(score));
        }
    } else {
        println!(
            "[{}] episode {}/{} completed",
            entry.info.title,
            entry.watched_episodes,
            entry.info.episodes
        );
    }

    mal.update_anime(entry.info.id, &tags)?;
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
