//! Installed applications, read from desktop entries.
//!
//! The Applications page, the per-application page and the Sound page's
//! application icons all start from the same list: every `.desktop` file in
//! `$XDG_DATA_HOME/applications` and `$XDG_DATA_DIRS/applications`, the
//! first one found for an id winning, as the shell's application catalogue
//! reads them. Entries marked `NoDisplay` or `Hidden`, entries that are not
//! applications and entries whose `TryExec` program is missing are left out.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::system;

/// One installed application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppEntry {
    /// The desktop file id without `.desktop`, for example `org.gnome.Calculator`.
    pub id: String,
    pub name: String,
    pub comment: String,
    /// The first `Categories` entry with a plain name, such as `Internet`;
    /// empty when none has one.
    pub category: String,
    /// An icon name or an absolute path, as the desktop entry gives it.
    pub icon: String,
    /// The program `Exec` runs, without arguments or field codes.
    pub program: String,
    pub mime_types: Vec<String>,
    pub desktop_file: PathBuf,
}

/// The values of one `[Desktop Entry]` group, keyed by the raw key
/// (`Name`, `Name[en_GB]`, ...).
fn read_entry(path: &Path) -> Option<HashMap<String, String>> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut values = HashMap::new();
    let mut in_entry = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            values
                .entry(key.trim().to_string())
                .or_insert_with(|| value.trim().to_string());
        }
    }
    Some(values)
}

/// `LANG` as the locale suffixes a desktop entry can use, most specific
/// first: `en_GB.UTF-8` gives `en_GB`, then `en`.
fn locale_suffixes() -> Vec<String> {
    let lang = std::env::var("LC_MESSAGES")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default();
    let lang = lang
        .split(['.', '@'])
        .next()
        .unwrap_or_default()
        .to_string();
    let mut suffixes = Vec::new();
    if !lang.is_empty() && lang != "C" && lang != "POSIX" {
        suffixes.push(lang.clone());
        if let Some((language, _)) = lang.split_once('_') {
            suffixes.push(language.to_string());
        }
    }
    suffixes
}

fn localised(values: &HashMap<String, String>, key: &str, suffixes: &[String]) -> String {
    suffixes
        .iter()
        .find_map(|suffix| values.get(&format!("{key}[{suffix}]")))
        .or_else(|| values.get(key))
        .cloned()
        .unwrap_or_default()
}

/// The program in an `Exec` line: the first word that is not an `env`
/// wrapper or a `NAME=value` assignment, reduced to its file name.
fn exec_program(exec: &str) -> String {
    let mut words = exec.split_whitespace().map(|word| word.trim_matches('"'));
    let mut program = "";
    for word in words.by_ref() {
        if word == "env" || word.contains('=') {
            continue;
        }
        program = word;
        break;
    }
    Path::new(program)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// The first `Categories` entry that has a plain name, or nothing.
fn plain_category(categories: &str) -> String {
    const NAMES: [(&str, &str); 14] = [
        ("WebBrowser", "Internet"),
        ("Network", "Internet"),
        ("Email", "Email"),
        ("Office", "Office"),
        ("Graphics", "Graphics"),
        ("Audio", "Music and audio"),
        ("Video", "Video"),
        ("AudioVideo", "Media"),
        ("Development", "Development"),
        ("Game", "Games"),
        ("Education", "Education"),
        ("Science", "Science"),
        ("System", "System"),
        ("Utility", "Utilities"),
    ];
    categories
        .split(';')
        .find_map(|category| {
            NAMES
                .iter()
                .find(|(key, _)| *key == category)
                .map(|(_, name)| name.to_string())
        })
        .unwrap_or_default()
}

fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    match std::env::var_os("XDG_DATA_HOME") {
        Some(dir) if !dir.is_empty() => dirs.push(PathBuf::from(dir)),
        _ => {
            if let Some(home) = std::env::var_os("HOME") {
                dirs.push(PathBuf::from(home).join(".local/share"));
            }
        }
    }
    let list = std::env::var("XDG_DATA_DIRS").unwrap_or_default();
    let list = if list.is_empty() {
        "/usr/local/share:/usr/share".to_string()
    } else {
        list
    };
    dirs.extend(
        list.split(':')
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from),
    );
    dirs
}

