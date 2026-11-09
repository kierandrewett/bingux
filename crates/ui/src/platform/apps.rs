//! Installed applications, read the way the XDG specifications say, without
//! GIO or GTK: desktop entries, the default application for a type, and
//! starting one.
//!
//! * Desktop Entry Specification: `applications/*.desktop` in `$XDG_DATA_HOME`
//!   and `$XDG_DATA_DIRS`, `Exec` with its field codes, `TryExec`, `Terminal`.
//! * Association between MIME types and applications: `mimeapps.list` in the
//!   config and data directories, desktop-specific lists first, `[Default
//!   Applications]` before `[Added Associations]`, `[Removed Associations]`
//!   respected, then `mimeinfo.cache`, then the entries' own `MimeType`.
//!
//! Everything here reads files and blocks briefly. Call it off the UI thread.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// One application's desktop entry, reduced to what opening a file needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopEntry {
    /// The desktop file id: `bingux-files.desktop`, or `org.example.App.desktop`.
    pub id: String,
    /// Where the file was read, for `%k`.
    pub path: Option<PathBuf>,
    pub name: String,
    /// The `Exec` line, field codes unexpanded. `None` for an entry that is
    /// started over D-Bus only.
    pub exec: Option<String>,
    /// `Icon`: an icon name or an absolute path. Empty when there is none.
    pub icon: String,
    pub mime_types: Vec<String>,
    pub terminal: bool,
    /// `TryExec`: the program that must exist for the entry to count.
    pub try_exec: Option<String>,
    /// `NoDisplay`: not shown in menus, but still a valid default.
    pub no_display: bool,
    /// `Path`: the working directory.
    pub working_directory: Option<PathBuf>,
}

/// The directories the lookups read, so a test can give its own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dirs {
    pub config_home: PathBuf,
    pub config_dirs: Vec<PathBuf>,
    pub data_home: PathBuf,
    pub data_dirs: Vec<PathBuf>,
    /// `XDG_CURRENT_DESKTOP`, lower case, for `gnoblin-mimeapps.list`.
    pub desktops: Vec<String>,
}

