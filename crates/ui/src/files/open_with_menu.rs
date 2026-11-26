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

use super::actions::{App, apps_for, launch_then, open_default};
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
        let (name, on_error) = (app.name.clone(), on_error.clone());
        launch_then(app, &path, move |started| {
            if started.is_err() {
                on_error(name);
            }
        });
    })
}

/// Open `path` in the application the system names for its type. It returns
/// at once; `failed` is called on the UI thread with what went wrong when
/// nothing could open the file.
pub fn open_in_default_application(path: &Path, failed: impl FnOnce(String) + 'static) {
    open_default(path, failed)
}

/// Show `path` in its folder, selected, in the Bingux file manager; when that
/// is not installed, open the folder with whatever the system opens folders with.
/// It returns at once; `done` is called on the UI thread with how it went.
pub fn show_in_files(path: &Path, done: impl FnOnce(io::Result<()>) + 'static) {
    let path = path.to_owned();
    crate::runtime::background_then(
        move || {
            let program = |name: &str, argument: &Path| {
                crate::runtime::spawn_detached(
                    &[std::ffi::OsStr::new(name), argument.as_os_str()],
                    None,
                )
            };
            program("bingux-files", &path)
                .or_else(|_| program("xdg-open", path.parent().unwrap_or(&path)))
        },
        done,
    );
}
