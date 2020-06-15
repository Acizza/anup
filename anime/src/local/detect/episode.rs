use super::common::{whitespace, INVALID_TITLE_CHARS};
use nom::branch::alt;
use nom::bytes::complete::take_while;
use nom::character::complete::char;
use nom::combinator::{map, recognize};
use nom::multi::separated_list1;
use nom::sequence::tuple;
use nom::IResult;

const SEPARATOR_CHAR: u8 = b'-';

/// Variant of the default parser that looks for episodes fitting a `<title> - <episode>` format.
///
/// ### Implementation Note
///
/// Internally, this variant reverses the supplied string before and after parsing, as it makes it much easier to parse titles correctly.
pub mod title_and_episode {
    use super::{reverse, separator_opt, title, whitespace};
    use crate::local::detect::common::replace_whitespace;
    use crate::local::ParsedEpisode;
    use crate::SeriesKind;
    use nom::branch::alt;
    use nom::combinator::map;
    use nom::sequence::{separated_pair, tuple};
    use nom::IResult;

    pub fn parse<S>(input: S) -> Option<ParsedEpisode>
    where
        S: AsRef<str>,
    {
        let input = input.as_ref().chars().rev().collect::<String>();

        let (_, (_, _, (title, episode, category))) =
            tuple((reverse::tags, whitespace, title_and_episode))(&input).ok()?;

        let title = title.chars().rev().collect::<String>();
        let cleaned = replace_whitespace(title);

        let episode = ParsedEpisode::new(Some(cleaned), episode, category);
        Some(episode)
    }

    fn title_and_episode(input: &str) -> IResult<&str, (&str, u32, SeriesKind)> {
        // Categories can be specified before or after the actual episode
        let ep_with_category = alt((
            map(
                tuple((reverse::category, whitespace, reverse::episode)),
                |(cat, _, ep)| (ep, cat),
            ),
            map(
                tuple((reverse::episode, whitespace, reverse::category)),
                |(ep, _, cat)| (ep, cat),
            ),
            // If we only have a category, we should assume that there's only one episode
            map(reverse::category, |cat| (1, cat)),
        ));

        let title_with_category = map(
            tuple((reverse::category, separator_opt, title)),
            |(cat, _, title)| (title, cat),
        );

        // We can have a category specified at either the episode marker or title marker, but not both
        alt((
            map(
                separated_pair(ep_with_category, separator_opt, title),
                |((ep, cat), title)| (title, ep, cat),
            ),
            map(
                separated_pair(reverse::episode, separator_opt, title_with_category),
                |(ep, (title, cat))| (title, ep, cat),
            ),
            map(
                separated_pair(reverse::episode, separator_opt, title),
                |(ep, title)| (title, ep, SeriesKind::Season),
            ),
        ))(input)
    }
}

/// Variant of the default parser that looks for episodes fitting a `<title> - <episode> - <desc>` format.
///
/// All episodes in this format are assumed to be season episodes.
///
/// ### Implementation Note
///
/// Internally, this variant reverses the supplied string before and after parsing, as it makes it much easier to parse titles correctly.
pub mod title_episode_desc {
    use super::{reverse, separator_opt, title, whitespace};
    use crate::local::detect::common::replace_whitespace;
    use crate::local::ParsedEpisode;
    use crate::SeriesKind;
    use nom::bytes::complete::take_till;
    use nom::character::is_digit;
    use nom::combinator::map;
    use nom::sequence::tuple;
    use nom::IResult;

    pub fn parse<S>(input: S) -> Option<ParsedEpisode>
    where
        S: AsRef<str>,
    {
        let input = input.as_ref().chars().rev().collect::<String>();

        let (_, (_, _, (title, episode))) =
            tuple((reverse::tags, whitespace, title_and_episode))(&input).ok()?;

        let title = title.chars().rev().collect::<String>();
        let cleaned = replace_whitespace(title);

        let episode = ParsedEpisode::new(Some(cleaned), episode, SeriesKind::Season);
        Some(episode)
    }

    fn title_and_episode(input: &str) -> IResult<&str, (&str, u32)> {
        let until_digit = take_till(|c: char| is_digit(c as u8));
        let title_episode = tuple((until_digit, reverse::episode, separator_opt, title));

        map(title_episode, |(_, episode, _, title)| (title, episode))(input)
    }
}

/// Variant of the default parser that looks for episodes fitting a `<episode> - <title>` format.
///
/// All episodes in this format are assumed to be season episodes.
pub mod episode_and_title {
    use super::{separator_opt, title, whitespace};
    use crate::local::detect::common::{replace_whitespace, tags};
    use crate::local::ParsedEpisode;
    use crate::SeriesKind;
    use nom::character::complete::{char, digit1, one_of};
    use nom::combinator::{map, map_res, opt};
    use nom::sequence::{separated_pair, tuple};
    use nom::IResult;

