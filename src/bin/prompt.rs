use chrono::{Local, NaiveDate};
use failure::{Error, ResultExt};
use input::{self, Answer};
use mal::{SeriesInfo, MAL};
use mal::list::{AnimeEntry, EntryTag, Status};
use Series;
use std;

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

fn get_today_naive() -> NaiveDate {
    Local::today().naive_utc()
}
