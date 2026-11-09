//! What the calculator keeps between runs: its history and settings under
//! `$XDG_STATE_HOME`, and the exchange rates under `$XDG_CACHE_HOME`.
//!
//! Files are written whole to a temporary name and renamed, so a crash never
//! leaves half a file, and a file that cannot be read is moved aside rather
//! than lost.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::eval::Angle;
use crate::rates::Rates;

/// The most history entries kept.
pub const MAX_HISTORY: usize = 500;

const FORMAT: u32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Basic,
    Scientific,
    Programmer,
    Convert,
}

impl Mode {
    pub const ALL: [Mode; 4] = [
        Mode::Basic,
        Mode::Scientific,
        Mode::Programmer,
        Mode::Convert,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Mode::Basic => "Basic",
            Mode::Scientific => "Scientific",
            Mode::Programmer => "Programmer",
            Mode::Convert => "Convert",
        }
    }

    /// Modes whose history is shared: Basic and Scientific read the same
    /// expressions.
    pub fn family(self) -> u8 {
        match self {
            Mode::Basic | Mode::Scientific => 0,
            Mode::Programmer => 1,
            Mode::Convert => 2,
        }
    }
}

/// One finished calculation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// What was typed, as the entry field held it.
    pub expr: String,
    /// The result as plain text: it reads back as a number.
    pub result: String,
    pub mode: Mode,
    /// Unix time.
    pub at: u64,
    /// A programmer entry's base (2, 8, 10 or 16), so the result is shown as
    /// it was; 0 for the others.
    #[serde(default)]
    pub base: u32,
    /// A programmer entry's word size, 0 for the others.
    #[serde(default)]
    pub bits: u32,
    #[serde(default)]
    pub signed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub mode: Mode,
    pub angle: Angle,
    /// Significant digits shown for a value that does not end.
    pub precision: usize,
    /// Separate the digits into groups.
    pub group: bool,
    /// The memory register as plain text.
    pub memory: String,
    pub base: u32,
    pub bits: u32,
    pub signed: bool,
    pub category: String,
    pub from: String,
    pub to: String,
    /// The window's history panel is open.
    pub history_open: bool,
    /// The window's width and height in logical pixels when it was last
    /// resized, so the next launch opens at the same size.
    pub window: Option<(u32, u32)>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            mode: Mode::Basic,
            angle: Angle::Degrees,
            precision: 16,
            group: true,
            memory: "0".to_string(),
            base: 10,
            bits: 64,
            signed: true,
            category: "length".to_string(),
            from: "km".to_string(),
            to: "mi".to_string(),
            history_open: true,
            window: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    pub format: u32,
    pub history: Vec<Entry>,
    pub settings: Settings,
}

impl Saved {
    /// Adds a finished calculation, newest last. The same calculation twice
    /// in a row is kept once, and the oldest entries drop off at the limit.
    pub fn push(&mut self, entry: Entry) {
        if self.history.last().is_some_and(|last| {
            last.expr == entry.expr && last.result == entry.result && last.mode == entry.mode
        }) {
            return;
        }
        self.history.push(entry);
        if self.history.len() > MAX_HISTORY {
            let extra = self.history.len() - MAX_HISTORY;
            self.history.drain(..extra);
        }
    }
}

fn base_dir(variable: &str, fallback: &[&str]) -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(variable).filter(|v| !v.is_empty()) {
        let dir = PathBuf::from(dir);
        if dir.is_absolute() {
            return Some(dir);
        }
    }
    let mut home = PathBuf::from(std::env::var_os("HOME").filter(|v| !v.is_empty())?);
    home.extend(fallback);
    Some(home)
}

/// `$XDG_STATE_HOME/bingux-calculator/state.json`.
pub fn state_path() -> Option<PathBuf> {
    Some(base_dir("XDG_STATE_HOME", &[".local", "state"])?.join("bingux-calculator/state.json"))
}

/// `$XDG_CACHE_HOME/bingux-calculator/rates.json`.
pub fn rates_path() -> Option<PathBuf> {
    Some(base_dir("XDG_CACHE_HOME", &[".cache"])?.join("bingux-calculator/rates.json"))
}

/// Reads the saved state; a missing or unreadable file gives the defaults,
/// and an unreadable one is kept as `state.json.broken`.
pub fn load_saved(path: &Path) -> Saved {
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<Saved>(&text) {
            Ok(saved) => saved,
            Err(_) => {
                let _ = std::fs::rename(path, path.with_extension("json.broken"));
                Saved::default()
            }
        },
        Err(_) => Saved::default(),
    }
}

pub fn save_saved(path: &Path, saved: &Saved) -> std::io::Result<()> {
    let mut saved = saved.clone();
    saved.format = FORMAT;
    write_json(path, &saved)
}