impl Dirs {
    /// The directories the environment names, with the specification's
    /// defaults.
    pub fn from_env() -> Self {
        let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
        let home = var("HOME").map(PathBuf::from);
        let absolute = |value: Option<std::ffi::OsString>| {
            value.map(PathBuf::from).filter(|path| path.is_absolute())
        };
        let list = |name: &str, default: &str| -> Vec<PathBuf> {
            var(name)
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| default.to_owned())
                .split(':')
                .filter(|part| !part.is_empty())
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .collect()
        };
        Self {
            config_home: absolute(var("XDG_CONFIG_HOME"))
                .or_else(|| home.as_ref().map(|home| home.join(".config")))
                .unwrap_or_default(),
            config_dirs: list("XDG_CONFIG_DIRS", "/etc/xdg"),
            data_home: absolute(var("XDG_DATA_HOME"))
                .or_else(|| home.as_ref().map(|home| home.join(".local/share")))
                .unwrap_or_default(),
            data_dirs: list("XDG_DATA_DIRS", "/usr/local/share:/usr/share"),
            desktops: var("XDG_CURRENT_DESKTOP")
                .map(|value| {
                    value
                        .to_string_lossy()
                        .split(':')
                        .filter(|name| !name.is_empty())
                        .map(str::to_ascii_lowercase)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// `applications` directories, most specific first.
    fn application_dirs(&self) -> Vec<PathBuf> {
        std::iter::once(&self.data_home)
            .chain(&self.data_dirs)
            .filter(|dir| !dir.as_os_str().is_empty())
            .map(|dir| dir.join("applications"))
            .collect()
    }

    /// The `mimeapps.list` files, highest priority first.
    fn mimeapps_lists(&self) -> Vec<PathBuf> {
        let mut lists = Vec::new();
        let mut add = |dir: PathBuf| {
            for desktop in &self.desktops {
                lists.push(dir.join(format!("{desktop}-mimeapps.list")));
            }
            lists.push(dir.join("mimeapps.list"));
        };
        for dir in std::iter::once(&self.config_home).chain(&self.config_dirs) {
            if !dir.as_os_str().is_empty() {
                add(dir.clone());
            }
        }
        for dir in self.application_dirs() {
            add(dir);
        }
        lists
    }
}

/// The entry a desktop file describes, `None` when it is hidden, is not an
/// application, or has neither `Exec` nor `DBusActivatable`.
pub fn parse_entry(id: &str, text: &str) -> Option<DesktopEntry> {
    let groups = parse_groups(text);
    let main = groups.get("Desktop Entry")?;
    let get = |key: &str| main.get(key).map(String::as_str);
    if get("Type") != Some("Application") || get("Hidden") == Some("true") {
        return None;
    }
    let exec = get("Exec")
        .map(str::to_owned)
        .filter(|exec| !exec.is_empty());
    if exec.is_none() && get("DBusActivatable") != Some("true") {
        return None;
    }
    Some(DesktopEntry {
        id: id.to_owned(),
        path: None,
        name: get("Name").unwrap_or(id).to_owned(),
        exec,
        icon: get("Icon").unwrap_or("").to_owned(),
        mime_types: get("MimeType").map(list_of).unwrap_or_default(),
        terminal: get("Terminal") == Some("true"),
        try_exec: get("TryExec")
            .map(str::to_owned)
            .filter(|name| !name.is_empty()),
        no_display: get("NoDisplay") == Some("true"),
        working_directory: get("Path")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute()),
    })
}

/// `key=value` lines grouped by `[Group]`. Localised keys (`Name[de]`) are
/// left out: the plain key is read. The first of a repeated key wins.
fn parse_groups(text: &str) -> HashMap<String, HashMap<String, String>> {
    let mut groups: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            current = Some(name.to_owned());
            continue;
        }
        let (Some(group), Some((key, value))) = (&current, line.split_once('=')) else {
            continue;
        };
        let key = key.trim();
        if key.contains('[') {
            continue;
        }
        groups
            .entry(group.clone())
            .or_default()
            .entry(key.to_owned())
            .or_insert_with(|| value.trim().to_owned());
    }
    groups
}

/// A `;`-separated list. `\;` is a literal semicolon.
fn list_of(value: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut item = String::new();
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\\' if chars.peek() == Some(&';') => {
                item.push(';');
                chars.next();
            }
            ';' => items.push(std::mem::take(&mut item)),
            other => item.push(other),
        }
    }
    items.push(item);
    items.retain(|item| !item.trim().is_empty());
    items
        .into_iter()
        .map(|item| item.trim().to_owned())
        .collect()
}

/// Whether the program an entry's `TryExec` names is installed. An entry with
/// no `TryExec` is.
pub fn is_installed(entry: &DesktopEntry) -> bool {
    let Some(program) = &entry.try_exec else {
        return true;
    };
    let path = Path::new(program);
    if path.is_absolute() {
        return is_executable(path);
    }
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| is_executable(&dir.join(program)))
    })
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// The desktop entry with this id (`org.example.App.desktop`). An id with a dash
/// may also be a file in a subdirectory (`kde4-foo.desktop` is `kde4/foo.desktop`).
pub fn find_entry(dirs: &Dirs, id: &str) -> Option<DesktopEntry> {
    for dir in dirs.application_dirs() {
        let mut candidates = vec![dir.join(id)];
        let stem = id.strip_suffix(".desktop").unwrap_or(id);
        let parts: Vec<&str> = stem.split('-').collect();
        for split in 1..parts.len() {
            candidates.push(
                dir.join(parts[..split].join("-"))
                    .join(format!("{}.desktop", parts[split..].join("-"))),
            );
        }
        for path in candidates {
            if let Ok(text) = std::fs::read_to_string(&path)
                && let Some(mut entry) = parse_entry(id, &text)
            {
                entry.path = Some(path);
                return Some(entry);
            }
        }
    }
    None
}

/// What one `mimeapps.list` or `mimeinfo.cache` says: group, then MIME type,
/// then the desktop ids in order.
fn parse_associations(text: &str) -> HashMap<String, HashMap<String, Vec<String>>> {
    parse_groups(text)
        .into_iter()
        .map(|(group, keys)| {
            let keys = keys
                .into_iter()
                .map(|(mime, ids)| (mime, list_of(&ids)))
                .collect();
            (group, keys)
        })
        .collect()
}

