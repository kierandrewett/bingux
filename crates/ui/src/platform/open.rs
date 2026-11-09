//! Opening a link or a file in the application the person has chosen for it.
//!
//! ```ignore
//! open::open_uri("https://bingux.org")?;
//! open::open_file(Path::new("/home/a/report.pdf"))?;
//! ```
//!
//! Three ways, tried in this order; the first that works ends the search:
//!
//! 1. **The portal**, `org.freedesktop.portal.OpenURI` (`OpenFile` for a file).
//!    The desktop shows its own chooser when it has no default, and a sandboxed
//!    application has no other way.
//! 2. **A desktop entry started directly.** The default application for the
//!    type comes from `mimeapps.list` ([`super::apps`]): a link by its scheme
//!    (`x-scheme-handler/https`), a file by its MIME type. Needs only the files
//!    the XDG specifications describe: no `xdg-open`, no GIO.
//! 3. **`xdg-open`**, when it is installed: its own knowledge of other desktops
//!    covers what the files above do not.
//!
//! Each call blocks for at most a few seconds and never opens two windows: a
//! step that fails to start the application is followed by the next, one that
//! started it is the answer. Call these off the UI thread, or use the `_then`
//! forms from an event handler.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};

use zbus::zvariant::{OwnedFd, OwnedObjectPath, Value};

use super::apps::{self, Arguments, DesktopEntry, Dirs};
use super::bus::{self, Target};

const PORTAL: Target = Target {
    destination: "org.freedesktop.portal.Desktop",
    path: "/org/freedesktop/portal/desktop",
    interface: "org.freedesktop.portal.OpenURI",
};

/// Which way opened it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Via {
    /// The portal accepted it.
    Portal,
    /// This desktop entry was started.
    DesktopEntry(String),
    /// `xdg-open` was started.
    XdgOpen,
}

/// Why nothing opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenError {
    /// The address or path is not something that can be opened. The text says
    /// what is wrong with it, in a sentence a person can read.
    Invalid(String),
    /// Nothing on this system opens it: no application is chosen for the type
    /// and `xdg-open` is not installed.
    NoApplication,
    /// An application was found and could not be started.
    Failed(String),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(why) | Self::Failed(why) => f.write_str(why),
            Self::NoApplication => f.write_str(&crate::tr!("ui-open-no-app-error")),
        }
    }
}

impl std::error::Error for OpenError {}

/// Open an address: a web page, a mail address, any URI with a handler. A
/// `file:` URI opens as the file it names.
pub fn open_uri(uri: &str) -> Result<Via, OpenError> {
    crate::runtime::assert_off_ui_thread("platform::open::open_uri");
    let scheme = scheme_of(uri)?;
    if scheme == "file" {
        let path = apps::path_of_file_uri(uri)
            .ok_or_else(|| OpenError::Invalid(crate::tr!("ui-open-not-local-error")))?;
        return open_file(&path);
    }
    if portal_open_uri(uri) {
        return Ok(Via::Portal);
    }
    let dirs = Dirs::from_env();
    for mime in scheme_types(&scheme) {
        if let Some(entry) = apps::default_for(&dirs, &mime)
            && launch(
                &entry,
                &Arguments {
                    files: &[],
                    uris: &[uri.to_owned()],
                },
            )
            .is_ok()
        {
            return Ok(Via::DesktopEntry(entry.id));
        }
    }
    xdg_open(uri)
}

/// Open a file or a folder in its default application.
pub fn open_file(path: &Path) -> Result<Via, OpenError> {
    crate::runtime::assert_off_ui_thread("platform::open::open_file");
    let metadata = std::fs::metadata(path)
        .map_err(|_| OpenError::Invalid(crate::tr!("ui-open-gone-error")))?;
    let folder = metadata.is_dir();
    if !folder && portal_open_file(path) {
        return Ok(Via::Portal);
    }
    let dirs = Dirs::from_env();
    for mime in file_types(path, folder) {
        if let Some(entry) = apps::default_for(&dirs, &mime)
            && launch(
                &entry,
                &Arguments {
                    files: &[path.to_path_buf()],
                    uris: &[],
                },
            )
            .is_ok()
        {
            return Ok(Via::DesktopEntry(entry.id));
        }
    }
    xdg_open(&path.to_string_lossy())
}

/// [`open_uri`] on the worker pool, calling `done` on the UI thread. For an
/// event handler.
pub fn open_uri_then(uri: String, done: impl FnOnce(Result<Via, OpenError>) + 'static) {
    crate::runtime::background_then(move || open_uri(&uri), done);
}

/// [`open_file`] on the worker pool, calling `done` on the UI thread.
pub fn open_file_then(
    path: std::path::PathBuf,
    done: impl FnOnce(Result<Via, OpenError>) + 'static,
) {
    crate::runtime::background_then(move || open_file(&path), done);
}

/// The scheme of `uri`, checked against RFC 3986: a letter, then letters,
/// digits, `+`, `-` and `.`. Control characters and spaces are not allowed
/// anywhere, so a string cannot smuggle a second argument or a line break.
fn scheme_of(uri: &str) -> Result<String, OpenError> {
    let invalid = || OpenError::Invalid(crate::tr!("ui-open-bad-address-error"));
    if uri.chars().any(|c| c.is_control() || c == ' ') {
        return Err(invalid());
    }
    let (scheme, rest) = uri.split_once(':').ok_or_else(invalid)?;
    let mut letters = scheme.chars();
    let valid = letters.next().is_some_and(|c| c.is_ascii_alphabetic())
        && letters.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !valid || rest.is_empty() {
        return Err(invalid());
    }
    Ok(scheme.to_ascii_lowercase())
}

