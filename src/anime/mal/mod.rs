extern crate rquery;
extern crate hyper;

mod request;

use self::hyper::status::StatusCode;
use self::request::RequestType::*;
use self::rquery::Document;

error_chain! {
    links {
        Request(request::Error, request::ErrorKind);
    }

    foreign_links {
        ConvertInt(::std::num::ParseIntError);
    }

    errors {
        DocumentError {
            description("malformed XML document")
            display("provided XML document was invalid")
        }

        ParseError {
            description("XML parse error")
            display("failed to parse XML data")
        }

        NotFound {
            description("specified anime not found")
            display("unable to find information for specified anime")
        }
    }
}

#[derive(Debug)]
pub struct AnimeInfo {
    pub id:       u32,
    pub name:     String,
    pub episodes: u32,
}

impl AnimeInfo {
    pub fn request(name: &str, username: String, password: String) -> Result<Vec<AnimeInfo>> {
        let req = match request::execute(Search(name.into()), username, Some(password)) {
            Ok(req) => req,
            Err(request::Error(request::ErrorKind::BadStatus(StatusCode::NoContent), _)) => {
                bail!(ErrorKind::NotFound)
            },
            Err(e) => bail!(e),
        };

        let doc = Document::new_from_xml_stream(req)
                    .map_err(|_| ErrorKind::DocumentError)?;

        let mut entries = Vec::new();

        for entry in doc.select_all("entry").map_err(|_| ErrorKind::ParseError)? {
            let select = |n| entry.select(n).map_err(|_| ErrorKind::ParseError);
            
            entries.push(AnimeInfo {
                id:       select("id")?.text().parse()?,
                name:     select("title")?.text().clone(),
                episodes: select("episodes")?.text().parse()?,
            });
        }

        Ok(entries)
    }

    pub fn request_watched(&self, username: String) -> Result<u32> {
        let req = request::execute(AnimeList(username.clone()), username, None)?;
        let doc = Document::new_from_xml_stream(req)
                    .map_err(|_| ErrorKind::DocumentError)?;

        for entry in doc.select_all("anime").map_err(|_| ErrorKind::ParseError)? {
            let id = entry.select("series_animedb_id")
                          .map_err(|_| ErrorKind::ParseError)?
                          .text()
                          .parse::<u32>()?;

            if id == self.id {
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
}