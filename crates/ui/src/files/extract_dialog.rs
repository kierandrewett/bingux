//! The Extract dialog (Extract to…): where to put the files, whether to make
//! a folder for them, what to do when a name is taken, and the password of a
//! locked archive.
//!
//! The same [form dialog](crate::shell::form_dialog) as Compress. "Extract
//! Here" skips it: it unpacks into a new folder beside the archive, never
//! replacing anything.
//!
//! What is in the archive, and whether it is locked, is read off the UI
//! thread when the dialog opens. A locked archive shows a password field; the
//! password is tried (7-Zip reads every locked byte once) before the dialog
//! closes, and a wrong one is said under the field, so nothing is started
//! that cannot work.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use bingux_files::ops::tool;
use bingux_files::ops::{Inspection, Overwrite, Password, UnpackOptions, check_password, inspect};
use bingux_files::{Cancel, ChooserRequest};
use freya::prelude::*;

use super::actions::Env;
use super::chooser::choose;
use super::compress::location_row;
use super::compress_run::run_unpack;
use crate::controls::{dropdown, form_field};
use crate::group::{Group, form_row};
use crate::motion::reveal_block;
use crate::rows::{property_row, switch_row};
use crate::shell::{form_dialog, open_form, use_overlays};
use crate::style::*;
use crate::surface::use_tokens;
use crate::task::{spawn_blocking, use_resource};
use crate::window::Window;

struct Launch {
    env: Env,
    archive: PathBuf,
    /// What Extract asked for, waiting for the dialog to be gone.
    pending: RefCell<Option<UnpackOptions>>,
}

struct Shared(Rc<Launch>);

