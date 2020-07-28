pub mod block;
pub mod color;
pub mod style;
pub mod text;
pub mod widget;

use crate::try_opt_ret;
use crate::tui::WrappingIndex;
use crate::user::RemoteType;
use std::borrow::Cow;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use termion::event::Key;
use tui::widgets::{ListState, TableState};

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
        match key {
            Key::Up | Key::Down => {
                let mut cur_index = WrappingIndex::new(self.0.selected().unwrap_or(0));

                match key {
                    Key::Up => cur_index.decrement(max_index),
                    Key::Down => cur_index.increment(max_index),
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

/// A data structure that can exposed as an array of items.
///
/// This is used in conjunction with `TypedSelectable`.
pub trait TypedListData: Sized {
    fn items<'a>() -> &'a [Self];
    fn item_str<'a>(item: &Self) -> Cow<'a, str>;

    fn len() -> usize {
        Self::items().len()
    }
}

impl TypedListData for RemoteType {
    fn items<'a>() -> &'a [Self] {
        Self::all()
    }

    fn item_str<'a>(item: &Self) -> Cow<'a, str> {
        item.as_str().into()
    }
}

/// A selectable widget that can be indexed as a type `T`.
pub struct TypedSelectable<T, S>(SelectWidgetState<S>, PhantomData<T>)
where
    T: TypedListData + Copy,
    S: SelectableWidget + Default;

impl<'a, T, S> TypedSelectable<T, S>
where
    T: TypedListData + Copy + 'a,
    S: SelectableWidget + Default,
{
    #[inline(always)]
    pub fn new() -> Self {
        Self(SelectWidgetState::new(), PhantomData)
    }

    /// Scrolls the currently selected entry based off of `key`.
    #[inline(always)]
    pub fn update_selected(&mut self, key: Key) {
        self.0.update_selected(key, T::len());
    }

    /// Returns all of `T`'s items as an iterator of strings.
    pub fn item_data() -> impl Iterator<Item = Cow<'a, str>> {
        T::items().iter().map(|item| T::item_str(item))
    }

    /// Returns the currently selected entry as `T`.
    pub fn selected(&self) -> Option<T> {
        let index = self.0.selected()?;
        T::items().get(index).copied()
    }

    #[inline(always)]
    pub fn state_mut(&mut self) -> &mut S {
        &mut self.0
    }
}
