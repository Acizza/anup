use std::ffi::OsString;
use std::io;
use std::process::ExitStatus;

#[cfg(windows)]
pub fn open_with_default<S>(file: S) -> io::Result<ExitStatus>
where
    S: Into<OsString>,
{
    use std::mem;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::process::ExitStatusExt;
    use std::ptr;
    use winapi::shared::minwindef::{DWORD, HKEY};
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::GetExitCodeProcess;
    use winapi::um::shellapi::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use winapi::um::synchapi::WaitForSingleObject;
    use winapi::um::winbase::INFINITE;
    use winapi::um::winnt::HANDLE;
    use winapi::um::winuser::SW_SHOW;

    let file_name = file.into()
        .encode_wide()
        .chain(Some(0).into_iter())
        .collect::<Vec<_>>();

    let mut exec_info = SHELLEXECUTEINFOW {
        cbSize: mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        hwnd: ptr::null_mut(),
        lpVerb: ptr::null(),
        lpFile: file_name.as_ptr(),
        lpParameters: ptr::null(),
        lpDirectory: ptr::null(),
        nShow: SW_SHOW,
        hInstApp: ptr::null_mut(),
        lpIDList: ptr::null_mut(),
        lpClass: ptr::null(),
        hkeyClass: ptr::null_mut() as HKEY,
        dwHotKey: 0,
        hMonitor: ptr::null_mut() as HANDLE,
        hProcess: ptr::null_mut() as HANDLE,
    };

    unsafe {
        ShellExecuteExW(&mut exec_info);
        WaitForSingleObject(exec_info.hProcess, INFINITE);

        let mut exit_code = 0 as DWORD;
        GetExitCodeProcess(exec_info.hProcess, &mut exit_code);

        CloseHandle(exec_info.hProcess);

        Ok(ExitStatus::from_raw(exit_code as u32))
    }
}

#[cfg(not(windows))]
pub fn open_with_default<S>(file: S) -> io::Result<ExitStatus>
where
    S: Into<OsString>,
{
    use std::process::Command;

    #[cfg(target_os = "windows")]
    const LAUNCH_PROGRAM: &str = "explorer";
    #[cfg(target_os = "macos")]
    const LAUNCH_PROGRAM: &str = "open";
    #[cfg(target_os = "linux")]
    const LAUNCH_PROGRAM: &str = "xdg-open";

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    compile_error!("support for opening URL's not implemented for this platform");

    let mut cmd = Command::new(LAUNCH_PROGRAM);
    cmd.arg(file.into());
    cmd.output().map(|output| output.status)
}
