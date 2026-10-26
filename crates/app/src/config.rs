//! The settings file (R14): `%APPDATA%\saw-auto-clicker\config.toml` by
//! default, loaded at startup and written on the first run and whenever the
//! settings window applies a change.

use saw_core::{CPS_RANGE, ClickButtons, Settings};
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
    jitter: Option<toml::Value>,
    /// What versions before 0.5.0 wrote for Jitter, read when `jitter` is
    /// absent: any percentage above zero means on.
    jitter_percent: Option<toml::Value>,
    buttons: Option<toml::Value>,
    hotkey: Option<toml::Value>,
    enable_on_startup: Option<toml::Value>,
    /// What versions before 0.5.0 wrote for `enable_on_startup`, read when it
    /// is absent.
    arm_on_startup: Option<toml::Value>,
}

/// The file as written.
#[derive(Serialize)]
struct FileOut {
    cps: u32,
    activation_delay_ms: u32,
    jitter: bool,
    buttons: &'static str,
    /// Empty for no hotkey.
    hotkey: String,
    enable_on_startup: bool,
}

const BUTTON_NAMES: [(ClickButtons, &str); 3] = [
    (ClickButtons::Left, "left"),
    (ClickButtons::Both, "both"),
    (ClickButtons::Right, "right"),
];

/// What [`load`] read.
#[derive(Debug, PartialEq)]
pub struct Loaded {
    pub settings: Settings,
    /// Whether the app enables when it starts. Off when the file doesn't say.
    pub enable_on_startup: bool,
    /// Whether there was no file, as on the first run after the first
    /// install. An upgrade or reinstall keeps the file.
    pub first_run: bool,
    /// Why some or all of the file was replaced by defaults, for the settings
    /// window to show. `None` when the file was read in full, or is missing.
    pub problem: Option<String>,
}

impl Loaded {
    /// Every default, with why they replaced the file if they did.
    pub fn defaults(problem: Option<String>) -> Loaded {
        Loaded {
            settings: Settings::default(),
            enable_on_startup: false,
            first_run: false,
            problem,
        }
    }
}

