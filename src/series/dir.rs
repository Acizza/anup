use super::{SeasonState, SeriesConfig};
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
    pub series_info: SeriesInfo,
    pub savefile: SaveData,
    pub subseries: String,
    pub path: PathBuf,
}

impl FolderData {
    const DEFAULT_SUBSERIES_NAME: &'static str = "main";

    pub fn load_dir(path: &Path, subseries: Option<String>) -> Result<FolderData, SeriesError> {
        let subseries = subseries.unwrap_or_else(|| FolderData::DEFAULT_SUBSERIES_NAME.into());

        let mut savefile = SaveData::from_dir(path)?;
        let series_info = FolderData::load_series_info(path, &mut savefile, &subseries)?;

        Ok(FolderData {
            series_info,
            savefile,
            subseries,
            path: PathBuf::from(path),
        })
    }

    fn load_series_info(
        path: &Path,
        savefile: &mut SaveData,
        subseries: &str,
    ) -> Result<SeriesInfo, SeriesError> {
        let mut ep_data = parse_episode_files_until_valid(path, &mut savefile.episode_matcher)?;
        let subseries_data = savefile.get_mut_subseries_entry(subseries);

        if let Some(info) = SeriesInfo::select_from_subseries(&mut ep_data, &subseries_data)? {
            return Ok(info);
        }

        let info = SeriesInfo::prompt_select_from_episodes(ep_data)?;
        subseries_data.files_title = Some(info.name.clone());

        Ok(info)
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
            let info = AnimeInfo::default();
            let entry = AnimeEntry::new(info);

            let mut season = SeasonState {
                state: entry,
                needs_info: true,
                needs_sync: config.offline_mode,
            };

            season.sync_info_from_remote(config, &self, cur_season)?;

            let subseries = self
                .savefile
                .get_mut_subseries_entry(self.subseries.clone());

            subseries.season_states.push(season);
        }

        Ok(())
    }

    pub fn calculate_season_offset(&self, mut range: Range<usize>) -> u32 {
        let num_seasons = self.seasons().len();
        range.start = num_seasons.min(range.start);
        range.end = num_seasons.min(range.end);

        let mut offset = 0;
        let seasons = self.seasons();

        for i in range {
            let season = &seasons[i];

            match season.state.info.episodes {
                Some(eps) => offset += eps,
                None => return offset,
            }
        }

        offset
    }

    pub fn delete_series_dir(&self) -> Result<(), SeriesError> {
        println!("WARNING: {} will be deleted", self.path.to_string_lossy());
        println!("is this ok? (y/N)");

        if !input::read_yn(Answer::No)? {
            return Ok(());
        }

        fs::remove_dir_all(&self.path)?;
        Ok(())
    }

    pub fn get_episode(&self, episode: u32) -> Result<&PathBuf, SeriesError> {
        self.series_info.get_episode(episode)
    }

    pub fn save(&self) -> Result<(), SeriesError> {
        self.savefile.write_to_file()
    }

    pub fn seasons(&self) -> &Vec<SeasonState> {
        &self.savefile.subseries[&self.subseries].season_states
    }

    pub fn seasons_mut(&mut self) -> &mut Vec<SeasonState> {
        &mut self
            .savefile
            .get_mut_subseries_entry(self.subseries.clone())
            .season_states
    }
}

pub type SubSeriesName = String;

#[derive(Serialize, Deserialize)]
pub struct SaveData {
    pub episode_matcher: Option<String>,
    #[serde(flatten)]
    pub subseries: HashMap<SubSeriesName, SubSeriesData>,
    #[serde(skip)]
    pub path: PathBuf,
}

impl SaveData {
    const DATA_FILE_NAME: &'static str = ".anup";

    pub fn new(path: PathBuf) -> SaveData {
        SaveData {
            episode_matcher: None,
            subseries: HashMap::new(),
            path,
        }
    }

