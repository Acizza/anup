use crate::series::{LoadedSeries, Series};
use std::ops::{Index, IndexMut};

pub struct Selection<T> {
    items: Vec<T>,
    index: WrappingIndex,
}

impl<T> Selection<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self {
            items,
            index: WrappingIndex::new(0),
        }
    }

    #[inline(always)]
    pub fn index(&self) -> usize {
        self.index.get()
    }

    #[inline(always)]
    pub fn selected(&self) -> Option<&T> {
        self.items.get(self.index.get())
    }

    #[inline(always)]
    pub fn selected_mut(&mut self) -> Option<&mut T> {
        self.items.get_mut(self.index.get())
    }

    #[inline(always)]
    pub fn inc_selected(&mut self) {
        self.index.increment(self.items.len())
    }

    #[inline(always)]
    pub fn dec_selected(&mut self) {
        self.index.decrement(self.items.len())
    }

    pub fn set_selected(&mut self, selected: usize) {
        if selected >= self.items.len() {
            return;
        }

        *self.index.get_mut() = selected;
    }

    #[inline(always)]
    pub fn push(&mut self, item: T) {
        self.items.push(item);
    }

    #[inline(always)]
    pub fn remove_selected(&mut self) -> Option<T> {
        self.remove_selected_with(Vec::remove)
    }

    #[inline(always)]
    pub fn swap_remove_selected(&mut self) -> Option<T> {
        self.remove_selected_with(Vec::swap_remove)
    }

    pub fn remove_selected_with<F>(&mut self, func: F) -> Option<T>
    where
        F: Fn(&mut Vec<T>, usize) -> T,
    {
        if self.items.is_empty() {
            return None;
        }

        let item = func(&mut self.items, self.index.get());

        if self.index == self.items.len() {
            self.index.decrement(self.items.len());
        }

        Some(item)
    }

    #[inline(always)]
    pub fn items_mut(&mut self) -> &mut Vec<T> {
        &mut self.items
    }

    #[inline(always)]
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter()
    }
}

impl Selection<LoadedSeries> {
    #[inline(always)]
    pub fn valid_selection_mut(&mut self) -> Option<&mut Series> {
        self.selected_mut().and_then(LoadedSeries::complete_mut)
    }
}

impl<T> Index<usize> for Selection<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.items[index]
    }
}

impl<T> IndexMut<usize> for Selection<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.items[index]
    }
}

impl<T> From<Vec<T>> for Selection<T> {
    fn from(value: Vec<T>) -> Self {
        Self::new(value)
    }
}

#[derive(Copy, Clone)]
pub struct WrappingIndex(usize);

impl WrappingIndex {
    #[inline(always)]
    pub fn new(index: usize) -> Self {
        Self(index)
    }

    #[inline(always)]
    pub fn get(self) -> usize {
        self.0
    }

    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut usize {
        &mut self.0
    }

    pub fn increment(&mut self, max: usize) {
        self.0 = if max > 0 { (self.0 + 1) % max } else { max };
    }

    pub fn decrement(&mut self, max: usize) {
        self.0 = if self.0 == 0 {
            max.saturating_sub(1)
        } else {
            self.0 - 1
        }
    }
}

impl PartialEq<usize> for WrappingIndex {
    fn eq(&self, other: &usize) -> bool {
        self.get() == *other
    }
}

impl<T> Index<WrappingIndex> for Vec<T> {
    type Output = T;

    fn index(&self, index: WrappingIndex) -> &Self::Output {
        &self[index.get()]
    }
}

impl<T> IndexMut<WrappingIndex> for Vec<T> {
    fn index_mut(&mut self, index: WrappingIndex) -> &mut Self::Output {
        &mut self[index.get()]
    }
}

impl Into<usize> for WrappingIndex {
    fn into(self) -> usize {
        self.0
    }
}
