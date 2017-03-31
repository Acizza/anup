use input::Answer;
use mal::{self, Auth, list};
use input;

error_chain! {
    links {
        Input(input::Error, input::ErrorKind);
        MALList(list::Error, list::ErrorKind);
    }

    errors {
        Exit {
            description("")
            display("")
        }
    }
}

pub fn select_found_anime(found: &[mal::AnimeInfo]) -> Result<mal::AnimeInfo> {
    if found.len() > 1 {
        println!("\nmultiple anime on MAL found");
        println!("input the number corrosponding with the intended anime:");

        for (i, info) in found.iter().enumerate() {
            println!("\t{} [{}]", i + 1, info.name);
        }

        let index = input::read_int(0, found.len() as i32)? - 1;

        Ok(found[index as usize].clone())
    } else {
        Ok(found[0].clone())
    }
}

pub fn rewatch(entry: &mut list::Entry, auth: &Auth) -> Result<()> {
    println!("[{}] already completed", entry.info.name);
    println!("\nwould you like to rewatch it? (Y/n)");
    println!("(note that you'll need to increase the rewatch count manually)");

    if input::read_yn(Answer::Yes)? {
        entry.start_rewatch(&auth)?;
        Ok(())
    } else {
        bail!(ErrorKind::Exit)
    }
}

pub fn add_to_list(info: &mal::AnimeInfo, auth: &Auth) -> Result<list::Entry> {
    println!("\n[{}] not on anime list\nwould you like to add it? (Y/n)", &info.name);

    if input::read_yn(Answer::Yes)? {
        Ok(list::add_to_watching(&info, &auth)?)
    } else {
        bail!(ErrorKind::Exit)
    }
}

pub fn completed(entry: &mut list::Entry, auth: &Auth) -> Result<()> {
    println!("[{}] completed!\nwould you like to rate it? (Y/n)", &entry.info.name);

    if input::read_yn(Answer::Yes)? {
        println!("\nenter a score between 1-10:");
        let score = input::read_int(1, 10)? as u8;

        entry.set_score(score, &auth)?;
    }

    Ok(())
}