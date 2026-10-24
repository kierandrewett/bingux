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
    /// Where the application came from and what it takes up. Filled in by
    /// [`with_details`]; `None` until then.
    pub details: Option<Details>,
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
                details: None,
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

/// Launch `app` from its desktop file with `gio launch`, off the UI thread.
/// The spawn and the short watch for an immediate failure run on a worker
/// thread. `on_failure` runs on the UI thread with the reason, so the caller
/// can show a toast.
pub fn launch(app: &AppEntry, on_failure: impl FnOnce(String) + 'static) {
    let file = app.desktop_file.clone();
    let name = app.name.clone();
    system::background_then(
        move || launch_blocking(&file, &name),
        move |result| {
            if let Err(reason) = result {
                on_failure(reason);
            }
        },
    );
}

/// How long to watch `gio launch` for an early failure. A launcher still
/// running after this has started the application, so it is left alone.
const LAUNCH_WATCH: Duration = Duration::from_secs(3);

/// Run `gio launch` and watch it briefly. Blocking: call from a worker thread.
fn launch_blocking(file: &Path, name: &str) -> Result<(), String> {
    let mut child = std::process::Command::new("gio")
        .arg("launch")
        .arg(file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not open {name}. {error}"))?;
    let started = Instant::now();
    while started.elapsed() < LAUNCH_WATCH {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => return Err(format!("Could not open {name}.")),
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => return Ok(()),
        }
    }
    // Still running: it is the application's own life now. Reap it quietly.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Where an application came from. Each source has its own way to remove it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Flatpak,
    Snap,
    AppImage,
    Rpm,
    Nix,
    /// A desktop entry in the user's own applications folder, with no package.
    Local,
    /// Installed by the system, but no package this page can see owns it.
    System,
}

impl Source {
    /// The name shown in lists and on the page.
    pub fn label(self) -> &'static str {
        match self {
            Source::Flatpak => "Flatpak",
            Source::Snap => "Snap",
            Source::AppImage => "AppImage",
            Source::Rpm => "RPM package",
            Source::Nix => "Nix",
            Source::Local => "Local desktop entry",
            Source::System => "System",
        }
    }
}

/// How an application is removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Removal {
    /// Run this command. The first word is the program.
    Command(Vec<String>),
    /// Delete these files.
    Files(Vec<PathBuf>),
    /// Not removed here. The text says what to do instead.
    Unavailable(&'static str),
}

/// Where an application came from and what it takes up. The package tools
/// are asked for it, so [`with_details`] runs on a background thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Details {
    pub source: Source,
    /// The Flatpak remote, the RPM vendor or the snap store. Empty when unknown.
    pub origin: String,
    pub version: String,
    /// Bytes on disk, when the package tool or the file system says.
    pub size: Option<u64>,
    /// The install directory, the package name or the file.
    pub location: String,
    pub removal: Removal,
}

const TOOL_TIMEOUT: Duration = Duration::from_secs(20);
const UNINSTALL_TIMEOUT: Duration = Duration::from_secs(900);

/// One Flatpak application from `flatpak list`.
struct FlatpakApp {
    version: String,
    origin: String,
    installation: String,
    size: Option<u64>,
}

/// Every installed Flatpak application, keyed by id, from one call.
fn flatpak_apps() -> HashMap<String, FlatpakApp> {
    if !system::which("flatpak") {
        return HashMap::new();
    }
    let columns = "--columns=application,version,origin,installation,size";
    let Ok(text) = system::run("flatpak", &["list", "--app", columns], TOOL_TIMEOUT) else {
        return HashMap::new();
    };
    text.lines()
        .filter_map(|line| {
            let mut columns = line.split('\t').map(str::trim);
            let id = columns.next().filter(|id| !id.is_empty())?;
            let app = FlatpakApp {
                version: columns.next().unwrap_or_default().to_string(),
                origin: columns.next().unwrap_or_default().to_string(),
                installation: columns.next().unwrap_or_default().to_string(),
                size: columns.next().and_then(parse_size),
            };
            Some((id.to_string(), app))
        })
        .collect()
}

/// A size as `flatpak list` prints it, such as `141.2 MB` (decimal units).
fn parse_size(text: &str) -> Option<u64> {
    let mut parts = text.split_whitespace();
    let number: f64 = parts.next()?.parse().ok()?;
    let factor = match parts.next().unwrap_or("B").to_ascii_lowercase().as_str() {
        "b" | "bytes" => 1.0,
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        _ => return None,
    };
    Some((number * factor) as u64)
}

/// The RPM package that owns a file.
struct Package {
    name: String,
    version: String,
    size: Option<u64>,
    vendor: String,
}

