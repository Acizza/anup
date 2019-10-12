use crate::err::{self, Result};
use crate::util;
use anime::remote::SeriesInfo;
use lazy_static::lazy_static;
use regex::Regex;
use snafu::{OptionExt, ResultExt};
use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};

pub fn best_matching_title<'a, I, S>(titles: I, name: S) -> Option<usize>
where
    I: Iterator<Item = Cow<'a, str>>,
    S: AsRef<str>,
{
    const MIN_CONFIDENCE: f32 = 0.6;
    util::closest_str_match_idx(titles, name, MIN_CONFIDENCE, strsim::jaro)
}

pub fn best_matching_folder<S, P>(name: S, dir: P) -> Result<PathBuf>
where
    S: AsRef<str>,
    P: AsRef<Path>,
{
    let dir = dir.as_ref();
    let entries = fs::read_dir(dir).context(err::FileIO { path: dir })?;

    let mut dirs = Vec::new();

    for entry in entries {
        let entry = entry.context(err::EntryIO { dir })?;
        let etype = entry.file_type().context(err::EntryIO { dir })?;

        if !etype.is_dir() {
            continue;
        }

        dirs.push(entry);
    }

    let dir = {
        let dir_names = dirs
            .iter()
            .filter_map(|name| parse_folder_title(name.file_name().to_string_lossy()))
            .map(Cow::Owned);

        let dir_idx = best_matching_title(dir_names, &name).context(err::NoMatchingSeries {
            name: name.as_ref(),
        })?;

        dirs.swap_remove(dir_idx)
    };

    Ok(dir.path())
}

fn parse_folder_title<S>(item: S) -> Option<String>
where
    S: AsRef<str>,
{
    lazy_static! {
        // This pattern parses titles out of strings like this:
        // [GroupName] Series Title (01-13) [1080p]
        static ref EXTRACT_SERIES_TITLE: Regex =
            Regex::new(r"(?:\[.+?\]\s*)?(?P<title>.+?)(?:\(|\[|$)").unwrap();
    }

    let caps = EXTRACT_SERIES_TITLE.captures(item.as_ref())?;
    let title = caps["title"].to_string();

    Some(title)
}

pub fn best_matching_info<S>(name: S, items: &[SeriesInfo]) -> Option<usize>
where
    S: AsRef<str>,
{
    let items = items
        .iter()
        .map(|info| Cow::Borrowed(info.title.romaji.as_ref()));

    best_matching_title(items, name)
}