/// Reads the settings and whether to enable on startup, using the default for
/// each value that is missing or invalid (Q1–Q3's for the settings, off for
/// enabling), and for all of them when the file is missing or isn't TOML.
pub fn load(path: &Path) -> Loaded {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Loaded {
                first_run: true,
                ..Loaded::defaults(None)
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
    Loaded::defaults(Some(format!("{problem}; using the defaults")))
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
    let jitter = match (file.jitter, file.jitter_percent) {
        (None, Some(percent)) => pick(
            &mut problems,
            "jitter_percent",
            Some(whole_in(&percent, 0..=100).map(|p| p > 0)),
            defaults.jitter,
        ),
        (jitter, _) => pick(
            &mut problems,
            "jitter",
            jitter.map(|v| boolean(&v)),
            defaults.jitter,
        ),
    };
    let buttons = pick(
        &mut problems,
        "buttons",
        file.buttons.map(|v| {
            BUTTON_NAMES
                .iter()
                .find(|(_, name)| v.as_str() == Some(name))
                .map(|&(buttons, _)| buttons)
                .ok_or_else(|| format!("must be \"left\", \"both\" or \"right\", got {v}"))
        }),
        defaults.buttons,
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
    let (name, value) = match (file.enable_on_startup, file.arm_on_startup) {
        (None, Some(old)) => ("arm_on_startup", Some(old)),
        (value, _) => ("enable_on_startup", value),
    };
    let enable_on_startup = pick(&mut problems, name, value.map(|v| boolean(&v)), false);
    let draft = Draft {
        cps,
        delay_ms,
        jitter,
        buttons,
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
        enable_on_startup,
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

fn boolean(value: &toml::Value) -> Result<bool, String> {
    value
        .as_bool()
        .ok_or_else(|| format!("must be true or false, got {value}"))
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

/// Writes the settings and whether to enable on startup, creating the directory
/// if needed. The file is replaced in one step, so a failed write leaves the
/// previous one intact.
pub fn save(path: &Path, settings: &Settings, enable_on_startup: bool) -> io::Result<()> {
    let draft = Draft::from(settings);
    let file = FileOut {
        cps: draft.cps,
        activation_delay_ms: draft.delay_ms,
        jitter: draft.jitter,
        buttons: BUTTON_NAMES
            .iter()
            .find(|&&(buttons, _)| buttons == draft.buttons)
            .map(|&(_, name)| name)
            .expect("every choice has a name"),
        hotkey: draft
            .hotkey
            .map_or_else(String::new, |h| HotkeyText(h).to_string()),
        enable_on_startup,
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

    use saw_core::{ClickButtons, Hotkey, Key, Modifiers};

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
        Settings::new(
            37,
            Duration::from_millis(600),
            true,
            ClickButtons::Right,
            Some(hotkey),
        )
        .unwrap()
    }

    #[test]
    fn a_missing_file_is_a_first_run_with_the_defaults_quietly() {
        let dir = TempDir::new("missing");
        assert_eq!(
            load(&dir.file()),
            Loaded {
                settings: Settings::default(),
                enable_on_startup: false,
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
                enable_on_startup: true,
                first_run: false,
                problem: None
            }
        );
        save(&dir.file(), &Settings::default(), false).unwrap();
        assert_eq!(
            load(&dir.file()),
            Loaded {
                settings: Settings::default(),
                enable_on_startup: false,
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
    fn a_missing_enable_on_startup_value_means_off() {
        let dir = TempDir::new("enable-missing");
        write(&dir.file(), "cps = 20\n");
        let loaded = load(&dir.file());
        assert!(!loaded.enable_on_startup);
        assert!(!loaded.first_run);
        assert_eq!(loaded.problem, None);
    }

    #[test]
    fn the_on_off_state_an_older_version_saved_is_ignored() {
        let dir = TempDir::new("old-armed");
        write(&dir.file(), "hotkey = \"Ctrl+Alt+F8\"\narmed = true\n");
        let loaded = load(&dir.file());
        assert!(!loaded.enable_on_startup);
        assert!(loaded.settings.hotkey().is_some());
        assert_eq!(loaded.problem, None);
    }

    #[test]
    fn an_enable_on_startup_value_that_is_not_a_boolean_means_off_with_a_reason() {
        let dir = TempDir::new("enable-wrong-type");
        write(&dir.file(), "cps = 20\nenable_on_startup = \"yes\"\n");
        let loaded = load(&dir.file());
        assert!(!loaded.enable_on_startup);
        assert_eq!(loaded.settings.cps(), 20);
        let problem = loaded.problem.unwrap();
        assert!(
            problem.contains("enable_on_startup must be true or false"),
            "{problem:?}"
        );
    }

    #[test]
    fn values_older_versions_wrote_are_read_when_the_new_ones_are_absent() {
        let dir = TempDir::new("legacy");
        write(&dir.file(), "jitter_percent = 30\narm_on_startup = true\n");
        let loaded = load(&dir.file());
        assert!(loaded.settings.jitter());
        assert!(loaded.enable_on_startup);
        assert_eq!(loaded.problem, None);

        write(&dir.file(), "jitter_percent = 0\n");
        assert!(!load(&dir.file()).settings.jitter());

        write(
            &dir.file(),
            "jitter = false\njitter_percent = 30\nenable_on_startup = false\narm_on_startup = true\n",
        );
        let loaded = load(&dir.file());
        assert!(!loaded.settings.jitter());
        assert!(!loaded.enable_on_startup);
    }

    #[test]
    fn a_bad_old_value_is_named_as_written() {
        let dir = TempDir::new("legacy-bad");
        write(
            &dir.file(),
            "jitter_percent = \"lots\"\narm_on_startup = 1\n",
        );
        let problem = load(&dir.file()).problem.unwrap();
        for name in ["jitter_percent", "arm_on_startup"] {
            assert!(problem.contains(name), "{name} not named in {problem:?}");
        }
    }

    #[test]
    fn each_button_choice_round_trips() {
        let dir = TempDir::new("buttons");
        for (buttons, name) in [
            (ClickButtons::Left, "left"),
            (ClickButtons::Both, "both"),
            (ClickButtons::Right, "right"),
        ] {
            let d = Settings::default();
            let settings =
                Settings::new(d.cps(), d.activation_delay(), d.jitter(), buttons, None).unwrap();
            save(&dir.file(), &settings, false).unwrap();
            let text = fs::read_to_string(dir.file()).unwrap();
            assert!(text.contains(&format!("buttons = \"{name}\"")), "{text}");
            assert_eq!(load(&dir.file()).settings, settings);
        }
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
            "jitter = true",
            "buttons = \"right\"",
            "hotkey = \"Shift+Win+K\"",
            "enable_on_startup = true",
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
            "cps = \"fast\"\nactivation_delay_ms = 600\njitter = 1\nbuttons = 2\nhotkey = 8\n",
        );
        let loaded = load(&dir.file());
        let d = Settings::default();
        assert_eq!(
            loaded.settings,
            Settings::new(
                d.cps(),
                Duration::from_millis(600),
                d.jitter(),
                d.buttons(),
                d.hotkey()
            )
            .unwrap()
        );
        let problem = loaded.problem.unwrap();
        for name in ["cps", "jitter", "buttons", "hotkey"] {
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
            "cps = 99\nactivation_delay_ms = 600\nbuttons = \"middle\"\nhotkey = \"Ctrl+Nope\"\n",
        );
        let loaded = load(&dir.file());
        let d = Settings::default();
        assert_eq!(
            loaded.settings,
            Settings::new(
                d.cps(),
                Duration::from_millis(600),
                d.jitter(),
                d.buttons(),
                d.hotkey()
            )
            .unwrap()
        );
        let problem = loaded.problem.unwrap();
        for name in ["cps", "buttons", "hotkey"] {
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
                settings: Settings::new(20, d.activation_delay(), d.jitter(), d.buttons(), None)
                    .unwrap(),
                enable_on_startup: false,
                first_run: false,
                problem: None
            }
        );
    }

    #[test]
    fn range_ends_are_accepted() {
        let dir = TempDir::new("ends");
        write(&dir.file(), "cps = 50\nactivation_delay_ms = 50\n");
        let loaded = load(&dir.file());
        assert_eq!(loaded.problem, None);
        assert_eq!(
            (loaded.settings.cps(), loaded.settings.activation_delay()),
            (50, Duration::from_millis(50))
        );
    }
}
