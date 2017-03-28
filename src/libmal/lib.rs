#[macro_use] extern crate error_chain;
extern crate rquery;
extern crate hyper;

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

        NotFound {
            description("specified anime not found")
            display("unable to find information for specified anime")
        }
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct SearchInfo {
    pub id:       u32,
    pub name:     String,
    pub episodes: u32,
}

pub fn find(name: &str, username: String, password: String) -> Result<Vec<SearchInfo>> {
    use request::RequestType::Find;
    
    let req = match request::get(Find(name.into()), username, Some(password)) {
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
        
        entries.push(SearchInfo {
            id:       select("id")?.text().parse()?,
            name:     select("title")?.text().clone(),
            episodes: select("episodes")?.text().parse()?,
        });
    }

    Ok(entries)
}