use crate::config::Config;
use crate::err::Result;
use crate::file::{FileType, SaveDir, SaveFile};
use anime::remote::{RemoteService, SeriesEntry, SeriesInfo, Status};
use chrono::{Local, NaiveDate};
use serde_derive::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Debug, Deserialize, Serialize)]
pub struct EntryState {
    entry: SeriesEntry,
    needs_sync: bool,
}

impl EntryState {
    pub fn new(entry: SeriesEntry) -> EntryState {
        EntryState {
            entry,
            needs_sync: false,
        }
    }

    pub fn force_sync_changes_to_remote<R, S>(&mut self, remote: &R, name: S) -> Result<()>
    where
        R: RemoteService + ?Sized,
        S: AsRef<str>,
    {
        if remote.is_offline() {
            self.save_with_id(self.entry.id, name.as_ref())?;
            return Ok(());
        }

        remote.update_list_entry(&self.entry)?;

        self.needs_sync = false;
        self.save_with_id(self.entry.id, name.as_ref())?;

        Ok(())
    }

    pub fn sync_changes_to_remote<R, S>(&mut self, remote: &R, name: S) -> Result<()>
    where
        R: RemoteService + ?Sized,
        S: AsRef<str>,
    {
        if !self.needs_sync {
            return Ok(());
        }

        self.force_sync_changes_to_remote(remote, name)
    }

    pub fn force_sync_changes_from_remote<R, S>(&mut self, remote: &R, name: S) -> Result<()>
    where
        R: RemoteService + ?Sized,
        S: AsRef<str>,
    {
        if remote.is_offline() {
            return Ok(());
        }

        let entry = match remote.get_list_entry(self.entry.id)? {
            Some(entry) => entry,
            None => SeriesEntry::new(self.entry.id),
        };

        self.entry = entry;
        self.needs_sync = false;
        self.save_with_id(self.entry.id, name.as_ref())?;

        Ok(())
    }

    pub fn sync_changes_from_remote<R, S>(&mut self, remote: &R, name: S) -> Result<()>
    where
        R: RemoteService + ?Sized,
        S: AsRef<str>,
    {
        if self.needs_sync {
            return Ok(());
        }

        self.force_sync_changes_from_remote(remote, name)
    }

    pub fn mark_as_dropped(&mut self, config: &Config) {
        if self.end_date().is_none()
            || (self.status() == Status::Rewatching && config.reset_dates_on_rewatch)
        {
            self.entry.end_date = Some(Local::today().naive_local());
        }

        self.entry.status = Status::Dropped;
        self.needs_sync = true;
    }

    pub fn mark_as_on_hold(&mut self) {
        self.entry.status = Status::OnHold;
        self.needs_sync = true;
    }

    #[inline(always)]
    pub fn needs_sync(&self) -> bool {
        self.needs_sync
    }

    #[inline(always)]
    pub fn watched_eps(&self) -> u32 {
        self.entry.watched_eps
    }

    #[inline(always)]
    pub fn set_watched_eps(&mut self, watched_eps: u32) {
        self.entry.watched_eps = watched_eps;
        self.needs_sync = true;
    }

    #[inline(always)]
    pub fn score(&self) -> Option<u8> {
        self.entry.score
    }

    #[inline(always)]
    pub fn set_score(&mut self, score: Option<u8>) {
        self.entry.score = score;
        self.needs_sync = true;
    }

    #[inline(always)]
    pub fn status(&self) -> Status {
        self.entry.status
    }

    #[inline(always)]
    pub fn set_status(&mut self, status: Status) {
        self.entry.status = status;
        self.needs_sync = true;
    }

    #[inline(always)]
    pub fn times_rewatched(&self) -> u32 {
        self.entry.times_rewatched
    }

    #[inline(always)]
    pub fn set_times_rewatched(&mut self, times_rewatched: u32) {
        self.entry.times_rewatched = times_rewatched;
        self.needs_sync = true;
    }

    #[inline(always)]
    pub fn start_date(&self) -> Option<NaiveDate> {
        self.entry.start_date
    }

    #[inline(always)]
    pub fn set_start_date(&mut self, start_date: Option<NaiveDate>) {
        self.entry.start_date = start_date;
        self.needs_sync = true;
    }

    #[inline(always)]
    pub fn end_date(&self) -> Option<NaiveDate> {
        self.entry.end_date
    }

