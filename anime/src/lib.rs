#![warn(clippy::pedantic)]
#![allow(clippy::clippy::cast_possible_truncation)]
#![allow(clippy::inline_always)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::shadow_unrelated)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::map_err_ignore)]
#![allow(clippy::upper_case_acronyms)]

#[cfg(feature = "diesel-support")]
#[macro_use]
extern crate diesel;

pub mod err;
pub mod local;
pub mod remote;

pub use err::{Error, Result};

/// Represents the type of a series.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum SeriesKind {
    /// TV episodes.
    Season,
    Movie,
    Special,
    OVA,
    ONA,
    Music,
}

impl Into<&'static str> for SeriesKind {
    fn into(self) -> &'static str {
        match self {
            Self::Season => "Season",
            Self::Movie => "Movie",
            Self::Special => "Special",
            Self::OVA => "OVA",
            Self::ONA => "ONA",
            Self::Music => "Music",
        }
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
