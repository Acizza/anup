use super::{Component, Draw};
use crate::err::{self, Result};
use crate::file::SerializedFile;
use crate::try_opt_r;
use crate::tui::component::input::Input;
use crate::tui::widget_util::{block, style, text, SelectWidgetState, TypedSelectable};
use crate::tui::UIState;
use crate::user::{RemoteType, UserInfo};
use anime::remote::anilist::AniList;
use anime::remote::{AccessToken, Remote, RemoteService};
use smallvec::SmallVec;
use snafu::ResultExt;
use std::borrow::Cow;
use std::process::Command;
use termion::event::Key;
use tui::backend::Backend;
use tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tui::style::Color;
use tui::terminal::Frame;
use tui::widgets::{List, ListState, Paragraph, Row, Table, TableState, Text};

type ServiceList = TypedSelectable<RemoteType, ListState>;

pub struct UserPanel {
    user_table_state: SelectWidgetState<TableState>,
    service_list: ServiceList,
    token_input: Input,
    current_panel: SelectedPanel,
}

impl UserPanel {
    pub fn new() -> Self {
        Self {
            user_table_state: SelectWidgetState::new(),
            service_list: TypedSelectable::new(),
            token_input: Input::with_label(false, "Paste Token"),
            current_panel: SelectedPanel::SelectUser,
        }
    }

    fn add_user_from_inputs(&mut self, state: &mut UIState) -> Result<()> {
        use anime::remote::anilist::Auth;

        let token_text = self.token_input.text();

        if token_text.is_empty() {
            return Ok(());
        }

        match self.service_list.selected() {
            Some(service @ RemoteType::AniList) => {
                let token = AccessToken::encode(token_text);
                let auth = Auth::retrieve(token.clone())?;

                let info = UserInfo::new(service, &auth.user.name);

                state.remote = AniList::Authenticated(auth).into();
                state.users.add_and_set_last(info, token);
                state.users.save()?;

                self.token_input.clear();
                Ok(())
            }
            None => Ok(()),
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

        if user.is_logged_in(&state.remote) {
            state.remote = Remote::offline();
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
                use anime::remote::anilist::Auth;

                let auth = Auth::retrieve(token.clone())?;

                state.users.last_used = Some(info.to_owned());
                state.remote = AniList::Authenticated(auth).into();
                state.users.save()?;
            }
        }

        Ok(())
    }

    fn open_auth_url(&self) -> Result<()> {
        let url = match try_opt_r!(self.service_list.selected()) {
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
            .context(err::OpenURL { opener })
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
                    Input::DRAW_WITH_LABEL_CONSTRAINT,
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

        self.token_input.selected = is_panel_selected;
        self.token_input.draw(&(), vert_split[0], frame);

        let services_text = ServiceList::item_data().map(Text::raw);

        let services_widget = List::new(services_text)
            .block(block::selectable("Service", is_panel_selected))
            .highlight_style(style::list_selector(is_panel_selected))
            .highlight_symbol(">");

        frame.render_stateful_widget(
            services_widget,
            vert_split[2],
            self.service_list.state_mut(),
        );

        let hint_text = [text::hint("Ctrl + O\n-\nOpen auth URL")];
        let hint_widget = Paragraph::new(hint_text.iter())
            .alignment(Alignment::Center)
            .wrap(false);
        frame.render_widget(hint_widget, vert_split[4]);
    }

    fn draw_user_selection_panel<B>(&mut self, state: &UIState, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let is_panel_selected = self.current_panel == SelectedPanel::SelectUser;

        let outline = block::selectable(None, is_panel_selected);
        frame.render_widget(outline, rect);

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
            .split(rect);

        self.draw_users_table(is_panel_selected, state, layout[0], frame);

        let key_hints_text = [
            text::hint("O - Go offline\n"),
            text::hint("D - Remove account\n"),
            text::hint("Enter - Login as selected"),
        ];

        let key_hints_widget = Paragraph::new(key_hints_text.iter())
            .alignment(Alignment::Center)
            .wrap(false);
        frame.render_widget(key_hints_widget, layout[2]);

        if state.remote.is_offline() {
            let offline_text = [text::with_color("Currently Offline", Color::Yellow)];

            let offline_widget = Paragraph::new(offline_text.iter())
                .alignment(Alignment::Center)
                .wrap(true);
            frame.render_widget(offline_widget, layout[3]);
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
        let row_data = state
            .users
            .get()
            .keys()
            .map(|user| {
                let is_logged_in = user.is_logged_in(&state.remote);

                let data = [
                    Cow::Borrowed(user.username.as_str()),
                    Cow::Borrowed(user.service.as_str()),
                ];

                (data, is_logged_in)
            })
            .collect::<SmallVec<[_; 4]>>();

        let users = row_data.iter().map(|(data, is_logged_in)| {
            if *is_logged_in {
                Row::StyledData(data.iter(), style::fg(Color::Blue))
            } else {
                Row::Data(data.iter())
            }
        });

        // Note: tables currently have inconsistent spacing between columns: https://github.com/fdehau/tui-rs/issues/299
        let users_widget = Table::new(["Username", "Service"].iter(), users)
            .widths([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .highlight_symbol(">")
            .highlight_style(style::list_selector(is_selected))
            .column_spacing(4);

        frame.render_stateful_widget(users_widget, rect, &mut self.user_table_state);
    }
}

impl Component for UserPanel {
    type State = UIState;
    type KeyResult = Result<ShouldReset>;

    fn process_key(&mut self, key: Key, state: &mut Self::State) -> Self::KeyResult {
        match key {
            Key::Esc => Ok(ShouldReset::Yes),
            Key::Char('\t') => {
                self.current_panel.increment();
                Ok(ShouldReset::No)
            }
            key => match self.current_panel {
                SelectedPanel::SelectUser => match key {
                    Key::Up | Key::Down => {
                        self.user_table_state
                            .update_selected(key, state.users.len());

                        Ok(ShouldReset::No)
                    }
                    Key::Char('\n') => {
                        self.login_as_selected_user(state)?;
                        Ok(ShouldReset::Yes)
                    }
                    Key::Char('d') => {
                        self.remove_selected_user(state)?;
                        Ok(ShouldReset::No)
                    }
                    Key::Char('o') => {
                        state.remote = Remote::offline();
                        Ok(ShouldReset::Yes)
                    }
                    _ => Ok(ShouldReset::No),
                },
                SelectedPanel::AddUser => match key {
                    Key::Up | Key::Down => {
                        self.service_list.update_selected(key);
                        Ok(ShouldReset::No)
                    }
                    Key::Ctrl('o') => {
                        self.open_auth_url()?;
                        Ok(ShouldReset::No)
                    }
                    Key::Char('\n') => {
                        self.add_user_from_inputs(state)?;
                        Ok(ShouldReset::No)
                    }
                    key => {
                        self.token_input.process_key(key);
                        Ok(ShouldReset::No)
                    }
                },
            },
        }
    }
}

impl<B> Draw<B> for UserPanel
where
    B: Backend,
{
    type State = UIState;

    fn draw(&mut self, state: &Self::State, rect: Rect, frame: &mut Frame<B>) {
        let horiz_split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
            .split(rect);

        self.draw_user_selection_panel(state, horiz_split[0], frame);
        self.draw_add_user_panel(horiz_split[1], frame);
    }
}

#[derive(Copy, Clone)]
pub enum ShouldReset {
    Yes,
    No,
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
