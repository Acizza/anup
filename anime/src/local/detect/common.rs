use nom::branch::alt;
use nom::bytes::complete::is_not;
use nom::character::complete::{char, one_of};
use nom::combinator::map;
use nom::multi::many0;
use nom::sequence::{delimited, tuple};
use nom::IResult;

pub const WHITESPACE_CHARS: [u8; 3] = [b' ', b'_', b'.'];
pub const INVALID_TITLE_CHARS: [u8; 4] = [b'[', b']', b'(', b')'];

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
    delimited(char('('), is_not(")"), char(')'))(input)
}

pub fn brackets(input: &str) -> IResult<&str, &str> {
    delimited(char('['), is_not("]"), char(']'))(input)
}

pub fn whitespace(input: &str) -> IResult<&str, ()> {
    let whitespace_char = one_of(WHITESPACE_CHARS.as_ref());
    map(many0(whitespace_char), |_| ())(input)
}

pub fn replace_whitespace<S>(string: S) -> String
where
    S: Into<String>,
{
    let mut string = string.into();

    for ch in WHITESPACE_CHARS.iter().filter(|&&ch| ch != b' ') {
        string = string.replace(*ch as char, " ");
    }

    string.trim().to_string()
}
