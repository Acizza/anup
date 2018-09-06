use super::{search_for_series_info, SeasonState, SeriesConfig};
use backend::{AnimeEntry, AnimeInfo, SyncBackend};
use error::SeriesError;
use input::{self, Answer};
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use toml;

pub struct FolderData {
    pub episodes: EpisodeData,
    pub savefile: SaveData,
    pub path: PathBuf,
}

impl FolderData {
    pub fn load_dir(path: &Path) -> Result<FolderData, SeriesError> {
        let mut savefile = SaveData::from_dir(path)?;
        let episodes = EpisodeData::parse_until_valid_pattern(path, &mut savefile.episode_matcher)?;

        Ok(FolderData {
            episodes,
            savefile,
            path: PathBuf::from(path),
        })
    }

    pub fn save(&self) -> Result<(), SeriesError> {
        self.savefile.write_to_file()
    }

    pub fn populate_season_data<B>(&mut self, config: &SeriesConfig<B>) -> Result<(), SeriesError>
    where
        B: SyncBackend,
    {
        let num_seasons = self.seasons().len();

        if num_seasons > config.season_num {
            return Ok(());
        }

        for cur_season in num_seasons..=config.season_num {
            let info = self.fetch_series_info(config, cur_season)?;
            let entry = AnimeEntry::new(info);

            let season = SeasonState {
                state: entry,
                needs_info: config.offline_mode,
                needs_sync: config.offline_mode,
            };

            self.seasons_mut().push(season);
        }

        Ok(())
    }

    pub fn fetch_series_info<B>(
        &mut self,
        config: &SeriesConfig<B>,
        cur_season: usize,
    ) -> Result<AnimeInfo, SeriesError>
    where
        B: SyncBackend,
    {
        if config.offline_mode {
            // Return existing data if we already have it, otherwise return barebones info
            if self.seasons().len() > config.season_num {
                let info = self.seasons()[config.season_num].state.info.clone();
                Ok(info)
            } else {
                let mut info = AnimeInfo::default();
                info.title = self.episodes.series_name.clone();

                Ok(info)
            }
        } else {
            search_for_series_info(&config.sync_service, &self.episodes.series_name, cur_season)
        }
    }

    pub fn sync_remote_season_info<B>(
        &mut self,
        config: &SeriesConfig<B>,
    ) -> Result<(), SeriesError>
    where
        B: SyncBackend,
    {
        if config.season_num >= self.seasons().len() {
            return Ok(());
        }

        let mut season_data = self.seasons_mut()[config.season_num].clone();

        if season_data.needs_info {
            season_data.state.info = self.fetch_series_info(config, config.season_num)?;

            // We want to stay in a needs-sync state in offline mode so the "real" info
            // can be inserted when the series is played in online mode
            if !config.offline_mode {
                season_data.needs_info = false;
            }
        }

        // Sync data from the backend when not offline
        if !config.offline_mode {
            let entry = config
                .sync_service
                .get_list_entry(season_data.state.info.clone())?;

            if let Some(entry) = entry {
                // If we don't have new data to report, we should sync the data from the backend to keep up with
                // any changes made outside of the program
                if !season_data.needs_sync {
                    season_data.state = entry;
                }
            }
        }

        self.seasons_mut()[config.season_num] = season_data;
        Ok(())
    }

    pub fn calculate_season_offset(&self, mut range: Range<usize>) -> u32 {
        let num_seasons = self.savefile.season_states.len();
        range.start = num_seasons.min(range.start);
        range.end = num_seasons.min(range.end);

        let mut offset = 0;

        for i in range {
            let season = &self.savefile.season_states[i];

            match season.state.info.episodes {
                Some(eps) => offset += eps,
                None => return offset,
            }
        }

        offset
    }

    pub fn try_remove_dir(&self) {
        let path = self.path.to_string_lossy();

        println!("WARNING: {} will be deleted", path);
        println!("is this ok? (y/N)");

        match input::read_yn(Answer::No) {
            Ok(true) => match fs::remove_dir_all(&self.path) {
                Ok(_) => (),
                Err(err) => {
                    eprintln!("failed to remove directory: {}", err);
                }
            },
            Ok(false) => (),
            Err(err) => {
                eprintln!("failed to read input: {}", err);
            }
        }
    }

    pub fn seasons(&self) -> &Vec<SeasonState> {
        &self.savefile.season_states
    }

    pub fn seasons_mut(&mut self) -> &mut Vec<SeasonState> {
        &mut self.savefile.season_states
    }
}

type SeriesName = String;
type EpisodeNum = u32;

#[derive(Debug)]
pub struct EpisodeData {
    pub series_name: String,
    pub episodes: HashMap<EpisodeNum, PathBuf>,
    pub custom_format: Option<String>,
}

impl EpisodeData {
    // This default pattern will match episodes in several common formats, such as:
    // [Group] Series Name - 01.mkv
    // [Group]_Series_Name_-_01.mkv
    // [Group].Series.Name.-.01.mkv
    // [Group] Series Name - 01 [tag 1][tag 2].mkv
    // [Group]_Series_Name_-_01_[tag1][tag2].mkv
    // [Group].Series.Name.-.01.[tag1][tag2].mkv
    // Series Name - 01.mkv
    // Series_Name_-_01.mkv
    // Series.Name.-.01.mkv
    pub const EP_FORMAT_REGEX: &'static str =
        r"(?:\[.+?\](?:\s+|_+|\.+))?(?P<name>.+?)(?:\s*|_*|\.*)-(?:\s*|_*|\.*)(?P<episode>\d+).*?\..+?";

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

        for entry in fs::read_dir(dir).map_err(SeriesError::Io)? {
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
            .ok_or(SeriesError::UnableToGetFilename)?;

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
                .replace('_', " ")
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

    pub fn parse_until_valid_pattern(
        path: &Path,
        pattern: &mut Option<String>,
    ) -> Result<EpisodeData, SeriesError> {
        loop {
            match EpisodeData::parse_dir(path, pattern.as_ref()) {
                Ok(data) => break Ok(data),
                Err(SeriesError::NoEpisodesFound) => {
                    println!("no episodes found");
                    println!("you will now be prompted to enter a custom regex pattern");
                    println!("when entering the pattern, please mark the series name and episode number with {{name}} and {{episode}}, respectively");
                    println!("example:");
                    println!("  filename: [SubGroup] Series Name - Ep01.mkv");
                    println!(r"  pattern: \[.+?\] {{name}} - Ep{{episode}}.mkv");
                    println!("please enter your custom pattern:");

                    *pattern = Some(input::read_line()?);
                }
                Err(err @ SeriesError::Regex(_))
                | Err(err @ SeriesError::UnknownRegexCapture(_)) => {
                    eprintln!("error parsing regex pattern: {}", err);
                    println!("please try again:");

                    *pattern = Some(input::read_line()?);
                }
                Err(err) => return Err(err),
            }
        }
    }

    pub fn get_episode(&self, episode: u32) -> Result<&PathBuf, SeriesError> {
        self.episodes
            .get(&episode)
            .ok_or_else(|| SeriesError::EpisodeNotFound(episode))
    }
}

#[derive(Serialize, Deserialize)]
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
