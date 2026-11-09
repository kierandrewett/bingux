//! What an application that shows one file needs from the file manager's side:
//! the "Open with" list for it, opening it in the default application, and
//! showing it in Files.
//!
//! The applications are the ones whose desktop entries name the file's type
//! (`actions::open_with` reads them once, on a worker). Nothing here assumes a
//! particular desktop: with no entries the list is empty, with no `bingux-files`
//! the folder opens in whatever the system opens folders with.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::actions::{App, apps_for, launch, open_default};
use crate::app_icon::AppChoice;
use crate::shell::{MenuItem, app_menu_items};

/// Read the desktop entries now, off the UI thread, so the first "Open with"
/// does not wait for the disk. Call it once from `render`.
pub fn warm_open_with() {
    super::actions::warm_open_with();
}

/// The applications that can open a file of `mime`, as menu rows with their
/// icons. Choosing one starts it on `path`; `on_error` is told if it could not
/// start (its text names the application). Empty until the entries are read.
pub fn open_with_items(
    path: &Path,
    mime: &str,
    on_error: impl Fn(String) + Clone + 'static,
) -> Vec<MenuItem> {
    let apps: Vec<App> = apps_for(mime);
    let choices: Vec<AppChoice> = apps
        .iter()
        .enumerate()
        .map(|(index, app)| AppChoice::new(index.to_string(), app.name.clone(), app.icon.clone()))
        .collect();
    let path: PathBuf = path.to_owned();
    app_menu_items(choices, move |chosen: &AppChoice| {
        let Some(app) = chosen
            .key
            .parse::<usize>()
            .ok()
            .and_then(|index| apps.get(index))
        else {
            return;
        };
        if launch(app, &path).is_err() {
            on_error(app.name.clone());
        }
    })
}

/// Open `path` in the application the system names for its type.
pub fn open_in_default_application(path: &Path) -> io::Result<()> {
    open_default(path)
}

/// Show `path` in its folder, selected, in the Bingux file manager; when that
/// is not installed, open the folder with whatever the system opens folders with.
pub fn show_in_files(path: &Path) -> io::Result<()> {
    let spawn = |program: &str, argument: &Path| {
        Command::new(program)
            .arg(argument)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|mut child| {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            })
    };
    match spawn("bingux-files", path) {
        Ok(()) => Ok(()),
        Err(_) => spawn("xdg-open", path.parent().unwrap_or(path)),
    }
}
