use nom::branch::alt;
use nom::bytes::complete::take_while;
use nom::character::complete::{char, one_of};
use nom::combinator::{map, recognize};
use nom::multi::{many0, separated_list1};
use nom::sequence::tuple;
use nom::IResult;

const WHITESPACE_CHARS: [u8; 3] = [b' ', b'_', b'.'];
const SEPARATOR_CHAR: u8 = b'-';
const INVALID_TITLE_CHARS: [u8; 5] = [b'[', b']', b'(', b')', SEPARATOR_CHAR];

/// Variant of the default parser that looks for episodes fitting a `<title> - <episode>` format.
///
/// ### Implementation Note
///
/// Internally, this variant reverses the supplied string before and after parsing, as it makes it much easier to parse titles correctly.
pub mod title_and_episode {
    use super::{replace_whitespace, separator_opt, title, whitespace};
    use nom::branch::alt;
    use nom::bytes::complete::{is_not, tag_no_case};
    use nom::character::complete::{char, digit1, one_of};
    use nom::combinator::{map, map_res, opt};
    use nom::multi::many0;
    use nom::sequence::{delimited, separated_pair, tuple};
    use nom::IResult;

    pub fn parse<S>(input: S) -> Option<(String, u32)>
    where
        S: AsRef<str>,
    {
        let input = input.as_ref().chars().rev().collect::<String>();

        let (_, (_, _, (title, episode))) =
            tuple((opt(tags), whitespace, title_and_episode))(&input).ok()?;

        let title = title.chars().rev().collect::<String>();
        let cleaned = replace_whitespace(title);

        Some((cleaned, episode))
    }

    fn title_and_episode(input: &str) -> IResult<&str, (&str, u32)> {
        let result = separated_pair(episode, separator_opt, title);
        map(result, |(ep, title)| (title, ep))(input)
    }

    fn episode(input: &str) -> IResult<&str, u32> {
        let ep = map_res(digit1, |s: &str| {
            let rev = s.chars().rev().collect::<String>();
            rev.parse::<u32>()
        });

        // These look for one of the following formats:
        // S<season>E<episode>
        // Ep <episode>
        // Episode <episode>
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
        let prefix = alt((season_marker, ep_prefix, e_prefix));

        map(tuple((ep, opt(prefix))), |(ep, _)| ep)(input)
    }

    fn tags(input: &str) -> IResult<&str, ()> {
        map(many0(tag), |_| ())(input)
    }

    fn tag(input: &str) -> IResult<&str, ()> {
        let surrounding = tuple((whitespace, metadata_block, whitespace));
        map(surrounding, |_| ())(input)
    }

    fn metadata_block(input: &str) -> IResult<&str, &str> {
        alt((brackets, parens))(input)
    }

    fn parens(input: &str) -> IResult<&str, &str> {
        delimited(char(')'), is_not("("), char('('))(input)
    }

    fn brackets(input: &str) -> IResult<&str, &str> {
        delimited(char(']'), is_not("["), char('['))(input)
    }
}

/// Variant of the default parser that looks for episodes fitting a `<episode> - <title>` format.
pub mod episode_and_title {
    use super::{replace_whitespace, separator_opt, title, whitespace};
    use nom::branch::alt;
    use nom::bytes::complete::is_not;
    use nom::character::complete::{char, digit1};
    use nom::combinator::{map, map_res, opt};
    use nom::multi::many0;
    use nom::sequence::{delimited, separated_pair, tuple};
    use nom::IResult;

    pub fn parse<S>(input: S) -> Option<(String, u32)>
    where
        S: AsRef<str>,
    {
        let input = input.as_ref();

        let (_, (_, _, (episode, title))) =
            tuple((opt(tags), whitespace, episode_and_title))(input).ok()?;

        let title = replace_whitespace(title);

        Some((title, episode))
    }

    fn tags(input: &str) -> IResult<&str, ()> {
        map(many0(tag), |_| ())(input)
    }

    fn tag(input: &str) -> IResult<&str, ()> {
        let surrounding = tuple((whitespace, metadata_block, whitespace));
        map(surrounding, |_| ())(input)
    }

    fn metadata_block(input: &str) -> IResult<&str, &str> {
        alt((brackets, parens))(input)
    }

    fn parens(input: &str) -> IResult<&str, &str> {
        delimited(char('('), is_not(")"), char(')'))(input)
    }

    fn brackets(input: &str) -> IResult<&str, &str> {
        delimited(char('['), is_not("]"), char(']'))(input)
    }

    fn episode_and_title(input: &str) -> IResult<&str, (u32, &str)> {
        separated_pair(episode, separator_opt, title)(input)
    }

    fn episode(input: &str) -> IResult<&str, u32> {
        let ep = map_res(digit1, |s: &str| s.parse::<u32>());

        let season_marker = tuple((char('S'), digit1));
        let ep_marker = tuple((opt(season_marker), char('E')));

        map(tuple((opt(ep_marker), ep)), |(_, ep)| ep)(input)
    }
}

fn title(input: &str) -> IResult<&str, &str> {
    use nom::{error::ErrorKind, Err};

    let title = take_while(|ch| !INVALID_TITLE_CHARS.contains(&(ch as u8)));
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

fn whitespace(input: &str) -> IResult<&str, ()> {
    let whitespace_char = one_of(WHITESPACE_CHARS.as_ref());
    map(many0(whitespace_char), |_| ())(input)
}

fn replace_whitespace<S>(string: S) -> String
where
    S: Into<String>,
{
    let mut string = string.into();

    for ch in WHITESPACE_CHARS.iter().filter(|&&ch| ch != b' ') {
        string = string.replace(*ch as char, " ");
    }

    string.trim().to_string()
}
