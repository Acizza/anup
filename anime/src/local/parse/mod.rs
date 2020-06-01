mod default;

use crate::err::{self, Error, Result};
use regex::Regex;
use snafu::{OptionExt, ResultExt};
use std::borrow::Cow;
use std::str;

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
#[derive(Clone, Debug)]
#[cfg_attr(
    feature = "diesel-support",
    derive(AsExpression, FromSqlRow),
    sql_type = "Text"
)]
pub struct EpisodeRegex {
    regex: Regex,
    has_title: bool,
}

impl EpisodeRegex {
    /// Create a new [EpisodeRegex](#struct.EpisodeRegex) with a specified regex pattern.
    ///
    /// The pattern must have a group named `episode` and optional one named `title`. If the episode
    /// group is not present, a `MissingMatcherGroups` error will be returned.
    ///
    /// # Example
    ///
    /// ```
    /// use anime::local::EpisodeRegex;
    ///
    /// let regex = EpisodeRegex::from_pattern(r"(?P<title>.+?) - (?P<episode>\d+)").unwrap();
    /// let pattern = r"(?P<title>.+?) - (?P<episode>\d+)";
    ///
    /// assert_eq!(regex.get().as_str(), pattern);
    /// ```
    pub fn from_pattern<S>(pattern: S) -> Result<Self>
    where
        S: AsRef<str>,
    {
        let pattern = pattern.as_ref();

        if !pattern.contains("(?P<episode>") {
            return Err(Error::MissingMatcherGroups);
        }

        let regex = Regex::new(pattern).context(err::Regex { pattern })?;

        Ok(Self {
            regex,
            has_title: pattern.contains("(?P<title>"),
        })
    }

    /// Create a new [EpisodeRegex](#struct.EpisodeRegex) with the specified regex pattern containing arbitrary `title` and `episode` groups.
    ///
    /// This works the same as [from_pattern](#method.from_pattern), but replaces text in the pattern matching `title` and `episode` to their regex
    /// representation first.
    ///
    /// # Example
    ///
    /// ```
    /// use anime::local::EpisodeRegex;
    ///
    /// let regex = EpisodeRegex::from_pattern_replacements("{title} - {episode}", "{title}", "{episode}").unwrap();
    /// let actual_pattern = r"(?P<title>.+) - (?P<episode>\d+)";
    ///
    /// assert_eq!(regex.get().as_str(), actual_pattern);
    /// ```
    #[inline]
    pub fn from_pattern_replacements<S, T, E>(pattern: S, title: T, episode: E) -> Result<Self>
    where
        S: AsRef<str>,
        T: AsRef<str>,
        E: AsRef<str>,
    {
        let pattern = pattern
            .as_ref()
            .replace(title.as_ref(), r"(?P<title>.+)")
            .replace(episode.as_ref(), r"(?P<episode>\d+)");

        Self::from_pattern(pattern)
    }

    /// Returns a reference to the inner `Regex` for the `EpisodeRegex`.
    #[inline(always)]
    pub fn get(&self) -> &Regex {
        &self.regex
    }
}

impl PartialEq for EpisodeRegex {
    fn eq(&self, other: &Self) -> bool {
        self.get().as_str() == other.get().as_str()
    }
}

#[cfg(feature = "diesel-support")]
impl<DB> FromSql<Text, DB> for EpisodeRegex
where
    DB: diesel::backend::Backend,
    String: FromSql<Text, DB>,
{
    fn from_sql(bytes: Option<&DB::RawValue>) -> deserialize::Result<Self> {
        let pattern = String::from_sql(bytes)?;
        let matcher = Self::from_pattern(pattern)
            .map_err(|err| format!("invalid episode regex pattern: {}", err))?;

        Ok(matcher)
    }
}

#[cfg(feature = "diesel-support")]
impl<DB> ToSql<Text, DB> for EpisodeRegex
where
    DB: diesel::backend::Backend,
    str: ToSql<Text, DB>,
{
    fn to_sql<W: Write>(&self, out: &mut Output<W, DB>) -> serialize::Result {
        let value = self.regex.as_str();
        value.to_sql(out)
    }
}

