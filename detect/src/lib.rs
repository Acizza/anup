pub mod err;

pub use err::{Error, Result};

use anime::remote::SeriesInfo;
use once_cell::sync::Lazy;
use regex::Regex;
use snafu::{OptionExt, ResultExt};
use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};

#[inline]
pub fn best_matching_title<'a, I, S>(titles: I, name: S) -> Option<usize>
where
    I: Iterator<Item = Cow<'a, str>>,
    S: AsRef<str>,
{
    const MIN_CONFIDENCE: f32 = 0.6;
    closest_str_match_idx(titles, name, MIN_CONFIDENCE, strsim::jaro)
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

pub fn parse_folder_title<S>(item: S) -> Option<String>
where
    S: AsRef<str>,
{
    // This pattern parses titles out of strings like this:
    // [GroupName] Series Title (01-13) [1080p]
    static EXTRACT_SERIES_TITLE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?:\[.+?\]\s*)?(?P<title>.+?)(?:\(|\[|$)").unwrap());

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

/// Find the most similar string to `value` in `items` and return the index of it.
///
/// `min_confidence` should be a value between 0 and 1 representing the minimum similarity
/// needed in order to have a match.
///
/// `algo` is meant to take functions from the `strsim` crate. However, if implementing
/// manually, then it should return a value between 0 and 1, representing the similarity
/// of two strings.
pub fn closest_str_match_idx<'a, I, S, F>(
    items: I,
    value: S,
    min_confidence: f32,
    algo: F,
) -> Option<usize>
where
    I: Iterator<Item = Cow<'a, str>>,
    S: AsRef<str>,
    F: Fn(&str, &str) -> f64,
{
    let mut max_score = 0.0;
    let mut best_match = None;

    // Casing can really skew the similarity score, so we should match everything
    // in lowercase
    let value = value.as_ref().to_ascii_lowercase();

    for (i, item) in items.enumerate() {
        let item = {
            let mut item = item.into_owned();
            item.make_ascii_lowercase();
            item
        };

        let score = algo(&item, &value) as f32;

        if score > max_score {
            // We want the first item to hit a ~1.0 score rather than the last one
            if score > 0.99 {
                return Some(i);
            }

            best_match = Some(i);
            max_score = score;
        }
    }

    if max_score < min_confidence {
        return None;
    }

    best_match
}