    #[inline(always)]
    pub fn set_end_date(&mut self, end_date: Option<NaiveDate>) {
        self.entry.end_date = end_date;
        self.needs_sync = true;
    }
}

impl SaveFile for EntryState {
    fn filename() -> &'static str {
        "entry_state.mpack"
    }

    fn save_dir() -> SaveDir {
        SaveDir::LocalData
    }

    fn file_type() -> FileType {
        FileType::MessagePack
    }
}

impl From<u32> for EntryState {
    fn from(id: u32) -> EntryState {
        let entry = SeriesEntry::new(id);
        EntryState::new(entry)
    }
}

#[derive(Debug)]
pub struct SeriesTracker<'a> {
    pub name: String,
    pub info: Cow<'a, SeriesInfo>,
    pub entry: EntryState,
}

impl<'a> SeriesTracker<'a> {
    pub fn init<I, S>(info: I, name: S) -> Result<SeriesTracker<'a>>
    where
        I: Into<Cow<'a, SeriesInfo>>,
        S: Into<String>,
    {
        let info = info.into();
        let name = name.into();

        let entry = match EntryState::load_with_id(info.id, name.as_ref()) {
            Ok(entry) => entry,
            Err(ref err) if err.is_file_nonexistant() => EntryState::from(info.id),
            Err(err) => return Err(err),
        };

        Ok(SeriesTracker { info, entry, name })
    }

    pub fn begin_watching<R>(&mut self, remote: &R, config: &Config) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        let entry = &mut self.entry;
        entry.sync_changes_from_remote(remote, &self.name)?;

        let last_status = entry.status();

        match last_status {
            Status::Watching | Status::Rewatching => {
                // There is an edge case where all episodes have been watched, but the status
                // is still set to watching / rewatching. Here we just start a rewatch
                if entry.watched_eps() >= self.info.episodes {
                    entry.set_status(Status::Rewatching);
                    entry.set_watched_eps(0);

                    if last_status == Status::Rewatching {
                        entry.set_times_rewatched(entry.times_rewatched() + 1);
                    }
                }
            }
            Status::Completed => {
                entry.set_status(Status::Rewatching);
                entry.set_watched_eps(0);
            }
            Status::PlanToWatch => entry.set_status(Status::Watching),
            Status::OnHold | Status::Dropped => {
                entry.set_status(Status::Watching);
                entry.set_watched_eps(0);
            }
        }

        if entry.start_date().is_none()
            || (last_status == Status::Completed
                && entry.status() == Status::Rewatching
                && config.reset_dates_on_rewatch)
        {
            entry.set_start_date(Some(Local::today().naive_local()));
        }

        entry.sync_changes_to_remote(remote, &self.name)?;

        Ok(())
    }

    pub fn episode_completed<R>(&mut self, remote: &R, config: &Config) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        let entry = &mut self.entry;
        let new_progress = entry.watched_eps() + 1;

        if new_progress >= self.info.episodes {
            // The watched episode range is inclusive, so it's fine to bump the watched count
            // if we're at exactly at the last episode
            if new_progress == self.info.episodes {
                entry.set_watched_eps(new_progress);
            }

            return self.series_complete(remote, config);
        }

        entry.set_watched_eps(new_progress);
        entry.sync_changes_to_remote(remote, &self.name)
    }

    pub fn episode_regressed<R>(&mut self, remote: &R) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        let entry = &mut self.entry;

        entry.set_watched_eps(entry.watched_eps().saturating_sub(1));

        let new_status = match entry.status() {
            Status::Completed if entry.times_rewatched() > 0 => Status::Rewatching,
            Status::Rewatching => Status::Rewatching,
            _ => Status::Watching,
        };

        entry.set_status(new_status);
        entry.sync_changes_to_remote(remote, &self.name)
    }

    pub fn series_complete<R>(&mut self, remote: &R, config: &Config) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        let entry = &mut self.entry;

        // A rewatch is typically only counted once the series is completed again
        if entry.status() == Status::Rewatching {
            entry.set_times_rewatched(entry.times_rewatched() + 1);
        }

        if entry.end_date().is_none()
            || (entry.status() == Status::Rewatching && config.reset_dates_on_rewatch)
        {
            entry.set_end_date(Some(Local::today().naive_local()));
        }

        entry.set_status(Status::Completed);
        entry.sync_changes_to_remote(remote, &self.name)?;

        Ok(())
    }
}
