use failure::{Error, ResultExt};
use get_today;
use mal::MAL;
use mal::list::List;
use mal::list::anime::{AnimeInfo, AnimeEntry, WatchStatus};
use regex::Regex;
use process;
use prompt;
use serde_json;
use std;
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

#[derive(Fail, Debug)]
pub enum SeriesError {
    #[fail(display = "episode {} not found", _0)]
    EpisodeNotFound(u32),
    #[fail(display = "season {} information not found", _0)]
    SeasonInfoNotFound(u32),
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
        let path = self.episodes.get(&ep_num).ok_or_else(|| {
            SeriesError::EpisodeNotFound(ep_num)
        })?;

        let output = process::open_with_default(path).output()?;
        Ok(output.status)
    }

    pub fn watch_season(&mut self, mal: &MAL, season: u32) -> Result<(), Error> {
        let (season_info, search_term) = match self.seasons().get(&season) {
            Some(season) => {
                let info = season.request_mal_info(mal)?;
                let name = self.name.clone();
                (info, name)
            }
            None => {
                let result = prompt::select_series_info(mal, &self.name)?;
                (result.info, result.search_term)
            }
        };

        // TODO: use the HashMap's Entry API instead when NLL (https://git.io/vNkV1) is stable-ish
        if !self.seasons().contains_key(&season) {
            let info = SeasonInfo::new(season_info.id, season_info.episodes, search_term);

            self.seasons_mut().insert(season, info);
            self.save_data()?;
        }

        let anime_list = mal.anime_list();
        let mut list_entry = Series::get_list_entry(&anime_list, &season_info)?;

        self.play_all_episodes(&anime_list, season, &mut list_entry)
    }

    fn get_season_ep_offset(&self, season: u32) -> Result<u32, Error> {
        let mut ep_offset = 0;

        for cur_season in 1..season {
            // TODO: handle case where previous season info doesn't exist?
            let season = self.get_season_data(cur_season)?;
            ep_offset += season.episodes;
        }

        Ok(ep_offset)
    }

    fn play_all_episodes(&self, list: &List<AnimeEntry>, season: u32, entry: &mut AnimeEntry) -> Result<(), Error> {
        let season_offset = self.get_season_ep_offset(season)?;

        loop {
            let watched = entry.values.watched_episodes() + 1;
            entry.values.set_watched_episodes(watched);
            let real_ep_num = watched + season_offset;

            if self.play_episode(real_ep_num)?.success() {
                prompt::update_watched_eps(list, entry)?;
            } else {
                prompt::abnormal_player_exit(list, entry)?;
            }

            list.update(entry)?;
            prompt::next_episode_options(list, entry)?;
        }
    }

    fn get_list_entry(anime_list: &List<AnimeEntry>, info: &AnimeInfo) -> Result<AnimeEntry, Error> {
        let list = anime_list.read().context("MAL list retrieval failed")?;
        let found = list.entries.into_iter().find(|e| e.series_info == *info);

        match found {
            Some(mut entry) => {
                if entry.values.status() == WatchStatus::Completed && !entry.values.rewatching() {
                    prompt::rewatch_series(anime_list, &mut entry)?;
                }

                Ok(entry)
            }
            None => {
                let mut entry = AnimeEntry::new(info.clone());

                entry.values
                     .set_status(WatchStatus::Watching)
                     .set_start_date(Some(get_today()));

                anime_list.add(&mut entry)?;
                Ok(entry)
            }
        }
    }

    pub fn seasons(&self) -> &HashMap<u32, SeasonInfo> {
        &self.data.seasons
    }

    pub fn seasons_mut(&mut self) -> &mut HashMap<u32, SeasonInfo> {
        &mut self.data.seasons
    }

    pub fn get_season_data(&self, season: u32) -> Result<&SeasonInfo, SeriesError> {
        self.data.seasons.get(&season).ok_or_else(|| {
            SeriesError::SeasonInfoNotFound(season)
        })
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
    pub fn new(id: u32, episodes: u32, search_title: String) -> SeasonInfo {
        SeasonInfo {
            series_id: id,
            episodes,
            search_title,
        }
    }

    pub fn request_mal_info(&self, mal: &MAL) -> Result<AnimeInfo, Error> {
        mal.search_anime(&self.search_title)
            .context("MAL search failed")?
            .into_iter()
            .find(|i| i.id == self.series_id)
            .ok_or_else(|| {
                SeasonInfoError::UnknownAnimeID(self.series_id, self.search_title.clone()).into()
            })
    }
}

#[derive(Fail, Debug)]
pub enum EpisodeDataError {
    #[fail(display = "multiple series found")]
    MultipleSeriesFound,
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
                    bail!(EpisodeDataError::MultipleSeriesFound);
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
            static ref EP_FORMAT: Regex = Regex::new(r"(?:\[.+?\]\s*)?(?P<series>.+?)\s*(?:-\s*)?(?P<episode>\d+).*?\..+?")
                .unwrap();
        }

        // Replace certain special characters with spaces since they can either
        // affect parsing or prevent finding results on MAL
        let filename = path.file_name()?.to_str().unwrap().replace('_', " ");

        let caps = EP_FORMAT.captures(&filename)?;

        let clean_name = {
            let raw = &caps["series"];
            raw.replace('.', " ")
               .replace(" -", ":") // Dashes typically represent a colon in file names
               .trim()
               .to_string()
        };

        let info = EpisodeInfo {
            series: clean_name,
            episode: caps["episode"].parse().ok()?,
        };

        Some(info)
    }
}
