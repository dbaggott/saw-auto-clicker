//! The tray icon's GUID, which Windows needs to address the icon from outside
//! the `tray-icon` crate, as the startup notification does (R14).
//!
//! Windows binds a GUID to the path of the executable that first registers
//! it, unless the executable is Authenticode-signed, and this one is not. A
//! fixed GUID would fail to register from any second path, so each path gets
//! its own, derived from the path.

use std::path::Path;

/// 128-bit FNV-1a, fixed by its specification, so a path's GUID stays the
/// same across Rust versions and releases of the app.
const FNV_OFFSET_BASIS: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
const FNV_PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

/// The GUID for the executable at `exe`, the same for any case of the path,
/// as Windows paths are.
pub fn for_exe(exe: &Path) -> u128 {
    let hash = fnv_1a(exe.to_string_lossy().to_lowercase().as_bytes());
    // An RFC 9562 version 8 (custom) UUID: version 8, variant 10.
    (hash & !(0xf << 76) & !(0x3 << 62)) | (0x8 << 76) | (0x2 << 62)
}

fn fnv_1a(bytes: &[u8]) -> u128 {
    bytes.iter().fold(FNV_OFFSET_BASIS, |hash, &byte| {
        (hash ^ u128::from(byte)).wrapping_mul(FNV_PRIME)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_path_in_any_case_gets_the_same_guid() {
        let a = for_exe(Path::new(r"C:\Program Files\saw-auto-clicker\saw-app.exe"));
        let b = for_exe(Path::new(r"c:\PROGRAM FILES\Saw-Auto-Clicker\SAW-APP.EXE"));
        assert_eq!(a, b);
    }

    #[test]
    fn two_paths_get_different_guids() {
        let installed = for_exe(Path::new(r"C:\Program Files\saw-auto-clicker\saw-app.exe"));
        let copied = for_exe(Path::new(r"C:\Users\me\Desktop\saw-app.exe"));
        assert_ne!(installed, copied);
    }

    #[test]
    fn the_guid_is_a_version_8_uuid() {
        let guid = for_exe(Path::new(r"C:\saw-app.exe"));
        assert_eq!((guid >> 76) & 0xf, 8);
        assert_eq!((guid >> 62) & 0x3, 0b10);
    }

    #[test]
    fn a_path_keeps_its_guid_across_releases() {
        // Pinned, so a change to the hash shows up here rather than as every
        // upgraded install losing its tray icon's taskbar placement.
        assert_eq!(
            for_exe(Path::new(r"C:\saw-app.exe")),
            0x021a_4d93_51d9_88fc_afaf_3261_67a6_52a1
        );
    }

    #[test]
    fn the_hash_is_fnv_1a() {
        // The 128-bit FNV-1a of "foobar" that php-fnv1a's test vectors give.
        assert_eq!(fnv_1a(b"foobar"), 0x343e_1662_793c_64bf_6f0d_3597_ba44_6f18);
    }
}