/// The package that owns each file, from one `rpm -qf` for all of them.
///
/// `rpm -qf` prints one line per argument, in order: the package's fields for
/// an owned file, or "file X is not owned by any package" for a stray one.
/// Files that are not there are dropped first, since rpm prints nothing for
/// them and that would shift every later answer. If the count still does not
/// match, the answers cannot be trusted, so none are used.
fn owning_packages(files: &[PathBuf]) -> HashMap<PathBuf, Package> {
    const FORMAT: &str = r"%{NAME}\t%{VERSION}-%{RELEASE}\t%{SIZE}\t%{VENDOR}\n";
    let files: Vec<&PathBuf> = files.iter().filter(|file| file.is_file()).collect();
    if files.is_empty() || !system::which("rpm") {
        return HashMap::new();
    }
    // rpm exits non-zero when a file is unowned, which `run` reports as an
    // error, so the script forces success and only stdout is read.
    const SCRIPT: &str = r#"fmt="$1"; shift; rpm -qf --queryformat "$fmt" "$@"; exit 0"#;
    let paths: Vec<&str> = files.iter().filter_map(|file| file.to_str()).collect();
    if paths.len() != files.len() {
        return HashMap::new();
    }
    let mut args = vec!["-c", SCRIPT, "sh", FORMAT];
    args.extend(paths);
    let Ok(text) = system::run("sh", &args, Duration::from_secs(120)) else {
        return HashMap::new();
    };
    let answers: Vec<Option<Package>> = text
        .lines()
        .map(|line| {
            if line.starts_with("file ") && line.ends_with("is not owned by any package") {
                return None;
            }
            let mut fields = line.split('\t');
            let name = fields.next()?.to_string();
            let version = fields.next().unwrap_or_default().to_string();
            let size = fields.next().and_then(|size| size.trim().parse().ok());
            let vendor = fields.next().unwrap_or_default().trim();
            let vendor = if vendor == "(none)" { "" } else { vendor };
            Some(Package {
                name,
                version,
                size,
                vendor: vendor.to_string(),
            })
        })
        .collect();
    if answers.len() != files.len() {
        return HashMap::new();
    }
    files
        .into_iter()
        .zip(answers)
        .filter_map(|(file, package)| Some((file.clone(), package?)))
        .collect()
}

/// The program `Exec` runs as written, with its directory when it has one.
/// [`exec_program`] keeps only the file name.
fn exec_path(desktop: &Path) -> String {
    let exec = read_entry(desktop)
        .and_then(|values| values.get("Exec").cloned())
        .unwrap_or_default();
    exec.split_whitespace()
        .find(|word| !word.contains('=') && *word != "env")
        .map(|word| word.trim_matches('"').to_string())
        .unwrap_or_default()
}

/// The snap a desktop file belongs to: `snap.firefox.firefox` or
/// `firefox_firefox` both give `firefox`.
fn snap_name(desktop: &Path) -> Option<String> {
    let stem = desktop.file_stem()?.to_str()?;
    let name = match stem.strip_prefix("snap.") {
        Some(rest) => rest.split('.').next()?,
        None => stem.split('_').next()?,
    };
    Some(name.to_string())
}

/// Whether a desktop file was made by the user rather than a package.
fn in_home(path: &Path, home: &Path) -> bool {
    !home.as_os_str().is_empty() && path.starts_with(home)
}

/// The `flatpak uninstall` command for one application. System installs
/// need administrator rights, so they go through `pkexec`.
fn flatpak_uninstall(installation: &str, id: &str) -> Vec<String> {
    let scope = match installation {
        "user" => "--user".to_string(),
        "system" => "--system".to_string(),
        other => format!("--installation={other}"),
    };
    let mut command = if installation == "user" {
        Vec::new()
    } else {
        vec!["pkexec".to_string()]
    };
    command.extend(
        [
            "flatpak",
            "uninstall",
            "--noninteractive",
            "--assumeyes",
            scope.as_str(),
            id,
        ]
        .into_iter()
        .map(String::from),
    );
    command
}