/// The application that opens `mime`, by the specification's order.
///
/// Per list, highest priority first: `[Default Applications]`, then `[Added
/// Associations]`; an id that a higher list removed, or that is not installed
/// or whose `TryExec` program is missing, is skipped. Then `mimeinfo.cache`,
/// then any installed entry that lists the type. `None` when nothing does.
pub fn default_for(dirs: &Dirs, mime: &str) -> Option<DesktopEntry> {
    let usable = |id: &str, removed: &HashSet<String>| -> Option<DesktopEntry> {
        if removed.contains(id) {
            return None;
        }
        find_entry(dirs, id).filter(|entry| is_installed(entry) && entry.exec.is_some())
    };
    let lists: Vec<HashMap<String, HashMap<String, Vec<String>>>> = dirs
        .mimeapps_lists()
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| parse_associations(&text))
        .collect();
    let mut removed: HashSet<String> = HashSet::new();
    for group in ["Default Applications", "Added Associations"] {
        // The removals of this list apply to the lists below it, and to
        // the added associations of the whole set.
        removed.clear();
        for list in &lists {
            if let Some(ids) = list.get(group).and_then(|types| types.get(mime)) {
                for id in ids {
                    if let Some(entry) = usable(id, &removed) {
                        return Some(entry);
                    }
                }
            }
            if let Some(ids) = list
                .get("Removed Associations")
                .and_then(|types| types.get(mime))
            {
                removed.extend(ids.iter().cloned());
            }
        }
    }
    removed.clear();
    for list in &lists {
        if let Some(ids) = list
            .get("Removed Associations")
            .and_then(|types| types.get(mime))
        {
            removed.extend(ids.iter().cloned());
        }
    }
    for dir in dirs.application_dirs() {
        let Ok(text) = std::fs::read_to_string(dir.join("mimeinfo.cache")) else {
            continue;
        };
        let cache = parse_associations(&text);
        if let Some(ids) = cache.get("MIME Cache").and_then(|types| types.get(mime)) {
            for id in ids {
                if let Some(entry) = usable(id, &removed) {
                    return Some(entry);
                }
            }
        }
    }
    scan(dirs).into_iter().find(|entry| {
        entry.mime_types.iter().any(|candidate| candidate == mime) && !removed.contains(&entry.id)
    })
}

/// Every installed entry, the first of an id winning, sorted by id so the
/// answer does not depend on directory order.
pub fn scan(dirs: &Dirs) -> Vec<DesktopEntry> {
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for dir in dirs.application_dirs() {
        let mut files: Vec<PathBuf> = walk_desktop_files(&dir);
        files.sort();
        for path in files {
            let Some(id) = entry_id(&dir, &path) else {
                continue;
            };
            if seen.contains(&id) {
                continue;
            }
            seen.insert(id.clone());
            if let Ok(text) = std::fs::read_to_string(&path)
                && let Some(mut entry) = parse_entry(&id, &text)
                && is_installed(&entry)
            {
                entry.path = Some(path);
                found.push(entry);
            }
        }
    }
    found.sort_by(|a, b| a.id.cmp(&b.id));
    found
}

/// `a/b.desktop` under `applications` is the id `a-b.desktop`.
fn entry_id(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let parts: Vec<String> = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    Some(parts.join("-"))
}

fn walk_desktop_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk_desktop_files(&path));
        } else if path
            .extension()
            .is_some_and(|extension| extension == "desktop")
        {
            files.push(path);
        }
    }
    files
}

/// What `Exec` field codes take their values from.
#[derive(Clone, Debug, Default)]
pub struct Arguments<'a> {
    /// Local files, for `%f` and `%F`.
    pub files: &'a [PathBuf],
    /// URIs, for `%u` and `%U`. When empty, the files as `file://` URIs.
    pub uris: &'a [String],
}

