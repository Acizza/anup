use backend::{AnimeEntry, AnimeInfo, Status, SyncBackend};
use chrono::{Date, Local};
use error::SeriesError;
use input::{self, Answer};
use process;
use regex::Regex;
use std;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use toml;

#[derive(Debug)]
pub struct Series<B>
where
    B: SyncBackend,
{
    sync_backend: B,
    pub episode_data: EpisodeData,
    pub save_data: SaveData,
    pub save_path: PathBuf,
}

impl<B> Series<B>
where
    B: SyncBackend,
{
    pub const DATA_FILE_NAME: &'static str = ".anup";

    pub fn from_data(episode_data: EpisodeData, sync_backend: B) -> Result<Series<B>, SeriesError> {
        let save_path = PathBuf::from(&episode_data.path).join(Series::<B>::DATA_FILE_NAME);
        let save_data = SaveData::from_path_or_default(&save_path)?;

        let series = Series {
            sync_backend,
            episode_data,
            save_data,
            save_path,
        };

        Ok(series)
    }

    pub fn load_season(&mut self, season: u32) -> Result<Season<B>, SeriesError> {
        let season_info = self.get_season_info(season)?;
        let season_ep_offset = self.calculate_season_offset(season);

        let list_entry = self.get_list_entry(season_info.clone())?;

        Season::init(self, list_entry, season_ep_offset)
    }

    pub fn save_data(&self) -> Result<(), SeriesError> {
        self.save_data.write_to(&self.save_path)
    }

    fn get_season_info(&mut self, season: u32) -> Result<AnimeInfo, SeriesError> {
        let num_seasons = self.save_data.seasons.len() as u32;

        if season >= num_seasons {
            let mut series = None;

            for cur_season in num_seasons..=season {
                println!(
                    "select the correct series for season {} of [{}]:",
                    1 + cur_season,
                    self.episode_data.series_name
                );

                let series_info = self.search_and_select_series(&self.episode_data.series_name)?;
                self.save_data.seasons.push(series_info.clone().into());

                series = Some(series_info);
            }

            self.save_data()?;

            // This unwrap should never fail, as the enclosing if statement ensures the for loop will set the
            // series variable at least once
            Ok(series.unwrap())
        } else {
            let season_info = &self.save_data.seasons[season as usize];

            let series = self
                .sync_backend
                .get_series_info_by_id(season_info.series_id)?;

            Ok(series)
        }
    }

    fn calculate_season_offset(&self, season: u32) -> u32 {
        let mut offset = 0;

        for cur_season in 0..(season as usize) {
            if let Some(season_eps) = self.save_data.seasons[cur_season].episodes {
                offset += season_eps;
            }
        }

        offset
    }

    fn search_and_select_series(&self, name: &str) -> Result<AnimeInfo, SeriesError> {
        let mut found = self.sync_backend.search_by_name(name)?;

        println!("{} results for [{}]:", B::name(), name);
        println!("enter the number next to the desired series:\n");

        println!("0 [custom search]");

        for (i, series) in found.iter().enumerate() {
            println!("{} [{}]", 1 + i, series.title);
        }

        let index = input::read_range(0, found.len())?;

        if index == 0 {
            println!("enter the name you want to search for:");

            let name = input::read_line()?;
            self.search_and_select_series(&name)
        } else {
            let info = found.swap_remove(index - 1);
            Ok(info)
        }
    }

    fn get_list_entry(&self, info: AnimeInfo) -> Result<AnimeEntry, SeriesError> {
        let found = self.sync_backend.get_list_entry(info.clone())?;

        match found {
            Some(entry) => Ok(entry),
            None => {
                let mut entry = AnimeEntry::new(info);
                entry.status = Status::Watching;
                entry.start_date = Some(Local::today());

                self.sync_backend.update_list_entry(&entry)?;
                Ok(entry)
            }
        }
    }
}

#[derive(Debug)]
pub struct Season<'a, B>
where
    B: 'a + SyncBackend,
{
    series: &'a Series<B>,
    pub list_entry: AnimeEntry,
    pub ep_offset: u32,
}

