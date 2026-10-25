//! The settings file (R14): `%APPDATA%\saw-auto-clicker\config.toml` by
//! default, loaded at startup and written on the first run and whenever the
//! settings window applies a change.

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
    arm_on_startup: Option<toml::Value>,
}

/// The file as written.
#[derive(Serialize)]
struct FileOut {
    cps: u32,
    activation_delay_ms: u32,
    jitter_percent: u32,
    /// Empty for no hotkey.
    hotkey: String,
    arm_on_startup: bool,
}

/// What [`load`] read.
#[derive(Debug, PartialEq)]
pub struct Loaded {
    pub settings: Settings,
    /// Whether the app arms when it starts. Off when the file doesn't say.
    pub arm_on_startup: bool,
    /// Whether there was no file, as on the first run after the first
    /// install. An upgrade or reinstall keeps the file.
    pub first_run: bool,
    /// Why some or all of the file was replaced by defaults, for the settings
    /// window to show. `None` when the file was read in full, or is missing.
    pub problem: Option<String>,
}

impl Loaded {
    /// Every default, for a file that exists but could not be used or for
    /// no file path at all, saying why.
    pub fn defaults(problem: String) -> Loaded {
        Loaded {
            settings: Settings::default(),
            arm_on_startup: false,
            first_run: false,
            problem: Some(problem),
        }
    }
}

/// Reads the settings and whether to arm on startup, using the default for
/// each value that is missing or invalid (Q1–Q3's for the settings, off for
/// arming), and for all of them when the file is missing or isn't TOML.
pub fn load(path: &Path) -> Loaded {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Loaded {
                settings: Settings::default(),
                arm_on_startup: false,
                first_run: true,
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
    Loaded::defaults(format!("{problem}; using the defaults"))
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
            let text = v.as_str().ok_or_else(|| {
                format!("must be text such as \"Ctrl+Alt+F8\", or \"\" for none, got {v}")
            })?;
            if text.trim().is_empty() {
                return Ok(None);
            }
            hotkey_text::parse(text)
                .and_then(hotkey_text::usable)
                .map(Some)
        }),
        defaults.hotkey,
    );
    let arm_on_startup = pick(
        &mut problems,
        "arm_on_startup",
        file.arm_on_startup.map(|v| {
            v.as_bool()
                .ok_or_else(|| format!("must be true or false, got {v}"))
        }),
        false,
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
    Loaded {
        settings,
        arm_on_startup,
        first_run: false,
        problem,
    }
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

/// Writes the settings and whether to arm on startup, creating the directory
/// if needed. The file is replaced in one step, so a failed write leaves the
/// previous one intact.
pub fn save(path: &Path, settings: &Settings, arm_on_startup: bool) -> io::Result<()> {
    let draft = Draft::from(settings);
    let file = FileOut {
        cps: draft.cps,
        activation_delay_ms: draft.delay_ms,
        jitter_percent: draft.jitter_percent,
        hotkey: draft
            .hotkey
            .map_or_else(String::new, |h| HotkeyText(h).to_string()),
        arm_on_startup,
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
        Settings::new(37, Duration::from_millis(600), 0.3, Some(hotkey)).unwrap()
    }

    #[test]
    fn a_missing_file_is_a_first_run_with_the_defaults_quietly() {
        let dir = TempDir::new("missing");
        assert_eq!(
            load(&dir.file()),
            Loaded {
                settings: Settings::default(),
                arm_on_startup: false,
                first_run: true,
                problem: None
            }
        );
    }

    #[test]
    fn saved_settings_load_back_and_the_directory_is_created() {
        let dir = TempDir::new("round-trip");
        save(&dir.file(), &custom(), true).unwrap();
        assert_eq!(
            load(&dir.file()),
            Loaded {
                settings: custom(),
                arm_on_startup: true,
                first_run: false,
                problem: None
            }
        );
        save(&dir.file(), &Settings::default(), false).unwrap();
        assert_eq!(
            load(&dir.file()),
            Loaded {
                settings: Settings::default(),
                arm_on_startup: false,
                first_run: false,
                problem: None
            }
        );
    }

    #[test]
    fn no_hotkey_is_written_as_empty_text() {
        let dir = TempDir::new("no-hotkey");
        save(&dir.file(), &Settings::default(), false).unwrap();
        let text = fs::read_to_string(dir.file()).unwrap();
        assert!(text.contains("hotkey = \"\""), "{text}");
        write(&dir.file(), "hotkey = \"  \"\n");
        let loaded = load(&dir.file());
        assert_eq!(loaded.settings.hotkey(), None);
        assert_eq!(loaded.problem, None);
    }

    #[test]
    fn a_missing_arm_on_startup_value_means_off() {
        let dir = TempDir::new("arm-missing");
        write(&dir.file(), "cps = 20\n");
        let loaded = load(&dir.file());
        assert!(!loaded.arm_on_startup);
        assert!(!loaded.first_run);
        assert_eq!(loaded.problem, None);
    }

    #[test]
    fn the_armed_state_an_older_version_saved_is_ignored() {
        let dir = TempDir::new("old-armed");
        write(&dir.file(), "hotkey = \"Ctrl+Alt+F8\"\narmed = true\n");
        let loaded = load(&dir.file());
        assert!(!loaded.arm_on_startup);
        assert!(loaded.settings.hotkey().is_some());
        assert_eq!(loaded.problem, None);
    }

    #[test]
    fn an_arm_on_startup_value_that_is_not_a_boolean_means_off_with_a_reason() {
        let dir = TempDir::new("arm-wrong-type");
        write(&dir.file(), "cps = 20\narm_on_startup = \"yes\"\n");
        let loaded = load(&dir.file());
        assert!(!loaded.arm_on_startup);
        assert_eq!(loaded.settings.cps(), 20);
        let problem = loaded.problem.unwrap();
        assert!(
            problem.contains("arm_on_startup must be true or false"),
            "{problem:?}"
        );
    }

    #[test]
    fn a_file_that_is_not_toml_is_not_a_first_run() {
        let dir = TempDir::new("garbage-not-first");
        write(&dir.file(), "[[[");
        assert!(!load(&dir.file()).first_run);
    }

    #[test]
    fn the_file_is_readable_toml() {
        let dir = TempDir::new("format");
        save(&dir.file(), &custom(), true).unwrap();
        let text = fs::read_to_string(dir.file()).unwrap();
        for line in [
            "cps = 37",
            "activation_delay_ms = 600",
            "jitter_percent = 30",
            "hotkey = \"Shift+Win+K\"",
            "arm_on_startup = true",
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
                arm_on_startup: false,
                first_run: false,
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
