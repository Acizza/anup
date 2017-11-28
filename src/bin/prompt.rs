use chrono::{Local, NaiveDate};
use failure::{Error, ResultExt};
use mal::{SeriesInfo, MAL};
use mal::list::{AnimeEntry, EntryTag, Status};
use Series;
use std;

mod input {
    use failure::Error;
    use std::io;

    pub fn read_line() -> Result<String, Error> {
        let mut buffer = String::new();
        io::stdin().read_line(&mut buffer)?;

        Ok(buffer[..buffer.len() - 1].to_string())
    }

    pub fn read_usize_range(min: usize, max: usize) -> Result<usize, Error> {
        loop {
            let input = read_line()?.parse()?;

            if input >= min && input <= max {
                return Ok(input);
            } else {
                println!("input must be between {}-{}", min, max);
            }
        }
    }

    #[derive(Debug)]
    pub enum Answer {
        Yes,
        No,
    }

    impl Into<bool> for Answer {
        fn into(self) -> bool {
            match self {
                Answer::Yes => true,
                Answer::No => false,
            }
        }
    }

    pub fn read_yn(default: Answer) -> Result<bool, Error> {
        let line = read_line()?;

        let answer = match line.as_str() {
            "Y" | "y" => true,
            "N" | "n" => false,
            _ => default.into(),
        };

        Ok(answer)
    }
}

use self::input::Answer;

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
