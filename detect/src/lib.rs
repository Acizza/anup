pub mod series_info {
    use anime::remote::SeriesInfo;
    use std::borrow::Cow;

    pub const MIN_CONFIDENCE: f32 = 0.85;

    #[inline]
    pub fn closest_match<'a, I, S>(items: I, name: S) -> Option<(usize, Cow<'a, SeriesInfo>)>
    where
        I: Iterator<Item = Cow<'a, SeriesInfo>>,
        S: Into<String>,
    {
        let mut name = name.into();
        name.make_ascii_lowercase();

        super::closest_match(items, MIN_CONFIDENCE, |info| {
            let title = info.title.romaji.to_ascii_lowercase();
            Some(strsim::jaro_winkler(&title, &name) as f32)
        })
    }
}

pub mod dir {
    use once_cell::sync::Lazy;
    use regex::Regex;
    use std::fs::DirEntry;

    pub const MIN_CONFIDENCE: f32 = 0.6;

    pub fn closest_match<I, S>(items: I, name: S) -> Option<DirEntry>
    where
        I: Iterator<Item = DirEntry>,
        S: Into<String>,
    {
        let mut name = name.into();
        name.make_ascii_lowercase();

        super::closest_match(items, MIN_CONFIDENCE, |dir| {
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
}

/// Find the best matching item in `items` via `matcher` and return it if the maximum confidence is greater than `min_confidence`.
///
/// `min_confidence` should be a value between 0.0 and 1.0.
///
/// `matcher` is used to compare each item in `items`. When returning Some, its value should be between 0.0 and 1.0.
/// This value represents the "confidence" (or similarity) between the item and some other value.
///
/// If `matcher` returns a confidence greater than 0.99, that item will be immediately returned.
pub fn closest_match<I, T, F>(items: I, min_confidence: f32, matcher: F) -> Option<(usize, T)>
where
    I: IntoIterator<Item = T>,
    F: Fn(&T) -> Option<f32>,
{
    let mut max_score = 0.0;
    let mut best_match = None;

    for (i, item) in items.into_iter().enumerate() {
        let score = match matcher(&item) {
            Some(score) => score,
            None => continue,
        };

        if score > max_score {
            best_match = Some((i, item));

            if score > 0.99 {
                return best_match;
            }

            max_score = score;
        }
    }

    if max_score < min_confidence {
        return None;
    }

    best_match
}
