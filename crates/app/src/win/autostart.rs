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

    pub fn is_enabled(&self) -> io::Result<bool> {
        Ok(self.command()?.is_some())
    }

    /// Points the value at `exe`, quoted so a path with spaces stays one
    /// argument.
    pub fn enable(&self, exe: &Path) -> io::Result<()> {
        let command = format!("\"{}\"", exe.display());
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
            TestKey(format!(
                r"Software\saw-auto-clicker-test-{}",
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
        assert!(!entry.is_enabled().unwrap());
        entry.disable().unwrap();

        let exe = PathBuf::from(r"C:\Program Files\saw auto clicker\saw-app.exe");
        entry.enable(&exe).unwrap();
        assert!(entry.is_enabled().unwrap());
        assert_eq!(
            entry.command().unwrap().as_deref(),
            Some(r#""C:\Program Files\saw auto clicker\saw-app.exe""#)
        );

        entry.disable().unwrap();
        assert!(!entry.is_enabled().unwrap());
    }
}
