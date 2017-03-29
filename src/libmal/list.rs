extern crate xml;
extern crate chrono;

use std::io::Write;
use request;
use request::RequestType;
use rquery::Document;
use super::{AnimeInfo, Auth, Status};
use self::chrono::date::Date;
use self::chrono::Local;
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
    }
}

#[derive(Debug)]
pub enum Tag {
    Episode(u32),
    Status(Status),
    StartDate(Option<Date<Local>>),
    FinishDate(Option<Date<Local>>),
    Score(u8),
    Rewatching(bool),
}

impl Tag {
    fn to_xml(&self) -> (&str, String) {
        use self::Tag::*;

        match *self {
            Episode(num)     => ("episode", num.to_string()),
            Status(status)   => ("status", (status as i32).to_string()),
            StartDate(date)  => ("date_start", get_date_format(date)),
            FinishDate(date) => ("date_finish", get_date_format(date)),
            Score(score)     => ("score", score.to_string()),
            Rewatching(val)  => ("enable_rewatching", (val as u8).to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub info:       AnimeInfo,
    pub watched:    u32,
    pub status:     Status,
    pub rewatching: bool,
}

impl Entry {
    pub fn update_watched(&mut self, watched: u32, auth: &Auth) -> Result<Status> {
        let mut tags = vec![Tag::Episode(watched)];

        let new_status = if watched >= self.info.episodes {
            tags.push(Tag::FinishDate(Some(Local::now().date())));

            if self.rewatching {
                tags.push(Tag::Rewatching(false));
                self.rewatching = false;
            }

            Status::Completed
        } else {
            Status::Watching
        };

        tags.push(Tag::Status(new_status));
        modify(self.info.id, Action::Update, &auth, tags.as_slice())?;

        self.watched = watched;
        self.status  = new_status;
        
        Ok(new_status)
    }

    pub fn set_score(&self, score: u8, auth: &Auth) -> Result<()> {
        modify(self.info.id, Action::Update, &auth, &[Tag::Score(score)])
    }

    pub fn start_rewatch(&mut self, auth: &Auth) -> Result<()> {
        modify(self.info.id, Action::Update, &auth, &[
            Tag::Rewatching(true),
            Tag::Episode(0),
            Tag::StartDate(Some(Local::now().date())),
            Tag::FinishDate(None),
        ])?;

        self.watched    = 0;
        self.rewatching = true;

        Ok(())
    }
}

// TODO: Convert to iterator
pub fn get_entries(username: String) -> Result<Vec<Entry>> {
    let req = request::get(RequestType::GetList(username), None)?;

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

        let episodes = entry
            .select("series_episodes")
            .map_err(|_| ErrorKind::ParseError)?
            .text()
            .parse()?;

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

        let rewatching = {
            let num = entry
                .select("my_rewatching")
                .map_err(|_| ErrorKind::ParseError)?
                .text()
                .parse::<u8>()?;

            num == 1
        };

        entries.push(Entry {
            info: AnimeInfo {
                id:       id,
                name:     name,
                episodes: episodes,
            },
            watched:    watched,
            status:     status,
            rewatching: rewatching,
        });
    }

    Ok(entries)
}

pub fn add_to_watching(info: &AnimeInfo, auth: &Auth) -> Result<Entry> {
    modify(info.id, Action::Add, &auth, &[
        Tag::Episode(0),
        Tag::Status(Status::Watching),
        Tag::StartDate(Some(Local::now().date())),
    ])?;

    Ok(Entry {
        info:       info.clone(),
        watched:    0,
        status:     Status::Watching,
        rewatching: false,
    })
}

#[derive(Debug)]
pub enum Action {
    Add,
    Update,
}

pub fn modify(id: u32, action: Action, auth: &Auth, tags: &[Tag]) -> Result<()> {
    let req_type = match action {
        Action::Add    => RequestType::Add(id),
        Action::Update => RequestType::Update(id),
    };

    exec_change(req_type, tags, &auth)
}

fn get_date_format(date: Option<Date<Local>>) -> String {
    match date {
        Some(date) => date.format("%m%d%Y").to_string(),
        None       => "00000000".to_string(),
    }
}

fn generate_anime_entry<W: Write>(dest: W, entries: &[Tag]) -> Result<()> {
    let mut writer = EmitterConfig::new().create_writer(dest);

    writer.write(XmlEvent::start_element("entry"))?;

    for tag in entries {
        let (name, value) = tag.to_xml();
        writer.write(XmlEvent::start_element(name))?;
        writer.write(XmlEvent::characters(&value))?;
        writer.write(XmlEvent::end_element())?;
    }

    writer.write(XmlEvent::end_element())?;
    Ok(())
}

fn exec_change(req_type: RequestType, tags: &[Tag], auth: &Auth) -> Result<()> {
    let mut xml = Vec::new();
    generate_anime_entry(&mut xml, tags)?;

    let body = String::from_utf8(xml)?;
    request::post(req_type, &body, &auth)?;

    Ok(())
}