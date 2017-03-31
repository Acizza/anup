#[macro_use] extern crate error_chain;
extern crate chrono;
extern crate hyper;
extern crate rquery;

mod request;
pub mod list;

use self::hyper::status::StatusCode;
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

        NotFound(name: String) {
            description("no anime found")
            display("[{}] not found on MAL", name)
        }
    }
}

#[derive(Debug)]
pub struct Auth {
    pub username: String,
    pub password: String,
}

impl Auth {
    pub fn new(username: String, password: String) -> Auth {
        Auth {
            username: username,
            password: password,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Status {
    Watching = 1,
    Completed,
    OnHold,
    Dropped,
    PlanToWatch = 6,
}

impl Status {
    pub fn parse(id: u32) -> Option<Status> {
        use Status::*;

        match id {
            1 => Some(Watching),
            2 => Some(Completed),
            3 => Some(OnHold),
            4 => Some(Dropped),
            6 => Some(PlanToWatch),
            _ => None
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnimeInfo {
    pub id:       u32,
    pub name:     String,
    pub episodes: u32,
}

pub fn find(name: &str, auth: &Auth) -> Result<Vec<AnimeInfo>> {
    use request::RequestType::Find;
    
    let req = match request::get(Find(name.into()), Some(&auth)) {
        Ok(req) => req,
        Err(request::Error(request::ErrorKind::BadStatus(StatusCode::NoContent), _)) => {
            bail!(ErrorKind::NotFound(name.into()))
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

pub fn find_by_id(name: &str, id: u32, auth: &Auth) -> Result<AnimeInfo> {
    find(name, auth)?
        .into_iter()
        .find(|i| i.id == id)
        .ok_or(ErrorKind::NotFound(name.into()).into())
}