impl PartialEq for Shared {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

fn name_of(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Open the dialog for `archive`.
pub(super) fn show_extract(env: &Env, archive: PathBuf) {
    let launch = Rc::new(Launch {
        env: env.clone(),
        archive: archive.clone(),
        pending: RefCell::new(None),
    });
    let after = launch.clone();
    let focus = env.focus;
    let closed = EventHandler::new(move |()| match after.pending.borrow_mut().take() {
        Some(options) => {
            let whole = format!("“{}”", name_of(&after.archive));
            run_unpack(&after.env, after.archive.clone(), options, whole);
        }
        None => focus.request_focus(),
    });
    open_form(
        env.overlays,
        format!("Extract “{}”", name_of(&archive)),
        Element::from(ExtractDialog { launch: Shared(launch) }),
        Some(closed),
    );
}

fn read_archive(archive: PathBuf) -> Result<Inspection, String> {
    inspect(&archive, None, &Cancel::new()).map_err(|error| error.to_string())
}

#[derive(PartialEq)]
struct ExtractDialog {
    launch: Shared,
}

impl Component for ExtractDialog {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = use_overlays();
        let in_context = use_try_consume::<Window>();
        let window = self.launch.0.env.window.or(in_context);
        let launch = self.launch.0.clone();
        let parent = launch.archive.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut destination = use_state({
            let parent = parent.clone();
            move || parent
        });
        let mut make_folder = use_state(|| true);
        let mut overwrite = use_state(|| Overwrite::KeepBoth);
        let password = use_state(String::new);
        let mut wrong = use_state(|| None::<String>);
        let mut checking = use_state(|| false);

        let inspection = use_resource(launch.archive.clone(), read_archive);
        let (found, failed) = inspection.with(|found| (found.value().cloned(), found.error().map(str::to_owned)));
        let locked = found.as_ref().is_some_and(|found| found.locked);
        let needs_seven = locked && tool::seven_zip().is_none();
        let blocked = failed
            .clone()
            .or_else(|| needs_seven.then(|| "Opening a locked archive needs 7-Zip, which is not installed.".to_string()))
            .or_else(|| (locked && password.read().is_empty()).then(|| "Enter the password.".to_string()))
            .or_else(|| found.is_none().then(|| "Reading the archive…".to_string()));

        let go = {
            let launch = launch.clone();
            EventHandler::new(move |()| {
                if *checking.peek() {
                    return;
                }
                let options = UnpackOptions {
                    dest_dir: destination.peek().clone(),
                    create_folder: *make_folder.peek(),
                    overwrite: *overwrite.peek(),
                    password: locked.then(|| Password::new(password.peek().clone())),
                };
                if !locked {
                    *launch.pending.borrow_mut() = Some(options);
                    overlays.dismiss_dialog();
                    return;
                }
                // Try the password first: a wrong one is said here, not after
                // the dialog has gone.
                checking.set(true);
                wrong.set(None);
                let (archive, tried) = (launch.archive.clone(), options.password.clone());
                let launch = launch.clone();
                spawn(async move {
                    let outcome = spawn_blocking(move || match &tried {
                        Some(pass) => check_password(&archive, pass, &Cancel::new()),
                        None => Ok(true),
                    })
                    .await;
                    checking.set(false);
                    match outcome {
                        Ok(true) => {
                            *launch.pending.borrow_mut() = Some(options);
                            overlays.dismiss_dialog();
                        }
                        Ok(false) => wrong.set(Some("That password is not right.".to_string())),
                        Err(error) => wrong.set(Some(error.to_string())),
                    }
                });
            })
        };

        let choose_where = EventHandler::new({
            let start = destination.read().clone();
            move |()| {
                let Some(window) = window else { return };
                choose(
                    window,
                    ChooserRequest::select_folder()
                        .title("Extract to")
                        .accept_label("Choose")
                        .current_folder(start.clone()),
                    move |chosen| {
                        if let Some(folder) = chosen.and_then(|chosen| chosen.paths.into_iter().next()) {
                            destination.set(folder);
                        }
                    },
                );
            }
        });
        let place = location_row(&destination.read(), choose_where);

        let contents = match (&found, &failed) {
            (_, Some(error)) => error.clone(),
            (Some(found), None) if found.names_hidden => "Locked: the names are hidden too".to_string(),
            (Some(found), None) if found.locked => format!("{} items, locked", found.entries),
            (Some(found), None) => format!("{} item{}", found.entries, if found.entries == 1 { "" } else { "s" }),
            (None, None) => "Reading…".to_string(),
        };
        let folder_name = found
            .as_ref()
            .map(|found| found.single_folder.clone().unwrap_or_else(|| found.stem.clone()))
            .unwrap_or_default();
        let archive_group = Group::titled("Archive")
            .row(property_row("Name", name_of(&launch.archive)))
            .row(property_row("Contents", contents));
        let target_group = Group::titled("Extract to")
            .row(place)
            .row(
                switch_row("Put the files in a new folder", *make_folder.read(), move |on: bool| make_folder.set(on))
                    .subtitle(if *make_folder.read() {
                        if folder_name.is_empty() { "A folder named after the archive.".to_string() } else { format!("“{folder_name}”.") }
                    } else {
                        "The files land loose in the folder.".to_string()
                    }),
            );
        let policy = Group::titled("If a name is taken").row(form_row(
            "Then",
            dropdown(
                vec![
                    ("keep".to_string(), "Keep both".to_string()),
                    ("replace".to_string(), "Replace (the old one goes to the Trash)".to_string()),
                    ("skip".to_string(), "Skip it".to_string()),
                ],
                match *overwrite.read() {
                    Overwrite::KeepBoth => "keep",
                    Overwrite::Replace => "replace",
                    Overwrite::Skip => "skip",
                },
                move |key: String| {
                    overwrite.set(match key.as_str() {
                        "replace" => Overwrite::Replace,
                        "skip" => Overwrite::Skip,
                        _ => Overwrite::KeepBoth,
                    });
                },
            )
            .title("If a name is taken"),
        ));
        let lock = Group::titled("Password").row(
            form_field("Password", password)
                .password()
                .error(wrong.read().clone())
                .on_submit(go.clone())
                .autofocus(),
        );
        let body = rect()
            .tw(&tokens, &[flex_col(), w_full(), gap(S4)])
            .child(Element::from(archive_group))
            .child(reveal_block(locked, S4, Element::from(lock)).lazy())
            .child(Element::from(target_group))
            .child(Element::from(policy));
        form_dialog(format!("Extract “{}”", name_of(&launch.archive)), body, "Extract", go)
            .blocked(blocked)
            .busy(checking.read().then(|| "Checking the password…".to_string()))
            .stays_open()
    }
}