/// The MIME types whose default opens a link of `scheme`, in order. A web
/// address falls back to the other web scheme and then to what opens HTML.
fn scheme_types(scheme: &str) -> Vec<String> {
    let mut types = vec![format!("x-scheme-handler/{scheme}")];
    match scheme {
        "http" => types.push("x-scheme-handler/https".into()),
        "https" => types.push("x-scheme-handler/http".into()),
        _ => return types,
    }
    types.push("text/html".into());
    types
}

/// The MIME types whose default opens `path`, most specific first: the type
/// from its name, and for text the generic `text/plain`.
fn file_types(path: &Path, folder: bool) -> Vec<String> {
    if folder {
        return vec!["inode/directory".into()];
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let known = bingux_files::mime::for_name(&name).mime;
    let mut types = vec![known.to_owned()];
    if known.starts_with("text/") && known != "text/plain" {
        types.push("text/plain".into());
    }
    types
}

/// Start `entry` for `arguments`. A terminal application has no terminal to
/// run in here, so it is left to the next way.
fn launch(entry: &DesktopEntry, arguments: &Arguments<'_>) -> Result<(), OpenError> {
    if entry.terminal {
        return Err(OpenError::NoApplication);
    }
    let line = apps::command_line(entry, arguments);
    apps::spawn(&line, entry.working_directory.as_deref()).map_err(|_| {
        OpenError::Failed(crate::tr!(
            "ui-open-app-not-started-error",
            app = &entry.name
        ))
    })
}

fn xdg_open(target: &str) -> Result<Via, OpenError> {
    use std::os::unix::process::CommandExt;
    let mut child = Command::new("xdg-open")
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => OpenError::NoApplication,
            _ => OpenError::Failed(crate::tr!("ui-open-not-started-error")),
        })?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(Via::XdgOpen)
}

fn portal_open_uri(uri: &str) -> bool {
    let options: HashMap<String, Value<'static>> = HashMap::new();
    bus::call::<_, OwnedObjectPath>(
        bus::ANSWER_WITHIN,
        PORTAL,
        "OpenURI",
        (String::new(), uri.to_owned(), options),
    )
    .is_ok()
}

fn portal_open_file(path: &Path) -> bool {
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let descriptor = OwnedFd::from(std::os::fd::OwnedFd::from(file));
    let options: HashMap<String, Value<'static>> = HashMap::new();
    bus::call::<_, OwnedObjectPath>(
        bus::ANSWER_WITHIN,
        PORTAL,
        "OpenFile",
        (String::new(), descriptor, options),
    )
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_well_formed_address_is_opened() {
        for good in [
            "https://bingux.org",
            "mailto:a@b.c",
            "x-scheme-handler+v2.1:thing",
        ] {
            assert!(scheme_of(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "no scheme",
            "bingux.org",
            "https:",
            "1http://x",
            "http://x y",
            "http://x\nHost: y",
            "-x://y",
            "ht tp://x",
        ] {
            assert!(scheme_of(bad).is_err(), "{bad:?}");
        }
        assert_eq!(scheme_of("HTTPS://X").unwrap(), "https");
    }

    #[test]
    fn a_link_asks_for_its_scheme_handler_then_the_other_web_scheme_then_html() {
        assert_eq!(scheme_types("mailto"), ["x-scheme-handler/mailto"]);
        assert_eq!(
            scheme_types("https"),
            [
                "x-scheme-handler/https",
                "x-scheme-handler/http",
                "text/html"
            ]
        );
    }

    #[test]
    fn a_file_asks_for_its_type_then_plain_text_for_any_text() {
        assert_eq!(
            file_types(Path::new("/a/b/photo.png"), false),
            ["image/png"]
        );
        assert_eq!(file_types(Path::new("/a/b"), true), ["inode/directory"]);
        let markdown = file_types(Path::new("/a/notes.md"), false);
        assert_eq!(markdown.last().map(String::as_str), Some("text/plain"));
    }

    #[test]
    fn a_file_that_is_gone_says_so() {
        assert_eq!(
            open_file(Path::new("/nonexistent/bingux-ui-test")),
            Err(OpenError::Invalid(crate::tr!("ui-open-gone-error")))
        );
    }

    #[test]
    fn a_default_application_is_started_directly_when_no_portal_answers() {
        // The scratch tree names an application that touches a marker file, as
        // a stand-in for a real one, and makes it the default for a link scheme.
        let root = std::env::temp_dir().join(format!("bingux-ui-open-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let applications = root.join("data/applications");
        std::fs::create_dir_all(&applications).unwrap();
        let marker = root.join("opened");
        // A program that writes its first argument to the marker.
        let script = root.join("marker.sh");
        std::fs::write(
            &script,
            format!("#!/bin/sh\necho \"$1\" > {}\n", marker.display()),
        )
        .unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::write(
            applications.join("marker.desktop"),
            format!(
                "[Desktop Entry]\nType=Application\nName=Marker\nExec={} %u\nMimeType=x-scheme-handler/bingux-test;\n",
                script.display()
            ),
        )
        .unwrap();
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::write(
            root.join("config/mimeapps.list"),
            "[Default Applications]\nx-scheme-handler/bingux-test=marker.desktop;\n",
        )
        .unwrap();
        let dirs = Dirs {
            config_home: root.join("config"),
            data_home: root.join("data"),
            ..Dirs::default()
        };
        let entry = apps::default_for(&dirs, "x-scheme-handler/bingux-test").expect("the default");
        launch(
            &entry,
            &Arguments {
                files: &[],
                uris: &["bingux-test://thing".to_owned()],
            },
        )
        .unwrap();
        for _ in 0..100 {
            if marker.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let written = std::fs::read_to_string(&marker).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(written.trim(), "bingux-test://thing");
    }
}
