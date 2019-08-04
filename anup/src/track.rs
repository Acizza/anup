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
    pub info: Cow<'a, SeriesInfo>,
    pub state: EntryState,
    pub name: String,
}

impl<'a> SeriesTracker<'a> {
    pub fn init<I, S>(info: I, name: S) -> Result<SeriesTracker<'a>>
    where
        I: Into<Cow<'a, SeriesInfo>>,
        S: Into<String>,
    {
        let info = info.into();
        let name = name.into();

        let state = match EntryState::load_with_id(info.id, name.as_ref()) {
            Ok(state) => state,
            Err(ref err) if err.is_file_nonexistant() => EntryState::from(info.id),
            Err(err) => return Err(err),
        };

        Ok(SeriesTracker { info, state, name })
    }

    pub fn begin_watching<R>(&mut self, remote: &R, config: &Config) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        let state = &mut self.state;
        state.sync_changes_from_remote(remote, &self.name)?;

        let last_status = state.status();

        match last_status {
            Status::Watching | Status::Rewatching => {
                // There is an edge case where all episodes have been watched, but the status
                // is still set to watching / rewatching. Here we just start a rewatch
                if state.watched_eps() >= self.info.episodes {
                    state.set_status(Status::Rewatching);
                    state.set_watched_eps(0);

                    if last_status == Status::Rewatching {
                        state.set_times_rewatched(state.times_rewatched() + 1);
                    }
                }
            }
            Status::Completed => {
                state.set_status(Status::Rewatching);
                state.set_watched_eps(0);
            }
            Status::PlanToWatch => state.set_status(Status::Watching),
            Status::OnHold | Status::Dropped => {
                state.set_status(Status::Watching);
                state.set_watched_eps(0);
            }
        }

        if state.start_date().is_none()
            || (last_status == Status::Completed
                && state.status() == Status::Rewatching
                && config.reset_dates_on_rewatch)
        {
            state.set_start_date(Some(Local::today().naive_local()));
        }

        state.sync_changes_to_remote(remote, &self.name)?;

        Ok(())
    }

    pub fn episode_completed<R>(&mut self, remote: &R, config: &Config) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        let state = &mut self.state;
        state.set_watched_eps(state.watched_eps() + 1);

        if state.watched_eps() >= self.info.episodes {
            return self.series_complete(remote, config);
        }

        state.sync_changes_to_remote(remote, &self.name)?;

        Ok(())
    }

    pub fn series_complete<R>(&mut self, remote: &R, config: &Config) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        let state = &mut self.state;

        // A rewatch is typically only counted once the series is completed again
        if state.status() == Status::Rewatching {
            state.set_times_rewatched(state.times_rewatched() + 1);
        }

        if state.end_date().is_none()
            || (state.status() == Status::Rewatching && config.reset_dates_on_rewatch)
        {
            state.set_end_date(Some(Local::today().naive_local()));
        }

        state.set_status(Status::Completed);
        state.sync_changes_to_remote(remote, &self.name)?;

        Ok(())
    }
}
