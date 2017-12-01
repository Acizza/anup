use failure::Error;
use mal;
use regex::Regex;
use process;
use std;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

#[derive(Fail, Debug)]
pub enum SeriesError {
    #[fail(display = "multiple series found")] MultipleSeriesFound,
    #[fail(display = "no episodes found")] NoEpisodesFound,
    #[fail(display = "episode {} not found", _0)] EpisodeNotFound(u32),
}

#[derive(Debug)]
pub struct EpisodeData {
    pub series_name: String,
    pub paths: HashMap<u32, PathBuf>,
}

impl EpisodeData {
    pub fn parse(path: &Path) -> Result<EpisodeData, Error> {
        let mut series = None;
        let mut episodes = HashMap::new();

        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();

            let ep_info = match EpisodeInfo::from_path(&path) {
                Some(info) => info,
                None => continue,
            };

            match series {
                Some(ref set_series) if set_series != &ep_info.series => {
                    bail!(SeriesError::MultipleSeriesFound);
                }
                None => series = Some(ep_info.series),
                _ => (),
            }

            episodes.insert(ep_info.number, path);
        }

        let series = series.ok_or(SeriesError::NoEpisodesFound)?;

        Ok(EpisodeData {
            series_name: series,
            paths: episodes,
        })
    }
}

#[derive(Debug)]
struct EpisodeInfo {
    pub series: String,
    pub number: u32,
}

impl EpisodeInfo {
    fn from_path(path: &Path) -> Option<EpisodeInfo> {
        if !path.is_file() {
            return None;
        }

        lazy_static! {
            static ref RE: Regex = Regex::new(r"(?:\[.+?\]\s*)?(?P<series>.+?)\s*-?\s*(?P<episode>\d+)\s*(?:\(|\[|\.)")
                .unwrap();
        }

        let filename = path.file_name()?.to_str().unwrap().replace('_', " ");

        let caps = RE.captures(&filename)?;

        Some(EpisodeInfo {
            series: caps["series"].into(),
            number: caps["episode"].parse().ok()?,
        })
    }
}

#[derive(Debug)]
pub struct Series {
    pub info: mal::SeriesInfo,
    pub episodes: HashMap<u32, PathBuf>,
}

impl Series {
    pub fn new(info: mal::SeriesInfo, ep_data: EpisodeData) -> Series {
        Series {
            info,
            episodes: ep_data.paths,
        }
    }

    pub fn play_episode(&self, ep_num: u32) -> Result<ExitStatus, Error> {
        let path = self.episodes.get(&ep_num)
            .ok_or(SeriesError::EpisodeNotFound(ep_num))?;

        let output = process::open_with_default(path).output()?;
        Ok(output.status)
    }
}
