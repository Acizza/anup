use crate::err::{self, Result};
use lazy_static::lazy_static;
use regex::Regex;
use serde_derive::{Deserialize, Serialize};
use snafu::{ensure, OptionExt, ResultExt};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct EpisodeMatcher(#[serde(with = "optional_regex_parser")] Option<Regex>);

impl EpisodeMatcher {
    pub fn new() -> EpisodeMatcher {
        EpisodeMatcher(None)
    }

    pub fn with_matcher<S>(matcher: S) -> Result<EpisodeMatcher>
    where
        S: Into<String>,
    {
        let matcher = matcher.into();
        let formatted = EpisodeMatcher::format_pattern(&matcher);
        let regex = Regex::new(&formatted).context(err::Regex { matcher: &matcher })?;

        Ok(EpisodeMatcher(Some(regex)))
    }

    fn format_pattern<S>(matcher: S) -> String
    where
        S: AsRef<str>,
    {
        matcher
            .as_ref()
            .replace("{title}", "(?P<title>.+)")
            .replace("{episode}", r"(?P<episode>\d+)")
    }

    pub fn get(&self) -> &Regex {
        lazy_static! {
            // This default pattern will match episodes in several common formats, such as:
            // [Group] Series Name - 01.mkv
            // [Group]_Series_Name_-_01.mkv
            // [Group].Series.Name.-.01.mkv
            // [Group] Series Name - 01 [tag 1][tag 2].mkv
            // [Group]_Series_Name_-_01_[tag1][tag2].mkv
            // [Group].Series.Name.-.01.[tag1][tag2].mkv
            // Series Name - 01.mkv
            // Series_Name_-_01.mkv
            // Series.Name.-.01.mkv
            static ref DEFAULT_MATCHER: Regex = {
                Regex::new(r"(?:\[.+?\](?:_+|\.+|\s*))?(?P<title>.+)(?:\s*|_*|\.*)(?:-|\.|_).*?(?P<episode>\d+)(?:\s*?\(|\s*?\[|\.mkv|\.mp4|\.avi)").unwrap()
            };
        }

        match &self.0 {
            Some(matcher) => matcher,
            None => &DEFAULT_MATCHER,
        }
    }
}

mod optional_regex_parser {
    use regex::Regex;
    use serde::de::{self, Visitor};
    use serde::{Deserializer, Serializer};
    use std::fmt;

    pub fn serialize<S>(regex: &Option<Regex>, ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match regex {
            Some(regex) => ser.serialize_some(regex.as_str()),
            None => ser.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(de: D) -> Result<Option<Regex>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct OptionalRegexVisitor;

        impl<'de> Visitor<'de> for OptionalRegexVisitor {
            type Value = Option<Regex>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an optional regex pattern")
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(None)
            }

            fn visit_some<D>(self, de: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = de.deserialize_str(RegexVisitor)?;
                Ok(Some(value))
            }
        }

        struct RegexVisitor;

        impl<'de> Visitor<'de> for RegexVisitor {
            type Value = Regex;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a regex pattern")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let regex = Regex::new(value)
                    .map_err(|err| de::Error::custom(format!("invalid regex pattern: {}", err)))?;

                Ok(regex)
            }
        }

        de.deserialize_option(OptionalRegexVisitor)
    }
}

#[derive(Debug)]
pub struct Episode {
    pub name: String,
    pub num: u32,
}

impl Episode {
    pub fn parse<'a, S>(name: S, matcher: &EpisodeMatcher) -> Result<Episode>
    where
        S: AsRef<str> + Into<String> + 'a,
    {
        let name = name.as_ref();
        let caps = matcher
            .get()
            .captures(name)
            .context(err::NoEpMatches { name })?;

        let name = caps
            .name("title")
            .context(err::NoEpisodeTitle { name })?
            .as_str()
            .trim()
            .to_string();

        let num = caps
            .name("episode")
            .and_then(|val| val.as_str().parse::<u32>().ok())
            .context(err::ExpectedEpNumber { name: &name })?;

        Ok(Episode { name, num })
    }
}

#[derive(Clone, Debug)]
pub struct EpisodeList {
    pub title: String,
    pub paths: HashMap<u32, PathBuf>,
}

impl EpisodeList {
    pub fn parse<P>(dir: P, matcher: &EpisodeMatcher) -> Result<EpisodeList>
    where
        P: AsRef<Path>,
    {
        let dir = dir.as_ref();
        let entries = fs::read_dir(dir).context(err::FileIO { path: dir })?;

        let mut title: Option<String> = None;
        let mut paths = HashMap::new();

        for entry in entries {
            let entry = entry.context(err::EntryIO { dir })?;
            let etype = entry.file_type().context(err::EntryIO { dir })?;

            if etype.is_dir() {
                continue;
            }

            let fname = entry.file_name();
            let fname = fname.to_string_lossy();

            // A .part extension indicates that the file is being downloaded
            if fname.ends_with(".part") {
                continue;
            }

            let episode = Episode::parse(fname, matcher)?;

            match &mut title {
                Some(name) => {
                    ensure!(
                        *name == episode.name,
                        err::MultipleTitles {
                            expecting: name.clone(),
                            found: episode.name
                        }
                    );
                }
                None => title = Some(episode.name),
            }

            paths.insert(episode.num, entry.path());
        }

        let title = title.context(err::NoEpisodes { path: dir })?;

        Ok(EpisodeList { title, paths })
    }

    pub fn get(&self, episode: u32) -> Option<&PathBuf> {
        self.paths.get(&episode)
    }
}

impl<'a> Into<Cow<'a, EpisodeList>> for EpisodeList {
    fn into(self) -> Cow<'a, EpisodeList> {
        Cow::Owned(self)
    }
}

impl<'a> Into<Cow<'a, EpisodeList>> for &'a EpisodeList {
    fn into(self) -> Cow<'a, EpisodeList> {
        Cow::Borrowed(self)
    }
}
