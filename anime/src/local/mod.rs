use crate::err::{self, Result};
use lazy_static::lazy_static;
use regex::Regex;
use serde_derive::{Deserialize, Serialize};
use snafu::{ensure, OptionExt, ResultExt};
use std::collections::HashMap;
use std::fs;
use std::ops::Deref;
use std::path::Path;

/// A regex pattern to parse episode files.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct EpisodeMatcher(#[serde(with = "optional_regex_parser")] Option<Regex>);

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
    pub const DEFAULT_PATTERN: &'static str = r"(?:\[.+?\](?:_+|\.+|\s*))?(?P<title>.+)(?:\s*|_*|\.*)(?:-|\.|_).*?(?P<episode>\d+)(?:\s*?\(|\s*?\[|\.mkv|\.mp4|\.avi)";

    /// Create a new `EpisodeMatcher` with the default matcher.
    #[inline]
    pub fn new() -> EpisodeMatcher {
        EpisodeMatcher(None)
    }

    /// Create a new `EpisodeMatcher` with a specified regex pattern.
    ///
    /// The pattern must have 2 groups named `title` and `episode`. If they
    /// are not present, a `MissingCustomMatcherGroup` error will be returned.
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
    pub fn from_pattern<S>(pattern: S) -> Result<EpisodeMatcher>
    where
        S: AsRef<str>,
    {
        let pattern = pattern.as_ref();

        ensure!(
            pattern.contains("(?P<title>"),
            err::MissingCustomMatcherGroup { group: "title" }
        );

        ensure!(
            pattern.contains("(?P<episode>"),
            err::MissingCustomMatcherGroup { group: "episode" }
        );

        let regex = Regex::new(pattern).context(err::Regex { pattern })?;
        Ok(EpisodeMatcher(Some(regex)))
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
        lazy_static! {
            static ref DEFAULT_MATCHER: Regex =
                Regex::new(EpisodeMatcher::DEFAULT_PATTERN).unwrap();
        }

        match &self.0 {
            Some(matcher) => matcher,
            None => &DEFAULT_MATCHER,
        }
    }
}

mod optional_regex_parser {
    use regex::Regex;
    use serde::de::{self, Visitor};
    use serde::{Deserializer, Serializer};
    use std::fmt;

    pub fn serialize<S>(regex: &Option<Regex>, ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match regex {
            Some(regex) => ser.serialize_some(regex.as_str()),
            None => ser.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(de: D) -> Result<Option<Regex>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct OptionalRegexVisitor;

        impl<'de> Visitor<'de> for OptionalRegexVisitor {
            type Value = Option<Regex>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an optional regex pattern")
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(None)
            }

            fn visit_some<D>(self, de: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = de.deserialize_str(RegexVisitor)?;
                Ok(Some(value))
            }
        }

        struct RegexVisitor;

        impl<'de> Visitor<'de> for RegexVisitor {
            type Value = Regex;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a regex pattern")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let regex = Regex::new(value)
                    .map_err(|err| de::Error::custom(format!("invalid regex pattern: {}", err)))?;

                Ok(regex)
            }
        }

        de.deserialize_option(OptionalRegexVisitor)
    }
}

/// Episode of an anime series on disk.
#[derive(Debug)]
pub struct Episode {
    /// The detected title of the anime series.
    pub series_name: String,
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

        let series_name = caps
            .name("title")
            .context(err::NoEpisodeTitle { name })?
            .as_str()
            .trim()
            .to_string();

        let num = caps
            .name("episode")
            .and_then(|val| val.as_str().parse::<u32>().ok())
            .context(err::ExpectedEpNumber { name })?;

        Ok(Episode { series_name, num })
    }
}

/// A mapping between episode numbers and their filename.
#[derive(Debug, Default)]
pub struct EpisodeMap(HashMap<u32, String>);

impl EpisodeMap {
    /// Detect all episodes in `dir` with the specified `matcher` and return them in a new `EpisodeMap`.
    pub fn parse<P>(dir: P, matcher: &EpisodeMatcher) -> Result<EpisodeMap>
    where
        P: AsRef<Path>,
    {
        let dir = dir.as_ref();
        let entries = fs::read_dir(dir).context(err::FileIO { path: dir })?;
        let mut results = HashMap::new();
        let mut last_title: Option<String> = None;

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

            match &mut last_title {
                Some(last_title) => ensure!(
                    *last_title == episode.series_name,
                    err::MultipleTitles {
                        expecting: last_title.clone(),
                        found: episode.series_name
                    }
                ),
                None => last_title = Some(episode.series_name.clone()),
            }

            results.insert(episode.num, filename.into_owned());
        }

        Ok(EpisodeMap(results))
    }
}

impl Deref for EpisodeMap {
    type Target = HashMap<u32, String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    fn episode_matcher_detect_no_group() {
        EpisodeMatcher::from_pattern("useless").unwrap();
    }

    #[test]
    #[should_panic]
    fn episode_matcher_detect_no_title_group() {
        EpisodeMatcher::from_pattern(r"(.+?) - (?P<episode>\d+)").unwrap();
    }

    #[test]
    #[should_panic]
    fn episode_matcher_detect_no_episode_group() {
        EpisodeMatcher::from_pattern(r"(?P<title>.+?) - \d+").unwrap();
    }
}
