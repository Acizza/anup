use chrono::{Local, NaiveDate};
use error::SeriesError;
use input::{self, Answer};
use mal::MAL;
use mal::list::{List, Status};
use mal::list::anime::{AnimeEntry, AnimeInfo};
use regex::Regex;
use toml;
use process;
use std;
use std::io::{Read, Write};
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

fn get_today() -> NaiveDate {
    Local::today().naive_utc()
}

#[derive(Debug)]
pub struct Series<'a> {
    pub mal: &'a MAL<'a>,
    pub data: SeriesData,
    pub save_data: SaveData,
    pub save_path: PathBuf,
}

impl<'a> Series<'a> {
    pub const DATA_FILE_NAME: &'static str = ".anup";

    pub fn from_dir(dir: &Path, mal: &'a MAL) -> Result<Series<'a>, SeriesError> {
        if !dir.is_dir() {
            return Err(SeriesError::NotADirectory(
                dir.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "err".into()),
            ));
        }

        let series_data = SeriesData::parse_dir(dir)?;
        let save_path = PathBuf::from(dir).join(Series::DATA_FILE_NAME);
        let save_data = SaveData::from_path_or_default(&save_path)?;

        let series = Series {
            mal,
            data: series_data,
            save_data,
            save_path,
        };

        Ok(series)
    }

    pub fn load_season(&mut self, season_num: u32) -> Result<Season, SeriesError> {
        let created_series_info = self.ensure_num_seasons(season_num)?;

        let season_info = {
            let index = season_num.saturating_sub(1);
            &self.save_data.seasons[index as usize]
        };

        let series_info = match created_series_info {
            Some(info) => info,
            None => season_info.request_mal_info(&self.mal.anime_list())?,
        };

        let episode_offset = self.calculate_season_offset(season_num);
        let list_entry = self.get_mal_list_entry(&series_info)?;

        Ok(Season::new(
            self.mal,
            list_entry,
            &self.data.episodes,
            episode_offset,
        ))
    }

    fn ensure_num_seasons(&mut self, num_seasons: u32) -> Result<Option<AnimeInfo>, SeriesError> {
        let mut created_series_info = None;
        let existing_seasons = self.save_data.seasons.len();

        if num_seasons as usize > existing_seasons {
            for cur_season in existing_seasons..(num_seasons as usize) {
                println!(
                    "select the correct series for season {} of [{}]:",
                    1 + cur_season,
                    self.data.name
                );

                let season_info = self.select_series_from_mal(&self.data.name)?;

                created_series_info = Some(season_info.info.clone());
                self.save_data.seasons.push(season_info.into());
            }

            self.save_data()?;
        }

        Ok(created_series_info)
    }

    fn calculate_season_offset(&self, season: u32) -> u32 {
        let mut offset = 0;

        for cur_season in 1..(season as usize) {
            offset += self.save_data.seasons[cur_season].episodes;
        }

        offset
    }

    pub fn save_data(&self) -> Result<(), SeriesError> {
        self.save_data.write_to(&self.save_path)
    }

    fn select_series_from_mal(&self, name: &str) -> Result<SeriesSelection, SeriesError> {
        let mut found = self.mal.anime_list().search_for(name)?;

        println!("MAL results for [{}]:", name);
        println!("enter the number next to the desired series:\n");

        println!("0 [custom search]");

        for (i, series) in found.iter().enumerate() {
            println!("{} [{}]", 1 + i, series.title);
        }

        let index = input::read_usize_range(0, found.len())?;

        if index == 0 {
            println!("enter the name you want to search for:");

            let name = input::read_line()?;
            self.select_series_from_mal(&name)
        } else {
            Ok(SeriesSelection::new(found.swap_remove(index - 1), name))
        }
    }

    fn get_mal_list_entry(&self, info: &AnimeInfo) -> Result<AnimeEntry, SeriesError> {
        let anime_list = self.mal.anime_list();

        let found = anime_list
            .read()?
            .entries
            .into_iter()
            .find(|e| e.series_info == *info);

        match found {
            Some(mut entry) => {
                if entry.values.status() == Status::Completed && !entry.values.rewatching() {
                    self.prompt_to_rewatch(&mut entry)?;
                }

                Ok(entry)
            }
            None => {
                let mut entry = AnimeEntry::new(info.clone());

                entry
                    .values
                    .set_status(Status::WatchingOrReading)
                    .set_start_date(Some(get_today()));

                anime_list.add(&mut entry)?;
                Ok(entry)
            }
        }
    }

