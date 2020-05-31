use crate::err::{Error, Result};
use crate::file;
use std::path::{Path, PathBuf};

#[macro_export]
macro_rules! try_opt_r {
    ($x:expr) => {
        match $x {
            Some(value) => value,
            None => return Ok(()),
        }
    };
}

#[macro_export]
macro_rules! try_opt_ret {
    ($x:expr) => {
        match $x {
            Some(value) => value,
            None => return,
        }
    };
}

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

pub fn closest_matching_dir<D, S>(dir: D, name: S) -> Result<PathBuf>
where
    D: AsRef<Path>,
    S: AsRef<str>,
{
    let name = name.as_ref();
    let files = file::read_dir(dir)?;

    detect::dir::closest_match(files.into_iter(), name).map_or_else(
        || Err(Error::NoMatchingSeriesOnDisk { name: name.into() }),
        |dir| Ok(dir.path()),
    )
}