    pub fn parse<S>(input: S) -> Option<ParsedEpisode>
    where
        S: AsRef<str>,
    {
        let input = input.as_ref();

        let (_, (_, _, (episode, title))) =
            tuple((tags, whitespace, episode_and_title))(input).ok()?;

        let title = replace_whitespace(title);
        let episode = ParsedEpisode::new(Some(title), episode, SeriesKind::Season);

        Some(episode)
    }

    fn episode_and_title(input: &str) -> IResult<&str, (u32, &str)> {
        separated_pair(episode, separator_opt, title)(input)
    }

    fn episode(input: &str) -> IResult<&str, u32> {
        let ep = map_res(digit1, |s: &str| s.parse::<u32>());

        let season_marker = tuple((char('S'), digit1));
        let ep_marker = tuple((opt(season_marker), char('E')));
        let version_suffix = map(tuple((one_of("vV"), digit1)), |_| ());

        let parsed_episode = tuple((opt(ep_marker), ep, opt(version_suffix)));

        map(parsed_episode, |(_, ep, _)| ep)(input)
    }
}

fn title(input: &str) -> IResult<&str, &str> {
    use nom::{error::ErrorKind, Err};

    let title = take_while(|ch| {
        let ch = ch as u8;
        !INVALID_TITLE_CHARS.contains(&ch) && ch != SEPARATOR_CHAR
    });

    let mut result = separated_list1(separator, title);
    let (slice, fragments) = result(input)?;

    let has_digit_fragment = fragments
        .into_iter()
        .any(|frag| !frag.chars().any(|ch| ch.is_alphabetic()));

    if has_digit_fragment {
        return Err(Err::Error((slice, ErrorKind::SeparatedList)));
    }

    recognize(result)(input)
}

fn separator(input: &str) -> IResult<&str, ()> {
    let dash_char = map(char(SEPARATOR_CHAR as char), |_| ());
    map(tuple((whitespace, dash_char, whitespace)), |_| ())(input)
}

fn separator_opt(input: &str) -> IResult<&str, ()> {
    alt((separator, whitespace))(input)
}

mod reverse {
    use super::whitespace;
    use crate::SeriesKind;
    use nom::branch::alt;
    use nom::bytes::complete::{is_not, tag_no_case};
    use nom::character::complete::{char, digit1, one_of};
    use nom::combinator::{map, map_res, opt};
    use nom::multi::many0;
    use nom::sequence::{delimited, tuple};
    use nom::IResult;

    macro_rules! maybe_plural {
        ($input:expr) => {
            tuple((opt(one_of("sS")), tag_no_case($input)))
        };
    }

    pub fn tags(input: &str) -> IResult<&str, ()> {
        map(many0(tag), |_| ())(input)
    }

    pub fn tag(input: &str) -> IResult<&str, ()> {
        let surrounding = tuple((whitespace, metadata_block, whitespace));
        map(surrounding, |_| ())(input)
    }

    pub fn metadata_block(input: &str) -> IResult<&str, &str> {
        alt((brackets, parens))(input)
    }

    pub fn parens(input: &str) -> IResult<&str, &str> {
        delimited(char(')'), is_not("("), char('('))(input)
    }

    pub fn brackets(input: &str) -> IResult<&str, &str> {
        delimited(char(']'), is_not("["), char('['))(input)
    }

    pub fn episode(input: &str) -> IResult<&str, u32> {
        let ep = map_res(digit1, |s: &str| {
            let rev = s.chars().rev().collect::<String>();
            rev.parse::<u32>()
        });

        // These look for one of the following formats:
        // S<season>E<episode>
        // Ep <episode>
        // Episode <episode>
        let prefix = {
            let season_marker = map(tuple((one_of("Ee"), digit1, one_of("Ss"))), |_| ());
            let ep_prefix = map(
                tuple((
                    whitespace,
                    // Reverse of "isode"
                    opt(tag_no_case("edosi")),
                    // Reverse of "ep"
                    tag_no_case("pe"),
                )),
                |_| (),
            );
            let e_prefix = map(one_of("Ee"), |_| ());
            alt((season_marker, ep_prefix, e_prefix))
        };

        let parsed_episode = tuple((opt(file_version), ep, opt(prefix)));

        map(parsed_episode, |(_, ep, _)| ep)(input)
    }

    pub fn file_version(input: &str) -> IResult<&str, ()> {
        map(tuple((digit1, one_of("vV"))), |_| ())(input)
    }

    pub fn category(input: &str) -> IResult<&str, SeriesKind> {
        let variants = alt((
            // Special
            map(maybe_plural!("laiceps"), |_| SeriesKind::Special),
            // OVA
            map(maybe_plural!("avo"), |_| SeriesKind::OVA),
            // ONA
            map(maybe_plural!("ano"), |_| SeriesKind::ONA),
            // Movie
            map(maybe_plural!("eivom"), |_| SeriesKind::Movie),
        ));

        map(tuple((opt(file_version), variants)), |(_, cat)| cat)(input)
    }
}
