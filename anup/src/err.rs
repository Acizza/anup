use anyhow::Error;
use std::io;

pub fn is_file_nonexistant(err: &Error) -> bool {
    match err.downcast_ref::<io::Error>() {
        Some(err) if err.kind() == io::ErrorKind::NotFound => true,
        _ => false,
    }
}
