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

#[derive(Copy, Clone)]
pub enum Prompt {
    EpisodeCompleted,
    UpdateScore,
    SeriesCompleted,
    AlreadyCompleted,
    BeginRewatch,
    ResumePausedSeries,
    PauseSeries(Status),
}

pub struct Series<B>
where
    B: SyncBackend,
{
    config: SeriesConfig<B>,
    dir: FolderData,
    season: SeasonState,
    ep_offset: u32,
}

impl<B> Series<B>
where
    B: SyncBackend,
{
    pub fn init(config: SeriesConfig<B>, mut dir: FolderData) -> Result<Series<B>, SeriesError> {
        dir.populate_season_data(&config)?;
        let ep_offset = dir.calculate_season_offset(0..config.season_num);
        let season = dir.seasons()[config.season_num].clone();

        let series = Series {
            config,
            dir,
            season,
            ep_offset,
        };

        Ok(series)
    }

    pub fn sync_remote_states(&mut self) -> Result<(), SeriesError> {
        self.season
            .sync_data_from_remote(&self.config, &self.dir, self.config.season_num)?;

        self.save_updated_season_data()
    }

    pub fn prepare_initial_state(&mut self) -> Result<(), SeriesError> {
        match self.season.state.status {
            Status::Watching | Status::Rewatching => {
                // Handle potential edge-case where all episodes have already been watched
                // but the series is still set in a watching state
                if !self.has_unwatched_episodes() {
                    self.prompt(Prompt::SeriesCompleted)?;
                }

                Ok(())
            }
            Status::PlanToWatch => self.set_list_entry_status(Status::Watching),
            Status::Completed => self.prompt(Prompt::AlreadyCompleted),
            Status::OnHold | Status::Dropped => self.prompt(Prompt::ResumePausedSeries),
        }
    }

    pub fn prompt(&mut self, prompt: Prompt) -> Result<(), SeriesError> {
        let state = &mut self.season.state;

        match prompt {
            Prompt::EpisodeCompleted => {
                let total_episodes = state
                    .info
                    .episodes
                    .map(|e| Cow::Owned(e.to_string()))
                    .unwrap_or_else(|| Cow::Borrowed("?"));

                println!(
                    "[{}] episode {}/{} completed",
                    state.info.title, state.watched_episodes, total_episodes
                );

                self.update_list_entry()?;
                self.prompt_next_episode_options()
            }
            Prompt::UpdateScore => {
                let (min_score, max_score) = self.config.sync_service.formatted_score_range();

                println!(
                    "enter your score between {} and {} (press return to skip):",
                    min_score, max_score
                );

                // Read & parse score input until we get a valid one
                loop {
                    let input = match input::read_line() {
                        Ok(ref input) if input.is_empty() => return Ok(()),
                        Ok(input) => input,
                        Err(err) => {
                            eprintln!("failed to read score input: {}", err);
                            return Ok(());
                        }
                    };

                    match self.config.sync_service.parse_score(&input) {
                        Ok(score) => {
                            state.score = Some(score);
                            break;
                        }
                        Err(err) => eprintln!("error: {}", err),
                    }
                }

                self.update_list_entry()
            }
            Prompt::SeriesCompleted => {
                println!("[{}] completed!", state.info.title);
                self.set_list_entry_status(Status::Completed)?;
                self.prompt_series_completed_options()
            }
            Prompt::AlreadyCompleted => {
                println!("[{}] already completed", state.info.title);
                self.prompt_series_completed_options()
            }
            Prompt::BeginRewatch => {
                println!("[{}] starting rewatch", state.info.title);
                println!("do you want to reset the start and end dates? (Y/n)");

                if input::read_yn(Answer::Yes)? {
                    state.start_date = None;
                    state.finish_date = None;
                }

                self.set_list_entry_status(Status::Rewatching)
            }
            Prompt::ResumePausedSeries => {
                println!(
                    "[{}] was previously put on hold or dropped",
                    state.info.title
                );

                println!("do you want to watch it from the beginning? (Y/n)");

                if input::read_yn(Answer::Yes)? {
                    state.watched_episodes = 0;
                }

                self.set_list_entry_status(Status::Watching)
            }
            Prompt::PauseSeries(status) => {
                self.set_list_entry_status(status)?;

                println!("do you want to remove the episodes on disk? (Y/n)");

                if input::read_yn(Answer::Yes)? {
                    self.dir.try_remove_dir();
                }

                Ok(())
            }
        }
    }

    pub fn prompt_next_episode_options(&mut self) -> Result<(), SeriesError> {
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

                self.prompt(Prompt::PauseSeries(status))?;
                Err(SeriesError::RequestExit)
            }
            "r" => {
                self.prompt(Prompt::UpdateScore)?;
                self.prompt_next_episode_options()
            }
            "x" => Err(SeriesError::RequestExit),
            _ => Ok(()),
        }
    }

    pub fn prompt_series_completed_options(&mut self) -> Result<(), SeriesError> {
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
                self.prompt(Prompt::UpdateScore)?;
                self.prompt_series_completed_options()
            }
            "w" => {
                self.prompt(Prompt::BeginRewatch)?;
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

    pub fn prompt_series_options(&mut self) -> Result<(), SeriesError> {
        let current_score_text: Cow<str> = match self.format_entry_score() {
            Some(score) => Cow::Owned(format!(" [{}]", score)),
            None => Cow::Borrowed(""),
        };

        println!("[{}] series options:", self.season.state.info.title);
        println!(
            "\t[r] rate{}\n\t[dd] delete local files\n\t[d] drop\n\t[h] put on hold\n\t[x] exit",
            current_score_text
        );

        let input = input::read_line()?.to_lowercase();

        match input.as_str() {
            "r" => {
                self.prompt(Prompt::UpdateScore)?;
                self.prompt_series_options()
            }
            "dd" => {
                self.dir.try_remove_dir();
                Err(SeriesError::RequestExit)
            }
            "d" | "h" => {
                let status = if input == "d" {
                    Status::Dropped
                } else {
                    Status::OnHold
                };

                self.prompt(Prompt::PauseSeries(status))?;
                Err(SeriesError::RequestExit)
            }
            "x" => Err(SeriesError::RequestExit),
            _ => self.prompt_series_options(),
        }
    }

    fn format_entry_score(&self) -> Option<String> {
        let state = &self.season.state;

        match state.score {
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

    fn set_list_entry_status(&mut self, status: Status) -> Result<(), SeriesError> {
        let state = &mut self.season.state;

        match status {
            Status::Watching => {
                // A series that was on hold probably already has a starting date, and it would make
                // more sense to use that one instead of replacing it
                if state.status != Status::OnHold {
                    state.start_date = Some(Local::today().naive_local());
                }

                state.finish_date = None;
            }
            Status::Rewatching => {
                if state.start_date.is_none() {
                    state.start_date = Some(Local::today().naive_local());
                    state.finish_date = None;
                }

                state.watched_episodes = 0;
            }
            Status::Completed => {
                if state.finish_date.is_none() {
                    state.finish_date = Some(Local::today().naive_local());
                }
            }
            Status::Dropped => {
                if state.finish_date.is_none() {
                    state.finish_date = Some(Local::today().naive_local());
                }
            }
            Status::OnHold | Status::PlanToWatch => (),
        }

        state.status = status;
        self.update_list_entry()
    }

    pub fn update_list_entry(&mut self) -> Result<(), SeriesError> {
        self.season.needs_sync = self.config.offline_mode;
        self.save_updated_season_data()?;

        if self.config.offline_mode {
            return Ok(());
        }

        self.config
            .sync_service
            .update_list_entry(&self.season.state)?;

        Ok(())
    }

    pub fn play_episode(&mut self, episode: u32) -> Result<(), SeriesError> {
        let absolute_ep = self.ep_offset + episode;
        let path = self.dir.get_episode(absolute_ep)?.clone();

        let status = process::open_with_default(path).map_err(SeriesError::FailedToOpenPlayer)?;

        if !status.success() {
            eprintln!("video player not exited normally");
            eprintln!("do you still want to count the episode as completed? (y/N)");

            if !input::read_yn(Answer::No)? {
                return Ok(());
            }
        }

        let state = &mut self.season.state;
        state.watched_episodes = episode.max(state.watched_episodes);

        Ok(())
    }

    pub fn play_all_episodes(&mut self) -> Result<(), SeriesError> {
        loop {
            let next_episode = self.season.state.watched_episodes + 1;
            self.play_episode(next_episode)?;

            if self.has_unwatched_episodes() {
                self.prompt(Prompt::EpisodeCompleted)?;
            } else {
                self.prompt(Prompt::SeriesCompleted)?;
                break;
            }
        }

        Ok(())
    }

    pub fn has_unwatched_episodes(&self) -> bool {
        let state = &self.season.state;

        match state.info.episodes {
            Some(total_eps) if total_eps > state.watched_episodes => true,
            _ => false,
        }
    }

    fn save_updated_season_data(&mut self) -> Result<(), SeriesError> {
        if self.config.season_num >= self.dir.seasons().len() {
            return Ok(());
        }

        self.dir.seasons_mut()[self.config.season_num] = self.season.clone();
        self.dir.save()
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

    println!(
        "select season {} by entering the number next to its name:\n",
        1 + season
    );

    println!("0 [manual search]");

    for (i, series) in found.iter().enumerate() {
        println!("{} [{}]", 1 + i, series.title);
    }

    let index = input::read_range(0, found.len())?;

    match index {
        0 => {
            println!("enter the name you want to search for:");
            let name = input::read_line()?;

            search_for_series_info(backend, &name, season)
        }
        _ => Ok(found.swap_remove(index - 1)),
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SeasonState {
    #[serde(flatten)]
    pub state: AnimeEntry,
    pub needs_info: bool,
    pub needs_sync: bool,
}

impl SeasonState {
    pub fn sync_info_from_remote<B>(
        &mut self,
        config: &SeriesConfig<B>,
        dir: &FolderData,
        season_num: usize,
    ) -> Result<(), SeriesError>
    where
        B: SyncBackend,
    {
        if !self.needs_info {
            return Ok(());
        }

        if config.offline_mode {
            // The series title is the only thing that can really be used when offline.
            // While things like the number of episode files present could be used as the
            // series episode count, it is common to only partially have a series downloaded
            self.state.info.title = dir.series_info.name.clone();
            return Ok(());
        }

        let info = search_for_series_info(&config.sync_service, &dir.series_info.name, season_num)?;

        self.state.info = info;
        // We only want to set this flag when online, since offline mode only provides
        // very basic information at best
        self.needs_info = false;
        Ok(())
    }

    pub fn sync_data_from_remote<B>(
        &mut self,
        config: &SeriesConfig<B>,
        dir: &FolderData,
        season_num: usize,
    ) -> Result<(), SeriesError>
    where
        B: SyncBackend,
    {
        self.sync_info_from_remote(config, dir, season_num)?;

        // There's no information we can sync when offline
        if config.offline_mode {
            return Ok(());
        }

        let entry = config
            .sync_service
            .get_list_entry(self.state.info.clone())?;

        if let Some(entry) = entry {
            // If we don't have new data to report, we should sync the data from the backend to keep up with
            // any changes made outside of the program
            if !self.needs_sync {
                self.state = entry;
            }
        }

        Ok(())
    }
}
