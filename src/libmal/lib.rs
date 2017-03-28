#[macro_use] extern crate error_chain;
extern crate rquery;
extern crate hyper;

mod request;
pub mod list;

use self::hyper::Url;
use self::hyper::status::StatusCode;
use self::rquery::Document;

pub const BASE_URL: &'static str = "https://myanimelist.net";

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

pub type Name     = String;
pub type Username = String;
pub type ID       = u32;

#[derive(Debug)]
pub enum RequestType {
    Find(Name),
    GetList(Username),
    Add(ID),
    Update(ID),
}

impl RequestType {
    fn get_url(&self) -> String {
        let mut url = Url::parse(BASE_URL).unwrap();
        use RequestType::*;

        match *self {
            Find(ref name) => {
                url.set_path("/api/anime/search.xml");
                url.query_pairs_mut().append_pair("q", &name);
                url.into_string()
            },
            GetList(ref name) => {
                url.set_path("/malappinfo.php");

                url.query_pairs_mut()
                    .append_pair("u", name)
                    .append_pair("status", "all")
                    .append_pair("type", "anime");

                url.into_string()
            },
            Add(id) => {
                url.set_path(&format!("/api/animelist/add/{}.xml", id));
                url.into_string()
            },
            Update(id) => {
                url.set_path(&format!("/api/animelist/update/{}.xml", id));
                url.into_string()
            },
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
    use RequestType::Find;
    
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