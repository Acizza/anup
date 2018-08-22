use backend::AnimeEntry;
use error::SeriesError;
use input;
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use toml;

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

#[derive(Clone, Serialize, Deserialize)]
pub struct SeasonState {
    #[serde(flatten)]
    pub state: AnimeEntry,
    pub needs_info: bool,
    pub needs_sync: bool,
}

type SeriesName = String;
type EpisodeNum = u32;

pub struct EpisodeData {
    pub series_name: String,
    pub episodes: HashMap<u32, PathBuf>,
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
