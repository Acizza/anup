pub mod block;
pub mod color;
pub mod style;
pub mod text;
pub mod widget;

use crate::{key::Key, try_opt_ret};
use crossterm::event::KeyCode;
use std::ops::{Deref, DerefMut};
use tui::widgets::{ListState, TableState};
use tui_utils::list::WrappingIndex;

/// A widget that can be selected / indexed.
pub trait SelectableWidget {
    fn select(&mut self, index: Option<usize>);
    fn selected(&self) -> Option<usize>;
}

impl SelectableWidget for ListState {
    fn select(&mut self, index: Option<usize>) {
        self.select(index)
    }

    fn selected(&self) -> Option<usize> {
        self.selected()
    }
}

impl SelectableWidget for TableState {
    fn select(&mut self, index: Option<usize>) {
        self.select(index)
    }

    fn selected(&self) -> Option<usize> {
        self.selected()
    }
}

/// Common functionality for a widget that can be selected / indexed.
#[derive(Default)]
pub struct SelectWidgetState<T>(T)
where
    T: SelectableWidget + Default;

impl<T> SelectWidgetState<T>
where
    T: SelectableWidget + Default,
{
    pub fn new() -> Self {
        let mut state = T::default();
        state.select(Some(0));
        Self(state)
    }

    pub fn unselected() -> Self {
        Self(T::default())
    }

    /// Scrolls the currently selected entry based off of `key`.
    pub fn update_selected(&mut self, key: Key, max_index: usize) {
        match *key {
            KeyCode::Up | KeyCode::Down => {
                let mut cur_index = WrappingIndex::new(self.0.selected().unwrap_or(0));

                match *key {
                    KeyCode::Up => cur_index.decrement(max_index),
                    KeyCode::Down => cur_index.increment(max_index),
                    _ => unreachable!(),
                }

                self.0.select(Some(cur_index.into()));
            }
            _ => (),
        }
    }

    /// Ensure that the currently selected entry is valid and fits within the given `max_index`.
    pub fn validate_selected(&mut self, max_index: usize) {
        let mut selected = WrappingIndex::new(try_opt_ret!(self.0.selected()));

        // Decrement our selected entry at max index so it doesn't go off-screen
        if selected == max_index {
            selected.decrement(max_index);
            self.0.select(Some(selected.into()));
        }
    }
}

impl<T> Deref for SelectWidgetState<T>
where
    T: SelectableWidget + Default,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for SelectWidgetState<T>
where
    T: SelectableWidget + Default,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
