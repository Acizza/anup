extern crate rquery;
extern crate hyper;

mod request;

use self::request::RequestType;
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
        let req = request::execute(RequestType::Search(name.into()), username, password)?;
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
}