use std::ffi::OsString;
use std::io;
use std::process::{Command, ExitStatus};

pub fn open_with_default<S>(arg: S) -> io::Result<ExitStatus>
where
    S: Into<OsString>,
{
    #[cfg(target_os = "macos")]
    const LAUNCH_PROGRAM: &str = "open";
    #[cfg(target_os = "linux")]
    const LAUNCH_PROGRAM: &str = "xdg-open";

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    compile_error!("support for opening URL's not implemented for this platform");

    let mut cmd = Command::new(LAUNCH_PROGRAM);
    cmd.arg(arg.into());
    cmd.output().map(|output| output.status)
}