    fn prompt_to_rewatch(&self, entry: &mut AnimeEntry) -> Result<(), SeriesError> {
        println!("[{}] already completed", entry.series_info.title);
        println!("do you want to rewatch it? (Y/n)");
        println!("(note that you have to increase the rewatch count manually)");

        if input::read_yn(Answer::Yes)? {
            entry.values.set_rewatching(true).set_watched_episodes(0);

            println!("do you want to reset the start and end date? (Y/n)");

            if input::read_yn(Answer::Yes)? {
                entry
                    .values
                    .set_start_date(Some(get_today()))
                    .set_finish_date(None);
            }

            self.mal.anime_list().update(entry)?;
        } else {
            // No point in continuing in this case
            std::process::exit(0);
        }

        Ok(())
    }
}

struct SeriesSelection {
    pub info: AnimeInfo,
    pub search_term: String,
}

impl SeriesSelection {
    fn new<S: Into<String>>(info: AnimeInfo, search_term: S) -> SeriesSelection {
        SeriesSelection {
            info,
            search_term: search_term.into(),
        }
    }
}

impl Into<SeasonInfo> for SeriesSelection {
    fn into(self) -> SeasonInfo {
        SeasonInfo {
            series_id: self.info.id,
            episodes: self.info.episodes,
            search_title: self.search_term,
        }
    }
}

#[derive(Debug)]
pub struct Season<'a> {
    pub mal: &'a MAL<'a>,
    pub list_entry: AnimeEntry,
    pub local_episodes: &'a HashMap<u32, PathBuf>,
    pub ep_offset: u32,
}

impl<'a> Season<'a> {
    pub fn new(
        mal: &'a MAL<'a>,
        list_entry: AnimeEntry,
        local_episodes: &'a HashMap<u32, PathBuf>,
        ep_offset: u32,
    ) -> Season<'a> {
        Season {
            mal,
            list_entry,
            local_episodes,
            ep_offset,
        }
    }

    pub fn play_episode(&mut self, relative_ep: u32) -> Result<(), SeriesError> {
        let ep_num = self.ep_offset + relative_ep;

        let path = self.local_episodes
            .get(&ep_num)
            .ok_or_else(|| SeriesError::EpisodeNotFound(ep_num))?;

        let status = process::open_with_default(path)
            .map_err(SeriesError::FailedToOpenPlayer)?;

        self.list_entry.values.set_watched_episodes(relative_ep);

        if !status.success() {
            println!("video player not exited normally");
            println!("do you still want to count the episode as watched? (y/N)");

            if !input::read_yn(Answer::No)? {
                return Ok(());
            }
        }

        if relative_ep >= self.list_entry.series_info.episodes {
            self.series_completed()?;
        } else {
            self.episode_completed()?;
        }

        Ok(())
    }

    pub fn play_all_episodes(&mut self) -> Result<(), SeriesError> {
        loop {
            let next_ep_num = self.list_entry.values.watched_episodes() + 1;

            self.play_episode(next_ep_num)?;
            self.next_episode_options()?;
        }
    }

    fn episode_completed(&mut self) -> Result<(), SeriesError> {
        let entry = &mut self.list_entry;

        println!(
            "[{}] episode {}/{} completed",
            entry.series_info.title,
            entry.values.watched_episodes(),
            entry.series_info.episodes
        );

        if !entry.values.rewatching() {
            entry.values.set_status(Status::WatchingOrReading);

            if entry.values.watched_episodes() <= 1 {
                entry.values.set_start_date(Some(get_today()));
            }
        }

        self.mal.anime_list().update(entry)?;
        Ok(())
    }

    fn series_completed(&mut self) -> Result<(), SeriesError> {
        let today = get_today();

        self.list_entry.values.set_status(Status::Completed);

        println!(
            "[{}] completed!\ndo you want to rate it? (Y/n)",
            self.list_entry.series_info.title
        );

        if input::read_yn(Answer::Yes)? {
            println!("enter your score between 1-10:");
            let score = input::read_usize_range(1, 10)? as u8;

            self.list_entry.values.set_score(score);
        }

        if self.list_entry.values.rewatching() {
            self.list_entry.values.set_rewatching(false);
        }

        self.add_series_finish_date(today)?;
        self.mal.anime_list().update(&mut self.list_entry)?;

        // Nothing to do now
        std::process::exit(0);
    }

    fn next_episode_options(&mut self) -> Result<(), SeriesError> {
        println!("options:");
        println!("\t[d] drop series\n\t[h] put series on hold\n\t[r] rate series\n\t[x] exit\n\t[n] watch next episode (default)");

        let input = input::read_line()?.to_lowercase();

        match input.as_str() {
            "d" => {
                self.list_entry.values.set_status(Status::Dropped);
                self.add_series_finish_date(get_today())?;
                self.mal.anime_list().update(&mut self.list_entry)?;

                std::process::exit(0);
            }
            "h" => {
                self.list_entry.values.set_status(Status::OnHold);
                self.mal.anime_list().update(&mut self.list_entry)?;

                std::process::exit(0);
            }
            "r" => {
                println!("enter your score between 1-10:");

                let score = input::read_usize_range(1, 10)? as u8;
                self.list_entry.values.set_score(score);

                self.mal.anime_list().update(&mut self.list_entry)?;
                self.next_episode_options()?;
            }
            "x" => std::process::exit(0),
            _ => (),
        }

        Ok(())
    }

    fn add_series_finish_date(&mut self, date: NaiveDate) -> Result<(), SeriesError> {
        let entry = &mut self.list_entry;

        // Someone may want to keep the original start / finish date for an
        // anime they're rewatching
        if entry.values.rewatching() && entry.values.finish_date().is_some() {
            println!("do you want to override the finish date? (Y/n)");

            if input::read_yn(Answer::Yes)? {
                entry.values.set_finish_date(Some(date));
            }
        } else {
            entry.values.set_finish_date(Some(date));
        }

        Ok(())
    }
}

