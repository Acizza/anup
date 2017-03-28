extern crate xml;

use std::io::Write;
use request;
use request::RequestType;
use request::RequestType::*;
use rquery::Document;
use super::Status;
use self::xml::writer::{EmitterConfig, XmlEvent};

error_chain! {
    links {
        Request(request::Error, request::ErrorKind);
    }

    foreign_links {
        ConvertInt(::std::num::ParseIntError);
        Utf8Error(::std::string::FromUtf8Error);
        Writer(self::xml::writer::Error);
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
pub struct Entry {
    pub id:      u32,
    pub name:    String,
    pub watched: u32,
    pub status:  Status,
}

// TODO: Convert to iterator
pub fn get_entries(username: String) -> Result<Vec<Entry>> {
    let req = request::get(GetList(username.clone()), username, None)?;
    let doc = Document::new_from_xml_stream(req)
                .map_err(|_| ErrorKind::DocumentError)?;

    let mut entries = Vec::new();

    for entry in doc.select_all("anime").map_err(|_| ErrorKind::ParseError)? {
        let id = entry
            .select("series_animedb_id")
            .map_err(|_| ErrorKind::ParseError)?
            .text()
            .parse::<u32>()?;

        let name = entry
            .select("series_title")
            .map_err(|_| ErrorKind::ParseError)?
            .text()
            .to_string();

        let watched = entry
            .select("my_watched_episodes")
            .map_err(|_| ErrorKind::ParseError)?
            .text()
            .parse()?;

        let status = {
            let status_id = entry
                .select("my_status")
                .map_err(|_| ErrorKind::ParseError)?
                .text()
                .parse()?;

            Status::parse(status_id).ok_or(ErrorKind::ParseError)?
        };

        entries.push(Entry {
            id:      id,
            name:    name,
            watched: watched,
            status:  status,
        });
    }

    Ok(entries)
}

fn generate_anime_entry<W: Write>(dest: W, entries: &[(&str, String)]) -> Result<()> {
    let mut writer = EmitterConfig::new().create_writer(dest);

    writer.write(XmlEvent::start_element("entry"))?;

    for &(name, ref value) in entries {
        writer.write(XmlEvent::start_element(name))?;
        writer.write(XmlEvent::characters(&value))?;
        writer.write(XmlEvent::end_element())?;
    }

    writer.write(XmlEvent::end_element())?;
    Ok(())
}

fn exec_change(req_type: RequestType, tags: &[(&str, String)], username: String, password: String)
    -> Result<()> {
        
    let mut xml = Vec::new();
    generate_anime_entry(&mut xml, tags)?;

    let body = String::from_utf8(xml)?;

    request::post(
        req_type,
        &body,
        username,
        password
    )?;

    Ok(())
}

pub type ID      = u32;
pub type Watched = u32;

pub enum Action {
    Add(ID, Watched),
    Update(ID, Watched, Status),
}

pub fn modify(action: Action, username: String, password: String) -> Result<()> {
    let (req_type, tags) = match action {
        Action::Add(id, watched) => {
            let tags = vec![
                ("episode", watched.to_string()),
                ("status", (Status::Watching as i32).to_string()),
            ];

            (Add(id), tags)
        },
        Action::Update(id, watched, status) => {
            let tags = vec![
                ("episode", watched.to_string()),
                ("status", (status as i32).to_string()),
            ];

            (Update(id), tags)
        },
    };

    exec_change(req_type, tags.as_slice(), username, password)
}