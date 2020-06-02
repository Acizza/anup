use once_cell::sync::Lazy;
use regex::Regex;
use std::fs::DirEntry;

#[inline]
pub fn closest_match<I, S>(name: S, min_confidence: f32, items: I) -> Option<DirEntry>
where
    I: Iterator<Item = DirEntry>,
    S: Into<String>,
{
    let mut name = name.into();
    name.make_ascii_lowercase();

    crate::closest_match(items, min_confidence, |dir| {
        let mut dir_name = parse_title(dir.file_name().to_string_lossy())?;
        dir_name.make_ascii_lowercase();

        Some(strsim::jaro(&dir_name, &name) as f32)
    })
    .map(|(_, dir)| dir)
}

#[inline]
pub fn parse_title<S>(dir: S) -> Option<String>
where
    S: AsRef<str>,
{
    // This pattern parses titles out of strings like this:
    // [GroupName] Series Title (01-13) [1080p]
    static TITLE_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?:\[.+?\]\s*)?(?P<title>.+?)(?:\(|\[|$)").unwrap());

    let caps = TITLE_REGEX.captures(dir.as_ref())?;
    let title = caps["title"].to_string();

    Some(title)
}
