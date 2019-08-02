pub mod err;
pub mod local;
pub mod remote;

pub use err::{Error, Result};

use local::EpisodeList;
use remote::{RemoteService, SeriesInfo};
use serde::{Deserialize, Serialize};
use snafu::ensure;
use std::borrow::Cow;
use std::ops::Index;
use std::path::PathBuf;

#[derive(Debug)]
pub struct Series<'a> {
    pub info: SeriesInfo,
    pub episodes: Cow<'a, EpisodeList>,
    pub episode_offset: u32,
}

impl<'a> Series<'a> {
    pub fn from_season_list<E>(
        seasons: &SeasonInfoList,
        season_num: usize,
        episodes: E,
    ) -> Result<Series<'a>>
    where
        E: Into<Cow<'a, EpisodeList>>,
    {
        ensure!(
            seasons.has(season_num),
            err::NoSeason {
                season: 1 + season_num
            }
        );

        let mut episode_offset = 0;

        for i in 0..season_num {
            episode_offset += seasons[i].episodes;
        }

        let info = seasons[season_num].clone();

        Ok(Series {
            info,
            episodes: episodes.into(),
            episode_offset,
        })
    }

    fn abs_episode_number(&self, episode: u32) -> u32 {
        self.episode_offset + episode
    }

    pub fn get_episode(&self, episode: u32) -> Option<&PathBuf> {
        if episode == 0 || episode > self.info.episodes {
            return None;
        }

        let ep_num = self.abs_episode_number(episode);
        self.episodes.get(ep_num)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SeasonInfoList(Vec<SeriesInfo>);

impl SeasonInfoList {
    fn season_entries_from_info<R>(remote: R, info: &SeriesInfo) -> Result<Vec<SeriesInfo>>
    where
        R: AsRef<RemoteService>,
    {
        // Since this may call a remote API, we should have our own internal rate limit
        // so we can't accidently spam someone's server *too* much
        const MAX_REQUESTS: usize = 10;

        let remote = remote.as_ref();

        let mut index = 0;
        let mut sequel = info.sequel;
        let mut entries = Vec::with_capacity(1);

        while let Some(seq) = sequel {
            let info = remote.search_info_by_id(seq)?;
            sequel = info.sequel;
            entries.push(info);

            index += 1;

            if index >= MAX_REQUESTS {
                break;
            }
        }

        Ok(entries)
    }

    pub fn from_info_and_remote<R>(info: SeriesInfo, remote: R) -> Result<SeasonInfoList>
    where
        R: AsRef<RemoteService>,
    {
        let entries = SeasonInfoList::season_entries_from_info(remote, &info)?;

        let mut all = Vec::with_capacity(1 + entries.len());
        all.push(info);
        all.extend(entries);

        Ok(SeasonInfoList(all))
    }

    pub fn add_from_remote<R>(&mut self, remote: R) -> Result<bool>
    where
        R: AsRef<RemoteService>,
    {
        let info = match self.get(self.len() - 1) {
            Some(info) => info,
            None => return Ok(false),
        };

        let entries = SeasonInfoList::season_entries_from_info(remote, &info)?;
        let any_added = !entries.is_empty();

        self.0.extend(entries);

        Ok(any_added)
    }

    #[inline]
    pub fn has(&self, season: usize) -> bool {
        season < self.0.len()
    }

    #[inline]
    pub fn get(&self, season: usize) -> Option<&SeriesInfo> {
        self.0.get(season)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Consumes the SeasonInfoList and returns the info for the season specified.
    ///
    /// # Panics
    ///
    /// Panics if `season` is out of bounds.
    #[inline]
    pub fn take_unchecked(mut self, season: usize) -> SeriesInfo {
        self.0.swap_remove(season)
    }

    #[inline]
    pub fn inner(&self) -> &Vec<SeriesInfo> {
        &self.0
    }
}

impl Index<usize> for SeasonInfoList {
    type Output = SeriesInfo;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}
