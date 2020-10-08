use anyhow::Error;
use std::io;

pub fn is_file_nonexistant(err: &Error) -> bool {
    matches!(err.downcast_ref::<io::Error>(), Some(err) if err.kind() == io::ErrorKind::NotFound)
}
