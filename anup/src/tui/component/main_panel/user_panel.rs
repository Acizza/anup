use super::{Component, ShouldReset};
use crate::try_opt_r;
use crate::tui::component::input::{Input, InputFlags};
use crate::tui::widget_util::{block, style, text, SelectWidgetState};
use crate::tui::UIState;
use crate::user::{RemoteType, UserInfo};
use crate::{file::SerializedFile, key::Key};
use crate::{
    remote::{RemoteLogin, RemoteStatus},
    tui::state::SharedState,
};
use anime::remote::anilist::AniList;
use anime::remote::{AccessToken, Remote, RemoteService};
use anyhow::{anyhow, Context, Result};
use crossterm::event::KeyCode;
use std::process::Command;
use tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tui::style::Color;
use tui::terminal::Frame;
use tui::text::Span;
use tui::widgets::{Row, Table, TableState};
use tui::{backend::Backend, style::Style};
use tui_utils::{
    list::{EnumListItems, SelectableEnum},
    widgets::{Fragment, SimpleList, SimpleText, TextFragments},
};

pub struct UserPanel {
    user_table_state: SelectWidgetState<TableState>,
    selected_service: SelectableEnum<RemoteType>,
    token_input: Input,
    current_panel: SelectedPanel,
    state: SharedState,
}

impl UserPanel {
    pub fn new(state: SharedState) -> Self {
        Self {
            user_table_state: SelectWidgetState::new(),
            selected_service: SelectableEnum::new(),
            token_input: Input::new(InputFlags::empty(), "Paste Token"),
            current_panel: SelectedPanel::SelectUser,
            state,
        }
    }

    fn add_user_from_inputs(&mut self, state: &mut UIState) -> Result<()> {
        use anime::remote::anilist::Auth;

        let token_text = self.token_input.text();

        if token_text.is_empty() {
            return Ok(());
        }

        match self.selected_service.selected() {
            service @ RemoteType::AniList => {
                let token = AccessToken::encode(token_text);
                let auth = Auth::retrieve(token.clone()).context("failed to get new user auth")?;

                let info = UserInfo::new(service, &auth.user.name);

                state.remote = RemoteStatus::LoggedIn(AniList::Authenticated(auth).into());
                state.users.add_and_set_last(info, token);
                state.users.save().context("failed to save new user")?;

                self.token_input.clear();
                Ok(())
            }
        }
    }