/// The command line an entry's `Exec` expands to: see [`expand_exec`].
pub fn command_line(entry: &DesktopEntry, arguments: &Arguments<'_>) -> Vec<String> {
    match &entry.exec {
        Some(exec) => expand_exec(
            exec,
            &entry.name,
            &entry.icon,
            entry.path.as_deref(),
            arguments,
        ),
        None => Vec::new(),
    }
}

/// Expand an `Exec` value for `arguments`: the line split into words as the
/// specification quotes them, field codes replaced (`%c` by `name`, `%i` by
/// `--icon icon`, `%k` by `path`), deprecated codes dropped. An `Exec` with no
/// code for the files or the address still gets the first one at the end, as
/// launchers do for entries written without one.
pub fn expand_exec(
    exec: &str,
    name: &str,
    icon: &str,
    path: Option<&Path>,
    arguments: &Arguments<'_>,
) -> Vec<String> {
    let words = split_exec(exec);
    let uris: Vec<String> = if arguments.uris.is_empty() {
        arguments.files.iter().map(|path| file_uri(path)).collect()
    } else {
        arguments.uris.to_vec()
    };
    let files: Vec<String> = arguments
        .files
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    let mut line = Vec::new();
    for word in words {
        match word.as_str() {
            "%f" => line.extend(files.first().cloned()),
            "%F" => line.extend(files.iter().cloned()),
            "%u" => line.extend(uris.first().cloned()),
            "%U" => line.extend(uris.iter().cloned()),
            "%i" => {
                if !icon.is_empty() {
                    line.push("--icon".to_owned());
                    line.push(icon.to_owned());
                }
            }
            "%c" => line.push(name.to_owned()),
            "%k" => line.extend(path.map(|path| path.to_string_lossy().into_owned())),
            "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => {}
            other => line.push(other.replace("%%", "%")),
        }
    }
    // No field code at all: the entry still gets what was opened.
    if !exec.contains('%') {
        line.extend(
            if files.is_empty() {
                uris.first()
            } else {
                files.first()
            }
            .cloned(),
        );
    }
    line
}

/// Split an `Exec` value into words: spaces separate, double quotes group,
/// and inside quotes a backslash escapes the next character.
fn split_exec(exec: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut chars = exec.chars();
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
    words
}

/// `file:///a%20b` for `/a b`.
pub fn file_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file://");
    for byte in path.as_os_str().as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(*byte as char);
            }
            other => uri.push_str(&format!("%{other:02X}")),
        }
    }
    uri
}

/// The path a `file:` URI names, `None` for another scheme or a host that is
/// not this machine.
pub fn path_of_file_uri(uri: &str) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let rest = uri.strip_prefix("file://")?;
    let path = rest.strip_prefix("localhost").unwrap_or(rest);
    if !path.starts_with('/') {
        return None;
    }
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    Some(PathBuf::from(std::ffi::OsString::from_vec(decoded)))
}