impl<'a, B> Season<'a, B>
where
    B: 'a + SyncBackend,
{
    fn init(
        series: &'a Series<B>,
        list_entry: AnimeEntry,
        ep_offset: u32,
    ) -> Result<Season<'a, B>, SeriesError> {
        let mut season = Season {
            series,
            list_entry,
            ep_offset,
        };

        match season.list_entry.status {
            Status::Completed => season.prompt_to_rewatch()?,
            _ => (),
        }

        Ok(season)
    }

    fn update_list_entry(&self) -> Result<(), SeriesError> {
        self.series
            .sync_backend
            .update_list_entry(&self.list_entry)?;

        Ok(())
    }

    pub fn play_episode(&mut self, relative_ep: u32) -> Result<(), SeriesError> {
        let ep_num = self.ep_offset + relative_ep;

        let path = self
            .series
            .episode_data
            .episodes
            .get(&ep_num)
            .ok_or_else(|| SeriesError::EpisodeNotFound(ep_num))?;

        let status = process::open_with_default(path).map_err(SeriesError::FailedToOpenPlayer)?;
        self.list_entry.watched_episodes = relative_ep;

        if !status.success() {
            println!("video player not exited normally");
            println!("do you still want to count the episode as watched? (y/N)");

            if !input::read_yn(Answer::No)? {
                return Ok(());
            }
        }

        match self.list_entry.info.episodes {
            Some(total_eps) if relative_ep >= total_eps => self.series_completed()?,
            _ => self.episode_completed()?,
        }

        Ok(())
    }

    pub fn play_all_episodes(&mut self) -> Result<(), SeriesError> {
        loop {
            let next_ep = self.list_entry.watched_episodes + 1;

            self.play_episode(next_ep)?;
            self.next_episode_options()?;
        }
    }

    fn episode_completed(&mut self) -> Result<(), SeriesError> {
        println!(
            "[{}] episode {}/{} completed",
            self.list_entry.info.title,
            self.list_entry.watched_episodes,
            self.list_entry
                .info
                .episodes
                .map(|e| e.to_string())
                .unwrap_or_else(|| "?".to_string())
        );

        if self.list_entry.status != Status::Rewatching {
            self.list_entry.status = Status::Watching;

            if self.list_entry.watched_episodes <= 1 {
                self.list_entry.start_date = Some(Local::today());
            }
        }

        self.update_list_entry()?;
        Ok(())
    }

    fn series_completed(&mut self) -> Result<(), SeriesError> {
        println!(
            "[{}] completed!\ndo you want to rate it? (Y/n)",
            self.list_entry.info.title
        );

        if input::read_yn(Answer::Yes)? {
            self.prompt_to_update_score();
        }

        self.list_entry.status = Status::Completed;
        self.add_series_finish_date(Local::today())?;
        self.update_list_entry()?;

        // Nothing to do now
        std::process::exit(0);
    }

    fn next_episode_options(&mut self) -> Result<(), SeriesError> {
        println!("options:");
        println!("\t[d] drop series\n\t[h] put series on hold\n\t[r] rate series\n\t[x] exit\n\t[n] watch next episode (default)");

        let input = input::read_line()?.to_lowercase();

        match input.as_str() {
            "d" => {
                self.list_entry.status = Status::Dropped;
                self.add_series_finish_date(Local::today())?;
                self.update_list_entry()?;

                std::process::exit(0);
            }
            "h" => {
                self.list_entry.status = Status::OnHold;
                self.update_list_entry()?;

                std::process::exit(0);
            }
            "r" => {
                self.prompt_to_update_score();
                self.update_list_entry()?;

                self.next_episode_options()?;
            }
            "x" => std::process::exit(0),
            _ => (),
        }

        Ok(())
    }

    fn prompt_to_update_score(&mut self) {
        let max_score = self.series.sync_backend.max_score();
        println!("enter your score between 1-{}", max_score);

        match input::read_range(1.0, f32::from(max_score)) {
            Ok(score) => self.list_entry.score = score,
            Err(err) => eprintln!("failed to get score: {}", err),
        }
    }

    fn add_series_finish_date(&mut self, date: Date<Local>) -> Result<(), SeriesError> {
        let entry = &mut self.list_entry;

        // Someone may want to keep the original start / finish date for an
        // anime they're rewatching
        if entry.status == Status::Rewatching && entry.finish_date.is_some() {
            println!("do you want to override the finish date? (Y/n)");

            if input::read_yn(Answer::Yes)? {
                entry.finish_date = Some(date);
            }
        } else {
            entry.finish_date = Some(date);
        }

        Ok(())
    }

    fn prompt_to_rewatch(&mut self) -> Result<(), SeriesError> {
        println!("[{}] already completed", self.list_entry.info.title);
        println!("do you want to rewatch it? (Y/n)");

        if input::read_yn(Answer::Yes)? {
            self.list_entry.status = Status::Rewatching;
            self.list_entry.watched_episodes = 0;

            println!("do you want to reset the start and end date? (Y/n)");

            if input::read_yn(Answer::Yes)? {
                self.list_entry.start_date = Some(Local::today());
                self.list_entry.finish_date = None;
            }

            self.update_list_entry()?;
        } else {
            // No point in continuing in this case
            std::process::exit(0);
        }

        Ok(())
    }
}

type SeriesName = String;
type EpisodeNum = u32;

#[derive(Debug)]
pub struct EpisodeData {
    pub series_name: String,
    pub episodes: HashMap<u32, PathBuf>,
    pub path: PathBuf,
}

impl EpisodeData {
    pub const EP_FORMAT_REGEX: &'static str =
        r"(?:\[.+?\]\s*)?(?P<series>.+?)\s*-\s*(?P<episode>\d+).*?\..+?";

    pub fn parse_dir(dir: &Path) -> Result<EpisodeData, SeriesError> {
        if !dir.is_dir() {
            return Err(SeriesError::NotADirectory(
                dir.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "err".into()),
            ));
        }

        let mut series_name = None;
        let mut episodes = HashMap::new();

        for entry in std::fs::read_dir(dir).map_err(SeriesError::Io)? {
            let path = entry.map_err(SeriesError::Io)?.path();

            if !path.is_file() {
                continue;
            }

            match EpisodeData::parse_filename(&path) {
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

        let name = series_name.ok_or(SeriesError::NoEpisodesFound)?;

        Ok(EpisodeData {
            series_name: name,
            episodes,
            path: dir.into(),
        })
    }

    fn parse_filename(path: &Path) -> Result<(SeriesName, EpisodeNum), SeriesError> {
        lazy_static! {
            static ref EP_FORMAT: Regex = Regex::new(EpisodeData::EP_FORMAT_REGEX).unwrap();
        }

        // Replace certain characters with spaces since they can prevent proper series
        // identification or prevent it from being found on a sync backend
        let filename = path
            .file_name()
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
        let file_contents = fs::read_to_string(path)?;
        let data = toml::from_str(&file_contents)?;

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
        let toml = toml::to_string_pretty(self)?;
        fs::write(path, toml)?;

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SeasonInfo {
    pub series_id: u32,
    pub episodes: Option<u32>,
}

impl From<AnimeInfo> for SeasonInfo {
    fn from(info: AnimeInfo) -> SeasonInfo {
        SeasonInfo {
            series_id: info.id,
            episodes: info.episodes,
        }
    }
}