/// The rates last downloaded, if the cache holds a readable set.
pub fn load_rates(path: &Path) -> Option<Rates> {
    let rates: Rates = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    (!rates.per_euro.is_empty()).then_some(rates)
}

pub fn save_rates(path: &Path, rates: &Rates) -> std::io::Result<()> {
    write_json(path, rates)
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
    let temporary = path.with_extension("json.tmp");
    {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(&text)?;
        file.sync_all()?;
    }
    std::fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(expr: &str, result: &str) -> Entry {
        Entry {
            expr: expr.into(),
            result: result.into(),
            mode: Mode::Basic,
            at: 1,
            base: 0,
            bits: 0,
            signed: false,
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("bingux-calc-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn history_survives_a_restart() {
        let dir = temp("round");
        let path = dir.join("state.json");
        let mut saved = Saved::default();
        saved.push(entry("1+1", "2"));
        saved.push(entry("2*3", "6"));
        saved.settings.mode = Mode::Scientific;
        saved.settings.angle = Angle::Radians;
        save_saved(&path, &saved).unwrap();
        let back = load_saved(&path);
        assert_eq!(back.history, saved.history);
        assert_eq!(back.settings.mode, Mode::Scientific);
        assert_eq!(back.settings.angle, Angle::Radians);
        assert_eq!(back.format, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_gives_the_defaults() {
        let dir = temp("missing");
        assert_eq!(load_saved(&dir.join("none.json")), Saved::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_broken_file_is_moved_aside() {
        let dir = temp("broken");
        let path = dir.join("state.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(load_saved(&path), Saved::default());
        assert!(!path.exists());
        assert!(dir.join("state.json.broken").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_older_file_with_fewer_fields_still_loads() {
        let dir = temp("older");
        let path = dir.join("state.json");
        std::fs::write(
            &path,
            r#"{"history":[{"expr":"2+2","result":"4","mode":"Basic","at":5}]}"#,
        )
        .unwrap();
        let loaded = load_saved(&path);
        assert_eq!(loaded.history.len(), 1);
        assert_eq!(loaded.settings, Settings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_window_size_is_kept_and_a_file_without_it_means_the_default_size() {
        let dir = temp("window");
        let path = dir.join("state.json");
        let mut saved = Saved::default();
        assert_eq!(saved.settings.window, None);
        saved.settings.window = Some((640, 480));
        save_saved(&path, &saved).unwrap();
        assert_eq!(load_saved(&path).settings.window, Some((640, 480)));
        std::fs::write(&path, r#"{"settings":{"mode":"Convert"}}"#).unwrap();
        let older = load_saved(&path);
        assert_eq!(older.settings.mode, Mode::Convert);
        assert_eq!(older.settings.window, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_save_leaves_no_temporary_file_and_replaces_the_old_one_whole() {
        let dir = temp("atomic");
        let path = dir.join("state.json");
        let mut saved = Saved::default();
        saved.push(entry("1+1", "2"));
        save_saved(&path, &saved).unwrap();
        saved.push(entry("2+2", "4"));
        save_saved(&path, &saved).unwrap();
        assert_eq!(load_saved(&path).history.len(), 2);
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, ["state.json"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_same_calculation_twice_is_kept_once_and_the_list_is_capped() {
        let mut saved = Saved::default();
        saved.push(entry("1+1", "2"));
        saved.push(entry("1+1", "2"));
        assert_eq!(saved.history.len(), 1);
        for i in 0..(MAX_HISTORY + 20) {
            saved.push(entry(&format!("{i}+1"), &format!("{}", i + 1)));
        }
        assert_eq!(saved.history.len(), MAX_HISTORY);
        assert_eq!(
            saved.history.last().unwrap().expr,
            format!("{}+1", MAX_HISTORY + 19)
        );
    }

    #[test]
    fn rates_are_cached_and_read_back() {
        let dir = temp("rates");
        let path = dir.join("rates.json");
        let mut rates = Rates::bundled();
        rates.fetched = 12345;
        save_rates(&path, &rates).unwrap();
        assert_eq!(load_rates(&path), Some(rates));
        assert!(load_rates(&dir.join("none.json")).is_none());
        std::fs::write(&path, "[]").unwrap();
        assert!(load_rates(&path).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_paths_follow_the_xdg_variables() {
        // The variables are process-wide, so this test is the only one to touch them.
        // SAFETY: no other test in this crate reads or writes the environment.
        unsafe {
            std::env::set_var("XDG_STATE_HOME", "/tmp/state-home");
            std::env::set_var("XDG_CACHE_HOME", "/tmp/cache-home");
        }
        assert_eq!(
            state_path().unwrap(),
            PathBuf::from("/tmp/state-home/bingux-calculator/state.json")
        );
        assert_eq!(
            rates_path().unwrap(),
            PathBuf::from("/tmp/cache-home/bingux-calculator/rates.json")
        );
    }
}
