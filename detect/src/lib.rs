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
pub fn best_matching_title<'a, S>(
    name: S,
    titles: impl Iterator<Item = Cow<'a, str>>,
) -> Option<Cow<'a, str>>
where
    S: Into<String>,
{
    const MIN_CONFIDENCE: f32 = 0.6;

    let name = {
        let mut name = name.into();
        name.make_ascii_lowercase();
        name
    };

    closest_match(titles, MIN_CONFIDENCE, |title| {
        let title = title.to_ascii_lowercase();
        Some(strsim::jaro(&title, &name) as f32)
    })
}

#[inline]
pub fn best_matching_info<S>(name: S, items: impl Iterator<Item = SeriesInfo>) -> Option<SeriesInfo>
where
    S: Into<String>,
{
    let name = {
        let mut name = name.into();
        name.make_ascii_lowercase();
        name
    };

    closest_match(items, 0.6, |info| {
        let title = info.title.romaji.to_ascii_lowercase();
        Some(strsim::jaro_winkler(&title, &name) as f32)
    })
}

pub fn best_matching_folder<S, P>(name: S, dir: P) -> Result<PathBuf>
where
    S: Into<String>,
    P: AsRef<Path>,
{
    const MIN_CONFIDENCE: f32 = 0.6;

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

    let name = {
        let mut name = name.into();
        name.make_ascii_lowercase();
        name
    };

    let dir = closest_match(dirs, MIN_CONFIDENCE, |dir| {
        let mut dir_name = parse_folder_title(dir.file_name().to_string_lossy())?;
        dir_name.make_ascii_lowercase();
        Some(strsim::jaro(&dir_name, &name) as f32)
    })
    .context(err::NoMatchingSeries { name })?;

    Ok(dir.path())
}

/// Find the best matching item in `items` via `matcher` and return it if the maximum confidence is greater than `min_confidence`.
///
/// `min_confidence` should be a value between 0.0 and 1.0.
///
/// `matcher` is used to compare each item in `items`. When returning Some, its value should be between 0.0 and 1.0.
/// This value represents the "confidence" (or similarity) between the item and some other value.
///
/// If `matcher` returns a confidence greater than 0.99, that item will be immediately returned.
pub fn closest_match<I, T, F>(items: I, min_confidence: f32, matcher: F) -> Option<T>
where
    I: IntoIterator<Item = T>,
    F: Fn(&T) -> Option<f32>,
{
    let mut max_score = 0.0;
    let mut best_match = None;

    for item in items.into_iter() {
        let score = match matcher(&item) {
            Some(score) => score,
            None => continue,
        };

        if score > max_score {
            if score > 0.99 {
                return Some(item);
            }

            best_match = Some(item);
            max_score = score;
        }
    }

    if max_score < min_confidence {
        return None;
    }

    best_match
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
