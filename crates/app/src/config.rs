//! The settings file (R14): `%APPDATA%\saw-auto-clicker\config.toml` by
//! default, loaded at startup and written whenever the settings window
//! applies a change.

use saw_core::{CPS_RANGE, Settings};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::form::{self, Draft};
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

/// The file as read. Each field is any TOML value, checked on its own, so
/// that one bad or missing value falls back to its own default without
/// discarding the others.
#[derive(Deserialize)]
struct FileIn {
    cps: Option<toml::Value>,
    activation_delay_ms: Option<toml::Value>,
    jitter_percent: Option<toml::Value>,
    hotkey: Option<toml::Value>,
}

/// The file as written.
#[derive(Serialize)]
struct FileOut {
    cps: u32,
    activation_delay_ms: u32,
    jitter_percent: u32,
    hotkey: String,
}

/// What [`load`] read.
#[derive(Debug, PartialEq)]
pub struct Loaded {
    pub settings: Settings,
    /// Why some or all of the file was replaced by defaults, for the settings
    /// window to show. `None` when the file was read in full, or is missing.
    pub problem: Option<String>,
}

/// Reads the settings, using Q1–Q3's default for each value that is missing
/// or invalid, and for all of them when the file is missing or isn't TOML.
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
    match toml::from_str::<FileIn>(&text) {
        Ok(file) => from_file(file),
        Err(e) => defaults_because(format!("{} is not valid TOML: {e}", path.display())),
    }
}

fn defaults_because(problem: String) -> Loaded {
    Loaded {
        settings: Settings::default(),
        problem: Some(format!("{problem}; using the defaults")),
    }
}

fn from_file(file: FileIn) -> Loaded {
    let defaults = Draft::from(&Settings::default());
    let mut problems = Vec::new();
    let mut whole = |name, value: Option<toml::Value>, range, default| {
        pick(
            &mut problems,
            name,
            value.map(|v| whole_in(&v, range)),
            default,
        )
    };
    let cps = whole("cps", file.cps, CPS_RANGE, defaults.cps);
    let delay_ms = whole(
        "activation_delay_ms",
        file.activation_delay_ms,
        form::delay_ms_range(),
        defaults.delay_ms,
    );
    let jitter_percent = whole(
        "jitter_percent",
        file.jitter_percent,
        form::jitter_percent_range(),
        defaults.jitter_percent,
    );
    let hotkey = pick(
        &mut problems,
        "hotkey",
        file.hotkey.map(|v| {
            v.as_str()
                .ok_or_else(|| format!("must be text such as \"Ctrl+Alt+F8\", got {v}"))
                .and_then(hotkey_text::parse)
                .and_then(hotkey_text::usable)
        }),
        defaults.hotkey,
    );
    let draft = Draft {
        cps,
        delay_ms,
        jitter_percent,
        hotkey,
    };
    let settings = draft
        .to_settings()
        .expect("each value was checked against its range");
    let problem = (!problems.is_empty()).then(|| {
        format!(
            "in the settings file, {}; using the default for each",
            problems.join(", ")
        )
    });
    Loaded { settings, problem }
}

/// A whole number within `range`.
fn whole_in(value: &toml::Value, range: std::ops::RangeInclusive<u32>) -> Result<u32, String> {
    let n = value
        .as_integer()
        .ok_or_else(|| format!("must be a whole number, got {value}"))?;
    u32::try_from(n)
        .ok()
        .filter(|n| range.contains(n))
        .ok_or_else(|| format!("must be {}–{}, got {n}", range.start(), range.end()))
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

/// Writes the settings, creating the directory if needed. The file is
/// replaced in one step, so a failed write leaves the previous one intact.
pub fn save(path: &Path, settings: &Settings) -> io::Result<()> {
    let draft = Draft::from(settings);
    let file = FileOut {
        cps: draft.cps,
        activation_delay_ms: draft.delay_ms,
        jitter_percent: draft.jitter_percent,
        hotkey: HotkeyText(draft.hotkey).to_string(),
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
    use std::time::Duration;

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
        assert!(loaded.problem.unwrap().contains("not valid TOML"));
    }

    #[test]
    fn a_value_of_the_wrong_type_falls_back_alone() {
        let dir = TempDir::new("wrong-type");
        write(
            &dir.file(),
            "cps = \"fast\"\nactivation_delay_ms = 600\njitter_percent = 12.5\nhotkey = 8\n",
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
    }

    #[test]
    fn a_negative_value_falls_back_alone() {
        let dir = TempDir::new("negative");
        write(&dir.file(), "cps = -1\nactivation_delay_ms = 600\n");
        let loaded = load(&dir.file());
        assert_eq!(loaded.settings.cps(), Settings::default().cps());
        assert_eq!(
            loaded.settings.activation_delay(),
            Duration::from_millis(600)
        );
        assert!(loaded.problem.unwrap().contains("cps must be 1–50, got -1"));
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