/// Where one application came from, from the first rule that matches:
/// Flatpak, snap, Nix, AppImage, a user's own desktop file, an RPM package,
/// and otherwise the system.
fn classify(
    app: &AppEntry,
    flatpak: &HashMap<String, FlatpakApp>,
    packages: &HashMap<PathBuf, Package>,
    home: &Path,
) -> Details {
    let desktop = app.desktop_file.as_path();
    let desktop_text = desktop.display().to_string();
    let details = |source, origin: &str, version: &str, size, location: String, removal| Details {
        source,
        origin: origin.to_string(),
        version: version.to_string(),
        size,
        location,
        removal,
    };
    if let Some(found) = flatpak.get(&app.id) {
        let base = match found.installation.as_str() {
            "system" => "/var/lib/flatpak".to_string(),
            "user" => home.join(".local/share/flatpak").display().to_string(),
            other => format!("Flatpak installation {other}"),
        };
        return details(
            Source::Flatpak,
            &found.origin,
            &found.version,
            found.size,
            format!("{base}/app/{}", app.id),
            Removal::Command(flatpak_uninstall(&found.installation, &app.id)),
        );
    }
    if desktop_text.contains("/flatpak/exports/") {
        return details(
            Source::Flatpak,
            "",
            "",
            None,
            desktop_text,
            Removal::Unavailable("Remove it with flatpak uninstall."),
        );
    }
    if desktop.starts_with("/var/lib/snapd/") {
        let name = snap_name(desktop).unwrap_or_else(|| app.id.clone());
        return details(
            Source::Snap,
            "Snap Store",
            "",
            None,
            format!("/snap/{name}"),
            Removal::Command(vec![
                "pkexec".to_string(),
                "snap".to_string(),
                "remove".to_string(),
                name,
            ]),
        );
    }
    let exec = exec_path(desktop);
    if desktop.starts_with("/nix/") || exec.starts_with("/nix/store/") {
        return details(
            Source::Nix,
            "",
            "",
            None,
            if exec.is_empty() { desktop_text } else { exec },
            Removal::Unavailable("Remove it from your Nix profile or configuration."),
        );
    }
    if exec.to_ascii_lowercase().ends_with(".appimage") {
        let size = std::fs::metadata(&exec).ok().map(|meta| meta.len());
        let mut files = vec![PathBuf::from(&exec)];
        if in_home(desktop, home) {
            files.push(desktop.to_path_buf());
        }
        let removal = if in_home(Path::new(&exec), home) {
            Removal::Files(files)
        } else {
            Removal::Unavailable("Ask an administrator to remove it.")
        };
        return details(
            Source::AppImage,
            "Stand-alone file",
            "",
            size,
            exec,
            removal,
        );
    }
    if in_home(desktop, home) {
        return details(
            Source::Local,
            "Made on this computer",
            "",
            None,
            desktop_text,
            Removal::Files(vec![desktop.to_path_buf()]),
        );
    }
    if let Some(package) = packages.get(desktop) {
        return details(
            Source::Rpm,
            &package.vendor,
            &package.version,
            package.size,
            format!("Package {}", package.name),
            Removal::Command(vec![
                "pkexec".to_string(),
                "dnf".to_string(),
                "remove".to_string(),
                "-y".to_string(),
                package.name.clone(),
            ]),
        );
    }
    details(
        Source::System,
        "",
        "",
        None,
        desktop_text,
        Removal::Unavailable("Installed by the system. Remove it with the system package tools."),
    )
}

/// Fill in [`Details`] for every application. This asks `flatpak` once and
/// `rpm` once per system file, so it takes a moment: call it on a background
/// thread, never the UI thread.
pub fn with_details(apps: &mut [AppEntry]) {
    let flatpak = flatpak_apps();
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let system_files: Vec<PathBuf> = apps
        .iter()
        .filter(|app| {
            !flatpak.contains_key(&app.id)
                && !in_home(&app.desktop_file, &home)
                && !app.desktop_file.starts_with("/var/lib/snapd/")
                && !app.desktop_file.starts_with("/nix/")
                && !app
                    .desktop_file
                    .to_string_lossy()
                    .contains("/flatpak/exports/")
        })
        .map(|app| app.desktop_file.clone())
        .collect();
    let packages = owning_packages(&system_files);
    for app in apps.iter_mut() {
        app.details = Some(classify(app, &flatpak, &packages, &home));
    }
}

/// Remove `app` the way its [`Details::removal`] says. The package tools
/// block, so call this from a background thread.
///
/// Under the snapshot feature, `BINGUX_SETTINGS_FAKE_UNINSTALL` only logs the
/// removal, so the dialog can be tested without removing anything.
pub fn uninstall(app: &AppEntry) -> Result<(), String> {
    let Some(details) = &app.details else {
        return Err("The details of this application are not known yet.".to_string());
    };
    if cfg!(feature = "snapshot") && std::env::var_os("BINGUX_SETTINGS_FAKE_UNINSTALL").is_some() {
        eprintln!("[fake uninstall] {}: {:?}", app.id, details.removal);
        return Ok(());
    }
    match &details.removal {
        Removal::Command(command) => {
            let (program, args) = command.split_first().ok_or("There is no command to run.")?;
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            system::run(program, &args, UNINSTALL_TIMEOUT)
                .map(|_| ())
                .map_err(|error| error.0)
        }
        Removal::Files(files) => files.iter().try_for_each(|file| {
            std::fs::remove_file(file).map_err(|error| format!("{}: {error}", file.display()))
        }),
        Removal::Unavailable(reason) => Err(reason.to_string()),
    }
}