    pub fn get_mut_subseries_entry<S>(&mut self, name: S) -> &mut SubSeriesData
    where
        S: Into<String>,
    {
        self.subseries
            .entry(name.into())
            .or_insert_with(SubSeriesData::new)
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
        let toml = toml::to_string(self)?;
        fs::write(&self.path, toml)?;

        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct SubSeriesData {
    pub files_title: Option<String>,
    pub season_states: Vec<SeasonState>,
}

impl SubSeriesData {
    pub fn new() -> SubSeriesData {
        SubSeriesData {
            files_title: None,
            season_states: Vec::new(),
        }
    }
}

pub type EpisodePaths = HashMap<u32, PathBuf>;
pub type SeriesEpisodes = HashMap<String, EpisodePaths>;

pub struct SeriesInfo {
    pub name: String,
    pub episodes: HashMap<u32, PathBuf>,
}

impl SeriesInfo {
    pub fn get_episode(&self, episode: u32) -> Result<&PathBuf, SeriesError> {
        self.episodes
            .get(&episode)
            .ok_or_else(|| SeriesError::EpisodeNotFound(episode))
    }

    pub fn select_from_subseries(
        ep_data: &mut SeriesEpisodes,
        subseries: &SubSeriesData,
    ) -> Result<Option<SeriesInfo>, SeriesError> {
        if let Some(name) = &subseries.files_title {
            let entry = ep_data.remove_entry(name);

            match entry {
                Some((name, episodes)) => return Ok(Some(SeriesInfo { name, episodes })),
                None => return Err(SeriesError::NoSeriesEpisodes),
            }
        }

        Ok(None)
    }

    pub fn prompt_select_from_episodes(info: SeriesEpisodes) -> Result<SeriesInfo, SeriesError> {
        if info.is_empty() {
            return Err(SeriesError::NoSeriesFound);
        }

        let mut info = info
            .into_iter()
            .map(|(name, eps)| SeriesInfo {
                name,
                episodes: eps,
            })
            .collect::<Vec<_>>();

        if info.len() == 1 {
            return Ok(info.swap_remove(0));
        }

        println!("multiple series found in directory");
        println!("please enter the number next to the episode files you want to use:");

        for (i, series) in info.iter().enumerate() {
            println!("{} [{}]", 1 + i, series.name);
        }

        let index = input::read_range(1, info.len())? - 1;
        let series = info.swap_remove(index);

        Ok(series)
    }
}

struct EpisodeFile {
    series_name: String,
    episode_num: u32,
}

impl EpisodeFile {
    fn parse(path: &Path, matcher: &Regex) -> Result<EpisodeFile, SeriesError> {
        // Replace certain characters with spaces since they can prevent proper series
        // identification or prevent it from being found on a sync backend
        let filename = path
            .file_name()
            .and_then(|path| path.to_str())
            .ok_or(SeriesError::UnableToGetFilename)?;

        let caps = matcher
            .captures(&filename)
            .ok_or(SeriesError::EpisodeRegexCaptureFailed)?;

        let series_name = caps
            .name("name")
            .ok_or_else(|| SeriesError::UnknownRegexCapture("name".into()))
            .map(|name| EpisodeFile::cleanup_title(name.as_str()))?;

        let episode = match caps.name("episode") {
            Some(capture) => capture
                .as_str()
                .parse()
                .map_err(SeriesError::EpisodeNumParseFailed)?,
            None => 1,
        };

        Ok(EpisodeFile {
            series_name,
            episode_num: episode,
        })
    }

    fn cleanup_title(title: &str) -> String {
        let mut clean_title = String::from(title);
        let bytes = unsafe { clean_title.as_bytes_mut() };

        for byte in bytes {
            match byte {
                b'.' | b'_' => *byte = b' ',
                _ => (),
            }
        }

        clean_title
    }
}

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
const EP_FORMAT_REGEX: &str =
    r"(?:\[.+?\](?:_+|\.+|\s*))?(?P<name>.+?)(?:\s*|_*|\.*)-(?:\s*|_*|\.*)(?P<episode>\d+)";

fn format_episode_parser_regex<'a, S>(pattern: Option<S>) -> Result<Cow<'a, Regex>, SeriesError>
where
    S: AsRef<str>,
{
    lazy_static! {
        static ref EP_FORMAT: Regex = Regex::new(EP_FORMAT_REGEX).unwrap();
    }

    match pattern {
        Some(pattern) => {
            let pattern = pattern
                .as_ref()
                .replace("{name}", "(?P<name>.+?)")
                .replace("{episode}", r"(?P<episode>\d+)");

            let regex = Regex::new(&pattern)?;
            Ok(Cow::Owned(regex))
        }
        None => Ok(Cow::Borrowed(&*EP_FORMAT)),
    }
}

pub fn parse_episode_files<S>(
    path: &Path,
    pattern: Option<S>,
) -> Result<SeriesEpisodes, SeriesError>
where
    S: AsRef<str>,
{
    if !path.is_dir() {
        return Err(SeriesError::NotADirectory(path.to_string_lossy().into()));
    }

    let pattern = format_episode_parser_regex(pattern)?;
    let mut data = HashMap::with_capacity(1);

    for entry in fs::read_dir(path).map_err(SeriesError::Io)? {
        let entry = entry.map_err(SeriesError::Io)?.path();

        if !entry.is_file() {
            continue;
        }

        let episode = match EpisodeFile::parse(&entry, pattern.as_ref()) {
            Ok(episode) => episode,
            Err(SeriesError::EpisodeRegexCaptureFailed) => continue,
            Err(err) => return Err(err),
        };

        let series = data
            .entry(episode.series_name)
            .or_insert_with(|| HashMap::with_capacity(1));

        series.insert(episode.episode_num, entry);
    }

    if data.is_empty() {
        return Err(SeriesError::NoSeriesFound);
    }

    Ok(data)
}

pub fn parse_episode_files_until_valid<S>(
    path: &Path,
    pattern: &mut Option<S>,
) -> Result<SeriesEpisodes, SeriesError>
where
    S: AsRef<str> + From<String>,
{
    loop {
        match parse_episode_files(path, pattern.as_ref()) {
            Ok(data) => return Ok(data),
            Err(SeriesError::NoSeriesFound) => {
                eprintln!("error: no series found");
                println!("you will now be prompted to enter a custom regex pattern");
                println!("when entering the pattern, please mark the series name and episode number with {{name}} and {{episode}}, respectively");
                println!("note: if you're trying to play a single file (like a movie) and it doesn't have an episode number, you can omit the {{episode}} marker");
                println!("example:");
                println!("  filename: [SubGroup] Series Name - Ep01 [1080p].mkv");
                println!(r"  pattern: \[.+?\] {{name}} - Ep{{episode}} \[1080p\].mkv");
                println!("please enter your custom pattern:");
            }
            Err(err @ SeriesError::Regex(_)) | Err(err @ SeriesError::UnknownRegexCapture(_)) => {
                eprintln!("error parsing regex pattern: {}", err);
                println!("please try again:");
            }
            Err(err) => return Err(err),
        }

        let line = input::read_line()?;
        *pattern = Some(line.into());
    }
}