/// An episode file parser.
///
/// It can be used with a default parser that tries to match as many formats as (reasonably) possible, or a custom one that
/// takes a regex pattern.
///
/// The default parser works well with files that are in one of the following formats:
///
/// `<tags> <title> - <episode> <tags>`
///
/// `<tags> <title> <episode> <tags>`
///
/// `<tags> <episode> - <title> <tags>`
///
/// `<tags> <episode> <title> <tags>`
///
/// The default parser also accounts for different types of whitespace characters, such as '.' and '_'.
#[derive(Clone, Debug)]
#[cfg_attr(
    feature = "diesel-support",
    derive(AsExpression, FromSqlRow),
    sql_type = "Text"
)]
pub enum EpisodeParser {
    Default,
    Custom(EpisodeRegex),
}

impl EpisodeParser {
    /// Create a new [EpisodeParser::Custom](#variant.Custom) with the specified regex pattern containing arbitrary `title` and `episode` groups.
    ///
    /// Refer to [EpisodeRegex::from_pattern_replacements](struct.EpisodeRegex.html#method.from_pattern_replacements) for more information.
    ///
    /// # Example
    ///
    /// ```
    /// use anime::local::EpisodeParser;
    ///
    /// let parser = EpisodeParser::custom_with_replacements("{title} - {episode}", "{title}", "{episode}").unwrap();
    /// let actual_pattern = r"(?P<title>.+) - (?P<episode>\d+)";
    ///
    /// match parser {
    ///     EpisodeParser::Default => unreachable!(),
    ///     EpisodeParser::Custom(regex) => assert_eq!(regex.get().as_str(), actual_pattern),
    /// }
    /// ```
    #[inline]
    pub fn custom_with_replacements<S, T, E>(pattern: S, title: T, episode: E) -> Result<Self>
    where
        S: AsRef<str>,
        T: AsRef<str>,
        E: AsRef<str>,
    {
        let regex = EpisodeRegex::from_pattern_replacements(pattern, title, episode)?;
        Ok(Self::Custom(regex))
    }

    /// Attempt to parse the given `filename` with the currently selected parser.
    ///
    /// # Example With Default Parser
    ///
    /// ```
    /// use anime::local::EpisodeParser;
    ///
    /// let parser = EpisodeParser::default();
    /// let result = parser.parse("[Header 1][Header 2] Series Title - 02.mkv").unwrap();
    ///
    /// assert_eq!(result.title, Some("Series Title".into()));
    /// assert_eq!(result.episode, 2);
    /// ```
    ///
    /// # Example With Custom Parser
    ///
    /// ```
    /// use anime::local::{EpisodeParser, EpisodeRegex};
    ///
    /// let regex = EpisodeRegex::from_pattern(r"Surrounded (?P<episode>\d+) Episode").unwrap();
    /// let parser = EpisodeParser::Custom(regex);
    /// let result = parser.parse("Surrounded 123 Episode").unwrap();
    ///
    /// assert_eq!(result.title, None);
    /// assert_eq!(result.episode, 123);
    /// ```
    #[inline]
    pub fn parse<'a, S>(&self, filename: S) -> Result<ParsedEpisode>
    where
        S: Into<Cow<'a, str>>,
    {
        let filename = filename.into();

        match self {
            Self::Default => Self::parse_with_default(filename),
            Self::Custom(regex) => Self::parse_with_regex(regex, filename),
        }
    }

    fn parse_with_default<S>(filename: S) -> Result<ParsedEpisode>
    where
        S: AsRef<str>,
    {
        let mut filename = filename.as_ref();

        // The filename extension can cause issues when trying to parse the <episode> - <title> format.
        // This is due to having '.' as a whitespace character, which causes the parser to interpret the
        // extension as part of the series title.
        if let Some(index) = filename.rfind('.') {
            filename = &filename[..index];
        }

        let (title, ep_num) = default::title_and_episode::parse(filename)
            .or_else(|| default::episode_and_title::parse(filename))
            .context(err::EpisodeParseFailed { filename })?;

        Ok(ParsedEpisode::new(Some(title), ep_num))
    }

