extern crate xml;

use std::io::Write;
use request;
use rquery::Document;
use super::RequestType::*;
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

pub fn get_watched_episodes(id: u32, username: String) -> Result<u32> {
    let req = request::get(GetList(username.clone()), username, None)?;
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

pub fn add(id: u32, watched: u32, username: String, password: String) -> Result<()> {
    let mut xml = Vec::new();

    let tags = vec![
        ("episode", watched.to_string()),
        ("status", (Status::Watching as i32).to_string()),
    ];

    generate_anime_entry(&mut xml, tags.as_slice())?;
    let body = String::from_utf8(xml)?;

    request::post(
        Add(id),
        &body,
        username.clone(),
        password.clone()
    )?;

    Ok(())
}