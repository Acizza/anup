use chrono::NaiveDate;
use error::PromptError;
use get_today;
use input::{self, Answer};
use mal::list::{List, Status};
use mal::list::anime::AnimeEntry;
use std;

fn prompt_to_add_finish_date(entry: &mut AnimeEntry, date: NaiveDate) -> Result<(), PromptError> {
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

pub fn next_episode_options(
    list: &List<AnimeEntry>,
    entry: &mut AnimeEntry,
) -> Result<(), PromptError> {
    println!("options:");
    println!("\t[d] drop series\n\t[h] put series on hold\n\t[r] rate series\n\t[x] exit\n\t[n] watch next episode (default)");

    let input = input::read_line()?.to_lowercase();

    match input.as_str() {
        "d" => {
            entry.values.set_status(Status::Dropped);
            prompt_to_add_finish_date(entry, get_today())?;

            list.update(entry)?;

            std::process::exit(0);
        }
        "h" => {
            entry.values.set_status(Status::OnHold);
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
