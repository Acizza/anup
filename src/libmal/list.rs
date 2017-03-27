use request;
use request::RequestType::*;
use rquery::Document;
use super::{ErrorKind, Result};

pub fn get_watched_episodes(id: u32, username: String) -> Result<u32> {
    let req = request::execute(AnimeList(username.clone()), username, None)?;
    let doc = Document::new_from_xml_stream(req)
                .map_err(|_| ErrorKind::DocumentError)?;

    for entry in doc.select_all("anime").map_err(|_| ErrorKind::ParseError)? {
        let entry_id = entry.select("series_animedb_id")
                        .map_err(|_| ErrorKind::ParseError)?
                        .text()
                        .parse::<u32>()?;

        if entry_id == id {
            let watched = entry
                .select("my_watched_episodes")
                .map_err(|_| ErrorKind::ParseError)?
                .text()
                .parse()?;
            
            return Ok(watched)
        }
    }

    Ok(0)
}