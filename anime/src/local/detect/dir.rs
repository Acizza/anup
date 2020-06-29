use super::common::{replace_whitespace, tags, whitespace, INVALID_TITLE_CHARS};
use nom::bytes::complete::take_while;
use nom::sequence::tuple;
use smallvec::SmallVec;
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

/// Attempts to generate a short and readable nickname for the given folder title.
///
/// A folder title can be obtained from the `parse_title` function.
pub fn generate_nickname<S>(title: S) -> Option<String>
where
    S: Into<String>,
{
    const SPACER: &str = "_";
    const TITLE_WHITESPACE: [u8; 4] = [b' ', b'_', b'.', b'-'];
    const SKIP_WORDS: [&str; 1] = ["the"];
    const SPECIAL_WORDS: [&str; 4] = ["special", "ova", "ona", "movie"];

    let is_special_word = |word: &str| {
        SPECIAL_WORDS
            .iter()
            .any(|special| word.starts_with(special))
    };

    let title = {
        let mut title = title.into();
        title.make_ascii_lowercase();
        title
    };

    let fragments = title
        .split(|ch| TITLE_WHITESPACE.contains(&(ch as u8)))
        .collect::<SmallVec<[_; 8]>>();

    let mut nickname: SmallVec<[&str; 4]> = SmallVec::new();

    let (fragments, end_fragment) = match fragments.last() {
        Some(last) if is_special_word(last) => (&fragments[..fragments.len() - 1], Some(*last)),
        Some(last) => {
            let end_fragment = parse_season_number(last);

            let fragments = if end_fragment.is_some() {
                &fragments[..fragments.len() - 1]
            } else {
                &fragments
            };

            (fragments, end_fragment)
        }
        None => return None,
    };

    let mut used_frags = 0;

    for fragment in fragments {
        let len = fragment.len();

        if len <= 2 || SKIP_WORDS.contains(fragment) {
            continue;
        }

        nickname.push(fragment);

        if len > 8 {
            break;
        }

        used_frags += 1;

        if used_frags >= 2 {
            break;
        }
    }

    if nickname.is_empty() {
        return None;
    }

    if let Some(end) = end_fragment {
        nickname.push(end);
    }

    Some(nickname.join(SPACER))
}

fn parse_season_number(slice: &str) -> Option<&str> {
    let is_digits = |digits: &[u8]| digits.iter().all(u8::is_ascii_digit);

    let offset = match slice.as_bytes() {
        [b's', b'0', b'1', ..] | [b's', b'1', ..] | [] => None,
        [b's', b'0', ..] => Some(2),
        [b's', rest @ ..] if is_digits(rest) => Some(1),
        rest if is_digits(rest) => Some(0),
        _ => None,
    };

    offset.map(|offset| &slice[offset..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nickname_generation() {
        let titles = vec![
            ("series title", Some("series_title")),
            ("longer series title test", Some("longer_series")),
            ("the series title", Some("series_title")),
            ("title of series", Some("title_series")),
            ("longfirstword of series", Some("longfirstword")),
            ("longfirstword S02", Some("longfirstword_2")),
            ("title longsecondword test", Some("title_longsecondword")),
            ("title test longthirdword", Some("title_test")),
            ("series title 2", Some("series_title_2")),
            ("longer series title 2", Some("longer_series_2")),
            ("longer series title s02", Some("longer_series_2")),
            ("series title s01", Some("series_title")),
            ("Yuragi-sou no Yuuna-san OVA", Some("yuragi_sou_ova")),
            ("Kaguya-sama wa wa Kokurasetai S2", Some("kaguya_sama_2")),
            ("series title OVAs", Some("series_title_ovas")),
            ("test s02", Some("test_2")),
            ("test s2", Some("test_2")),
            ("s.m.o.l S02", None),
            ("s.m.o.l OVA", None),
            ("s2", None),
        ];

        for (title, expected) in titles {
            assert_eq!(
                generate_nickname(title).as_deref(),
                expected,
                "nickname mismatch for title: {}",
                title
            );
        }
    }
}
