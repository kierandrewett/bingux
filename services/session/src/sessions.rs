//! Desktop sessions from `wayland-sessions` directories on `XDG_DATA_DIRS`.

use serde::Serialize;
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

const DEFAULT_DATA_DIRS: &str = "/usr/local/share:/usr/share";
const DEFAULT_PATH: &str = "/usr/local/bin:/usr/bin:/bin";
pub const PREFERRED_SESSION: &str = "gnoblin";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub comment: String,
    /// The `Exec=` line split with shell rules. greetd joins it again for `sh -c`.
    pub command: Vec<String>,
    /// `DesktopNames=` without the trailing separator, for `XDG_CURRENT_DESKTOP`.
    #[serde(rename = "desktopNames")]
    pub desktop_names: String,
}

#[derive(Debug, Serialize)]
pub struct Catalog {
    pub sessions: Vec<Session>,
}

impl Catalog {
    pub fn read() -> Self {
        let data_dirs = env::var("XDG_DATA_DIRS").unwrap_or_else(|_| DEFAULT_DATA_DIRS.to_string());
        let mut seen: Vec<PathBuf> = Vec::new();
        for directory in data_dirs
            .split(':')
            .filter(|directory| !directory.is_empty())
        {
            let path = Path::new(directory).join("wayland-sessions");
            if !seen.contains(&path) {
                seen.push(path);
            }
        }
        Self {
            sessions: collect(&seen, executable_available),
        }
    }
}

pub fn collect(directories: &[PathBuf], available: fn(&str) -> bool) -> Vec<Session> {
    let mut entries: BTreeMap<String, Session> = BTreeMap::new();
    for directory in directories {
        let Ok(listing) = fs::read_dir(directory) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = listing
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "desktop")
            })
            .collect();
        paths.sort();
        for path in paths {
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            // The first directory on XDG_DATA_DIRS wins, as for desktop files.
            if entries.contains_key(id) {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            if let Some(session) = parse(id, &text, available) {
                entries.insert(id.to_string(), session);
            }
        }
    }
    let mut sessions: Vec<Session> = entries.into_values().collect();
    sessions.sort_by_key(|session| (session.id != PREFERRED_SESSION, session.name.to_lowercase()));
    sessions
}

pub fn parse(id: &str, text: &str, available: fn(&str) -> bool) -> Option<Session> {
    let entry = desktop_entry_group(text);
    let is_true = |key: &str| {
        entry
            .get(key)
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
    };
    if is_true("Hidden") || is_true("NoDisplay") {
        return None;
    }
    let command = shlex::split(entry.get("Exec").map(String::as_str).unwrap_or("").trim())?;
    let executable = command.first()?;
    if !available(executable) {
        return None;
    }
    if let Some(try_exec) = entry
        .get("TryExec")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        if !available(try_exec) {
            return None;
        }
    }
    Some(Session {
        id: id.to_string(),
        name: entry.get("Name").cloned().unwrap_or_else(|| id.to_string()),
        comment: entry.get("Comment").cloned().unwrap_or_default(),
        command,
        desktop_names: entry
            .get("DesktopNames")
            .map(|value| value.trim().trim_end_matches(';').to_string())
            .unwrap_or_default(),
    })
}

/// Keys of the `[Desktop Entry]` group. Localised keys (`Name[en_GB]`) are
/// ignored; the greeter shows the unlocalised name.
fn desktop_entry_group(text: &str) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    let mut in_group = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_group = line == "[Desktop Entry]";
            continue;
        }
        if !in_group {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            if !key.contains('[') {
                values.insert(key.to_string(), value.trim().to_string());
            }
        }
    }
    values
}

fn executable_available(name: &str) -> bool {
    if name.contains('/') {
        return is_executable(Path::new(name));
    }
    let path = env::var("PATH").unwrap_or_else(|_| DEFAULT_PATH.to_string());
    path.split(':')
        .filter(|directory| !directory.is_empty())
        .any(|directory| is_executable(&Path::new(directory).join(name)))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn always(_: &str) -> bool {
        true
    }

    fn never(_: &str) -> bool {
        false
    }

    #[test]
    fn parses_exec_desktop_names_and_skips_hidden_entries() {
        let text = "[Desktop Entry]\nName=Gnoblin\nName[de]=Gnoblin DE\nComment=Standalone\nExec=/usr/bin/gnoblin --xwayland\nDesktopNames=Gnoblin;\n";
        let session = parse("gnoblin", text, always).unwrap();
        assert_eq!(session.name, "Gnoblin");
        assert_eq!(session.command, vec!["/usr/bin/gnoblin", "--xwayland"]);
        assert_eq!(session.desktop_names, "Gnoblin");
        assert!(parse("x", "[Desktop Entry]\nExec=foo\nHidden=true\n", always).is_none());
        assert!(parse("x", "[Desktop Entry]\nExec=foo\n", never).is_none());
        assert!(parse("x", "[Desktop Entry]\nName=Broken\n", always).is_none());
        assert!(parse("x", "[Other]\nExec=foo\n", always).is_none());
    }

    #[test]
    fn first_data_dir_wins_and_gnoblin_sorts_first() {
        let root = env::temp_dir().join(format!("bingux-session-sessions-{}", std::process::id()));
        let first = root.join("first/wayland-sessions");
        let second = root.join("second/wayland-sessions");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        fs::write(
            first.join("sway.desktop"),
            "[Desktop Entry]\nName=Sway first\nExec=sway\n",
        )
        .unwrap();
        fs::write(
            second.join("sway.desktop"),
            "[Desktop Entry]\nName=Sway second\nExec=sway\n",
        )
        .unwrap();
        fs::write(
            second.join("gnoblin.desktop"),
            "[Desktop Entry]\nName=Gnoblin\nExec=gnoblin\n",
        )
        .unwrap();
        fs::write(
            second.join("aaa.desktop"),
            "[Desktop Entry]\nName=Aaa\nExec=aaa\n",
        )
        .unwrap();
        let sessions = collect(&[first, second], always);
        let ids: Vec<&str> = sessions.iter().map(|session| session.id.as_str()).collect();
        assert_eq!(ids, vec!["gnoblin", "aaa", "sway"]);
        assert_eq!(sessions[2].name, "Sway first");
        fs::remove_dir_all(root).unwrap();
    }
}
