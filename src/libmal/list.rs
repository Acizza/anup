use chrono::NaiveDate;
use failure::{Error, SyncFailure};
use minidom::Element;
use SeriesInfo;

#[derive(Debug)]
pub struct AnimeEntry {
    pub info: SeriesInfo,
    pub watched_episodes: u32,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub status: Status,
}

#[derive(Fail, Debug)]
#[fail(display = "{} does not map to any Status enum variants", _0)]
pub struct InvalidStatus(pub i32);

#[derive(Debug, Clone)]
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

#[derive(Debug)]
pub enum EntryTag {
    Episode(u32),
    Status(Status),
    StartDate(NaiveDate),
    FinishDate(NaiveDate),
    Score(u8),
    Rewatching(bool),
}

macro_rules! elem_with_txt {
    ($name:expr, $value:expr) => {{
        let mut elem = Element::bare($name);
        elem.append_text_node($value);
        elem
    }};
}

impl EntryTag {
    // TODO: adjust visibility
    pub fn build_xml_resp(stats: &[EntryTag]) -> Result<String, Error> {
        let mut entry = Element::bare("entry");

        for stat in stats {
            use self::EntryTag::*;

            let child = match *stat {
                Episode(num) => elem_with_txt!("episode", num.to_string()),
                Status(ref status) => elem_with_txt!("status", (status.clone() as i32).to_string()),
                StartDate(date) => elem_with_txt!("date_start", date_to_str(&date)),
                FinishDate(date) => elem_with_txt!("date_finish", date_to_str(&date)),
                Score(score) => elem_with_txt!("score", score.to_string()),
                Rewatching(v) => elem_with_txt!("enable_rewatching", (v as u8).to_string()),
            };

            entry.append_child(child);
        }

        let mut buffer = Vec::new();
        entry.write_to(&mut buffer).map_err(SyncFailure::new)?;

        Ok(String::from_utf8(buffer)?)
    }
}

fn date_to_str(date: &NaiveDate) -> String {
    date.format("%m%d%Y").to_string()
}
