use crate::file::{FileFormat, SaveDir, SerializedFile};
use crossterm::event::KeyCode;
use serde::de::{self, Deserializer, Visitor};
use serde::ser::Serializer;
use serde_derive::{Deserialize, Serialize};
use std::ops::Mul;
use std::path::PathBuf;
use std::result;

#[derive(Deserialize, Serialize)]
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
            ..Self::default()
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

impl SerializedFile for Config {
    fn filename() -> &'static str {
        "config"
    }

    fn save_dir() -> SaveDir {
        SaveDir::Config
    }

    fn format() -> FileFormat {
        FileFormat::Toml
    }
}

#[derive(Deserialize, Serialize)]
pub struct EpisodeConfig {
    #[serde(rename = "percent_watched_to_progress")]
    pub pcnt_must_watch: Percentage,
    pub player: String,
    pub player_args: Vec<String>,
}

impl Default for EpisodeConfig {
    fn default() -> Self {
        Self {
            pcnt_must_watch: Percentage::new(50.0),
            player: String::from("mpv"),
            player_args: Vec::new(),
        }
    }
}

#[derive(Copy, Clone, Deserialize, Serialize)]
pub struct Percentage(#[serde(with = "Percentage")] f32);

impl Percentage {
    pub fn new(value: f32) -> Self {
        Self(value / 100.0)
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

#[derive(Default, Deserialize, Serialize)]
pub struct TuiConfig {
    pub keys: TuiKeys,
}

#[derive(Deserialize, Serialize)]
pub struct TuiKeys {
    pub play_next_episode: KeyCode,
}

impl Default for TuiKeys {
    fn default() -> TuiKeys {
        TuiKeys {
            play_next_episode: KeyCode::Enter,
        }
    }
}