/// Every application desktop file under `directory`, as `(id, path)`. A file
/// in a subdirectory `a/b.desktop` has the id `a-b`.
fn desktop_files(directory: &Path, prefix: &str, files: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            desktop_files(&path, &format!("{prefix}{name}-"), files);
        } else if let Some(stem) = name.strip_suffix(".desktop") {
            files.push((format!("{prefix}{stem}"), path));
        }
    }
}

type Reused = Mutex<Option<(Instant, Vec<AppEntry>)>>;
static LIST: LazyLock<Reused> = LazyLock::new(Default::default);

/// Every visible application, sorted by name. The list is reused for 30
/// seconds, so the Sound page's five-second poll does not read every desktop
/// file each time; [`rescan`] reads them again.
pub fn installed() -> Vec<AppEntry> {
    if let Ok(list) = LIST.lock()
        && let Some((read_at, apps)) = list.as_ref()
        && read_at.elapsed() < Duration::from_secs(30)
    {
        return apps.clone();
    }
    rescan()
}

/// Read every desktop file now and replace the reused list.
pub fn rescan() -> Vec<AppEntry> {
    let apps = read_installed();
    if let Ok(mut list) = LIST.lock() {
        *list = Some((Instant::now(), apps.clone()));
    }
    apps
}

fn read_installed() -> Vec<AppEntry> {
    let suffixes = locale_suffixes();
    let mut seen: HashMap<String, ()> = HashMap::new();
    let mut apps = Vec::new();
    for directory in data_dirs() {
        let mut files = Vec::new();
        desktop_files(&directory.join("applications"), "", &mut files);
        files.sort();
        for (id, path) in files {
            if seen.insert(id.clone(), ()).is_some() {
                continue;
            }
            let Some(values) = read_entry(&path) else {
                continue;
            };
            let flag = |key: &str| values.get(key).is_some_and(|value| value == "true");
            if values.get("Type").map(String::as_str) != Some("Application")
                || flag("NoDisplay")
                || flag("Hidden")
            {
                continue;
            }
            if let Some(try_exec) = values.get("TryExec")
                && !try_exec.is_empty()
                && !(try_exec.starts_with('/') && Path::new(try_exec).exists()
                    || !try_exec.starts_with('/') && system::which(try_exec))
            {
                continue;
            }
            let name = localised(&values, "Name", &suffixes);
            if name.is_empty() {
                continue;
            }
            apps.push(AppEntry {
                id,
                name,
                comment: localised(&values, "Comment", &suffixes),
                category: plain_category(
                    values
                        .get("Categories")
                        .map(String::as_str)
                        .unwrap_or_default(),
                ),
                icon: values.get("Icon").cloned().unwrap_or_default(),
                program: exec_program(values.get("Exec").map(String::as_str).unwrap_or_default()),
                mime_types: values
                    .get("MimeType")
                    .map(|list| {
                        list.split(';')
                            .filter(|mime| !mime.is_empty())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
                desktop_file: path,
            });
        }
    }
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps
}

/// The application an audio stream belongs to. `app_id` is PipeWire's
/// `application.id`, `binary` its `application.process.binary` and `name`
/// its `application.name`. The first match in that order wins.
pub fn for_stream(apps: &[AppEntry], app_id: &str, binary: &str, name: &str) -> Option<AppEntry> {
    let lowered = name.to_lowercase();
    apps.iter()
        .find(|app| !app_id.is_empty() && app.id == app_id)
        .or_else(|| {
            apps.iter()
                .find(|app| !binary.is_empty() && app.program == binary)
        })
        .or_else(|| apps.iter().find(|app| app.name.to_lowercase() == lowered))
        .cloned()
}

/// Launch `app` from its desktop file with `gio launch`. The child runs on
/// its own, and a thread waits for it so it does not stay a zombie.
pub fn launch(app: &AppEntry) {
    let spawned = std::process::Command::new("gio")
        .arg("launch")
        .arg(&app.desktop_file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    if let Ok(mut child) = spawned {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}
