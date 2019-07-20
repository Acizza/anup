use crate::err::{self, Result};
use crate::series::local::Episode;
use crate::series::remote::SeriesInfo;
use lazy_static::lazy_static;
use regex::Regex;
use snafu::{OptionExt, ResultExt};
use std::f32;
use std::fs;
use std::path::{Path, PathBuf};

pub fn best_matching_title<S, I>(name: S, titles: I) -> Option<usize>
where
    S: Into<String>,
    I: IntoIterator<Item = String>,
{
    const MIN_CONFIDENCE: f32 = 0.6;

    let name = {
        let mut name = name.into();
        name.make_ascii_lowercase();
        name
    };

    let mut max_score = 0.0;
    let mut title_idx = None;

    for (i, title) in titles.into_iter().enumerate() {
        let title = match parse_title(title) {
            Some(mut title) => {
                title.make_ascii_lowercase();
                title
            }
            None => continue,
        };

        let score = strsim::jaro(&title, &name) as f32;

        if score > max_score {
            if score >= 0.99 {
                return Some(i);
            }

            title_idx = Some(i);
            max_score = score;
        }
    }

    if max_score < MIN_CONFIDENCE {
        return None;
    }

    title_idx
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

    let name = name.as_ref();

    let filenames = dirs
        .iter()
        .map(|name| name.file_name().to_string_lossy().into_owned());

    let dir_idx = best_matching_title(name, filenames).context(err::NoMatchingSeries { name })?;
    let dir = dirs.swap_remove(dir_idx);

    Ok(dir.path())
}

pub fn best_matching_info<S>(name: S, items: &[SeriesInfo]) -> Option<usize>
where
    S: Into<String>,
{
    // TODO: avoid cloning?
    let items = items
        .iter()
        .map(|info| info.title.clone())
        .collect::<Vec<_>>();

    best_matching_title(name, items)
}

pub fn parse_title<S>(item: S) -> Option<String>
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
    let title = Episode::clean_title(&caps["title"]);

    Some(title)
}