    fn parse_with_regex<S>(regex: &EpisodeRegex, filename: S) -> Result<ParsedEpisode>
    where
        S: AsRef<str>,
    {
        let filename = filename.as_ref();

        let caps = regex
            .get()
            .captures(filename)
            .context(err::EpisodeParseFailed { filename })?;

        let series_name = if regex.has_title {
            caps.name("title")
                .context(err::NoEpisodeTitle { filename })?
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
            .context(err::ExpectedEpNumber { filename })?;

        Ok(ParsedEpisode::new(series_name, num))
    }

    /// Returns true if the current parser supports title parsing.
    ///
    /// The default parser will always return true.
    #[inline]
    pub fn has_title(&self) -> bool {
        match self {
            Self::Default => true,
            Self::Custom(regex) => regex.has_title,
        }
    }
}

impl Default for EpisodeParser {
    fn default() -> Self {
        Self::Default
    }
}

impl PartialEq for EpisodeParser {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Default, Self::Default) => true,
            (Self::Default, Self::Custom(_)) => false,
            (Self::Custom(_), Self::Default) => false,
            (Self::Custom(pat1), Self::Custom(pat2)) => pat1 == pat2,
        }
    }
}

impl<'a> Into<Cow<'a, Self>> for EpisodeParser {
    fn into(self) -> Cow<'a, Self> {
        Cow::Owned(self)
    }
}

impl<'a> Into<Cow<'a, EpisodeParser>> for &'a EpisodeParser {
    fn into(self) -> Cow<'a, EpisodeParser> {
        Cow::Borrowed(self)
    }
}

#[cfg(feature = "diesel-support")]
impl<DB> FromSql<Nullable<Text>, DB> for EpisodeParser
where
    DB: diesel::backend::Backend,
    EpisodeRegex: FromSql<Text, DB>,
{
    fn from_sql(bytes: Option<&DB::RawValue>) -> deserialize::Result<Self> {
        if bytes.is_some() {
            let regex = EpisodeRegex::from_sql(bytes)?;
            Ok(Self::Custom(regex))
        } else {
            Ok(Self::default())
        }
    }
}

#[cfg(feature = "diesel-support")]
impl<DB> ToSql<Text, DB> for EpisodeParser
where
    DB: diesel::backend::Backend,
    EpisodeRegex: ToSql<Text, DB>,
{
    fn to_sql<W: Write>(&self, out: &mut Output<W, DB>) -> serialize::Result {
        use diesel::serialize::IsNull;

        match self {
            Self::Default => Ok(IsNull::Yes),
            Self::Custom(regex) => regex.to_sql(out),
        }
    }
}

/// The detected title and episode number from an episode file.
pub struct ParsedEpisode {
    /// The parsed title of the episode file.
    pub title: Option<String>,
    /// The parsed episode number of the episode file.
    pub episode: u32,
}

impl ParsedEpisode {
    #[inline(always)]
    fn new(title: Option<String>, episode: u32) -> Self {
        Self { title, episode }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    enum ExpectedTitle {
        Default(&'static str),
        Custom(&'static str, &'static str),
    }

    impl ExpectedTitle {
        const DEFAULT_TITLE: &'static str = "Series Title";

        fn fmt(&self) -> &'static str {
            match self {
                Self::Default(fmt) => fmt,
                Self::Custom(fmt, _) => fmt,
            }
        }

        fn expected(&self) -> &'static str {
            match self {
                Self::Default(_) => Self::DEFAULT_TITLE,
                Self::Custom(_, title) => title,
            }
        }
    }

