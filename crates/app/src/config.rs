//! The settings file (R14): `%APPDATA%\saw-auto-clicker\config.toml` by
//! default, loaded at startup and written whenever the settings window
//! applies a change.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use saw_core::{ACTIVATION_DELAY_RANGE, CPS_RANGE, JITTER_RANGE, Settings};
use serde::{Deserialize, Serialize};

use crate::hotkey_text::{self, HotkeyText};

/// Where the settings live for this user, or `None` when `APPDATA` is unset.
pub fn default_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(
        PathBuf::from(appdata)
            .join("saw-auto-clicker")
            .join("config.toml"),
    )
}

/// The file's layout. Every field is optional so that one bad or missing
/// value falls back to its own default without discarding the others.
#[derive(Default, Deserialize, Serialize)]
struct File {
    cps: Option<u32>,
    activation_delay_ms: Option<u64>,
    jitter_percent: Option<u32>,
    hotkey: Option<String>,
}

/// What [`load`] read.
#[derive(Debug, PartialEq)]
pub struct Loaded {
    pub settings: Settings,
    /// Why some or all of the file was replaced by defaults, for the settings
    /// window to show. `None` when the file was read in full, or is missing.
    pub problem: Option<String>,
}

/// Reads the settings, using Q1–Q3's default for each value that is missing,
/// or out of range, and for all of them when the file is missing, isn't TOML,
/// or holds a value of the wrong type.
pub fn load(path: &Path) -> Loaded {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Loaded {
                settings: Settings::default(),
                problem: None,
            };
        }
        Err(e) => return defaults_because(format!("{} could not be read: {e}", path.display())),
    };
    match toml::from_str::<File>(&text) {
        Ok(file) => from_file(file),
        Err(e) => defaults_because(format!(
            "{} could not be read as settings: {e}",
            path.display()
        )),
    }
}

fn defaults_because(problem: String) -> Loaded {
    Loaded {
        settings: Settings::default(),
        problem: Some(format!("{problem}; using the defaults")),
    }
}

fn from_file(file: File) -> Loaded {
    let defaults = Settings::default();
    let mut problems = Vec::new();
    let cps = pick(
        &mut problems,
        "cps",
        file.cps
            .map(|v| in_range(v, *CPS_RANGE.start(), *CPS_RANGE.end())),
        defaults.cps(),
    );
    let delay_ms_range = (
        ACTIVATION_DELAY_RANGE.start().as_millis() as u64,
        ACTIVATION_DELAY_RANGE.end().as_millis() as u64,
    );
    let delay = pick(
        &mut problems,
        "activation_delay_ms",
        file.activation_delay_ms
            .map(|v| in_range(v, delay_ms_range.0, delay_ms_range.1).map(Duration::from_millis)),
        defaults.activation_delay(),
    );
    let jitter = pick(
        &mut problems,
        "jitter_percent",
        file.jitter_percent.map(|v| {
            let max = (JITTER_RANGE.end() * 100.0).round() as u32;
            in_range(v, 0, max).map(|p| f64::from(p) / 100.0)
        }),
        defaults.jitter(),
    );
    let hotkey = pick(
        &mut problems,
        "hotkey",
        file.hotkey
            .map(|text| hotkey_text::parse(&text).and_then(hotkey_text::usable)),
        defaults.hotkey(),
    );
    let settings = Settings::new(cps, delay, jitter, hotkey)
        .expect("each value was checked against its range");
    let problem = (!problems.is_empty()).then(|| {
        format!(
            "in the settings file, {}; using the default for each",
            problems.join(", ")
        )
    });
    Loaded { settings, problem }
}

/// A value read from the file if it is present and valid, else `default`,
/// noting in `problems` why a present value was not used.
fn pick<T>(
    problems: &mut Vec<String>,
    name: &str,
    value: Option<Result<T, String>>,
    default: T,
) -> T {
    match value {
        None => default,
        Some(Ok(v)) => v,
        Some(Err(why)) => {
            problems.push(format!("{name} {why}"));
            default
        }
    }
}

fn in_range<T: PartialOrd + Copy + std::fmt::Display>(
    value: T,
    min: T,
    max: T,
) -> Result<T, String> {
    if (min..=max).contains(&value) {
        Ok(value)
    } else {
        Err(format!("must be {min}–{max}, got {value}"))
    }
}

