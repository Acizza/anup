use crate::err::{self, Error, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use snafu::{ensure, OptionExt, ResultExt};
use std::collections::HashMap;
use std::fs;
use std::ops::Deref;
use std::path::Path;

#[cfg(feature = "diesel-support")]
use {
    diesel::{
        deserialize::{self, FromSql},
        serialize::{self, Output, ToSql},
        sql_types::{Nullable, Text},
    },
    std::io::Write,
};

/// A regex pattern to parse episode files.
#[derive(Clone, Debug, Default)]
#[cfg_attr(
    feature = "diesel-support",
    derive(AsExpression, FromSqlRow),
    sql_type = "Text"
)]
pub struct EpisodeMatcher {
    matcher: Option<Regex>,
    has_title: bool,
}

impl EpisodeMatcher {
    /// The default regex pattern to match episodes in several common formats, such as:
    ///
    /// * [Group] Series Name - 01.mkv
    /// * [Group]_Series_Name_-_01.mkv
    /// * [Group].Series.Name.-.01.mkv
    /// * [Group] Series Name - 01 [tag 1][tag 2].mkv
    /// * [Group]_Series_Name_-_01_[tag1][tag2].mkv
    /// * [Group].Series.Name.-.01.[tag1][tag2].mkv
    /// * Series Name - 01.mkv
    /// * Series_Name_-_01.mkv
    /// * Series.Name.-.01.mkv
    pub const DEFAULT_PATTERN: &'static str = r"(?:\[.+?\](?:_+|\.+|\s*))?(?P<title>.+)(?:\s*|_*|\.*)(?:-|\.|_).*?(?P<episode>\d+)(?:\s*?\(|\s*?\[|\.mkv|\.mp4|\.mpv|\.avi)";

    /// Create a new `EpisodeMatcher` with the default matcher.
    #[inline]
    pub fn new() -> Self {
        Self {
            matcher: None,
            has_title: true,
        }
    }

    /// Create a new `EpisodeMatcher` with a specified regex pattern.
    ///
    /// The pattern must have a group named `episode` and optional one named `title`. If the episode
    /// group is not present, a `MissingMatcherGroups` error will be returned.
    ///
    /// # Example
    ///
    /// ```
    /// use anime::local::EpisodeMatcher;
    ///
    /// let pattern = r"(?P<title>.+?) - (?P<episode>\d+)";
    /// let matcher = EpisodeMatcher::from_pattern(pattern).unwrap();
    ///
    /// assert_eq!(matcher.get().as_str(), pattern);
    /// ```
    #[inline]
    pub fn from_pattern<S>(pattern: S) -> Result<Self>
    where
        S: AsRef<str>,
    {
        let pattern = pattern.as_ref();

        if !pattern.contains("(?P<episode>") {
            return Err(Error::MissingMatcherGroups);
        }

        let matcher = Regex::new(pattern).context(err::Regex { pattern })?;

        Ok(Self {
            matcher: Some(matcher),
            has_title: pattern.contains("(?P<title>"),
        })
    }

    /// Returns a reference to the inner `Regex` for the `EpisodeMatcher`.
    ///
    /// # Example
    ///
    /// ```
    /// use anime::local::EpisodeMatcher;
    ///
    /// let default_matcher = EpisodeMatcher::new();
    /// let custom_matcher = EpisodeMatcher::from_pattern(r"(?P<title>.+?) - (?P<episode>\d+)").unwrap();
    ///
    /// assert_eq!(default_matcher.get().as_str(), EpisodeMatcher::DEFAULT_PATTERN);
    /// assert_eq!(custom_matcher.get().as_str(), r"(?P<title>.+?) - (?P<episode>\d+)");
    /// ```
    #[inline]
    pub fn get(&self) -> &Regex {
        static DEFAULT_MATCHER: Lazy<Regex> =
            Lazy::new(|| Regex::new(EpisodeMatcher::DEFAULT_PATTERN).unwrap());

        match &self.matcher {
            Some(matcher) => matcher,
            None => &DEFAULT_MATCHER,
        }
    }
}

