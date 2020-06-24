use super::info::SeriesInfo;
use crate::config::Config;
use crate::database::schema::series_entries;
use crate::database::Database;
use anime::remote::{Remote, RemoteService, SeriesDate, Status};
use anyhow::Result;
use chrono::Local;
use diesel::prelude::*;

#[derive(Debug, Queryable, Insertable)]
#[table_name = "series_entries"]
pub struct SeriesEntry {
    id: i32,
    watched_episodes: i16,
    score: Option<i16>,
    status: anime::remote::Status,
    times_rewatched: i16,
    start_date: Option<SeriesDate>,
    end_date: Option<SeriesDate>,
    needs_sync: bool,
}

impl SeriesEntry {
    pub fn load(db: &Database, entry_id: i32) -> diesel::QueryResult<Self> {
        use crate::database::schema::series_entries::dsl::*;

        series_entries.filter(id.eq(entry_id)).get_result(db.conn())
    }

    pub fn save(&self, db: &Database) -> diesel::QueryResult<usize> {
        use crate::database::schema::series_entries::dsl::*;

        diesel::replace_into(series_entries)
            .values(self)
            .execute(db.conn())
    }

    pub fn entries_that_need_sync(db: &Database) -> diesel::QueryResult<Vec<Self>> {
        use crate::database::schema::series_entries::dsl::*;

        series_entries.filter(needs_sync.eq(true)).load(db.conn())
    }

    pub fn from_remote(remote: &Remote, info: &SeriesInfo) -> Result<Self> {
        match remote.get_list_entry(info.id as u32)? {
            Some(entry) => Ok(Self::from(entry)),
            None => Ok(Self::from(info.id)),
        }
    }

    pub fn force_sync_to_remote(&mut self, remote: &Remote) -> Result<()> {
        if remote.is_offline() {
            return Ok(());
        }

        remote.update_list_entry(&self.into())?;
        self.needs_sync = false;
        Ok(())
    }

    pub fn sync_to_remote(&mut self, remote: &Remote) -> Result<()> {
        if !self.needs_sync {
            return Ok(());
        }

        self.force_sync_to_remote(remote)
    }

    pub fn force_sync_from_remote(&mut self, remote: &Remote) -> Result<()> {
        if remote.is_offline() {
            return Ok(());
        }

        *self = match remote.get_list_entry(self.id() as u32)? {
            Some(entry) => Self::from(entry),
            None => Self::from(self.id()),
        };

        Ok(())
    }

    pub fn sync_from_remote(&mut self, remote: &Remote) -> Result<()> {
        if self.needs_sync {
            return Ok(());
        }

        self.force_sync_from_remote(remote)
    }

    #[inline(always)]
    pub fn needs_sync(&self) -> bool {
        self.needs_sync
    }

    pub fn set_status(&mut self, status: Status, config: &Config) {
        match status {
            Status::Watching if self.start_date().is_none() => {
                self.start_date = Some(Local::today().naive_local().into());
            }
            Status::Rewatching
                if self.start_date().is_none()
                    || (self.status() == Status::Completed && config.reset_dates_on_rewatch) =>
            {
                self.start_date = Some(Local::today().naive_local().into());
            }
            Status::Completed
                if self.end_date().is_none()
                    || (self.status() == Status::Rewatching && config.reset_dates_on_rewatch) =>
            {
                self.end_date = Some(Local::today().naive_local().into());
            }
            Status::Dropped if self.end_date.is_none() => {
                self.end_date = Some(Local::today().naive_local().into());
            }
            _ => (),
        }

        self.status = status;
        self.needs_sync = true;
    }
}

macro_rules! impl_series_entry_getters_setters {
    ($($field:ident: $field_ty:ty => $setter:tt,)+) => {
        impl SeriesEntry {
            $(
            #[inline(always)]
            pub fn $field(&self) -> $field_ty {
                self.$field
            }

            impl_series_entry_getters_setters!(setter $field, $field_ty, $setter);
            )+
        }
    };

    (setter $field:ident, $field_ty:ty, !) => {};

    (setter $field:ident, $field_ty:ty, $setter:ident) => {
        #[inline(always)]
        pub fn $setter(&mut self, value: $field_ty) {
            self.$field = value;
            self.needs_sync = true;
        }
    }
}

impl_series_entry_getters_setters!(
    id: i32 => !,
    status: Status => !,
    watched_episodes: i16 => set_watched_episodes,
    score: Option<i16> => set_score,
    times_rewatched: i16 => set_times_rewatched,
    start_date: Option<SeriesDate> => !,
    end_date: Option<SeriesDate> => !,
);

impl Into<anime::remote::SeriesEntry> for &mut SeriesEntry {
    fn into(self) -> anime::remote::SeriesEntry {
        anime::remote::SeriesEntry {
            id: self.id as u32,
            watched_eps: self.watched_episodes as u32,
            score: self.score.map(|score| score as u8),
            status: self.status,
            times_rewatched: self.times_rewatched as u32,
            start_date: self.start_date,
            end_date: self.end_date,
        }
    }
}

impl From<anime::remote::SeriesEntry> for SeriesEntry {
    fn from(entry: anime::remote::SeriesEntry) -> Self {
        Self {
            id: entry.id as i32,
            watched_episodes: entry.watched_eps as i16,
            score: entry.score.map(Into::into),
            status: entry.status,
            times_rewatched: entry.times_rewatched as i16,
            start_date: entry.start_date,
            end_date: entry.end_date,
            needs_sync: false,
        }
    }
}

impl From<i32> for SeriesEntry {
    fn from(id: i32) -> Self {
        let remote_entry = anime::remote::SeriesEntry::new(id as u32);
        Self::from(remote_entry)
    }
}
