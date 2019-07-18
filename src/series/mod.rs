pub mod detect;
pub mod local;
pub mod remote;

use crate::err::{self, Result};
use crate::file::{FileType, SaveDir, SaveFileInDir};
use crate::process;
use local::EpisodeList;
use remote::{RemoteService, SeriesInfo};
use serde::{Deserialize, Serialize};
use snafu::{ensure, OptionExt, ResultExt};
use std::ops::Range;
use std::path::PathBuf;

#[derive(Debug)]
pub struct Series {
    pub info: SeriesInfo,
    pub episodes: EpisodeList,
    pub episode_range: Option<Range<u32>>,
}

impl Series {
    fn abs_episode_number(&self, episode: u32) -> u32 {
        match &self.episode_range {
            Some(range) => range.start + episode,
            None => episode,
        }
    }

    pub fn get_episode(&self, episode: u32) -> Option<&PathBuf> {
        if let Some(range) = &self.episode_range {
            if episode >= range.end {
                return None;
            }
        }

        let ep_num = self.abs_episode_number(episode);
        self.episodes.get(ep_num)
    }

    pub fn play_episode(&self, episode: u32) -> Result<()> {
        let path = self.get_episode(episode).context(err::EpisodeNotFound {
            episode,
            series: &self.info.title,
        })?;

        let status = process::open_with_default(path).context(err::FailedToPlayEpisode {
            episode,
            series: &self.info.title,
        })?;

        ensure!(status.success(), err::AbnormalPlayerExit { path });
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SeasonInfoList(Vec<SeriesInfo>);

impl SeasonInfoList {
    fn season_entries_from_info<R>(
        remote: R,
        info: &SeriesInfo,
        max: Option<usize>,
    ) -> Result<Vec<SeriesInfo>>
    where
        R: AsRef<RemoteService>,
    {
        // Since this may call a remote API, we should have our own internal rate limit
        // so we can't accidently spam someone's server *too* much
        const ABSOLUTE_MAX: usize = 10;

        let max = max.map(|max| max.min(ABSOLUTE_MAX)).unwrap_or(ABSOLUTE_MAX);

        if max < 1 {
            return Ok(Vec::new());
        }

        let remote = remote.as_ref();

        let mut index = 0;
        let mut sequel = info.sequel;
        let mut entries = Vec::with_capacity(1);

        while let Some(seq) = sequel {
            let info = remote.search_info_by_id(seq)?;
            sequel = info.sequel;
            entries.push(info);

            index += 1;

            if index >= max {
                break;
            }
        }

        Ok(entries)
    }

    pub fn from_info_and_remote<R>(
        info: SeriesInfo,
        remote: R,
        max: Option<usize>,
    ) -> Result<SeasonInfoList>
    where
        R: AsRef<RemoteService>,
    {
        let entries = SeasonInfoList::season_entries_from_info(remote, &info, max)?;

        let mut all = Vec::with_capacity(1 + entries.len());
        all.push(info);
        all.extend(entries);

        Ok(SeasonInfoList(all))
    }

    pub fn add_from_remote_upto<R>(&mut self, remote: R, upto: usize) -> Result<bool>
    where
        R: AsRef<RemoteService>,
    {
        let info = match self.get(self.len() - 1) {
            Some(info) => info,
            None => return Ok(false),
        };

        let entries = SeasonInfoList::season_entries_from_info(remote, &info, Some(upto))?;
        let any_added = !entries.is_empty();

        self.0.extend(entries);

        Ok(any_added)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn get(&self, season: usize) -> Option<&SeriesInfo> {
        self.0.get(season)
    }

    pub fn has(&self, season: usize) -> bool {
        season < self.0.len()
    }

    pub fn take(mut self, season: usize) -> Option<SeriesInfo> {
        if season >= self.0.len() {
            return None;
        }

        Some(self.0.swap_remove(season))
    }
}

impl SaveFileInDir for SeasonInfoList {
    fn filename() -> &'static str {
        "season_info.mpack"
    }

    fn save_dir() -> SaveDir {
        SaveDir::LocalData
    }

    fn file_type() -> FileType {
        FileType::MessagePack
    }
}
