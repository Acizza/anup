pub mod dir;
pub mod episode;

mod common;

use crate::err::{Error, Result};
use crate::SeriesKind;
use smallvec::SmallVec;
use std::borrow::Cow;
use std::ops::{Deref, DerefMut};
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

/// An episode file parser.
///
/// It can be used with a default parser that tries to match as many formats as (reasonably) possible, or with a custom pattern.
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
    Custom(CustomPattern),
}

impl EpisodeParser {
    /// Create a new [EpisodeParser::Custom](#variant.Custom) with the specified custom pattern.
    ///
    /// # Example
    ///
    /// ```
    /// use anime::local::EpisodeParser;
    ///
    /// let parser = EpisodeParser::custom("Series Title - #.mkv");
    /// let value = "Series Title - 12.mkv";
    ///
    /// let result = parser.parse("Series Title - 12.mkv").unwrap();
    /// assert_eq!(result.episode, 12);
    /// ```
    #[inline]
    pub fn custom<S>(pattern: S) -> Self
    where
        S: Into<String>,
    {
        let pattern = CustomPattern::new(pattern);
        Self::Custom(pattern)
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
    #[inline]
    pub fn parse<'a, S>(&self, filename: S) -> Result<ParsedEpisode>
    where
        S: Into<Cow<'a, str>>,
    {
        let filename = filename.into();

        match self {
            Self::Default => Self::parse_with_default(filename),
            Self::Custom(pattern) => Self::parse_with_pattern(pattern, filename),
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

        episode::title_and_episode::parse(filename)
            .or_else(|| episode::episode_and_title::parse(filename))
            .or_else(|| episode::title_episode_desc::parse(filename))
            .ok_or_else(|| Error::EpisodeParseFailed {
                filename: filename.into(),
            })
    }

    fn parse_with_pattern<S>(pattern: &CustomPattern, filename: S) -> Result<ParsedEpisode>
    where
        S: AsRef<str>,
    {
        let filename = filename.as_ref();

        let ep_num = pattern
            .detect_episode(filename)
            .ok_or_else(|| Error::EpisodeParseFailed {
                filename: filename.into(),
            })?;

        // TODO: look for special / OVA / ONA / movie in the title to categorize properly
        let episode = ParsedEpisode::new(None, ep_num, SeriesKind::Season);
        Ok(episode)
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
    CustomPattern: FromSql<Text, DB>,
{
    fn from_sql(bytes: Option<&DB::RawValue>) -> deserialize::Result<Self> {
        if bytes.is_some() {
            let pattern = CustomPattern::from_sql(bytes)?;
            Ok(Self::Custom(pattern))
        } else {
            Ok(Self::default())
        }
    }
}

#[cfg(feature = "diesel-support")]
impl<DB> ToSql<Text, DB> for EpisodeParser
where
    DB: diesel::backend::Backend,
    CustomPattern: ToSql<Text, DB>,
{
    fn to_sql<W: Write>(&self, out: &mut Output<W, DB>) -> serialize::Result {
        use diesel::serialize::IsNull;

        match self {
            Self::Default => Ok(IsNull::Yes),
            Self::Custom(pattern) => pattern.to_sql(out),
        }
    }
}

/// A custom pattern to match episodes with.
///
/// This is intended to be a very simple regex replacement.
/// The pattern matches given input 1-to-1, except when `*` and `#` are encountered.

/// * `*` is a wildcard and will match everything up to the next character in the pattern.
/// * `#` is an episode marker and will only match digits. Everything after this character is ignored.
///
/// Both pattern characters can be escaped by having at least two of them next to each other, like so:
/// * `**`
/// * `##`
///
/// # Example
///
/// ```
/// use anime::local::detect::CustomPattern;
///
/// let pattern = CustomPattern::new("[*] Series Title - EP#");
/// assert_eq!(pattern.detect_episode("[Test Tag] Series Title - ep12"), Some(12));
/// ```
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "diesel-support",
    derive(AsExpression, FromSqlRow),
    sql_type = "Text"
)]
pub struct CustomPattern(String);

impl CustomPattern {
    /// The character used to represent a wildcard.
    pub const WILDCARD: char = '*';
    /// The character used to mark where episodes are.
    pub const EPISODE_MARKER: char = '#';

    /// Create a new `CustomPattern` with the specified `pattern`.
    #[inline(always)]
    pub fn new<S>(pattern: S) -> Self
    where
        S: Into<String>,
    {
        Self(pattern.into())
    }

    fn sum_char_digits(first: char, value_chars: impl Iterator<Item = char>) -> u32 {
        let rest = value_chars
            .take_while(char::is_ascii_digit)
            .collect::<SmallVec<[_; 3]>>();

        let first = [first];

        first
            .iter()
            .chain(rest.iter())
            .rev()
            .enumerate()
            .map(|(base, ch)| ch.to_digit(10).unwrap_or(0) * 10u32.pow(base as u32))
            .sum::<u32>()
    }

    /// Executes the current pattern to find an episode number in the specified `value`.
    ///
    /// This will always return `None` if the current pattern does not have a `#` character to mark the location of episodes.
    pub fn detect_episode<S>(&self, value: S) -> Option<u32>
    where
        S: AsRef<str>,
    {
        let mut value_chars = value.as_ref().chars();
        let mut pattern_chars = self.0.chars().peekable();
        let mut cur_pattern_char = pattern_chars.next();

        while let Some(value_ch) = value_chars.next() {
            match cur_pattern_char {
                Some(Self::WILDCARD) => match pattern_chars.peek() {
                    Some(&Self::EPISODE_MARKER) if value_ch.is_ascii_digit() => {
                        return Some(Self::sum_char_digits(value_ch, value_chars))
                    }
                    Some(wildcard_end) => {
                        if value_ch.eq_ignore_ascii_case(wildcard_end) {
                            // Our next pattern character should be after both the wildcard and ending character
                            cur_pattern_char =
                                pattern_chars.next().and_then(|_| pattern_chars.next());
                        }
                    }
                    None => break,
                },
                Some(Self::EPISODE_MARKER) => match pattern_chars.peek() {
                    // Interpret another episode marker as an escape
                    Some(&Self::EPISODE_MARKER) => cur_pattern_char = pattern_chars.next(),
                    Some(_) | None => {
                        if value_ch.is_ascii_digit() {
                            return Some(Self::sum_char_digits(value_ch, value_chars));
                        }
                    }
                },
                // Test for a 1-to-1 character match
                Some(ch) if ch.eq_ignore_ascii_case(&value_ch) => {
                    cur_pattern_char = pattern_chars.next()
                }
                Some(_) | None => break,
            }
        }

        None
    }

    /// Returns true if the current pattern contains the episode marker character.
    #[inline]
    pub fn has_episode_marker(&self) -> bool {
        self.0.contains(Self::EPISODE_MARKER)
    }

    /// Returns a reference to the pattern string.
    #[inline(always)]
    pub fn inner(&self) -> &String {
        &self.0
    }

    /// Returns a mutable reference to the pattern string.
    #[inline(always)]
    pub fn inner_mut(&mut self) -> &mut String {
        &mut self.0
    }
}

impl Deref for CustomPattern {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        self.inner()
    }
}

impl DerefMut for CustomPattern {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner_mut()
    }
}