#[cfg(feature = "diesel-support")]
impl<DB> FromSql<Nullable<Text>, DB> for EpisodeMatcher
where
    DB: diesel::backend::Backend,
    String: FromSql<Text, DB>,
{
    fn from_sql(bytes: Option<&DB::RawValue>) -> deserialize::Result<Self> {
        match bytes {
            Some(_) => {
                let pattern = String::from_sql(bytes)?;
                let matcher = Self::from_pattern(pattern)
                    .map_err(|err| format!("invalid episode matcher pattern: {}", err))?;

                Ok(matcher)
            }
            None => Ok(Self::new()),
        }
    }
}

#[cfg(feature = "diesel-support")]
impl<DB> ToSql<Text, DB> for EpisodeMatcher
where
    DB: diesel::backend::Backend,
    str: ToSql<Text, DB>,
{
    fn to_sql<W: Write>(&self, out: &mut Output<W, DB>) -> serialize::Result {
        let value = self.matcher.as_ref().map(|matcher| matcher.as_str());
        value.to_sql(out)
    }
}

/// Episode of an anime series on disk.
#[derive(Debug)]
pub struct Episode {
    /// The detected title of the anime series.
    pub series_name: Option<String>,
    /// The detected episode number.
    pub num: u32,
}

impl Episode {
    pub fn parse<'a, S>(name: S, matcher: &EpisodeMatcher) -> Result<Episode>
    where
        S: AsRef<str> + Into<String> + 'a,
    {
        let name = name.as_ref();

        let caps = matcher
            .get()
            .captures(name)
            .context(err::NoEpMatches { name })?;

        let series_name = if matcher.has_title {
            caps.name("title")
                .context(err::NoEpisodeTitle { name })?
                .as_str()
                .trim()
                .to_string()
                .into()
        } else {
            None
        };

        let num = caps
            .name("episode")
            .and_then(|val| val.as_str().parse::<u32>().ok())
            .context(err::ExpectedEpNumber { name })?;

        Ok(Episode { series_name, num })
    }
}

type EpisodeMap = HashMap<u32, String>;

/// A mapping between episode numbers and their filename.
#[derive(Debug, Default)]
pub struct Episodes(EpisodeMap);

impl Episodes {
    #[inline(always)]
    pub fn new(episodes: EpisodeMap) -> Self {
        Self(episodes)
    }

    /// Find all series and episodes in `dir` with the specified `matcher`.
    ///
    /// The matcher must have the title group specified, or a `NeedTitleGroup` error will be returned.
    pub fn parse_all<P>(dir: P, matcher: &EpisodeMatcher) -> Result<HashMap<String, Self>>
    where
        P: AsRef<Path>,
    {
        ensure!(matcher.has_title, err::NeedTitleGroup);

        let mut results = HashMap::with_capacity(1);

        Self::parse_eps_in_dir_with(dir, matcher, |episode, filename| {
            let entry = results
                .entry(episode.series_name.unwrap())
                .or_insert_with(|| Self::new(HashMap::with_capacity(13)));

            entry.0.insert(episode.num, filename);
            Ok(())
        })?;

        Ok(results)
    }

    /// Find the first matching series episodes in `dir` with the specified `matcher`.
    pub fn parse<P>(dir: P, matcher: &EpisodeMatcher) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let mut last_title: Option<String> = None;
        let mut results = HashMap::with_capacity(13);

        Self::parse_eps_in_dir_with(dir, matcher, |episode, filename| {
            if let Some(series_name) = episode.series_name {
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

            results.insert(episode.num, filename);
            Ok(())
        })?;

        Ok(Self::new(results))
    }

    fn parse_eps_in_dir_with<P, F>(dir: P, matcher: &EpisodeMatcher, mut inserter: F) -> Result<()>
    where
        P: AsRef<Path>,
        F: FnMut(Episode, String) -> Result<()>,
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

            let episode = Episode::parse(filename.as_ref(), matcher)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn episode_matcher_default_has_title() {
        let matcher = EpisodeMatcher::new();
        assert!(matcher.has_title)
    }

    #[test]
    #[should_panic]
    fn episode_matcher_detect_no_group() {
        EpisodeMatcher::from_pattern("useless").unwrap();
    }

    #[test]
    fn episode_matcher_detect_no_title_group() {
        EpisodeMatcher::from_pattern(r"(.+?) - (?P<episode>\d+)").unwrap();
    }

    #[test]
    #[should_panic]
    fn episode_matcher_detect_no_episode_group() {
        EpisodeMatcher::from_pattern(r"(?P<title>.+?) - \d+").unwrap();
    }
}