type SeriesName = String;
type EpisodeNum = u32;

#[derive(Debug)]
pub struct SeriesData {
    pub name: String,
    pub episodes: HashMap<u32, PathBuf>,
}

impl SeriesData {
    pub const EP_FORMAT_REGEX: &'static str =
        r"(?:\[.+?\]\s*)?(?P<series>.+?)\s*-\s*(?P<episode>\d+).*?\..+?";

    fn parse_dir(dir: &Path) -> Result<SeriesData, SeriesError> {
        let mut series_name = None;
        let mut episodes = HashMap::new();

        for entry in std::fs::read_dir(dir).map_err(SeriesError::Io)? {
            let path = entry.map_err(SeriesError::Io)?.path();

            if !path.is_file() {
                continue;
            }

            match SeriesData::parse_filename(&path) {
                Ok((ep_name, ep_num)) => {
                    match series_name {
                        Some(ref series_name) if &ep_name != series_name => {
                            return Err(SeriesError::MultipleSeriesFound);
                        }
                        Some(_) => (),
                        None => series_name = Some(ep_name),
                    }

                    episodes.insert(ep_num, path);
                }
                Err(SeriesError::EpisodeRegexCaptureFailed) => continue,
                Err(e) => return Err(e),
            }
        }

        let series = series_name.ok_or(SeriesError::NoEpisodesFound)?;

        Ok(SeriesData {
            name: series,
            episodes,
        })
    }

    fn parse_filename(path: &Path) -> Result<(SeriesName, EpisodeNum), SeriesError> {
        lazy_static! {
            static ref EP_FORMAT: Regex = Regex::new(SeriesData::EP_FORMAT_REGEX)
                .unwrap();
        }

        // Replace certain special characters with spaces since they can either
        // affect parsing or prevent finding results on MAL
        let filename = path.file_name()
            .and_then(|path| path.to_str())
            .ok_or(SeriesError::UnableToGetFilename)?
            .replace('_', " ");

        let caps = EP_FORMAT
            .captures(&filename)
            .ok_or(SeriesError::EpisodeRegexCaptureFailed)?;

        let series_name = {
            let raw_name = &caps["series"];

            raw_name.replace('.', " ")
            .replace(" -", ":") // Dashes typically represent a colon in file names
            .trim()
            .to_string()
        };

        let episode = caps["episode"]
            .parse()
            .map_err(SeriesError::EpisodeNumParseFailed)?;

        Ok((series_name, episode))
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SaveData {
    pub seasons: Vec<SeasonInfo>,
}

impl SaveData {
    fn from_path(path: &Path) -> Result<SaveData, SeriesError> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let data = toml::from_str(&contents)?;
        Ok(data)
    }

    fn from_path_or_default(path: &Path) -> Result<SaveData, SeriesError> {
        if path.exists() {
            SaveData::from_path(path)
        } else {
            Ok(SaveData::default())
        }
    }

    fn write_to(&self, path: &Path) -> Result<(), SeriesError> {
        let mut file = File::create(path)?;
        let toml = toml::to_string_pretty(self)?;

        write!(file, "{}", toml)?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SeasonInfo {
    pub series_id: u32,
    pub episodes: u32,
    pub search_title: String,
}

impl SeasonInfo {
    pub fn request_mal_info(&self, list: &List<AnimeEntry>) -> Result<AnimeInfo, SeriesError> {
        list.search_for(&self.search_title)?
            .into_iter()
            .find(|i| i.id == self.series_id)
            .ok_or_else(|| SeriesError::UnknownAnimeID(self.series_id, self.search_title.clone()))
    }
}