#[cfg(feature = "diesel-support")]
impl<DB> FromSql<Text, DB> for CustomPattern
where
    DB: diesel::backend::Backend,
    String: FromSql<Text, DB>,
{
    fn from_sql(bytes: Option<&DB::RawValue>) -> deserialize::Result<Self> {
        let pattern = String::from_sql(bytes)?;
        Ok(Self::new(pattern))
    }
}

#[cfg(feature = "diesel-support")]
impl<DB> ToSql<Text, DB> for CustomPattern
where
    DB: diesel::backend::Backend,
    String: ToSql<Text, DB>,
{
    fn to_sql<W: Write>(&self, out: &mut Output<W, DB>) -> serialize::Result {
        self.0.to_sql(out)
    }
}

/// The detected title and episode number from an episode file.
#[derive(Debug)]
pub struct ParsedEpisode {
    /// The parsed title of the episode file.
    pub title: Option<String>,
    /// The parsed episode number of the episode file.
    pub episode: u32,
    pub category: SeriesKind,
}

impl ParsedEpisode {
    #[inline(always)]
    fn new(title: Option<String>, episode: u32, category: SeriesKind) -> Self {
        Self {
            title,
            episode,
            category,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    enum Expected {
        Default(&'static str),
        CustomTitle(&'static str, &'static str),
        CustomCategory(&'static str, SeriesKind),
        CustomCategoryAndEpisode(&'static str, SeriesKind, u32),
    }

    impl Expected {
        const DEFAULT_TITLE: &'static str = "Series Title";

        fn fmt(&self) -> &'static str {
            match self {
                Self::Default(fmt) => fmt,
                Self::CustomTitle(fmt, _) => fmt,
                Self::CustomCategory(fmt, _) => fmt,
                Self::CustomCategoryAndEpisode(fmt, _, _) => fmt,
            }
        }

        fn expected_title(&self) -> &'static str {
            match self {
                Self::Default(_)
                | Self::CustomCategory(_, _)
                | Self::CustomCategoryAndEpisode(_, _, _) => Self::DEFAULT_TITLE,
                Self::CustomTitle(_, title) => title,
            }
        }

        fn expected_category(&self) -> SeriesKind {
            match self {
                Self::Default(_) | Self::CustomTitle(_, _) => SeriesKind::Season,
                Self::CustomCategory(_, cat) | Self::CustomCategoryAndEpisode(_, cat, _) => *cat,
            }
        }

        fn expected_episode(&self) -> u32 {
            match self {
                Self::Default(_) | Self::CustomTitle(_, _) | Self::CustomCategory(_, _) => 12,
                Self::CustomCategoryAndEpisode(_, _, ep) => *ep,
            }
        }
    }

    #[test]
    fn episode_format_detection() {
        let def = Expected::Default;
        let cus = Expected::CustomTitle;
        let cus_cat = Expected::CustomCategory;
        let cus_cat_ep = Expected::CustomCategoryAndEpisode;

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
                "[Header 1] Non @ Alpha ' Betic : Characters - 12 [10].mkv",
                "Non @ Alpha ' Betic : Characters",
            ),
            def("[Header 1].Series.Title.E12.[10].mkv"),
            def("[Header 1].Series.Title.Ep.12.[10].mkv"),
            def("[Header 1].Series.Title.Episode.12.[10].mkv"),
            def("[Header 1] Series Title - 12v2.mkv"),
            def("[Header 1] 12v2 - Series Title.mkv"),
            def("Series Title 12 An Episode Description (1080p).mkv"),
            def("Series Title - 12 An Episode Description.mkv"),
            def("Series Title - 12 - An Episode Description.mkv"),
            cus(
                "Series Title 2 12 An Episode Description [1080p].mkv",
                "Series Title 2",
            ),
            cus_cat("Series Title OVA - 12.mkv", SeriesKind::OVA),
            cus_cat("Series Title OVAs - 12.mkv", SeriesKind::OVA),
            cus_cat("Series Title Special - 12.mkv", SeriesKind::Special),
            cus_cat("Series Title Specials - 12.mkv", SeriesKind::Special),
            cus_cat("Series Title ONA - 12.mkv", SeriesKind::ONA),
            cus_cat("Series Title Movie - 12.mkv", SeriesKind::Movie),
            cus_cat("Series Title - OVA12.mkv", SeriesKind::OVA),
            cus_cat("Series Title - OVA 12 [Tag].mkv", SeriesKind::OVA),
            cus_cat("Series Title - 12 OVA.mkv", SeriesKind::OVA),
            cus_cat_ep("Series Title - OVA [Tag].mkv", SeriesKind::OVA, 1),
            cus_cat_ep("Series Title - OVAv2.mkv", SeriesKind::OVA, 1),
            cus_cat_ep("Series Title - Special [Tag].mkv", SeriesKind::Special, 1),
            cus_cat_ep("Series Title - OVA [Tag].mkv", SeriesKind::OVA, 1),
            cus_cat_ep("Series Title OVA [Tag].mkv", SeriesKind::OVA, 1),
            cus_cat_ep("Series Title OVAv2 [Tag].mkv", SeriesKind::OVA, 1),
            cus_cat("Series Title - Specials - 12.mkv", SeriesKind::Special),
            cus_cat("[Tag] Series Title ep 12 OVA (Tag).mkv", SeriesKind::OVA),
        ];

        let parser = EpisodeParser::default();

        for format in &formats {
            match parser.parse(format.fmt()) {
                Ok(parsed) => {
                    match parsed.title {
                        Some(title) => assert_eq!(
                            title,
                            format.expected_title(),
                            "episode title mismatch: {:?}",
                            format
                        ),
                        None => panic!(
                            "expected series title, got nothing while parsing format: {:?}",
                            format
                        ),
                    }

                    assert_eq!(
                        parsed.category,
                        format.expected_category(),
                        "episode category mismatch: {:?}",
                        format
                    );

                    assert_eq!(
                        parsed.episode,
                        format.expected_episode(),
                        "episode number mismatch: {:?}",
                        format
                    );
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
    fn title_detection() {
        let def = Expected::Default;
        let cus = Expected::CustomTitle;

        let titles = vec![
            def("Series Title"),
            def("[Tag 1] Series Title (01-13)"),
            def("[Tag 1] Series Title (01-13) [Tag 2]"),
            cus("[Tag 1] Series - Title (01-13) [Tag 2]", "Series - Title"),
            def("[Tag.1].Series.Title.(01-13).[Tag.2]"),
        ];

        for title in titles {
            match dir::parse_title(title.fmt()) {
                Some(parsed) => assert_eq!(
                    parsed,
                    title.expected_title(),
                    "parsed title mismatch: {}",
                    title.fmt()
                ),
                None => panic!("failed to parse title: {}", title.fmt()),
            }
        }
    }

    #[test]
    fn custom_pattern_detection() {
        let pairs = vec![
            ("", "", None),
            ("Series Title - #.mkv", "Series Title - 12.mkv", Some(12)),
            ("Series*- #", "Series Title - 12.mkv", Some(12)),
            ("*#", "Series Title - 12.mkv", Some(12)),
            (
                "[*] Series Title -*- #.mkv",
                "[Tag] Series Title - Episode Description - 12.mkv",
                Some(12),
            ),
            ("[*] Series*Title - #", "[Tag] Series Title - 1", Some(1)),
            (
                "[*] Series*Title*-*#",
                "[Tag] Series Title - 123",
                Some(123),
            ),
            (
                "[*] Series Title - # This Doesn't Matter",
                "[Tag Test] Series Title - 1234 - Different Suffix",
                Some(1234),
            ),
            (
                "[*] Series With Asterisk** -*-*#",
                "[Tag] Series With Asterisk* - Description - 12",
                Some(12),
            ),
            (
                "[*] Series With Asterisk*** -*-*#",
                "[Tag] Series With Asterisk** - Description - 12",
                Some(12),
            ),
            (
                "[*] Series With Asterisk**#",
                "[Tag] Series With Asterisk*12",
                Some(12),
            ),
            (
                "[*] Series With Dash## #",
                "[Tag] Series With Dash# 12",
                Some(12),
            ),
            ("series title - ep#", "SeRiEs TiTle - EP12", Some(12)),
            ("**S*e**#", "*Series Title*12", Some(12)),
            ("[*] Series Title - #", "[Tag] Series Title - FOILED!", None),
            ("Series Title", "Series Title", None),
            ("Series Title #", "Series Title", None),
            ("*", "Test 12", None),
        ];

        for (format, value, expected) in pairs {
            let pattern = CustomPattern::new(format);
            let result = pattern.detect_episode(value);

            assert_eq!(
                result, expected,
                "custom pattern mismatch:\n\tpattern: {}\n\tvalue: {}",
                format, value
            );
        }
    }
}
