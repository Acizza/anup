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
    offline_mode: bool,
    sync_backend: B,
    pub episode_data: EpisodeData,
    pub save_data: SaveData,
}

impl<B> Series<B>
where
    B: SyncBackend,
{
    pub fn load(
        offline_mode: bool,
        path: &Path,
        sync_backend: B,
    ) -> Result<Series<B>, SeriesError> {
        let mut save_data = SaveData::from_dir(path)?;
        let episode_data = Series::<B>::parse_episode_data(path, &mut save_data)?;

        let series = Series {
            offline_mode,
            sync_backend,
            episode_data,
            save_data,
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
                    println!("you will now be prompted to enter a custom regex pattern");
                    println!("when entering the pattern, please mark the series name and episode number with {{name}} and {{episode}}, respectively");
                    println!("example:");
                    println!("  filename: [SubGroup] Series Name - Ep01.mkv");
                    println!(r"  pattern: \[.+?\] {{name}} - Ep{{episode}}.mkv");
                    println!("please enter your custom pattern:");

                    save_data.episode_matcher = Some(input::read_line()?);
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
        let season_state = self.get_season_state(season)?;
        let season_ep_offset = self.calculate_season_offset(season)?;

        let list_entry = self.get_list_entry(season, season_state.info)?;
        let offline_mode = self.offline_mode;

        self.save_data()?;

        Season::init(self, offline_mode, list_entry, season, season_ep_offset)
    }

    pub fn save_data(&self) -> Result<(), SeriesError> {
        self.save_data.write_to_file()
    }

    fn get_season_state(&mut self, season: u32) -> Result<AnimeEntry, SeriesError> {
        let num_seasons = self.save_data.season_states.len() as u32;

        if season >= num_seasons {
            let mut series = None;

            // Get new season info up to the desired season
            for cur_season in num_seasons..=season {
                let series_info = self.get_series_info(&self.episode_data.series_name, cur_season)?;

                let entry = AnimeEntry::new(series_info);

                let season_state = SeasonState {
                    state: entry.clone(),
                    needs_sync: self.offline_mode,
                };

                self.save_data.season_states.push(season_state);
                series = Some(entry);
            }

            // This unwrap should never fail, as the enclosing if statement ensures the for loop will set the
            // series variable at least once
            Ok(series.unwrap())
        } else {
            // When we already have the required season data, we can perform any necessary syncing and return it directly
            // TODO: this entire block can likely be simplified when NLL becomes stable

            let mut season_state = self.season_state(season).clone();

            if season_state.needs_sync {
                season_state.state.info =
                    self.get_series_info(&self.episode_data.series_name, season)?;

                // We want to stay in a needs-sync state in offline mode so the "real" info
                // can be inserted when the series is played in online mode
                if !self.offline_mode {
                    season_state.needs_sync = false;
                }

                *self.season_state_mut(season) = season_state.clone();
            }

            Ok(season_state.state)
        }
    }

    fn calculate_season_offset(&mut self, season: u32) -> Result<u32, SeriesError> {
        let mut offset = 0;

        for cur_season in 0..(season as usize) {
            let num_episodes = self.season_state(cur_season as u32).state.info.episodes;

            match num_episodes {
                Some(eps) => offset += eps,
                None => {
                    println!(
                        "please enter the number of episodes for season {} of [{}]:",
                        1 + cur_season,
                        self.episode_data.series_name
                    );

                    let eps = input::read_range(1, ::std::u32::MAX)?;

                    self.season_state_mut(cur_season as u32).state.info.episodes = Some(eps);
                    offset += eps;
                }
            }
        }

        Ok(offset)
    }

    fn get_series_info(&self, name: &str, season: u32) -> Result<AnimeInfo, SeriesError> {
        if self.offline_mode {
            let info = if self.save_data.season_states.len() > season as usize {
                self.season_state(season).state.info.clone()
            } else {
                let mut info = AnimeInfo::default();
                info.title = self.episode_data.series_name.clone();

                info
            };

            return Ok(info);
        }

        println!("[{}] searching on {}..", name, B::name());

        let mut found = self.sync_backend.search_by_name(name)?;

        if !found.is_empty() {
            println!(
                "select season {} by entering the number next to its name:\n",
                1 + season
            );

            println!("0 [custom search]");

            for (i, series) in found.iter().enumerate() {
                println!("{} [{}]", 1 + i, series.title);
            }

            let index = input::read_range(0, found.len())?;

            if index == 0 {
                println!("enter the name you want to search for:");

                let name = input::read_line()?;
                self.get_series_info(&name, season)
            } else {
                let info = found.swap_remove(index - 1);
                Ok(info)
            }
        } else {
            println!("no results found\nplease enter a custom search term:");

            let name = input::read_line()?;
            self.get_series_info(&name, season)
        }
    }

    fn get_list_entry(&mut self, season: u32, info: AnimeInfo) -> Result<AnimeEntry, SeriesError> {
        if self.offline_mode {
            return Ok(self.season_state(season).state.clone());
        }

        let found = self.sync_backend.get_list_entry(info.clone())?;

        match found {
            Some(entry) => {
                // When the list entry already exists, we should sync the data we have locally
                // to the new list entry data
                self.season_state_mut(season).state = entry.clone();
                Ok(entry)
            }
            None => {
                // When the list entry doesn't exist, we should "upload" our existing local data to
                // the backend
                let mut entry = self.season_state(season).state.clone();
                entry.info = info;

                Ok(entry)
            }
        }
    }

    fn season_state(&self, season: u32) -> &SeasonState {
        &self.save_data.season_states[season as usize]
    }

    fn season_state_mut(&mut self, season: u32) -> &mut SeasonState {
        &mut self.save_data.season_states[season as usize]
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SaveData {
    pub episode_matcher: Option<String>,
    pub season_states: Vec<SeasonState>,
    #[serde(skip)]
    pub path: PathBuf,
}

impl SaveData {
    const DATA_FILE_NAME: &'static str = ".anup";

    pub fn new(path: PathBuf) -> SaveData {
        SaveData {
            episode_matcher: None,
            season_states: Vec::new(),
            path,
        }
    }

    pub fn from_dir(path: &Path) -> Result<SaveData, SeriesError> {
        let path = PathBuf::from(path).join(SaveData::DATA_FILE_NAME);

        if !path.exists() {
            return Ok(SaveData::new(path));
        }

        let file_contents = fs::read_to_string(&path)?;

        let mut save_data: SaveData = toml::from_str(&file_contents)?;
        save_data.path = path;

        Ok(save_data)
    }

    pub fn write_to_file(&self) -> Result<(), SeriesError> {
        let toml = toml::to_string_pretty(self)?;
        fs::write(&self.path, toml)?;

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeasonState {
    #[serde(flatten)]
    state: AnimeEntry,
    needs_sync: bool,
}

#[derive(Debug)]
pub struct Season<'a, B>
where
    B: 'a + SyncBackend,
{
    series: &'a mut Series<B>,
    offline_mode: bool,
    season_num: u32,
    pub list_entry: AnimeEntry,
    pub ep_offset: u32,
}

impl<'a, B> Season<'a, B>
where
    B: 'a + SyncBackend,
{
    fn init(
        series: &'a mut Series<B>,
        offline_mode: bool,
        list_entry: AnimeEntry,
        season_num: u32,
        ep_offset: u32,
    ) -> Result<Season<'a, B>, SeriesError> {
        let mut season = Season {
            series,
            offline_mode,
            season_num,
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

    fn update_list_entry(&mut self) -> Result<(), SeriesError> {
        self.series.save_data.season_states[self.season_num as usize].state =
            self.list_entry.clone();

        self.series.save_data.write_to_file()?;

        if self.offline_mode {
            return Ok(());
        }

        self.series
            .sync_backend
            .update_list_entry(&self.list_entry)?;

        Ok(())
    }

    pub fn play_episode(&mut self, relative_ep: u32) -> Result<(), SeriesError> {
        let ep_num = self.ep_offset + relative_ep;

        let path = {
            self.series
                .episode_data
                .episodes
                .get(&ep_num)
                .ok_or_else(|| SeriesError::EpisodeNotFound(ep_num))?
                .clone() // TODO: remove clone and block when NLL is stable
        };

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
                    self.list_entry.start_date = Some(Local::today().naive_local());
                }

                self.list_entry.finish_date = None;
            }
            Status::Rewatching => {
                println!("[{}] already completed", self.list_entry.info.title);
                println!("do you want to reset the start and end dates of the series? (Y/n)");

                if input::read_yn(Answer::Yes)? {
                    self.list_entry.start_date = Some(Local::today().naive_local());
                    self.list_entry.finish_date = None;
                }

                self.list_entry.watched_episodes = 0;
            }
            Status::Completed => {
                if self.list_entry.finish_date.is_none() {
                    self.list_entry.finish_date = Some(Local::today().naive_local());
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
                    self.list_entry.finish_date = Some(Local::today().naive_local());
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
