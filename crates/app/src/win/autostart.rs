//! Start at login (R17): a value under the per-user `Run` key naming the
//! executable, which Windows launches at sign-in. Per-user, so no admin
//! rights are needed.

use std::io;
use std::path::Path;

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::HSTRING;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "saw-auto-clicker";

/// The argument the value passes, so a start at sign-in stays in the tray
/// rather than opening the settings window.
pub const AT_LOGIN: &str = "--at-login";

/// One value under a key of `HKEY_CURRENT_USER`.
pub struct StartAtLogin {
    subkey: HSTRING,
    value: HSTRING,
}

impl StartAtLogin {
    pub fn for_this_user() -> StartAtLogin {
        StartAtLogin::at(RUN_KEY, VALUE_NAME)
    }

    fn at(subkey: &str, value: &str) -> StartAtLogin {
        StartAtLogin {
            subkey: HSTRING::from(subkey),
            value: HSTRING::from(value),
        }
    }

    /// Whether the value starts `exe`.
    pub fn starts(&self, exe: &Path) -> io::Result<bool> {
        Ok(self
            .command()?
            .is_some_and(|command| command.eq_ignore_ascii_case(&command_for(exe))))
    }

    /// Rewrites a value that starts `exe` the way versions before 0.5.0 did,
    /// without [`AT_LOGIN`], so sign-in stops opening the settings window.
    pub fn update(&self, exe: &Path) -> io::Result<()> {
        if self
            .command()?
            .is_some_and(|command| command.eq_ignore_ascii_case(&quoted(exe)))
        {
            self.enable(exe)?;
        }
        Ok(())
    }

    /// Points the value at `exe`.
    pub fn enable(&self, exe: &Path) -> io::Result<()> {
        self.set_command(&command_for(exe))
    }

    fn set_command(&self, command: &str) -> io::Result<()> {
        let data: Vec<u16> = command.encode_utf16().chain([0]).collect();
        let result = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &self.subkey,
                &self.value,
                REG_SZ.0,
                Some(data.as_ptr().cast()),
                (data.len() * size_of::<u16>()) as u32,
            )
        };
        check(result)
    }

    /// Removing a value that isn't there succeeds.
    pub fn disable(&self) -> io::Result<()> {
        let result = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, &self.subkey, &self.value) };
        if result == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        check(result)
    }

    /// The command line the value holds, or `None` when there is no value.
    fn command(&self) -> io::Result<Option<String>> {
        let mut len = 0u32;
        // Sizing call: `len` comes back in bytes, including the NUL.
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &self.subkey,
                &self.value,
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut len),
            )
        };
        if result == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        check(result)?;
        let mut data = vec![0u16; (len as usize).div_ceil(size_of::<u16>())];
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &self.subkey,
                &self.value,
                RRF_RT_REG_SZ,
                None,
                Some(data.as_mut_ptr().cast()),
                Some(&mut len),
            )
        };
        check(result)?;
        let chars = &data[..len as usize / size_of::<u16>()];
        let text = chars.strip_suffix(&[0]).unwrap_or(chars);
        Ok(Some(String::from_utf16_lossy(text)))
    }
}

fn command_for(exe: &Path) -> String {
    format!("{} {AT_LOGIN}", quoted(exe))
}

/// Quoted so a path with spaces stays one argument.
fn quoted(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

fn check(result: WIN32_ERROR) -> io::Result<()> {
    if result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(result.0 as i32))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use windows::Win32::System::Registry::RegDeleteKeyW;

    use super::*;

    /// A key of its own under `HKEY_CURRENT_USER\Software`, so the test never
    /// touches the real `Run` key; deleted on drop.
    struct TestKey(String);

    impl TestKey {
        fn new() -> TestKey {
            TestKey::new_named("round-trip")
        }

        fn new_named(name: &str) -> TestKey {
            TestKey(format!(
                r"Software\saw-auto-clicker-test-{name}-{}",
                std::process::id()
            ))
        }
    }

    impl Drop for TestKey {
        fn drop(&mut self) {
            let _ = unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, &HSTRING::from(&self.0)) };
        }
    }

    #[test]
    fn enable_and_disable_add_and_remove_the_value() {
        let key = TestKey::new();
        let entry = StartAtLogin::at(&key.0, VALUE_NAME);
        let exe = PathBuf::from(r"C:\Program Files\saw auto clicker\saw-app.exe");
        assert!(!entry.starts(&exe).unwrap());
        entry.disable().unwrap();

        entry.enable(&exe).unwrap();
        assert!(entry.starts(&exe).unwrap());
        assert_eq!(
            entry.command().unwrap().as_deref(),
            Some(r#""C:\Program Files\saw auto clicker\saw-app.exe" --at-login"#)
        );

        entry.disable().unwrap();
        assert!(!entry.starts(&exe).unwrap());
        assert_eq!(entry.command().unwrap(), None);
    }

    #[test]
    fn update_adds_the_argument_to_an_older_value_for_this_copy_only() {
        let key = TestKey::new_named("update");
        let entry = StartAtLogin::at(&key.0, VALUE_NAME);
        let exe = Path::new(r"C:\saw\saw-app.exe");
        entry.update(exe).unwrap();
        assert_eq!(entry.command().unwrap(), None);

        entry.set_command(r#""D:\other\saw-app.exe""#).unwrap();
        entry.update(exe).unwrap();
        assert!(!entry.starts(exe).unwrap());

        entry.set_command(r#""C:\saw\saw-app.exe""#).unwrap();
        entry.update(exe).unwrap();
        assert!(entry.starts(exe).unwrap());
    }

    #[test]
    fn a_value_for_another_copy_does_not_count() {
        let key = TestKey::new_named("other-copy");
        let entry = StartAtLogin::at(&key.0, VALUE_NAME);
        entry.enable(Path::new(r"D:\old\saw-app.exe")).unwrap();
        assert!(!entry.starts(Path::new(r"C:\new\saw-app.exe")).unwrap());
        assert!(entry.starts(Path::new(r"d:\OLD\saw-app.exe")).unwrap());
    }
}
