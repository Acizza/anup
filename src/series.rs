use backend::{AnimeEntry, AnimeInfo, Status, SyncBackend};
use chrono::Local;
use error::SeriesError;
use input::{self, Answer};
use process;
use regex::Regex;
use std;
use std::borrow::Cow;
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

    pub fn load(path: &Path, sync_backend: B) -> Result<Series<B>, SeriesError> {
        let save_path = PathBuf::from(path).join(Series::<B>::DATA_FILE_NAME);
        let mut save_data = SaveData::from_path_or_default(&save_path)?;

        let episode_data = Series::<B>::parse_episode_data(path, &mut save_data)?;

        let series = Series {
            sync_backend,
            episode_data,
            save_data,
            save_path,
        };

        Ok(series)
    }

    fn parse_episode_data(
        path: &Path,
        save_data: &mut SaveData,
    ) -> Result<EpisodeData, SeriesError> {
        loop {
            match EpisodeData::parse_dir(path, save_data.episode_matcher.as_ref()) {
                Ok(data) => break Ok(data),
                Err(SeriesError::NoEpisodesFound) => {
                    println!("no episodes found");
                    println!("do you want to create a custom regex matcher? (Y/n)");

                    if input::read_yn(Answer::Yes)? {
                        println!("note: mark the series name and episode number with {{name}} and {{episode}}");
                        println!("example:");
                        println!("filename: [SubGroup] Series Name - Ep01.mkv");
                        println!(r"custom pattern: \[.+?\] {{name}} - Ep{{episode}}.mkv");

                        save_data.episode_matcher = Some(input::read_line()?);
                    } else {
                        return Err(SeriesError::RequestExit);
                    }
                }
                Err(err @ SeriesError::Regex(_))
                | Err(err @ SeriesError::UnknownRegexCapture(_)) => {
                    eprintln!("error parsing regex pattern: {}", err);
                    println!("please try again:");
                    save_data.episode_matcher = Some(input::read_line()?);
                }
                Err(err) => return Err(err),
            }
        }
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
                let series_info =
                    self.search_and_select_series(&self.episode_data.series_name, 1 + cur_season)?;

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

    fn search_and_select_series(&self, name: &str, season: u32) -> Result<AnimeInfo, SeriesError> {
        let mut found = self.sync_backend.search_by_name(name)?;

        println!("[{}] search results from [{}]:", name, B::name());
        println!(
            "select season {} by entering the number next to its name:\n",
            season
        );

        println!("0 [custom search]");

        for (i, series) in found.iter().enumerate() {
            println!("{} [{}]", 1 + i, series.title);
        }

        let index = input::read_range(0, found.len())?;

        if index == 0 {
            println!("enter the name you want to search for:");

            let name = input::read_line()?;
            self.search_and_select_series(&name, season)
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

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SaveData {
    pub episode_matcher: Option<String>,
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
            Status::Watching | Status::Rewatching => (),
            Status::Completed => season.update_series_status(Status::Rewatching)?,
            Status::OnHold | Status::Dropped => season.prompt_watch_paused_series()?,
            Status::PlanToWatch => season.update_series_status(Status::Watching)?,
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
            Some(total_eps) if relative_ep >= total_eps => {
                self.update_series_status(Status::Completed)?;
                return Err(SeriesError::RequestExit);
            }
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

    fn episode_completed(&self) -> Result<(), SeriesError> {
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

        self.update_list_entry()
    }

    fn next_episode_options(&mut self) -> Result<(), SeriesError> {
        let current_score_text: Cow<str> = match self.try_read_entry_score() {
            Some(score) => format!(" [{}]", score).into(),
            None => "".into(),
        };

        println!("options:");
        println!("\t[d] drop series\n\t[h] put series on hold\n\t[r] rate series{}\n\t[x] exit\n\t[n] watch next episode (default)",
            current_score_text
        );

        let input = input::read_line()?.to_lowercase();

        match input.as_str() {
            "d" => {
                self.update_series_status(Status::Dropped)?;
                Err(SeriesError::RequestExit)
            }
            "h" => {
                self.update_series_status(Status::OnHold)?;
                Err(SeriesError::RequestExit)
            }
            "r" => {
                self.prompt_to_update_score();
                self.update_list_entry()?;

                self.next_episode_options()
            }
            "x" => Err(SeriesError::RequestExit),
            _ => Ok(()),
        }
    }

    fn prompt_to_update_score(&mut self) {
        let (min_score, max_score) = self.series.sync_backend.formatted_score_range();
        println!("enter your score between {} and {}:", min_score, max_score);

        let input = match input::read_line() {
            Ok(input) => input,
            Err(err) => {
                eprintln!("failed to read score: {}", err);
                return;
            }
        };

        match self.series.sync_backend.parse_score(&input) {
            Ok(score) => self.list_entry.score = Some(score),
            Err(err) => eprintln!("failed to parse score: {}", err),
        }
    }

    fn prompt_watch_paused_series(&mut self) -> Result<(), SeriesError> {
        println!(
            "[{}] was put on hold or dropped\ndo you want to watch it from the beginning? (Y/n)",
            self.list_entry.info.title
        );

        if input::read_yn(Answer::Yes)? {
            self.list_entry.watched_episodes = 0;
        }

        self.update_series_status(Status::Watching)?;
        Ok(())
    }

    fn update_series_status(&mut self, status: Status) -> Result<(), SeriesError> {
        match status {
            Status::Watching => {
                // A series that was on hold probably already has a starting date, and it would make
                // more sense to use that one instead of replacing it
                if self.list_entry.status != Status::OnHold {
                    self.list_entry.start_date = Some(Local::today());
                }

                self.list_entry.finish_date = None;
            }
            Status::Rewatching => {
                println!("[{}] already completed", self.list_entry.info.title);
                println!("do you want to reset the start and end dates of the series? (Y/n)");

                if input::read_yn(Answer::Yes)? {
                    self.list_entry.start_date = Some(Local::today());
                    self.list_entry.finish_date = None;
                }

                self.list_entry.watched_episodes = 0;
            }
            Status::Completed => {
                if self.list_entry.finish_date.is_none() {
                    self.list_entry.finish_date = Some(Local::today());
                }

                println!(
                    "[{}] completed!\ndo you want to rate it? (Y/n)",
                    self.list_entry.info.title
                );

                if input::read_yn(Answer::Yes)? {
                    self.prompt_to_update_score();
                }
            }
            Status::Dropped => {
                if self.list_entry.finish_date.is_none() {
                    self.list_entry.finish_date = Some(Local::today());
                }
            }
            Status::OnHold | Status::PlanToWatch => (),
        }

        self.list_entry.status = status;
        self.update_list_entry()?;

        Ok(())
    }

    fn try_read_entry_score(&self) -> Option<String> {
        match self.list_entry.score {
            Some(score) => {
                let cur_score = self.series.sync_backend.format_score(score);

                match cur_score {
                    Ok(score) => Some(score),
                    Err(err) => {
                        eprintln!("failed to read existing list entry score: {}", err);
                        None
                    }
                }
            }
            None => None,
        }
    }
}

type SeriesName = String;
type EpisodeNum = u32;

#[derive(Debug)]
pub struct EpisodeData {
    pub series_name: String,
    pub episodes: HashMap<u32, PathBuf>,
    pub custom_format: Option<String>,
}

impl EpisodeData {
    pub const EP_FORMAT_REGEX: &'static str =
        r"(?:\[.+?\]\s*)?(?P<name>.+?)\s*-\s*(?P<episode>\d+).*?\..+?";

    fn get_matcher<'a, S>(custom_format: Option<S>) -> Result<Cow<'a, Regex>, SeriesError>
    where
        S: AsRef<str>,
    {
        lazy_static! {
            static ref EP_FORMAT: Regex = Regex::new(EpisodeData::EP_FORMAT_REGEX).unwrap();
        }

        match custom_format {
            Some(raw_pattern) => {
                let pattern = raw_pattern
                    .as_ref()
                    .replace("{name}", "(?P<name>.+?)")
                    .replace("{episode}", r"(?P<episode>\d+)");

                let regex = Regex::new(&pattern)?;
                Ok(Cow::Owned(regex))
            }
            None => Ok(Cow::Borrowed(&*EP_FORMAT)),
        }
    }

    pub fn parse_dir<S>(dir: &Path, custom_format: Option<S>) -> Result<EpisodeData, SeriesError>
    where
        S: AsRef<str>,
    {
        if !dir.is_dir() {
            return Err(SeriesError::NotADirectory(
                dir.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "err".into()),
            ));
        }

        let matcher = EpisodeData::get_matcher(custom_format)?;

        let mut series_name = None;
        let mut episodes = HashMap::new();

        for entry in std::fs::read_dir(dir).map_err(SeriesError::Io)? {
            let path = entry.map_err(SeriesError::Io)?.path();

            if !path.is_file() {
                continue;
            }

            match EpisodeData::parse_filename(&path, matcher.as_ref()) {
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
            custom_format: None,
        })
    }

    fn parse_filename(
        path: &Path,
        matcher: &Regex,
    ) -> Result<(SeriesName, EpisodeNum), SeriesError> {
        // Replace certain characters with spaces since they can prevent proper series
        // identification or prevent it from being found on a sync backend
        let filename = path
            .file_name()
            .and_then(|path| path.to_str())
            .ok_or(SeriesError::UnableToGetFilename)?
            .replace('_', " ");

        let caps = matcher
            .captures(&filename)
            .ok_or(SeriesError::EpisodeRegexCaptureFailed)?;

        let series_name = {
            let raw_name = caps
                .name("name")
                .map(|c| c.as_str())
                .ok_or_else(|| SeriesError::UnknownRegexCapture("name".into()))?;

            raw_name
                .replace('.', " ")
                .replace(" -", ":") // Dashes typically represent a colon in file names
                .trim()
                .to_string()
        };

        let episode = caps
            .name("episode")
            .ok_or_else(|| SeriesError::UnknownRegexCapture("episode".into()))
            .and_then(|cap| {
                cap.as_str()
                    .parse()
                    .map_err(SeriesError::EpisodeNumParseFailed)
            })?;

        Ok((series_name, episode))
    }
}