/// Writes the settings, creating the directory if needed. The file is
/// replaced in one step, so a failed write leaves the previous one intact.
pub fn save(path: &Path, settings: &Settings) -> io::Result<()> {
    let file = File {
        cps: Some(settings.cps()),
        activation_delay_ms: Some(settings.activation_delay().as_millis() as u64),
        jitter_percent: Some((settings.jitter() * 100.0).round() as u32),
        hotkey: Some(HotkeyText(settings.hotkey()).to_string()),
    };
    let text = toml::to_string(&file).map_err(io::Error::other)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let temporary = path.with_extension("toml.tmp");
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use saw_core::{Hotkey, Key, Modifiers};

    use super::*;

    /// A fresh directory under the system temp dir, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> TempDir {
            let dir =
                std::env::temp_dir().join(format!("saw-app-config-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        fn file(&self) -> PathBuf {
            self.0.join("saw-auto-clicker").join("config.toml")
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn custom() -> Settings {
        let hotkey = Hotkey {
            modifiers: Modifiers {
                shift: true,
                meta: true,
                ..Modifiers::default()
            },
            key: Key::K,
        };
        Settings::new(37, Duration::from_millis(600), 0.3, hotkey).unwrap()
    }

    #[test]
    fn a_missing_file_gives_the_defaults_quietly() {
        let dir = TempDir::new("missing");
        assert_eq!(
            load(&dir.file()),
            Loaded {
                settings: Settings::default(),
                problem: None
            }
        );
    }

    #[test]
    fn saved_settings_load_back_and_the_directory_is_created() {
        let dir = TempDir::new("round-trip");
        save(&dir.file(), &custom()).unwrap();
        assert_eq!(
            load(&dir.file()),
            Loaded {
                settings: custom(),
                problem: None
            }
        );
        save(&dir.file(), &Settings::default()).unwrap();
        assert_eq!(load(&dir.file()).settings, Settings::default());
    }

    #[test]
    fn the_file_is_readable_toml() {
        let dir = TempDir::new("format");
        save(&dir.file(), &custom()).unwrap();
        let text = fs::read_to_string(dir.file()).unwrap();
        for line in [
            "cps = 37",
            "activation_delay_ms = 600",
            "jitter_percent = 30",
            "hotkey = \"Shift+Win+K\"",
        ] {
            assert!(text.contains(line), "{line:?} missing from:\n{text}");
        }
    }

    #[test]
    fn a_file_that_is_not_toml_gives_the_defaults_with_a_reason() {
        let dir = TempDir::new("garbage");
        write(&dir.file(), "cps = = 3\n[[[");
        let loaded = load(&dir.file());
        assert_eq!(loaded.settings, Settings::default());
        assert!(
            loaded
                .problem
                .unwrap()
                .contains("could not be read as settings")
        );
    }

    #[test]
    fn a_value_of_the_wrong_type_gives_the_defaults() {
        let dir = TempDir::new("wrong-type");
        write(&dir.file(), "cps = \"fast\"\n");
        let loaded = load(&dir.file());
        assert_eq!(loaded.settings, Settings::default());
        assert!(loaded.problem.is_some());
    }

    #[test]
    fn each_bad_value_falls_back_alone() {
        let dir = TempDir::new("partial");
        write(
            &dir.file(),
            "cps = 99\nactivation_delay_ms = 600\njitter_percent = 51\nhotkey = \"Ctrl+Nope\"\n",
        );
        let loaded = load(&dir.file());
        let d = Settings::default();
        assert_eq!(
            loaded.settings,
            Settings::new(d.cps(), Duration::from_millis(600), d.jitter(), d.hotkey()).unwrap()
        );
        let problem = loaded.problem.unwrap();
        for name in ["cps", "jitter_percent", "hotkey"] {
            assert!(problem.contains(name), "{name} not named in {problem:?}");
        }
        assert!(!problem.contains("activation_delay_ms"), "{problem:?}");
    }

    #[test]
    fn missing_values_take_their_defaults_and_unknown_ones_are_ignored() {
        let dir = TempDir::new("sparse");
        write(&dir.file(), "cps = 20\ncolour = \"red\"\n");
        let d = Settings::default();
        assert_eq!(
            load(&dir.file()),
            Loaded {
                settings: Settings::new(20, d.activation_delay(), d.jitter(), d.hotkey()).unwrap(),
                problem: None
            }
        );
    }

    #[test]
    fn range_ends_are_accepted() {
        let dir = TempDir::new("ends");
        write(
            &dir.file(),
            "cps = 50\nactivation_delay_ms = 50\njitter_percent = 50\n",
        );
        let loaded = load(&dir.file());
        assert_eq!(loaded.problem, None);
        assert_eq!(
            (
                loaded.settings.cps(),
                loaded.settings.activation_delay(),
                loaded.settings.jitter()
            ),
            (50, Duration::from_millis(50), 0.5)
        );
    }
}
