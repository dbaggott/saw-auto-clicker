//! The apps a user can choose to auto-click in (R21): which executable a
//! window belongs to, and which apps have a window open.

/// `names` with duplicates that differ only in case dropped, sorted ignoring
/// case.
pub(crate) fn distinct_sorted(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut keyed: Vec<(String, String)> = names
        .into_iter()
        .map(|name| (name.to_lowercase(), name))
        .collect();
    keyed.sort();
    keyed.dedup_by(|a, b| a.0 == b.0);
    keyed.into_iter().map(|(_, name)| name).collect()
}

#[cfg(windows)]
pub use win::running_apps;
#[cfg(windows)]
pub(crate) use win::{executable_name, window_process};

#[cfg(windows)]
mod win {
    use std::ffi::OsString;
    use std::io;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;

    use windows::Win32::Foundation::{HANDLE, HWND, LPARAM};
    use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
    use windows::Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible,
    };
    use windows::core::{BOOL, PWSTR};

    use super::distinct_sorted;
    use crate::handle::OwnedHandle;

    /// The id of the process that owns `window`, and a handle to it with
    /// `PROCESS_QUERY_LIMITED_INFORMATION`.
    pub(crate) fn window_process(window: HWND) -> io::Result<(u32, OwnedHandle)> {
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(window, Some(&mut pid));
            if pid == 0 {
                return Err(io::Error::last_os_error());
            }
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?;
            Ok((pid, OwnedHandle::new(process)))
        }
    }

    /// The file name of the executable `process` runs, such as `app.exe`.
    /// `process` needs `PROCESS_QUERY_LIMITED_INFORMATION`.
    pub(crate) fn executable_name(process: HANDLE) -> io::Result<String> {
        // Long enough for any path Windows allows.
        let mut buf = vec![0u16; 32_768];
        let mut len = buf.len() as u32;
        unsafe {
            QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(buf.as_mut_ptr()),
                &mut len,
            )?;
        }
        let path = PathBuf::from(OsString::from_wide(&buf[..len as usize]));
        path.file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .ok_or_else(|| io::Error::other("the executable's name is not valid Unicode"))
    }

    /// The executable file names of the programs with a visible, titled
    /// top-level window now, other than this one, without duplicates and
    /// sorted ignoring case. A window whose process can't be read is left
    /// out.
    pub fn running_apps() -> Vec<String> {
        let mut windows: Vec<HWND> = Vec::new();
        unsafe {
            // A failed enumeration leaves the windows found before it.
            let _ = EnumWindows(Some(collect), LPARAM(&raw mut windows as isize));
        }
        let own = unsafe { GetCurrentProcessId() };
        let names = windows.into_iter().filter(|&w| shown(w)).filter_map(|w| {
            let (pid, process) = window_process(w).ok()?;
            (pid != own).then(|| executable_name(process.raw()).ok())?
        });
        distinct_sorted(names)
    }

    unsafe extern "system" fn collect(window: HWND, windows: LPARAM) -> BOOL {
        let windows = unsafe { &mut *(windows.0 as *mut Vec<HWND>) };
        windows.push(window);
        true.into()
    }

    /// Whether `window` is one a user sees: visible, not cloaked (as the
    /// windows of suspended Store apps and other virtual desktops are), and
    /// titled.
    fn shown(window: HWND) -> bool {
        unsafe {
            if !IsWindowVisible(window).as_bool() || GetWindowTextLengthW(window) == 0 {
                return false;
            }
            let mut cloaked = 0u32;
            let read = DwmGetWindowAttribute(
                window,
                DWMWA_CLOAKED,
                (&raw mut cloaked).cast(),
                size_of::<u32>() as u32,
            );
            read.is_err() || cloaked == 0
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use windows::Win32::System::Threading::GetCurrentProcess;

        #[test]
        fn this_process_has_a_readable_executable_name() {
            // The pseudo-handle needs no closing.
            let process = unsafe { GetCurrentProcess() };
            let expected = std::env::current_exe().unwrap();
            assert_eq!(
                executable_name(process).unwrap(),
                expected.file_name().unwrap().to_str().unwrap()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_differing_only_in_case_appear_once_sorted_ignoring_case() {
        let names = ["b.exe", "A.exe", "a.EXE", "Écran.exe", "c.exe", "écran.exe"];
        assert_eq!(
            distinct_sorted(names.map(String::from)),
            ["A.exe", "b.exe", "c.exe", "Écran.exe"]
        );
    }
}
