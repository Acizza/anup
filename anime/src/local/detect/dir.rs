use super::common::{replace_whitespace, tags, whitespace, INVALID_TITLE_CHARS};
use nom::bytes::complete::take_while;
use nom::sequence::tuple;
use std::fs::DirEntry;
use std::path::Path;

#[inline]
pub fn closest_match<I, S>(name: S, min_confidence: f32, items: I) -> Option<DirEntry>
where
    I: Iterator<Item = DirEntry>,
    S: Into<String>,
{
    let mut name = name.into();
    name.make_ascii_lowercase();

    crate::closest_match(items, min_confidence, |dir| {
        let mut dir_name = parse_title(dir.file_name())?;
        dir_name.make_ascii_lowercase();

        Some(strsim::jaro(&dir_name, &name) as f32)
    })
    .map(|(_, dir)| dir)
}

#[inline]
pub fn parse_title<S>(dir: S) -> Option<String>
where
    S: AsRef<Path>,
{
    let dir = dir.as_ref();
    let dir_name = dir.file_name()?.to_string_lossy();

    let title = take_while(|ch| !INVALID_TITLE_CHARS.contains(&(ch as u8)));
    let (_, (_, _, parsed)) = tuple((tags, whitespace, title))(&dir_name).ok()?;
    let parsed = replace_whitespace(parsed);

    Some(parsed)
}