    fn selected_user<'a>(&'a self, state: &'a UIState) -> Option<(&'a UserInfo, &'a AccessToken)> {
        let index = self.user_table_state.selected()?;
        state.users.get().iter().nth(index)
    }

    fn remove_selected_user(&mut self, state: &mut UIState) -> Result<()> {
        let user = {
            let (user, _) = try_opt_r!(self.selected_user(state));
            user.to_owned()
        };

        let remote = state.remote.get_logged_in()?;

        if user.is_logged_in(remote) {
            state.remote = RemoteStatus::LoggedIn(Remote::offline());
        }

        state.users.remove(&user);

        // Since our user table has been changed, we should make sure our selected user is still valid
        self.user_table_state.validate_selected(state.users.len());

        state.users.save()
    }

    fn login_as_selected_user(&mut self, state: &mut UIState) -> Result<()> {
        let (info, token) = try_opt_r!(self.selected_user(state));

        match info.service {
            RemoteType::AniList => {
                self.state.login_to_remote_async(RemoteLogin::AniList(
                    info.username.clone(),
                    token.clone(),
                ));

                state.users.last_used = Some(info.to_owned());
                state.users.save()?;
            }
        }

        Ok(())
    }

    fn open_auth_url(&self) -> Result<()> {
        let url = match self.selected_service.selected() {
            RemoteType::AniList => anime::remote::anilist::auth_url(crate::ANILIST_CLIENT_ID),
        };

        #[cfg(target_os = "linux")]
        let opener = "xdg-open";
        #[cfg(target_os = "macos")]
        let opener = "open";
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        compile_error!("must specify URL opener for this platform");

        Command::new(opener)
            .arg(url)
            .spawn()
            .with_context(|| anyhow!("failed to open URL in browser with {}", opener))
            .map(|_| ())
    }

    fn draw_add_user_panel<B>(&mut self, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let is_panel_selected = self.current_panel == SelectedPanel::AddUser;

        let outline = block::selectable("Add User", is_panel_selected);
        frame.render_widget(outline, rect);

        let vert_split = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    // Token input
                    Constraint::Length(Input::DRAW_LINES_REQUIRED),
                    // Spacer
                    Constraint::Length(1),
                    // Service selection
                    Constraint::Min(4),
                    // Spacer
                    Constraint::Length(1),
                    // Hint text
                    Constraint::Length(3),
                ]
                .as_ref(),
            )
            .vertical_margin(2)
            .horizontal_margin(4)
            .split(rect);

        self.token_input.set_selected(is_panel_selected);
        self.token_input.draw(vert_split[0], frame);

        let services_block = block::selectable("Service", is_panel_selected);
        let services_block_area = services_block.inner(vert_split[2]);

        frame.render_widget(services_block, vert_split[2]);

        let services = RemoteType::items()
            .iter()
            .copied()
            .map(RemoteType::as_str)
            .map(Span::raw);

        let services_widget = SimpleList::new(services)
            .highlight_symbol(Span::styled(">", style::list_selector(is_panel_selected)))
            .select(Some(self.selected_service.index() as u16));

        frame.render_widget(services_widget, services_block_area);

        let hint_fragments = [
            Fragment::span(text::hint("Ctrl + O")),
            Fragment::Line,
            Fragment::span(text::hint("-")),
            Fragment::Line,
            Fragment::span(text::hint("Open auth URL")),
        ];

        let hint_widget = TextFragments::new(&hint_fragments).alignment(Alignment::Center);
        frame.render_widget(hint_widget, vert_split[4]);
    }

    fn draw_user_selection_panel<B>(&mut self, state: &UIState, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let is_panel_selected = self.current_panel == SelectedPanel::SelectUser;

        let block = block::selectable(None, is_panel_selected);
        let block_area = block.inner(rect);

        frame.render_widget(block, rect);

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Min(10),
                    Constraint::Length(1),
                    Constraint::Length(4),
                    Constraint::Length(2),
                ]
                .as_ref(),
            )
            .horizontal_margin(1)
            .split(block_area);

        self.draw_users_table(is_panel_selected, state, layout[0], frame);

        let key_hints_fragments = [
            Fragment::span(text::hint("O - Go offline")),
            Fragment::Line,
            Fragment::span(text::hint("D - Remove account")),
            Fragment::Line,
            Fragment::span(text::hint("Enter - Login as selected")),
        ];

        let key_hints_widget =
            TextFragments::new(&key_hints_fragments).alignment(Alignment::Center);

        frame.render_widget(key_hints_widget, layout[2]);

        let yellow_text = |value| text::with_color(value, Color::Yellow);

        match &state.remote {
            RemoteStatus::LoggingIn(username) => {
                let fragments = [
                    Fragment::span(yellow_text("Logging In As ")),
                    Fragment::span(yellow_text(&username)),
                ];

                let widget = TextFragments::new(&fragments).alignment(Alignment::Center);
                frame.render_widget(widget, layout[3]);
            }
            RemoteStatus::LoggedIn(remote) if remote.is_offline() => {
                let widget =
                    SimpleText::new(yellow_text("Currently Offline")).alignment(Alignment::Center);
                frame.render_widget(widget, layout[3]);
            }
            RemoteStatus::LoggedIn(_) => (),
        }
    }

    fn draw_users_table<B>(
        &mut self,
        is_selected: bool,
        state: &UIState,
        rect: Rect,
        frame: &mut Frame<B>,
    ) where
        B: Backend,
    {
        let remote = state.remote.get_logged_in();

        let users = state.users.get().keys().map(|user| {
            let is_logged_in = remote
                .as_ref()
                .map(|remote| user.is_logged_in(remote))
                .unwrap_or(false);

            let data = [user.username.as_str(), user.service.as_str()];

            let style = if is_logged_in {
                style::fg(Color::Blue)
            } else {
                Style::default()
            };

            Row::new(data.to_vec()).style(style)
        });

        let header = Row::new(vec!["Username", "Service"]);

        let users_widget = Table::new(users)
            .header(header)
            .widths([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .highlight_symbol(">")
            .highlight_style(style::list_selector(is_selected))
            .column_spacing(4);

        frame.render_stateful_widget(users_widget, rect, &mut self.user_table_state);
    }

    pub fn draw<B: Backend>(&mut self, state: &UIState, rect: Rect, frame: &mut Frame<B>) {
        let horiz_split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
            .split(rect);

        self.draw_user_selection_panel(state, horiz_split[0], frame);
        self.draw_add_user_panel(horiz_split[1], frame);
    }
}

impl Component for UserPanel {
    type State = UIState;
    type KeyResult = Result<ShouldReset>;

    fn process_key(&mut self, key: Key, state: &mut Self::State) -> Self::KeyResult {
        match *key {
            KeyCode::Esc => Ok(ShouldReset::Yes),
            KeyCode::Tab => {
                self.current_panel.increment();
                Ok(ShouldReset::No)
            }
            _ => match self.current_panel {
                SelectedPanel::SelectUser => match *key {
                    KeyCode::Up | KeyCode::Down => {
                        self.user_table_state
                            .update_selected(key, state.users.len());

                        Ok(ShouldReset::No)
                    }
                    KeyCode::Enter => {
                        self.login_as_selected_user(state)?;
                        Ok(ShouldReset::Yes)
                    }
                    KeyCode::Char('d') => {
                        self.remove_selected_user(state)?;
                        Ok(ShouldReset::No)
                    }
                    KeyCode::Char('o') => {
                        state.remote = RemoteStatus::LoggedIn(Remote::offline());
                        Ok(ShouldReset::Yes)
                    }
                    _ => Ok(ShouldReset::No),
                },
                SelectedPanel::AddUser => match *key {
                    KeyCode::Up | KeyCode::Down => {
                        match *key {
                            KeyCode::Up => self.selected_service.decrement(),
                            KeyCode::Down => self.selected_service.increment(),
                            _ => unreachable!(),
                        }

                        Ok(ShouldReset::No)
                    }
                    KeyCode::Char('o') if key.ctrl_pressed() => {
                        self.open_auth_url()?;
                        Ok(ShouldReset::No)
                    }
                    KeyCode::Enter => {
                        self.add_user_from_inputs(state)?;
                        Ok(ShouldReset::No)
                    }
                    _ => {
                        self.token_input.process_key(key);
                        Ok(ShouldReset::No)
                    }
                },
            },
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
enum SelectedPanel {
    SelectUser,
    AddUser,
}

impl SelectedPanel {
    fn next(self) -> Self {
        match self {
            Self::SelectUser => Self::AddUser,
            Self::AddUser => Self::SelectUser,
        }
    }

    #[inline(always)]
    fn increment(&mut self) {
        *self = self.next();
    }
}
