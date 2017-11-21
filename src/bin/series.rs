use std;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use failure::Error;
use regex::Regex;

#[derive(Fail, Debug)]
pub enum SeriesError {
    #[fail(display = "multiple series found")]
    MultipleSeriesFound,
    #[fail(display = "no episodes found")]
    NoEpisodesFound,
}

type EpisodeNumber = u32;

#[derive(Debug)]
pub struct Series {
    pub name:     String,
    pub episodes: HashMap<EpisodeNumber, PathBuf>,
}

impl Series {
    pub fn from_path(path: &Path) -> Result<Series, Error> {
        let mut series = None;
        let mut episodes = HashMap::new();

        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();

            let ep_info = match EpisodeInfo::from_path(&path) {
                Some(info) => info,
                None       => continue,
            };

            match series {
                Some(ref set_series) if set_series != &ep_info.series => {
                    return Err(SeriesError::MultipleSeriesFound.into());
                },
                None => series = Some(ep_info.series),
                _ => (),
            }

            episodes.insert(ep_info.number, path);
        }

        let series = series.ok_or(SeriesError::NoEpisodesFound)?;

        Ok(Series {
            name: series,
            episodes: episodes,
        })
    }
}

#[derive(Debug)]
struct EpisodeInfo {
    pub series: String,
    pub number: EpisodeNumber,
}

impl EpisodeInfo {
    fn from_path(path: &Path) -> Option<EpisodeInfo> {
        if !path.is_file() {
            return None
        }

        lazy_static! {
            static ref RE: Regex = Regex::new(r"(?:\[.+?\]\s*)?(?P<series>.+?)\s*-?\s*(?P<episode>\d+)\s*(?:\(|\[|\.)")
                .unwrap();
        }

        let filename = path.file_name()?
            .to_str()
            .unwrap()
            .replace('_', " ");

        let caps = RE.captures(&filename)?;

        Some(EpisodeInfo {
            series: caps["series"].into(),
            number: caps["episode"].parse().ok()?,
        })
    }
}