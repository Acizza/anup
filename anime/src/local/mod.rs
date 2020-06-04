pub mod detect;

pub use detect::{EpisodeParser, EpisodeRegex, ParsedEpisode};

use crate::err::{self, Result};
use snafu::{ensure, ResultExt};
use std::cmp::{Ord, Ordering, PartialOrd};
use std::collections::HashMap;
use std::fs;
use std::ops::Deref;
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

/// A list of episodes on disk.
#[derive(Debug, Default)]
pub struct Episodes(Vec<Episode>);

impl Episodes {
    /// Create a new `Episodes` struct with the specified `episodes`.
    ///
    /// This function assumes that `episodes` is sorted by episode number.
    /// Expect issues with the [get()](#method.get) method if they are in fact not sorted.
    #[inline(always)]
    pub fn with_sorted(episodes: Vec<Episode>) -> Self {
        Self(episodes)
    }

    /// Find all series and episodes in `dir` with the specified `parser`.
    ///
    /// The matcher must have the title group specified, or a `NeedTitleGroup` error will be returned.
    /// The returned episodes are also guaranteed to be sorted.
    pub fn parse_all<P>(dir: P, parser: &EpisodeParser) -> Result<HashMap<String, Self>>
    where
        P: AsRef<Path>,
    {
        ensure!(parser.has_title(), err::NeedTitleGroup);

        let mut results = HashMap::with_capacity(1);

        Self::parse_eps_in_dir_with(dir, parser, |parsed, filename| {
            let entry = results
                .entry(parsed.title.unwrap())
                .or_insert_with(|| Self(Vec::with_capacity(13)));

            entry.0.push(Episode::new(parsed.episode, filename));
            Ok(())
        })?;

        for series in results.values_mut() {
            series.0.sort_unstable();
            series.0.dedup();
        }

        Ok(results)
    }

    /// Find the first matching series episodes in `dir` with the specified `parser`.
    ///
    /// The returned episodes are guaranteed to be sorted.
    pub fn parse<P>(dir: P, parser: &EpisodeParser) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let mut last_title: Option<String> = None;
        let mut results = Vec::with_capacity(13);

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

            results.push(Episode::new(parsed.episode, filename));
            Ok(())
        })?;

        results.sort_unstable();
        results.dedup();

        Ok(Self(results))
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

    /// Get a reference to the episode with the specified `number`.
    #[inline]
    pub fn get(&self, number: u32) -> Option<&Episode> {
        self.0
            .binary_search_by_key(&number, |episode| episode.number)
            .ok()
            .map(|index| &self.0[index])
    }
}

impl Deref for Episodes {
    type Target = Vec<Episode>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
