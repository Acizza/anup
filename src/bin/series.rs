use failure::{Error, ResultExt};
use mal::{self, MAL};
use regex::Regex;
use process;
use serde_json;
use std;
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

#[derive(Fail, Debug)]
pub enum SeriesError {
    #[fail(display = "multiple series found")] MultipleSeriesFound,
    #[fail(display = "episode {} not found", _0)] EpisodeNotFound(u32),
    #[fail(display = "season {} information not found", _0)] SeasonInfoNotFound(u32),
}

#[derive(Debug)]
pub struct Series {
    pub name: String,
    pub data: SeriesData,
    pub episodes: HashMap<u32, PathBuf>,
    data_path: PathBuf,
}

impl Series {
    pub const DATA_FILE_NAME: &'static str = ".anitrack";

    pub fn from_path(path: &Path) -> Result<Series, Error> {
        let data_path = PathBuf::from(path).join(Series::DATA_FILE_NAME);
        let data = SeriesData::from_path_or_default(&data_path)?;

        let ep_data = EpisodeData::parse(path)?;

        Ok(Series {
            name: ep_data.series_name,
            data,
            episodes: ep_data.paths,
            data_path,
        })
    }

    pub fn play_episode(&self, ep_num: u32) -> Result<ExitStatus, Error> {
        let path = self.episodes.get(&ep_num)
            .ok_or(SeriesError::EpisodeNotFound(ep_num))?;

        let output = process::open_with_default(path).output()?;
        Ok(output.status)
    }

    pub fn set_season_data(&mut self, season: u32, info: SeasonInfo) {
        self.data.seasons.insert(season, info);
    }

    pub fn get_season_data(&self, season: u32) -> Result<&SeasonInfo, SeriesError> {
        self.data.seasons.get(&season).ok_or(SeriesError::SeasonInfoNotFound(season))
    }

    pub fn has_season_data(&self, season: u32) -> bool {
        self.data.seasons.contains_key(&season)
    }

    pub fn save_data(&self) -> Result<(), Error> {
        self.data.write_to(&self.data_path)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SeriesData {
    pub seasons: HashMap<u32, SeasonInfo>,
}

impl SeriesData {
    fn from_path(path: &Path) -> Result<SeriesData, Error> {
        let file = File::open(path)?;
        let data = serde_json::from_reader(file)?;

        Ok(data)
    }

    fn from_path_or_default(path: &Path) -> Result<SeriesData, Error> {
        if path.exists() {
            SeriesData::from_path(path)
        } else {
            Ok(SeriesData::default())
        }
    }

    fn write_to(&self, path: &Path) -> Result<(), Error> {
        let file = File::create(path)?;
        serde_json::to_writer_pretty(file, self)?;

        Ok(())
    }
}

#[derive(Fail, Debug)]
pub enum SeasonInfoError {
    #[fail(display = "no anime with id {} found with name [{}] on MAL", _0, _1)]
    UnknownAnimeID(u32, String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SeasonInfo {
    pub series_id: u32,
    pub episodes: u32,
    pub search_title: String,
}

impl SeasonInfo {
    pub fn create_basic(id: u32, episodes: u32, search_title: String) -> SeasonInfo {
        SeasonInfo {
            series_id: id,
            episodes,
            search_title,
        }
    }

    pub fn request_mal_info(&self, mal: &MAL) -> Result<mal::SeriesInfo, Error> {
        mal.search(&self.search_title)
            .context("MAL search failed")?
            .into_iter()
            .find(|i| i.id == self.series_id)
            .ok_or(SeasonInfoError::UnknownAnimeID(self.series_id, self.search_title.clone()).into())
    }
}

#[derive(Fail, Debug)]
pub enum EpisodeDataError {
    #[fail(display = "no episodes found")]
    NoEpisodesFound,
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
            let path = entry?.path();

            let info = match EpisodeInfo::parse(&path) {
                Some(info) => info,
                None => continue,
            };

            if let Some(ref series) = series {
                if series != &info.series {
                    bail!(SeriesError::MultipleSeriesFound);
                }
            } else {
                series = Some(info.series);
            }

            episodes.insert(info.episode, path);
        }

        let series = series.ok_or(EpisodeDataError::NoEpisodesFound)?;

        Ok(EpisodeData {
            series_name: series,
            paths: episodes,
        })
    }
}

#[derive(Debug)]
struct EpisodeInfo {
    series: String,
    episode: u32,
}

impl EpisodeInfo {
    fn parse(path: &Path) -> Option<EpisodeInfo> {
        if !path.is_file() {
            return None;
        }

        lazy_static! {
            static ref EP_FORMAT: Regex = Regex::new(r"(?:\[.+?\]\s*)?(?P<series>.+?)\s*-?\s*(?P<episode>\d+)\s*(?:\(|\[|\.)")
                .unwrap();
        }

        let filename = path.file_name()?.to_str().unwrap().replace('_', " ");
        let caps = EP_FORMAT.captures(&filename)?;

        let info = EpisodeInfo {
            series: caps["series"].into(),
            episode: caps["episode"].parse().ok()?,
        };

        Some(info)
    }
}