/// Start `line` on its own: not a child the caller waits for, in its own process
/// group so a signal to this program does not reach it, with its output
/// dropped. A thread reaps it when it ends.
pub fn spawn(line: &[String], working_directory: Option<&Path>) -> io::Result<()> {
    use std::os::unix::process::CommandExt;
    let Some((program, arguments)) = line.split_first() else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty command"));
    };
    let mut command = Command::new(program);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    if let Some(directory) = working_directory {
        command.current_dir(directory);
    }
    let mut child = command.spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWER: &str = "[Desktop Entry]\nType=Application\nName=Image Viewer\nName[de]=Bildbetrachter\n\
Exec=viewer --open %f\nIcon=viewer-icon\nMimeType=image/png;image/jpeg;\n\
[Desktop Action new]\nName=Other\nExec=nope\n";

    /// A scratch directory tree that removes itself.
    struct Tree(PathBuf);

    impl Tree {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("bingux-ui-apps-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            Self(root)
        }

        fn write(&self, relative: &str, text: &str) {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }

        fn dirs(&self) -> Dirs {
            Dirs {
                config_home: self.0.join("config"),
                config_dirs: vec![self.0.join("etc/xdg")],
                data_home: self.0.join("data"),
                data_dirs: vec![self.0.join("usr/share")],
                desktops: vec!["gnoblin".into()],
            }
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn app(name: &str, mime: &str) -> String {
        format!(
            "[Desktop Entry]\nType=Application\nName={name}\nExec={name} %U\nMimeType={mime};\n"
        )
    }

    #[test]
    fn a_desktop_entry_is_read() {
        let entry = parse_entry("viewer.desktop", VIEWER).expect("an application");
        assert_eq!(entry.name, "Image Viewer");
        assert_eq!(entry.exec.as_deref(), Some("viewer --open %f"));
        assert_eq!(entry.icon, "viewer-icon");
        assert_eq!(entry.mime_types, ["image/png", "image/jpeg"]);
        assert!(!entry.terminal && !entry.no_display);
    }

    #[test]
    fn hidden_entries_and_other_types_are_not_applications() {
        assert!(
            parse_entry(
                "a.desktop",
                &VIEWER.replace("Name=Image Viewer", "Name=X\nHidden=true")
            )
            .is_none()
        );
        assert!(parse_entry("a.desktop", &VIEWER.replace("Application", "Link")).is_none());
        assert!(parse_entry("a.desktop", "[Desktop Entry]\nType=Application\nName=A\n").is_none());
        // A hidden-from-menus entry is still an application.
        assert!(
            parse_entry(
                "a.desktop",
                &VIEWER.replace("Name=Image Viewer", "Name=X\nNoDisplay=true")
            )
            .is_some()
        );
    }

    #[test]
    fn a_list_keeps_an_escaped_semicolon() {
        assert_eq!(list_of("a;b\\;c;;d;"), ["a", "b;c", "d"]);
    }

    #[test]
    fn field_codes_are_expanded() {
        let mut entry = parse_entry("v.desktop", VIEWER).unwrap();
        entry.exec = Some(r#"/opt/a b/viewer --title "My \"App\"" %i %U 100%% %c"#.into());
        let files = [PathBuf::from("/tmp/x y.png")];
        let line = command_line(
            &entry,
            &Arguments {
                files: &files,
                uris: &[],
            },
        );
        assert_eq!(
            line,
            [
                "/opt/a",
                "b/viewer",
                "--title",
                "My \"App\"",
                "--icon",
                "viewer-icon",
                "file:///tmp/x%20y.png",
                "100%",
                "Image Viewer",
            ]
        );
    }

    #[test]
    fn an_entry_with_no_file_code_still_gets_the_file() {
        let mut entry = parse_entry("v.desktop", VIEWER).unwrap();
        entry.exec = Some("tool".into());
        let files = [PathBuf::from("/t")];
        assert_eq!(
            command_line(
                &entry,
                &Arguments {
                    files: &files,
                    uris: &[]
                }
            ),
            ["tool", "/t"]
        );
        entry.exec = Some("tool %F".into());
        let two = [PathBuf::from("/a"), PathBuf::from("/b")];
        assert_eq!(
            command_line(
                &entry,
                &Arguments {
                    files: &two,
                    uris: &[]
                }
            ),
            ["tool", "/a", "/b"]
        );
    }

    #[test]
    fn a_uri_is_given_as_asked() {
        let mut entry = parse_entry("v.desktop", VIEWER).unwrap();
        entry.exec = Some("browser %u".into());
        let uris = ["https://example.org/?q=a b".to_owned()];
        assert_eq!(
            command_line(
                &entry,
                &Arguments {
                    files: &[],
                    uris: &uris
                }
            ),
            ["browser", "https://example.org/?q=a b"]
        );
    }

    #[test]
    fn file_uris_round_trip() {
        for path in ["/home/a/My Files/é.png", "/tmp/100%/x#y", "/plain"] {
            let uri = file_uri(Path::new(path));
            assert_eq!(
                path_of_file_uri(&uri).as_deref(),
                Some(Path::new(path)),
                "{uri}"
            );
        }
        assert_eq!(
            path_of_file_uri("file://localhost/etc").as_deref(),
            Some(Path::new("/etc"))
        );
        assert_eq!(path_of_file_uri("file://elsewhere/etc"), None);
        assert_eq!(path_of_file_uri("https://example.org"), None);
    }

    #[test]
    fn the_user_default_comes_before_the_system_one_and_removals_hold() {
        let tree = Tree::new("defaults");
        tree.write(
            "usr/share/applications/system.desktop",
            &app("system", "text/plain"),
        );
        tree.write(
            "usr/share/applications/mine.desktop",
            &app("mine", "text/plain"),
        );
        tree.write(
            "usr/share/applications/gone.desktop",
            &app("gone", "text/plain"),
        );
        tree.write(
            "usr/share/applications/mimeapps.list",
            "[Default Applications]\ntext/plain=system.desktop;\n",
        );
        tree.write(
            "config/mimeapps.list",
            "[Default Applications]\ntext/plain=missing.desktop;mine.desktop;\n",
        );
        let found = default_for(&tree.dirs(), "text/plain").expect("a default");
        assert_eq!(
            found.id, "mine.desktop",
            "the user's list wins and skips what is not installed"
        );

        // A removal in the user's list takes the id out of the lists below it.
        tree.write(
            "config/mimeapps.list",
            "[Removed Associations]\ntext/plain=system.desktop;\n",
        );
        let found = default_for(&tree.dirs(), "text/plain").expect("another default");
        assert_ne!(found.id, "system.desktop");
    }

    #[test]
    fn the_desktop_specific_list_comes_first() {
        let tree = Tree::new("desktop-list");
        tree.write("usr/share/applications/a.desktop", &app("a", "image/png"));
        tree.write("usr/share/applications/b.desktop", &app("b", "image/png"));
        tree.write(
            "config/mimeapps.list",
            "[Default Applications]\nimage/png=a.desktop;\n",
        );
        tree.write(
            "config/gnoblin-mimeapps.list",
            "[Default Applications]\nimage/png=b.desktop;\n",
        );
        assert_eq!(
            default_for(&tree.dirs(), "image/png").unwrap().id,
            "b.desktop"
        );
    }

    #[test]
    fn added_associations_and_the_entries_own_types_are_the_fallbacks() {
        let tree = Tree::new("fallbacks");
        tree.write(
            "usr/share/applications/own.desktop",
            &app("own", "audio/flac"),
        );
        tree.write(
            "usr/share/applications/added.desktop",
            &app("added", "audio/ogg"),
        );
        tree.write(
            "config/mimeapps.list",
            "[Added Associations]\naudio/x-wav=added.desktop;\n",
        );
        tree.write(
            "usr/share/applications/mimeinfo.cache",
            "[MIME Cache]\naudio/ogg=added.desktop;\n",
        );
        let dirs = tree.dirs();
        assert_eq!(
            default_for(&dirs, "audio/x-wav").unwrap().id,
            "added.desktop"
        );
        assert_eq!(default_for(&dirs, "audio/ogg").unwrap().id, "added.desktop");
        assert_eq!(default_for(&dirs, "audio/flac").unwrap().id, "own.desktop");
        assert!(default_for(&dirs, "video/mp4").is_none());
    }

    #[test]
    fn an_entry_whose_program_is_missing_is_skipped() {
        let tree = Tree::new("tryexec");
        tree.write(
            "usr/share/applications/broken.desktop",
            "[Desktop Entry]\nType=Application\nName=Broken\nExec=broken %f\nTryExec=/nonexistent/broken\nMimeType=text/x-test;\n",
        );
        tree.write(
            "usr/share/applications/ok.desktop",
            &app("ok", "text/x-test"),
        );
        tree.write(
            "config/mimeapps.list",
            "[Default Applications]\ntext/x-test=broken.desktop;ok.desktop;\n",
        );
        assert_eq!(
            default_for(&tree.dirs(), "text/x-test").unwrap().id,
            "ok.desktop"
        );
    }

    #[test]
    fn an_id_with_a_dash_may_live_in_a_subdirectory() {
        let tree = Tree::new("subdir");
        tree.write(
            "usr/share/applications/vendor/tool.desktop",
            &app("tool", "text/x-vendor"),
        );
        let found = find_entry(&tree.dirs(), "vendor-tool.desktop").expect("found in vendor/");
        assert_eq!(found.name, "tool");
        assert_eq!(scan(&tree.dirs())[0].id, "vendor-tool.desktop");
    }
}
