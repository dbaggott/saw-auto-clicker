//! The elevated-target check (R16, C5): whether an injection now would reach
//! the window under the cursor, judged by comparing integrity levels.

/// UIPI blocks injection into a higher integrity level, and does so silently,
/// so a level that can't be read counts as higher.
pub(crate) fn injectable(own_level: u32, target_level: Option<u32>) -> bool {
    target_level.is_some_and(|level| level <= own_level)
}

#[cfg(windows)]
pub(crate) use win::TargetCheck;

#[cfg(windows)]
mod win {
    use std::io;
    use std::mem::size_of;

    use windows::Win32::Foundation::{HANDLE, POINT};
    use windows::Win32::Security::{
        GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL,
        TOKEN_QUERY, TokenIntegrityLevel,
    };
    use windows::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GA_ROOT, GetAncestor, GetCursorPos, GetWindowThreadProcessId, WindowFromPoint,
    };

    use super::injectable;
    use crate::handle::OwnedHandle;

    pub(crate) struct TargetCheck {
        own_level: u32,
    }

    impl TargetCheck {
        pub(crate) fn new() -> io::Result<TargetCheck> {
            // The pseudo-handle needs no closing.
            let own_level = unsafe { integrity_level(GetCurrentProcess()) }?;
            Ok(TargetCheck { own_level })
        }

        /// The target is the top-level window under the cursor.
        pub(crate) fn target_injectable(&self) -> bool {
            injectable(self.own_level, target_level().ok())
        }
    }

    fn target_level() -> io::Result<u32> {
        unsafe {
            let mut cursor = POINT::default();
            GetCursorPos(&mut cursor)?;
            let window = GetAncestor(WindowFromPoint(cursor), GA_ROOT);
            if window.is_invalid() {
                return Err(io::Error::other("no window under the cursor"));
            }
            let mut pid = 0;
            GetWindowThreadProcessId(window, Some(&mut pid));
            if pid == 0 {
                return Err(io::Error::last_os_error());
            }
            let process =
                OwnedHandle::new(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?);
            integrity_level(process.raw())
        }
    }

    /// The last subauthority of the token's mandatory label SID, which is the
    /// `SECURITY_MANDATORY_*_RID` level. `process` needs
    /// `PROCESS_QUERY_LIMITED_INFORMATION`, which `OpenProcessToken` requires.
    unsafe fn integrity_level(process: HANDLE) -> io::Result<u32> {
        unsafe {
            let mut token = HANDLE::default();
            OpenProcessToken(process, TOKEN_QUERY, &mut token)?;
            let token = OwnedHandle::new(token);
            let mut len = 0;
            // Sizing call: fails with ERROR_INSUFFICIENT_BUFFER and sets `len`.
            let _ = GetTokenInformation(token.raw(), TokenIntegrityLevel, None, 0, &mut len);
            if (len as usize) < size_of::<TOKEN_MANDATORY_LABEL>() {
                return Err(io::Error::last_os_error());
            }
            // u64 elements align the buffer for the struct's pointer field.
            let mut buf = vec![0u64; (len as usize).div_ceil(size_of::<u64>())];
            GetTokenInformation(
                token.raw(),
                TokenIntegrityLevel,
                Some(buf.as_mut_ptr().cast()),
                len,
                &mut len,
            )?;
            let label = &*buf.as_ptr().cast::<TOKEN_MANDATORY_LABEL>();
            let sid = label.Label.Sid;
            let count = *GetSidSubAuthorityCount(sid);
            if count == 0 {
                return Err(io::Error::other("integrity label SID has no level"));
            }
            Ok(*GetSidSubAuthority(sid, u32::from(count) - 1))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// SECURITY_MANDATORY_LOW_RID to SECURITY_MANDATORY_SYSTEM_RID.
        const LEVELS: std::ops::RangeInclusive<u32> = 0x1000..=0x4000;

        #[test]
        fn this_process_has_a_readable_level() {
            let check = TargetCheck::new().unwrap();
            assert!(LEVELS.contains(&check.own_level), "{:#x}", check.own_level);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEDIUM: u32 = 0x2000;
    const HIGH: u32 = 0x3000;

    #[test]
    fn only_a_readable_level_no_higher_than_ours_is_injectable() {
        assert!(injectable(MEDIUM, Some(MEDIUM)));
        assert!(injectable(MEDIUM, Some(0x1000)));
        assert!(!injectable(MEDIUM, Some(HIGH)));
        assert!(!injectable(MEDIUM, None));
        assert!(injectable(HIGH, Some(HIGH)));
    }
}
