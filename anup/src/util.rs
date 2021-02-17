use parking_lot::Mutex;
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};
use tokio::task;

#[macro_export]
macro_rules! try_opt_r {
    ($x:expr) => {
        match $x {
            Some(value) => value,
            None => return Ok(()),
        }
    };
}

#[macro_export]
macro_rules! try_opt_ret {
    ($x:expr) => {
        match $x {
            Some(value) => value,
            None => return,
        }
    };
}

pub fn hm_from_mins<F>(total_mins: F) -> String
where
    F: Into<f32>,
{
    let total_mins = total_mins.into();

    let hours = (total_mins / 60.0).floor() as u8;
    let minutes = (total_mins % 60.0).floor() as u8;

    format!("{:02}:{:02}H", hours, minutes)
}

pub type ArcMutex<T> = Arc<Mutex<T>>;

pub fn arc_mutex<T>(value: T) -> ArcMutex<T> {
    Arc::new(Mutex::new(value))
}

pub struct ScopedTask<T>(task::JoinHandle<T>);

impl<T> Deref for ScopedTask<T> {
    type Target = task::JoinHandle<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for ScopedTask<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> From<task::JoinHandle<T>> for ScopedTask<T> {
    fn from(task: task::JoinHandle<T>) -> Self {
        Self(task)
    }
}

impl<T> Drop for ScopedTask<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}