    #[test]
    fn episode_format_detection() {
        const EXPECTED_EP_NUM: u32 = 12;

        let def = ExpectedTitle::Default;
        let cus = ExpectedTitle::Custom;

        let formats = vec![
            def("Series Title - 12.mkv"),
            def("Series Title - E12.mkv"),
            def("  Series Title - 12.mkv"),
            def("[Header 1] Series Title - 12.mkv"),
            def("[Header 1][Header 2] Series Title - 12.mkv"),
            def("[Header 1] [Header 2] Series Title - 12.mkv"),
            def("[Header]Series Title - 12.mkv"),
            def("[Header 1][Header 2]Series Title - 12.mkv"),
            def("Series Title 12.mkv"),
            def("[Header 1] Series Title 12.mkv"),
            def("[Header 1] Series Title E12 [1080].mkv"),
            def("[Header 1] Series Title 12 [1080].mkv"),
            def("[Header.1].Series.Title.-.12.mkv"),
            def("[Header_1]_Series_Title_12.mkv"),
            def("[Header 1] Series Title - 12 [10].mkv"),
            def("[Header 1] Series Title - 12 [10][test].mkv"),
            def("[Header 1] Series Title - S01E12 (10).mkv"),
            def("[Header 1] Series Title - E12 (10).mkv"),
            cus("[Header 1] 1 Series Title - 12 [10].mkv", "1 Series Title"),
            cus(
                "[Header 1] 1 2 Series Title - 12 [10].mkv",
                "1 2 Series Title",
            ),
            cus(
                "[Header 1] 1 2 Series Title 3 - 12 [10].mkv",
                "1 2 Series Title 3",
            ),
            cus("[Header 1] Series 2 Title - 12 [10].mkv", "Series 2 Title"),
            cus("[Header 1] Series Title 02 - 12.mkv", "Series Title 02"),
            cus("[Header 1] Series Title 2 12", "Series Title 2"),
            def("12 Series Title.mkv"),
            def("S01E12 - Series Title.mkv"),
            def("E12 - Series Title.mkv"),
            def("12 - Series Title.mkv"),
            def("12_Series_Title.mkv"),
            def("12_-_Series_Title.mkv"),
            def("[Header 1] 12 Series Title.mkv"),
            def("[Header.1].12.Series.Title.mkv"),
            def("[Header 1] 12 - Series Title.mkv"),
            def("[Header 1] 12 Series Title [1080].mkv"),
            def("[Header 1] 12 - Series Title [1080][test].mkv"),
            cus(
                "[Header 1] 12 - Series Title 02 [1080][test].mkv",
                "Series Title 02",
            ),
            cus(
                "[Header 1] 12 - 1 Series Title 2 [10].mkv",
                "1 Series Title 2",
            ),
            def("(Header 1) (Header 2) Series Title - 12.mkv"),
            cus(
                "[Header 1] Mutli-Separated 1-Title 2 - 12 [10].mkv",
                "Mutli-Separated 1-Title 2",
            ),
            cus("[Header 1] Mutli - Title - 12 [10].mkv", "Mutli - Title"),
            cus("[Header 1] 12 - Multi - Title [10].mkv", "Multi - Title"),
            cus(
                "[Header 1] Non @ Alpha ' Numeric : Characters - 12 [10].mkv",
                "Non @ Alpha ' Numeric : Characters",
            ),
            def("[Header 1].Series.Title.E12.[10].mkv"),
            def("[Header 1].Series.Title.Ep.12.[10].mkv"),
            def("[Header 1].Series.Title.Episode.12.[10].mkv"),
        ];

        let parser = EpisodeParser::default();

        for format in &formats {
            match parser.parse(format.fmt()) {
                Ok(parsed) => {
                    assert_eq!(
                        parsed.episode, EXPECTED_EP_NUM,
                        "episode number mismatch: {:?}",
                        format
                    );

                    match parsed.title {
                        Some(title) => assert_eq!(
                            title,
                            format.expected(),
                            "episode title mismatch: {:?}",
                            format
                        ),
                        None => panic!(
                            "expected series title, got nothing while parsing format: {:?}",
                            format
                        ),
                    }
                }
                Err(err) => panic!(
                    "failed to parse episode format: {:?} :: err = {}",
                    format, err
                ),
            }
        }
    }

    #[test]
    fn ambiguous_episode_format_detection() {
        let formats = vec![
            "[Header 1] 12 - Series Title - 12.mkv",
            "[Header 1] 12 - Multi - Title - 12 [10].mkv",
        ];

        let parser = EpisodeParser::default();

        for format in &formats {
            match parser.parse(*format) {
                Ok(parsed) => panic!(
                    "ambiguous episode format was parsed:\ntitle = {:?}\nepisode = {}\nformat = {}",
                    parsed.title, parsed.episode, format
                ),
                Err(_) => (),
            }
        }
    }

    #[test]
    #[should_panic]
    fn episode_regex_detect_no_group() {
        EpisodeRegex::from_pattern("useless").unwrap();
    }

    #[test]
    fn episode_regex_detect_no_title_group() {
        EpisodeRegex::from_pattern(r"(.+?) - (?P<episode>\d+)").unwrap();
    }

    #[test]
    #[should_panic]
    fn episode_regex_detect_no_episode_group() {
        EpisodeRegex::from_pattern(r"(?P<title>.+?) - \d+").unwrap();
    }
}
