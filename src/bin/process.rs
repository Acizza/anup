use std::path::Path;
use std::process::Command;

#[cfg(target_os = "windows")]
pub const START_PROGRAM: &str = "explorer";
#[cfg(target_os = "macos")]
pub const START_PROGRAM: &str = "open";
#[cfg(target_os = "linux")]
pub const START_PROGRAM: &str = "xdg-open";

pub fn open_with_default(file: &Path) -> Command {
    let mut cmd = Command::new(START_PROGRAM);
    cmd.arg(file);
    cmd
}
