use chrono::NaiveDate;
use failure::{Error, SyncFailure};
use minidom::Element;
use MissingXMLNode;
use RequestURL;
use reqwest;
use SearchEntry;

#[derive(Debug)]
pub struct ListEntry {
    pub info: SearchEntry,
    pub watched_episodes: u32,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub status: Status,
}

#[derive(Fail, Debug)]
#[fail(display = "{} does not map to any Status enum variants", _0)]
pub struct InvalidStatus(pub i32);

#[derive(Debug)]
pub enum Status {
    Watching = 1,
    Completed,
    OnHold,
    Dropped,
    PlanToWatch = 6,
}

impl Status {
    pub fn from_i32(value: i32) -> Result<Status, InvalidStatus> {
        match value {
            1 => Ok(Status::Watching),
            2 => Ok(Status::Completed),
            3 => Ok(Status::OnHold),
            4 => Ok(Status::Dropped),
            6 => Ok(Status::PlanToWatch),
            i => Err(InvalidStatus(i)),
        }
    }
}

pub fn get_for_user(username: &str) -> Result<Vec<ListEntry>, Error> {
    let resp = reqwest::get(&RequestURL::AnimeList(username).to_string())?.text()?;
    let root: Element = resp.parse().map_err(SyncFailure::new)?;

    let mut entries = Vec::new();

    for child in root.children().skip(1) {
        let get_child = |name| {
            child
                .children()
                .find(|c| c.name() == name)
                .map(|c| c.text())
                .ok_or(MissingXMLNode(name))
        };

        let entry = ListEntry {
            info: SearchEntry {
                id: get_child("series_animedb_id")?.parse()?,
                title: get_child("series_title")?,
                episodes: get_child("series_episodes")?.parse()?,
            },
            watched_episodes: get_child("my_watched_episodes")?.parse()?,
            start_date: parse_str_date(&get_child("my_start_date")?),
            end_date: parse_str_date(&get_child("my_finish_date")?),
            status: Status::from_i32(get_child("my_status")?.parse()?)?,
        };

        entries.push(entry);
    }

    Ok(entries)
}

fn parse_str_date(date: &str) -> Option<NaiveDate> {
    if date != "0000-00-00" {
        NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
    } else {
        None
    }
}
