use crate::err::Result;
use crate::file::{SaveDir, TomlFile};
use serde::de::{self, Deserializer, Visitor};
use serde::ser::Serializer;
use serde_derive::{Deserialize, Serialize};
use std::borrow::Cow;
use std::ops::Mul;
use std::path::{Path, PathBuf};
use std::result;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub series_dir: PathBuf,
    pub reset_dates_on_rewatch: bool,
    pub episode: EpisodeConfig,
    pub tui: TuiConfig,
}

impl Config {
    pub fn new<P>(series_dir: P) -> Self
    where
        P: Into<PathBuf>,
    {
        Self {
            series_dir: series_dir.into(),
            ..Default::default()
        }
    }

    pub fn load_or_create() -> Result<Self> {
        match Self::load() {
            Ok(config) => Ok(config),
            Err(ref err) if err.is_file_nonexistant() => {
                // Fallback path is ~/anime/
                let mut dir = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("~/"));
                dir.push("anime");

                let config = Config::new(dir);
                config.save()?;

                Ok(config)
            }
            Err(err) => Err(err),
        }
    }

    pub fn stripped_path<'a, P>(&self, path: P) -> PathBuf
    where
        P: Into<Cow<'a, Path>>,
    {
        let path = path.into();

        match path.strip_prefix(&self.series_dir) {
            Ok(stripped) => stripped.into(),
            Err(_) => path.into_owned(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        // Default series dir is ~/anime/
        let mut series_dir = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("~/"));
        series_dir.push("anime");

        Self {
            series_dir,
            reset_dates_on_rewatch: false,
            episode: EpisodeConfig::default(),
            tui: TuiConfig::default(),
        }
    }
}

impl TomlFile for Config {
    fn filename() -> &'static str {
        "config"
    }

    fn save_dir() -> SaveDir {
        SaveDir::Config
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EpisodeConfig {
    #[serde(rename = "percent_watched_to_progress")]
    pub pcnt_must_watch: Percentage,
    pub player: String,
    pub player_args: Vec<String>,
}

impl Default for EpisodeConfig {
    fn default() -> EpisodeConfig {
        EpisodeConfig {
            pcnt_must_watch: Percentage::new(50.0),
            player: String::from("mpv"),
            player_args: Vec::new(),
        }
    }
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub struct Percentage(#[serde(with = "Percentage")] f32);

impl Percentage {
    pub fn new(value: f32) -> Percentage {
        Percentage(value / 100.0)
    }

    fn deserialize<'de, D>(de: D) -> result::Result<f32, D::Error>
    where
        D: Deserializer<'de>,
    {
        use std::fmt;

        struct PercentageVisitor;

        impl<'de> Visitor<'de> for PercentageVisitor {
            type Value = f32;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a positive percentage number")
            }

            fn visit_f32<E>(self, value: f32) -> result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                if value.is_sign_negative() {
                    return Err(E::custom(format!(
                        "percentage must be greater than 0: {}",
                        value
                    )));
                }

                Ok(value / 100.0)
            }

            fn visit_f64<E>(self, value: f64) -> result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                if value.is_sign_negative() {
                    return Err(E::custom(format!(
                        "percentage must be greater than 0: {}",
                        value
                    )));
                }

                Ok(value as f32 / 100.0)
            }
        }

        de.deserialize_f32(PercentageVisitor)
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn serialize<S>(value: &f32, ser: S) -> result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ser.serialize_f32(value * 100.0)
    }

    #[inline(always)]
    pub fn as_multiplier(self) -> f32 {
        self.0
    }
}

impl Mul<Percentage> for f32 {
    type Output = f32;

    fn mul(self, other: Percentage) -> Self::Output {
        self * other.as_multiplier()
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TuiConfig {
    pub keys: TuiKeys,
}

impl Default for TuiConfig {
    fn default() -> TuiConfig {
        TuiConfig {
            keys: TuiKeys::default(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TuiKeys {
    pub play_next_episode: char,
    pub run_last_command: char,
}

impl Default for TuiKeys {
    fn default() -> TuiKeys {
        TuiKeys {
            play_next_episode: '\n',
            run_last_command: ';',
        }
    }
}
