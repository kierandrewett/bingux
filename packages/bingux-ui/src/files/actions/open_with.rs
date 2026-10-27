//! Open with: the installed applications that say they can open a file's
//! type, read from the desktop entries of the XDG data directories.
//!
//! Desktop entries are optional on a Bingux system: with none, the list is
//! empty and the menu says so. Nothing here needs a particular distribution.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use bingux_files::location::file_uri;

/// How many applications the menu lists for one type.
const MENU_LIMIT: usize = 12;

/// An application that can open files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::files) struct App {
    pub name: String,
    /// The `Exec` line, with its field codes.
    pub exec: String,
    pub mimes: Vec<String>,
}

static APPS: OnceLock<Vec<App>> = OnceLock::new();

/// Every application that has a desktop entry. Reads the disk the first time,
/// so call it from a worker; [`warm`] does.
fn apps() -> &'static [App] {
    APPS.get_or_init(scan)
}

/// Read the desktop entries on a thread now, so the first menu does not wait
/// for the disk.
pub(super) fn warm() {
    if APPS.get().is_none() {
        std::thread::spawn(|| {
            apps();
        });
    }
}

/// The applications that open `mime`, by name. Empty until [`warm`] has
/// finished, rather than blocking the menu.
pub(in crate::files) fn apps_for(mime: &str) -> Vec<App> {
    let Some(all) = APPS.get() else {
        return Vec::new();
    };
    let generic = mime.starts_with("text/").then_some("text/plain");
    let mut found: Vec<App> = all
        .iter()
        .filter(|app| {
            app.mimes
                .iter()
                .any(|candidate| candidate == mime || Some(candidate.as_str()) == generic)
        })
        .cloned()
        .collect();
    found.sort_by_key(|app| app.name.to_lowercase());
    found.dedup_by(|a, b| a.name == b.name);
    found.truncate(MENU_LIMIT);
    found
}

fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    match std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        Some(home) => dirs.push(PathBuf::from(home)),
        None => {
            if let Some(home) = std::env::var_os("HOME") {
                dirs.push(Path::new(&home).join(".local/share"));
            }
        }
    }
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
    dirs.extend(system.split(':').filter(|dir| !dir.is_empty()).map(PathBuf::from));
    dirs.into_iter().map(|dir| dir.join("applications")).collect()
}

fn scan() -> Vec<App> {
    let mut found: Vec<App> = Vec::new();
    for dir in data_dirs() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "desktop") {
                continue;
            }
            if let Ok(text) = fs::read_to_string(&path)
                && let Some(app) = parse_desktop_entry(&text)
            {
                found.push(app);
            }
        }
    }
    found
}

/// The application a desktop entry describes, or `None` for one that is
/// hidden, is not an application, or opens no types.
fn parse_desktop_entry(text: &str) -> Option<App> {
    let mut in_entry = false;
    let (mut name, mut exec, mut mimes) = (None, None, Vec::new());
    let (mut hidden, mut application) = (false, true);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Name" => name = Some(value.trim().to_owned()),
            "Exec" => exec = Some(value.trim().to_owned()),
            "MimeType" => {
                mimes = value
                    .split(';')
                    .filter(|mime| !mime.is_empty())
                    .map(str::to_owned)
                    .collect();
            }
            "NoDisplay" | "Hidden" => hidden |= value.trim() == "true",
            "Type" => application = value.trim() == "Application",
            _ => {}
        }
    }
    if hidden || !application || mimes.is_empty() {
        return None;
    }
    Some(App {
        name: name?,
        exec: exec?,
        mimes,
    })
}

/// The command line of `app` for `path`: the `Exec` line split into words,
/// the file field codes replaced and the rest dropped.
fn command_line(app: &App, path: &Path) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut chars = app.exec.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' if quoted => {
                if let Some(next) = chars.next() {
                    word.push(next);
                }
            }
            ' ' | '\t' if !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            other => {
                word.push(other);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    let file = path.to_string_lossy().into_owned();
    let uri = file_uri(path);
    let mut line = Vec::new();
    for word in words {
        match word.as_str() {
            "%f" | "%F" => line.push(file.clone()),
            "%u" | "%U" => line.push(uri.clone()),
            "%i" | "%c" | "%k" | "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => {}
            other => line.push(other.replace("%%", "%")),
        }
    }
    // An entry with no field code still gets the file, as most launchers do.
    if !app.exec.contains('%') {
        line.push(file);
    }
    line
}

/// Start `app` on `path`. It runs on its own: it is not a child of the file
/// view's work, and its output is dropped.
pub(in crate::files) fn launch(app: &App, path: &Path) -> io::Result<()> {
    let line = command_line(app, path);
    let Some((program, arguments)) = line.split_first() else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty command"));
    };
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    // Reap it when it ends, so it does not stay a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Open `path` in the application the system names for its type, as a
/// double click does in any file manager (`xdg-open`, which asks the MIME
/// database and the user's defaults). It runs on its own and its output is
/// dropped.
pub(in crate::files) fn open_default(path: &Path) -> io::Result<()> {
    let mut child = Command::new("xdg-open")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str = "[Desktop Entry]\nType=Application\nName=Image Viewer\n\
Exec=viewer --open %f\nMimeType=image/png;image/jpeg;\n\
[Desktop Action new]\nName=Other\nExec=nope\n";

    #[test]
    fn reads_a_desktop_entry() {
        let app = parse_desktop_entry(ENTRY).expect("an application");
        assert_eq!(app.name, "Image Viewer");
        assert_eq!(app.exec, "viewer --open %f");
        assert_eq!(app.mimes, ["image/png", "image/jpeg"]);
    }

    #[test]
    fn hidden_entries_and_entries_without_types_are_skipped() {
        // A key in another group does not hide the application.
        assert!(parse_desktop_entry(&format!("{ENTRY}NoDisplay=true")).is_some());
        let hidden = ENTRY.replace("Name=Image Viewer", "Name=X\nNoDisplay=true");
        assert!(parse_desktop_entry(&hidden).is_none());
        assert!(parse_desktop_entry("[Desktop Entry]\nName=A\nExec=a\n").is_none());
    }

    #[test]
    fn command_line_fills_field_codes() {
        let app = App {
            name: "A".into(),
            exec: r#"/opt/a b/viewer --title "My \"App\"" %i %U 100%%"#.into(),
            mimes: vec![],
        };
        let line = command_line(&app, Path::new("/tmp/x y.png"));
        assert_eq!(
            line,
            [
                "/opt/a",
                "b/viewer",
                "--title",
                "My \"App\"",
                "file:///tmp/x%20y.png",
                "100%",
            ]
        );
        let bare = App {
            name: "B".into(),
            exec: "tool".into(),
            mimes: vec![],
        };
        assert_eq!(command_line(&bare, Path::new("/t")), ["tool", "/t"]);
    }
}
