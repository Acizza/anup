mod parse;

pub use parse::{EpisodeParser, EpisodeRegex, ParsedEpisode};

use crate::err::{self, Result};
use snafu::{ensure, ResultExt};
use std::collections::HashMap;
use std::fs;
use std::ops::Deref;
use std::path::Path;

type EpisodeMap = HashMap<u32, String>;

/// A mapping between episode numbers and their filename.
#[derive(Debug, Default)]
pub struct Episodes(EpisodeMap);

impl Episodes {
    #[inline(always)]
    pub fn new(episodes: EpisodeMap) -> Self {
        Self(episodes)
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
            let entry = results
                .entry(parsed.title.unwrap())
                .or_insert_with(|| Self::new(HashMap::with_capacity(13)));

            entry.0.insert(parsed.episode, filename);
            Ok(())
        })?;

        Ok(results)
    }

    /// Find the first matching series episodes in `dir` with the specified `parser`.
    pub fn parse<P>(dir: P, parser: &EpisodeParser) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let mut last_title: Option<String> = None;
        let mut results = HashMap::with_capacity(13);

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

            results.insert(parsed.episode, filename);
            Ok(())
        })?;

        Ok(Self::new(results))
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
}

impl Deref for Episodes {
    type Target = HashMap<u32, String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
