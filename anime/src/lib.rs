#[cfg(feature = "diesel-support")]
#[macro_use]
extern crate diesel;

pub mod err;
pub mod local;
pub mod remote;

pub use err::{Error, Result};
