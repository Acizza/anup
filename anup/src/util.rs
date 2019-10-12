use std::borrow::Cow;

pub fn ms_from_mins<F>(mins: F) -> String
where
    F: Into<f32>,
{
    let mins = mins.into();
    let m = mins.floor() as u32;
    let s = (mins * 60.0 % 60.0).floor() as u32;

    format!("{:02}:{:02}", m, s)
}

pub fn hm_from_mins<F>(mins: F) -> String
where
    F: Into<f32>,
{
    let mins = mins.into();
    let h = (mins / 60.0).floor() as u32;
    let m = (mins % 60.0).floor() as u32;

    format!("{:02}:{:02}H", h, m)
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

/// Find and return the most similar string to `value` in `items`. Note that this
/// function attemps to move the found item out of the `items` slice. If that is
/// not desired, consider using `closest_str_match_idx` instead.
///
/// `min_confidence` should be a value between 0 and 1 representing the minimum similarity
/// needed in order to have a match.
///
/// `algo` is meant to take functions from the `strsim` crate. However, if implementing
/// manually, then it should return a value between 0 and 1, representing the similarity
/// of two strings.
pub fn closest_str_match<'a, S, F>(
    items: &[&'a str],
    value: S,
    min_confidence: f32,
    algo: F,
) -> Option<&'a str>
where
    S: AsRef<str>,
    F: Fn(&str, &str) -> f64,
{
    let idx = closest_str_match_idx(
        items.iter().map(|&x| Cow::Borrowed(x)),
        value,
        min_confidence,
        algo,
    )?;

    Some(items[idx])
}
