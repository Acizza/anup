pub mod dir;

use self::dir::FolderData;
use backend::{AnimeEntry, AnimeInfo, Status, SyncBackend};
use chrono::Local;
use error::SeriesError;
use input::{self, Answer};
use process;
use std::borrow::Cow;

pub struct SeriesConfig<B>
where
    B: SyncBackend,
{
    pub offline_mode: bool,
    pub sync_service: B,
    pub season_num: usize,
}

pub struct Series<B>
where
    B: SyncBackend,
{
    config: SeriesConfig<B>,
    dir: FolderData,
    ep_offset: u32,
}

impl<B> Series<B>
where
    B: SyncBackend,
{
    pub fn new(config: SeriesConfig<B>, dir: FolderData) -> Series<B> {
        Series {
            config,
            dir,
            ep_offset: 0,
        }
    }

    pub fn prepare(&mut self) -> Result<(), SeriesError> {
        self.dir.populate_season_data(&self.config)?;
        self.dir.sync_remote_season_info(&self.config)?;

        self.ep_offset = self.dir.calculate_season_offset(0..self.config.season_num);
        self.dir.save()?;

        self.prepare_list_entry()?;

        Ok(())
    }

    fn prepare_list_entry(&mut self) -> Result<(), SeriesError> {
        let state = &self.cur_season().state;

        match state.status {
            Status::Watching | Status::Rewatching => {
                // Handle potential edge-case where all episodes have already been watched
                // but the series is still set in a watching state
                if let Some(total_eps) = state.info.episodes {
                    if state.watched_episodes >= total_eps {
                        self.update_list_entry_status(Status::Completed)?;
                        self.prompt_series_completed_options()?;
                    }
                }

                Ok(())
            }
            Status::PlanToWatch => self.update_list_entry_status(Status::Watching),
            Status::Completed => {
                println!("[{}] already completed", state.info.title);
                self.prompt_series_completed_options()
            }
            Status::OnHold | Status::Dropped => self.prompt_to_watch_paused_series(),
        }
    }

    fn update_list_entry_status(&mut self, status: Status) -> Result<(), SeriesError> {
        match status {
            Status::Watching => {
                let entry = &mut self.cur_season_mut().state;

                // A series that was on hold probably already has a starting date, and it would make
                // more sense to use that one instead of replacing it
                if entry.status != Status::OnHold {
                    entry.start_date = Some(Local::today().naive_local());
                }

                entry.finish_date = None;
            }
            Status::Rewatching => {
                let entry = &mut self.cur_season_mut().state;

                println!("[{}] starting rewatch", entry.info.title);
                println!("do you want to reset the start and end dates of the series? (Y/n)");

                if input::read_yn(Answer::Yes)? {
                    entry.start_date = Some(Local::today().naive_local());
                    entry.finish_date = None;
                }

                entry.watched_episodes = 0;
            }
            Status::Completed => {
                let entry = &mut self.cur_season_mut().state;

                if entry.finish_date.is_none() {
                    entry.finish_date = Some(Local::today().naive_local());
                }

                println!("[{}] completed!", entry.info.title);
            }
            Status::Dropped => {
                let entry = &mut self.cur_season_mut().state;

                if entry.finish_date.is_none() {
                    entry.finish_date = Some(Local::today().naive_local());
                }
            }
            Status::OnHold | Status::PlanToWatch => (),
        }

        let entry = &mut self.cur_season_mut().state;
        entry.status = status;

        self.update_list_entry()?;

        Ok(())
    }

    pub fn update_list_entry(&mut self) -> Result<(), SeriesError> {
        self.cur_season_mut().needs_sync = self.config.offline_mode;
        self.dir.save()?;

        if self.config.offline_mode {
            return Ok(());
        }

        self.config
            .sync_service
            .update_list_entry(&self.cur_season().state)?;

        Ok(())
    }

    pub fn play_episode(&mut self, episode: u32) -> Result<(), SeriesError> {
        let absolute_ep = self.ep_offset + episode;
        let path = self.dir.episodes.get_episode(absolute_ep)?.clone();

        let status = process::open_with_default(path).map_err(SeriesError::FailedToOpenPlayer)?;

        let entry = &mut self.cur_season_mut().state;
        entry.watched_episodes = episode.max(entry.watched_episodes);

        if !status.success() {
            eprintln!("video player not exited normally");
            eprintln!("do you still want to count the episode as completed? (y/N)");

            if !input::read_yn(Answer::No)? {
                return Ok(());
            }
        }

        match entry.info.episodes {
            Some(total_eps) if episode >= total_eps => {
                self.update_list_entry_status(Status::Completed)?;
                self.prompt_series_completed_options()?;

                return Err(SeriesError::RequestExit);
            }
            _ => self.prompt_episode_completed()?,
        }

        Ok(())
    }

    pub fn play_all_episodes(&mut self) -> Result<(), SeriesError> {
        self.prepare()?;

        loop {
            let next_episode = self.cur_season().state.watched_episodes + 1;

            self.play_episode(next_episode)?;
            self.prompt_next_episode_options()?;
        }
    }

    fn prompt_episode_completed(&mut self) -> Result<(), SeriesError> {
        let entry = &self.cur_season().state;

        let total_episodes = entry
            .info
            .episodes
            .map(|e| Cow::Owned(e.to_string()))
            .unwrap_or_else(|| Cow::Borrowed("?"));

        println!(
            "[{}] episode {}/{} completed",
            entry.info.title, entry.watched_episodes, total_episodes
        );

        self.update_list_entry()?;
        Ok(())
    }

    fn prompt_next_episode_options(&mut self) -> Result<(), SeriesError> {
        let current_score_text: Cow<str> = match self.format_entry_score() {
            Some(score) => Cow::Owned(format!(" [{}]", score)),
            None => Cow::Borrowed(""),
        };

        println!("series options:");
        println!("\t[d] drop\n\t[h] put on hold\n\t[r] rate{}\n\t[x] exit\n\t[n] watch next episode (default)", current_score_text);

        let input = input::read_line()?.to_lowercase();

        match input.as_str() {
            "d" | "h" => {
                let status = if input == "d" {
                    Status::Dropped
                } else {
                    Status::OnHold
                };

                self.update_list_entry_status(status)?;

                println!("do you want to remove the episodes on disk? (Y/n)");

                if input::read_yn(Answer::Yes)? {
                    self.dir.try_remove_dir();
                }

                Err(SeriesError::RequestExit)
            }
            "r" => {
                self.prompt_to_update_score();
                self.update_list_entry()?;

                self.prompt_next_episode_options()
            }
            "x" => Err(SeriesError::RequestExit),
            _ => Ok(()),
        }
    }

    fn prompt_series_completed_options(&mut self) -> Result<(), SeriesError> {
        let current_score_text: Cow<str> = match self.format_entry_score() {
            Some(score) => Cow::Owned(format!(" [{}]", score)),
            None => Cow::Borrowed(""),
        };

        println!("series options:");
        println!(
            "\t[r] rate{}\n\t[w] rewatch\n\t[d] delete local files\n\t[x] exit",
            current_score_text
        );

        let input = input::read_line()?.to_lowercase();

        match input.as_str() {
            "r" => {
                self.prompt_to_update_score();
                self.update_list_entry()?;

                self.prompt_series_completed_options()
            }
            "w" => {
                self.update_list_entry_status(Status::Rewatching)?;
                self.play_all_episodes()
            }
            "d" => {
                self.dir.try_remove_dir();
                Err(SeriesError::RequestExit)
            }
            "x" => Err(SeriesError::RequestExit),
            _ => Ok(()),
        }
    }

    fn prompt_to_update_score(&mut self) {
        let (min_score, max_score) = self.config.sync_service.formatted_score_range();

        println!(
            "enter your score between {} and {} (press return to skip):",
            min_score, max_score
        );

        // TODO: use read_range() with empty line bypassing
        let input = match input::read_line() {
            Ok(ref input) if input.is_empty() => return,
            Ok(input) => input,
            Err(err) => {
                eprintln!("failed to read score input: {}", err);
                return;
            }
        };

        match self.config.sync_service.parse_score(&input) {
            Ok(score) => {
                let entry = &mut self.cur_season_mut().state;
                entry.score = Some(score);
            }
            Err(err) => eprintln!("failed to parse score: {}", err),
        }
    }

    fn prompt_to_watch_paused_series(&mut self) -> Result<(), SeriesError> {
        let entry = &mut self.cur_season_mut().state;

        println!(
            "[{}] was previously put on hold or dropped",
            entry.info.title
        );

        println!("do you want to watch it from the beginning? (Y/n)");

        if input::read_yn(Answer::Yes)? {
            entry.watched_episodes = 0;
        }

        self.update_list_entry_status(Status::Watching)?;
        Ok(())
    }

    fn format_entry_score(&self) -> Option<String> {
        let entry = &self.cur_season().state;

        match entry.score {
            Some(score) => {
                let formatted_score = self.config.sync_service.format_score(score);

                match formatted_score {
                    Ok(score) => Some(score),
                    Err(err) => {
                        eprintln!("failed to read existing list entry score: {}", err);
                        None
                    }
                }
            }
            None => None,
        }
    }

    pub fn cur_season(&self) -> &SeasonState {
        &self.dir.seasons()[self.config.season_num]
    }

    pub fn cur_season_mut(&mut self) -> &mut SeasonState {
        &mut self.dir.seasons_mut()[self.config.season_num]
    }
}

pub fn search_for_series_info<B>(
    backend: &B,
    name: &str,
    season: usize,
) -> Result<AnimeInfo, SeriesError>
where
    B: SyncBackend,
{
    println!("[{}] searching on {}..", name, B::name());

    let mut found = backend.search_by_name(name)?;

    if !found.is_empty() {
        println!(
            "select season {} by entering the number next to its name:\n",
            1 + season
        );

        println!("0 [custom search]");

        for (i, series) in found.iter().enumerate() {
            println!("{} [{}]", 1 + i, series.title);
        }

        let index = input::read_range(0, found.len())?;

        if index == 0 {
            println!("enter the name you want to search for:");

            let name = input::read_line()?;
            search_for_series_info(backend, &name, season)
        } else {
            let info = found.swap_remove(index - 1);
            Ok(info)
        }
    } else {
        println!("no results found\nplease enter a custom search term:");

        let name = input::read_line()?;
        search_for_series_info(backend, &name, season)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SeasonState {
    #[serde(flatten)]
    pub state: AnimeEntry,
    pub needs_info: bool,
    pub needs_sync: bool,
}
