pub mod detect;

pub use detect::{EpisodeParser, EpisodeRegex, ParsedEpisode};

use crate::err::{self, Result};
use crate::SeriesKind;
use snafu::{ensure, ResultExt};
use std::cmp::{Ord, Ordering, PartialOrd};
use std::collections::HashMap;
use std::fs;
use std::ops::{Deref, DerefMut};
use std::path::Path;

/// An episode on disk.
#[derive(Debug)]
pub struct Episode {
    pub number: u32,
    pub filename: String,
}

impl Episode {
    #[inline(always)]
    pub fn new(number: u32, filename: String) -> Self {
        Self { number, filename }
    }
}

impl Ord for Episode {
    fn cmp(&self, other: &Self) -> Ordering {
        self.number.cmp(&other.number)
    }
}

impl PartialOrd for Episode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Episode {
    fn eq(&self, other: &Self) -> bool {
        self.number == other.number
    }
}

impl Eq for Episode {}

/// A list of episodes sorted by episode number.
#[derive(Debug, Default)]
pub struct SortedEpisodes(Vec<Episode>);

impl SortedEpisodes {
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new `SortedEpisodes` struct with the given `episodes`.
    ///
    /// The given `episodes` will be sorted.
    pub fn with_episodes(episodes: Vec<Episode>) -> Self {
        let mut episodes = Self(episodes);
        episodes.sort();
        episodes
    }

    #[inline(always)]
    fn with_capacity(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }

    /// Consumes the struct and returns the contained episodes.
    #[inline(always)]
    pub fn take(self) -> Vec<Episode> {
        self.0
    }

    #[inline(always)]
    fn push(&mut self, episode: Episode) {
        self.0.push(episode);
    }

    /// Returns a reference to the episode with the specified `number`.
    #[inline]
    pub fn find(&self, episode_num: u32) -> Option<&Episode> {
        self.0
            .binary_search_by_key(&episode_num, |ep| ep.number)
            .ok()
            .map(|index| &self.0[index])
    }

    fn sort(&mut self) {
        self.0.sort_unstable();
        self.0.dedup();
    }
}

impl Deref for SortedEpisodes {
    type Target = Vec<Episode>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub type EpisodeMap = HashMap<SeriesKind, SortedEpisodes>;

/// A list of episodes on disk.
#[derive(Debug, Default)]
pub struct CategorizedEpisodes(EpisodeMap);

impl CategorizedEpisodes {
    /// Create a new `CategorizedEpisodes` struct with the specified `episodes`.
    #[inline(always)]
    pub fn with_sorted(episodes: EpisodeMap) -> Self {
        Self(episodes)
    }

    /// Returns true if multiple episode categories are present.
    #[inline(always)]
    pub fn has_multiple_categories(&self) -> bool {
        self.0.len() > 1
    }

    /// Consumes the struct and returns episodes if only one episode category is present.
    #[inline]
    pub fn take_only_category(self) -> Option<SortedEpisodes> {
        if self.has_multiple_categories() {
            return None;
        }

        self.0.into_iter().next().map(|(_, episodes)| episodes)
    }

    /// Consumes the struct and returns seasonal episodes, or, if there's only one episode category, those episodes.
    #[inline]
    pub fn take_season_episodes_or_present(mut self) -> Option<SortedEpisodes> {
        self.0
            .remove(&SeriesKind::Season)
            .or_else(|| self.take_only_category())
    }

    /// Consumes the struct and returns the contained episodes.
    #[inline(always)]
    pub fn take(self) -> EpisodeMap {
        self.0
    }

    /// Find all series and episodes in `dir` with the specified `parser`.
    ///
    /// The matcher must have the title group specified, or a `NeedTitleGroup` error will be returned.
    pub fn parse_all<P>(dir: P, parser: &EpisodeParser) -> Result<HashMap<String, Self>>
    where
        P: AsRef<Path>,
    {
        ensure!(parser.has_title(), err::NeedTitleGroup);

        let mut results = HashMap::with_capacity(1);

        Self::parse_eps_in_dir_with(dir, parser, |parsed, filename| {
            let title_entry = results
                .entry(parsed.title.unwrap())
                .or_insert_with(|| Self(HashMap::with_capacity(1)));

            let cat_eps = title_entry
                .0
                .entry(parsed.category)
                .or_insert_with(|| SortedEpisodes::with_capacity(1));

            let episode = Episode::new(parsed.episode, filename);
            cat_eps.push(episode);

            Ok(())
        })?;

        for series in results.values_mut() {
            Self::sort_all(&mut series.0);
        }

        Ok(results)
    }

    /// Find the first matching series episodes in `dir` with the specified `parser`.
    pub fn parse<P>(dir: P, parser: &EpisodeParser) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let mut last_title: Option<String> = None;
        let mut episodes = HashMap::with_capacity(1);

        Self::parse_eps_in_dir_with(dir, parser, |parsed, filename| {
            if let Some(series_name) = parsed.title {
                match &mut last_title {
                    Some(last_title) => ensure!(
                        *last_title == series_name,
                        err::MultipleTitles {
                            expecting: last_title.clone(),
                            found: series_name
                        }
                    ),
                    None => last_title = Some(series_name),
                }
            }

            let cat_epsisodes = episodes
                .entry(parsed.category)
                .or_insert_with(|| SortedEpisodes::with_capacity(1));

            let episode = Episode::new(parsed.episode, filename);
            cat_epsisodes.push(episode);

            Ok(())
        })?;

        Self::sort_all(&mut episodes);

        Ok(Self(episodes))
    }

    fn parse_eps_in_dir_with<P, F>(dir: P, parser: &EpisodeParser, mut inserter: F) -> Result<()>
    where
        P: AsRef<Path>,
        F: FnMut(ParsedEpisode, String) -> Result<()>,
    {
        let dir = dir.as_ref();
        let entries = fs::read_dir(dir).context(err::FileIO { path: dir })?;

        for entry in entries {
            let entry = entry.context(err::EntryIO { dir })?;
            let entry_type = entry.file_type().context(err::EntryIO { dir })?;

            if entry_type.is_dir() {
                continue;
            }

            let filename = entry.file_name();
            let filename = filename.to_string_lossy();

            // The .part extension is commonly used to indicate that a file is incomplete
            if filename.ends_with(".part") {
                continue;
            }

            let episode = parser.parse(filename.as_ref())?;
            inserter(episode, filename.into_owned())?;
        }

        Ok(())
    }

    fn sort_all(episode_cats: &mut EpisodeMap) {
        for episodes in episode_cats.values_mut() {
            episodes.sort();
        }
    }
}

impl Deref for CategorizedEpisodes {
    type Target = EpisodeMap;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for CategorizedEpisodes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
