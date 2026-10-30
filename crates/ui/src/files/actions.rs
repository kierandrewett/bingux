//! What a person can do to files in a file view: open, cut, copy, paste,
//! rename, create, trash, delete, compress, bookmark, look at properties,
//! undo. One place decides what is available, runs it, and answers the keys.
//!
//! The view owns the selection and the listing and passes them in as an
//! [`Env`]. Menus ([`super::context_menu`]), the keyboard ([`handle_key`]) and
//! drag and drop ([`Command::MoveInto`]) all go through [`run`], so they
//! cannot disagree. [`available`] is pure over a [`Facts`] summary, so the
//! rules are tested without a window.
//!
//! Blocking work (every call into [`backend`]) runs on the worker pool and
//! reports back through [`Ui`]: a busy bar with a cancel button, a notice for
//! the status line, and the inline rename field.

mod backend;
mod bin;
mod clipboard;
mod names;
mod open_with;
mod transfer;
mod undo;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use bingux_files::location::file_uri;
use bingux_files::places::Places;
use bingux_files::{Cancel, Entry, Listing, Selection, Sort};
use freya::prelude::*;

use super::columns::ColumnLayout;
use super::properties;
use super::zoom::Zoom;
use crate::shell::Overlays;
use crate::window::Window;
use crate::task::spawn_blocking;

pub(super) use names::{check_name, stem_chars};
pub(super) use open_with::{App, apps_for, launch, open_default};
pub(super) use clipboard::is_cut;

/// The sidebar's Empty Trash asks the file view beside it to empty the Trash:
/// the view owns the busy line and the status notice the work reports in. The
/// request is a counter in a window-wide context; whichever view sees it first
/// does it, so a window with two views asks once.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct EmptyTrashRequest(pub State<u32>);

/// Provide the window's [`EmptyTrashRequest`], from a component above the
/// sidebar and the view.
pub(super) fn provide_empty_trash() -> EmptyTrashRequest {
    use_provide_root_context(|| EmptyTrashRequest(State::create(0)))
}

/// Ask the views of the window to empty the Trash.
pub(super) fn request_empty_trash(mut request: EmptyTrashRequest) {
    let next = *request.0.peek() + 1;
    request.0.set(next);
}

/// Whether this view is the one to act on request number `wanted`.
pub(super) fn claim_empty_trash(wanted: u32) -> bool {
    static LAST: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    wanted != 0 && LAST.swap(wanted, Ordering::SeqCst) != wanted
}

/// How long a notice stays in the status line.
const NOTICE_TIME: Duration = Duration::from_secs(4);

/// What a view lets a person do. A chooser is a file view with the changes a
/// person expects from one, and without the actions that leave it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Capabilities {
    /// Open with another application.
    pub open_with: bool,
    /// Open a folder in a new tab.
    pub new_tab: bool,
    /// Rename, create, trash, delete, paste, cut and compress.
    pub modify: bool,
    pub bookmarks: bool,
    pub properties: bool,
}

impl Capabilities {
    pub const CHOOSER: Self = Self {
        open_with: false,
        new_tab: false,
        modify: true,
        bookmarks: true,
        properties: true,
    };
    pub const FILES_APP: Self = Self {
        open_with: true,
        new_tab: true,
        modify: true,
        bookmarks: true,
        properties: true,
    };
}

/// A folder or file being renamed in place.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Rename {
    pub path: PathBuf,
    /// Just created: Escape keeps the item with its first name.
    pub created: bool,
}

/// A line for the status line, for a few seconds.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Notice {
    pub text: String,
    pub error: bool,
    /// Tells one notice from the next, so an old one's timer does not clear
    /// a newer notice.
    pub id: u64,
}

/// A running operation, for the status line.
#[derive(Clone)]
pub(super) struct Busy {
    /// "Copying 3 of 12".
    pub label: String,
    /// `None` while the total is not known yet.
    pub fraction: Option<f32>,
    /// `None` for work that cannot be stopped part way.
    pub cancel: Option<Cancel>,
}

/// How many file operations (copy, move, compress, extract, emptying the
/// Trash) are running in this process. Each is counted from when it claims
/// the busy slot until it finishes, on the window's own task, so it is right
/// also when the tab that began it was closed meanwhile.
static RUNNING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// An operation claimed the busy slot.
pub(super) fn operation_began() {
    RUNNING.fetch_add(1, Ordering::AcqRel);
}

/// An operation gave the busy slot up.
pub(super) fn operation_ended() {
    let _ = RUNNING.fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| count.checked_sub(1));
}

/// Whether a file operation is running: closing the window now would stop
/// it half way, so the host asks first.
pub fn operations_running() -> bool {
    RUNNING.load(Ordering::Acquire) > 0
}

/// Cheap `Copy` state the view owns and shares with its rows and tiles.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct Ui {
    pub rename: State<Option<Rename>>,
    pub notice: State<Option<Notice>>,
    pub busy: State<Option<Busy>>,
}

/// The view's [`Ui`]. Call it from `render`.
pub(super) fn use_ui() -> Ui {
    // The first Open with menu should not wait for the disk.
    use_hook(open_with::warm);
    use_hook(super::compress::warm);
    Ui {
        rename: use_state(|| None),
        notice: use_state(|| None),
        busy: use_state(|| None),
    }
}

/// The view's own settings, which the empty-space menu changes.
#[derive(Clone)]
pub(super) struct ViewControls {
    pub show_hidden: bool,
    pub set_show_hidden: EventHandler<bool>,
    pub sort: Sort,
    pub set_sort: EventHandler<Sort>,
    pub zoom: Zoom,
    pub layout: State<ColumnLayout>,
}

/// Everything an action needs to know about the view it acts on.
#[derive(Clone)]
pub(super) struct Env {
    /// `None` while the Recent place shows.
    pub folder: Option<PathBuf>,
    /// Kept for the places that show a path relative to the home folder.
    #[allow(dead_code)]
    pub home: PathBuf,
    pub caps: Capabilities,
    /// For dialogs and menus.
    pub overlays: Overlays,
    pub ui: Ui,
    pub selection: State<Selection>,
    pub listing: Arc<Listing>,
    /// The entries on show, as indices into `listing.entries`, in display
    /// order: what Select all selects.
    pub order: Arc<Vec<usize>>,
    /// Read the folder again now. Also used after an operation, though the
    /// watcher fires as well.
    pub reload: EventHandler<()>,
    /// Navigate to a folder.
    pub open_folder: EventHandler<PathBuf>,
    /// As a double click on `listing.entries[index]`.
    pub activate: EventHandler<usize>,
    pub open_new_tab: EventHandler<PathBuf>,
    /// The folders that are bookmarks now.
    pub bookmarks: Arc<Vec<PathBuf>>,
    /// Called after a bookmark was added or removed, so the sidebar reads
    /// the bookmarks again.
    pub bookmarks_changed: EventHandler<()>,
    pub view: ViewControls,
    /// Open Quick Look on the selection (`preview.rs`).
    pub preview: EventHandler<()>,
    /// The view's keyboard focus, which a dialog opened from the keyboard
    /// gives back when it closes.
    pub focus: AccessibilityId,
    /// The window the view is in, for a dialog it opens (Restore to… asks for
    /// a folder). `None` where there is no window.
    pub window: Option<Window>,
}

/// A thing a person can ask for.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Command {
    Open,
    /// Quick Look: see the selected file in a large card.
    Preview,
    /// Open the one selected file with this application.
    OpenWith(App),
    OpenInNewTab,
    Cut,
    Copy,
    /// Into the folder on show.
    Paste,
    /// Into this folder, from a folder's own menu.
    PasteInto(PathBuf),
    Rename,
    NewFolder,
    NewFile,
    /// A new file that starts as a copy of this template.
    NewFromTemplate(PathBuf),
    Trash,
    DeletePermanently,
    /// Put the selected items of the Trash back where they were.
    Restore,
    /// Put the selected items of the Trash into a folder to choose.
    RestoreTo,
    /// Empty every Trash folder. Only offered while the Trash is on show.
    EmptyTrash,
    CopyPath,
    CopyUri,
    AddBookmark,
    RemoveBookmark,
    /// Open the Compress dialog (archive, or make pictures, films and songs smaller).
    Compress,
    /// Open the Compress dialog for what is selected now, from the menu of
    /// empty space (which a Ctrl or Shift right click opens with the
    /// selection kept).
    CompressSelection,
    /// Compress with the default options and no questions, in this format.
    CompressQuick(bingux_files::ops::Format),
    /// Extract Here: into a new folder beside the archive, replacing nothing.
    Extract,
    /// Extract to…: the dialog.
    ExtractTo,
    Properties,
    SelectAll,
    InvertSelection,
    Refresh,
    Undo,
    /// Drag and drop: move or copy `paths` into `dest`.
    MoveInto {
        paths: Vec<PathBuf>,
        dest: PathBuf,
        copy: bool,
    },
}

impl Command {
    /// Whether this changes files, so a view that cannot change them hides it.
    fn changes_files(&self) -> bool {
        matches!(
            self,
            Command::Cut
                | Command::Paste
                | Command::PasteInto(_)
                | Command::Rename
                | Command::NewFolder
                | Command::NewFile
                | Command::NewFromTemplate(_)
                | Command::Trash
                | Command::DeletePermanently
                | Command::Restore
                | Command::RestoreTo
                | Command::EmptyTrash
                | Command::Compress
                | Command::CompressSelection
                | Command::CompressQuick(_)
                | Command::Extract
                | Command::ExtractTo
                | Command::Undo
                | Command::MoveInto { .. }
        )
    }
}

/// Whether a command can be used right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Availability {
    Enabled,
    /// Shown greyed out. The reason is for a tooltip or the status line.
    Disabled(&'static str),
    /// Not offered at all.
    Hidden,
}

impl Availability {
    pub(super) fn is_enabled(self) -> bool {
        self == Availability::Enabled
    }

    pub(super) fn is_shown(self) -> bool {
        self != Availability::Hidden
    }
}

/// What [`availability`] needs to know, with no window in it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Facts {
    pub caps: Capabilities,
    /// A real folder is on show (not the Recent place).
    pub in_folder: bool,
    pub selected: usize,
    /// How many of the selected entries are folders.
    pub folders: usize,
    pub can_paste: bool,
    /// The one selected folder is a bookmark.
    pub bookmarked: bool,
    /// The one selected file is an archive and the tool to open it exists.
    pub extractable: bool,
    pub can_compress: bool,
    /// Entries on show.
    pub shown: usize,
    /// The label of the change Ctrl+Z would take back.
    pub undo: Option<&'static str>,
    /// Whether the folder on show is in the Trash.
    pub trash: TrashView,
}

impl Facts {
    /// The facts of `env` and `selected`. `paste` asks the clipboard, which
    /// can take a moment, so only ask when the answer is used.
    pub(super) fn of(env: &Env, selected: &[Entry], paste: bool) -> Facts {
        let single = match selected {
            [entry] => Some(entry),
            _ => None,
        };
        Facts {
            caps: env.caps,
            in_folder: env.folder.is_some(),
            selected: selected.len(),
            folders: selected.iter().filter(|entry| entry.is_dir()).count(),
            can_paste: paste && can_paste(),
            bookmarked: single
                .is_some_and(|entry| entry.is_dir() && env.bookmarks.contains(&entry.path)),
            extractable: env.caps.modify
                && single.is_some_and(|entry| !entry.is_dir() && backend::can_extract(&entry.path)),
            can_compress: env.caps.modify
                && !selected.is_empty()
                && bingux_files::ops::Format::ALL.iter().any(|format| format.is_available()),
            shown: env.order.len(),
            undo: undo::next_label(),
            trash: TrashView::of(env.folder.as_deref()),
        }
    }
}

/// Whether the folder on show is in the Trash, and how far in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TrashView {
    /// An ordinary folder.
    No,
    /// The `files` folder of a bin (the Trash place): every entry is an item
    /// with an info file saying where it was and when it was deleted.
    Root,
    /// A folder inside something that was trashed.
    Inside,
}

impl TrashView {
    /// Where `folder` is, by its name alone.
    pub(super) fn of(folder: Option<&Path>) -> TrashView {
        match folder.and_then(|folder| backend::trash_place(folder)) {
            None => TrashView::No,
            Some(place) if place.is_root() => TrashView::Root,
            Some(_) => TrashView::Inside,
        }
    }

    /// Any part of the Trash is on show.
    pub(super) fn on(self) -> bool {
        self != TrashView::No
    }
}

/// What the Trash changes about the rules. Nothing is made, renamed, cut,
/// packed or bookmarked there (making says why it cannot); a paste into the
/// Trash itself is a move to the Trash, and into a folder of it is refused;
/// the only delete is the permanent one (it always asks); the rest follows
/// the ordinary rules (`None`).
fn in_trash(facts: &Facts, command: &Command) -> Option<Availability> {
    use Availability::{Disabled, Enabled, Hidden};
    Some(match command {
        Command::NewFolder | Command::NewFile | Command::NewFromTemplate(_) => {
            Disabled("Nothing can be made in the Trash.")
        }
        Command::Paste if facts.trash == TrashView::Root => {
            if facts.can_paste {
                Enabled
            } else {
                Disabled("There is nothing to paste.")
            }
        }
        Command::Paste => Disabled("Nothing can be put inside a folder in the Trash."),
        Command::Cut
        | Command::PasteInto(_)
        | Command::Rename
        | Command::Trash
        | Command::Compress
        | Command::CompressSelection
        | Command::CompressQuick(_)
        | Command::Extract
        | Command::ExtractTo
        | Command::AddBookmark
        | Command::RemoveBookmark => Hidden,
        Command::DeletePermanently => {
            if facts.selected > 0 {
                Enabled
            } else {
                Hidden
            }
        }
        _ => return None,
    })
}

const NO_FOLDER: &str = "Open a folder first.";

/// The rules: what is on offer for this summary of the selection.
pub(super) fn availability(facts: &Facts, command: &Command) -> Availability {
    use Availability::{Disabled, Enabled, Hidden};
    let caps = facts.caps;
    let some = facts.selected > 0;
    let one = facts.selected == 1;
    let all_folders = some && facts.folders == facts.selected;
    if command.changes_files() && !caps.modify {
        return Hidden;
    }
    if facts.trash.on()
        && let Some(state) = in_trash(facts, command)
    {
        return state;
    }
    match command {
        Command::Open => match facts.selected {
            0 => Hidden,
            1 => Enabled,
            _ if caps.open_with => Enabled,
            _ => Disabled("Select one item to open it."),
        },
        Command::Preview => {
            if some {
                Enabled
            } else {
                Hidden
            }
        }
        Command::OpenWith(_) => {
            if !caps.open_with || !some || facts.folders > 0 {
                Hidden
            } else if one {
                Enabled
            } else {
                Disabled("Select one file to choose how to open it.")
            }
        }
        Command::OpenInNewTab => {
            if !caps.new_tab || facts.folders == 0 {
                Hidden
            } else if all_folders {
                Enabled
            } else {
                Disabled("Only folders open in a new tab.")
            }
        }
        Command::Cut
        | Command::Trash
        | Command::DeletePermanently
        | Command::Copy
        | Command::CopyPath
        | Command::CopyUri => {
            if some {
                Enabled
            } else {
                Hidden
            }
        }
        Command::Paste => {
            if !facts.in_folder {
                Disabled(NO_FOLDER)
            } else if !facts.can_paste {
                Disabled("There is nothing to paste.")
            } else {
                Enabled
            }
        }
        Command::PasteInto(_) => {
            if facts.can_paste {
                Enabled
            } else {
                Disabled("There is nothing to paste.")
            }
        }
        Command::Rename => {
            if one {
                Enabled
            } else {
                Hidden
            }
        }
        Command::NewFolder | Command::NewFile | Command::NewFromTemplate(_) => {
            if facts.in_folder {
                Enabled
            } else {
                Disabled(NO_FOLDER)
            }
        }
        Command::AddBookmark => {
            if caps.bookmarks && one && all_folders && !facts.bookmarked {
                Enabled
            } else {
                Hidden
            }
        }
        Command::RemoveBookmark => {
            if caps.bookmarks && one && all_folders && facts.bookmarked {
                Enabled
            } else {
                Hidden
            }
        }
        Command::Compress => {
            if !some {
                Hidden
            } else if facts.can_compress {
                Enabled
            } else {
                Disabled("No archive tool is installed.")
            }
        }
        Command::CompressSelection => {
            if facts.can_compress {
                Enabled
            } else {
                Disabled("No archive tool is installed.")
            }
        }
        Command::CompressQuick(format) => {
            if !some {
                Hidden
            } else if format.is_available() {
                Enabled
            } else {
                Disabled("That archive tool is not installed.")
            }
        }
        Command::Extract | Command::ExtractTo => {
            if facts.extractable {
                Enabled
            } else {
                Hidden
            }
        }
        Command::Properties => {
            if !caps.properties {
                Hidden
            } else if some || facts.in_folder {
                Enabled
            } else {
                Disabled(NO_FOLDER)
            }
        }
        Command::SelectAll => {
            if facts.shown > 0 {
                Enabled
            } else {
                Disabled("There is nothing to select.")
            }
        }
        Command::InvertSelection => {
            if facts.shown > 0 {
                Enabled
            } else {
                Disabled("There is nothing to select.")
            }
        }
        Command::Refresh => Enabled,
        Command::EmptyTrash => {
            if !facts.trash.on() {
                Hidden
            } else if facts.shown == 0 {
                Disabled("The Trash is empty.")
            } else {
                Enabled
            }
        }
        // Only items of the Trash itself can be restored: what is inside a
        // folder that was trashed has no info file of its own.
        Command::Restore | Command::RestoreTo => {
            if facts.trash == TrashView::Root && some {
                Enabled
            } else {
                Hidden
            }
        }
        Command::Undo => {
            if facts.undo.is_some() {
                Enabled
            } else {
                Disabled("There is nothing to undo.")
            }
        }
        Command::MoveInto { .. } => Enabled,
    }
}

/// Whether `command` can be used on `selected` in `env`.
pub(super) fn available(env: &Env, selected: &[Entry], command: &Command) -> Availability {
    let paste = matches!(command, Command::Paste | Command::PasteInto(_));
    availability(&Facts::of(env, selected, paste), command)
}

/// Whether a paste would bring files: the held ones, or a file URI list on
/// the system clipboard.
pub(super) fn can_paste() -> bool {
    clipboard::has_files()
}

/// Show `text` in the status line for a few seconds.
pub(super) fn notify(ui: Ui, text: impl Into<String>, error: bool) {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    set(
        ui.notice,
        Some(Notice {
            text: text.into(),
            error,
            id,
        }),
    );
    spawn_in_window(async move {
        timer(NOTICE_TIME).await;
        let current = ui.notice.try_peek().and_then(|slot| (*slot).as_ref().map(|notice| notice.id));
        if current == Some(id) {
            set(ui.notice, None);
        }
    });
}

/// Write a state that a task may outlive: nothing happens if its owner is gone.
pub(super) fn set<T: 'static>(state: State<T>, value: T) {
    let mut state = state;
    if let Some(mut slot) = state.try_write() {
        *slot = value;
    }
}

fn paths_of(selected: &[Entry]) -> Vec<PathBuf> {
    selected.iter().map(|entry| entry.path.clone()).collect()
}

/// What to call `paths` in a sentence: the name, or the count.
pub(super) fn describe(paths: &[PathBuf]) -> String {
    match paths {
        [only] => format!(
            "“{}”",
            only.file_name()
                .map_or_else(|| only.to_string_lossy(), |name| name.to_string_lossy())
        ),
        many => format!("{} items", many.len()),
    }
}

/// Do `command` to `selected`.
pub(super) fn run(env: &Env, selected: &[Entry], command: Command) {
    if command.changes_files() && !env.caps.modify {
        return;
    }
    match command {
        Command::Open => open(env, selected),
        Command::Preview => env.preview.call(()),
        Command::OpenWith(app) => {
            if let [entry] = selected
                && let Err(error) = open_with::launch(&app, &entry.path)
            {
                notify(env.ui, format!("Could not start {}: {error}.", app.name), true);
            }
        }
        Command::OpenInNewTab => {
            for entry in selected.iter().filter(|entry| entry.is_dir()) {
                env.open_new_tab.call(entry.path.clone());
            }
        }
        Command::Cut => clipboard::put(paths_of(selected), true),
        Command::Copy => clipboard::put(paths_of(selected), false),
        Command::Paste => {
            if let Some(folder) = env.folder.clone() {
                transfer::paste(env, folder);
            }
        }
        Command::PasteInto(folder) => transfer::paste(env, folder),
        Command::Rename => {
            if let [entry] = selected {
                set(
                    env.ui.rename,
                    Some(Rename {
                        path: entry.path.clone(),
                        created: false,
                    }),
                );
            }
        }
        Command::NewFolder => create(env, true),
        Command::NewFile => create(env, false),
        Command::NewFromTemplate(template) => create_from_template(env, template),
        Command::Trash => bin::trash(env, paths_of(selected)),
        Command::DeletePermanently => bin::confirm_delete(env, paths_of(selected)),
        Command::Restore => bin::restore(env, paths_of(selected)),
        Command::RestoreTo => bin::restore_to(env, paths_of(selected)),
        Command::CopyPath => {
            let text = selected
                .iter()
                .map(|entry| entry.path.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("\n");
            copy_text(env, text, "Path copied.", "Paths copied.", selected.len());
        }
        Command::CopyUri => {
            let text = selected
                .iter()
                .map(|entry| file_uri(&entry.path))
                .collect::<Vec<_>>()
                .join("\n");
            copy_text(env, text, "Location copied.", "Locations copied.", selected.len());
        }
        Command::AddBookmark => bookmark(env, selected, true),
        Command::RemoveBookmark => bookmark(env, selected, false),
        Command::Compress => super::compress::show_compress(env, selected),
        Command::CompressSelection => {
            let held = held_entries(env);
            super::compress::show_compress(env, &held);
        }
        Command::CompressQuick(format) => super::compress_run::quick(env, selected, format),
        Command::Extract => {
            if let [entry] = selected {
                super::compress_run::extract_here(env, entry.path.clone());
            }
        }
        Command::ExtractTo => {
            if let [entry] = selected {
                super::extract_dialog::show_extract(env, entry.path.clone());
            }
        }
        Command::Properties => properties::show_properties(env, selected),
        Command::SelectAll => {
            let order = visible_paths(env);
            let order: Vec<&Path> = order.iter().map(PathBuf::as_path).collect();
            let mut selection = env.selection;
            selection.write().select_all(&order);
        }
        Command::InvertSelection => {
            let order = visible_paths(env);
            let order: Vec<&Path> = order.iter().map(PathBuf::as_path).collect();
            let mut selection = env.selection;
            let held: Vec<PathBuf> = selection.peek().paths().to_vec();
            let mut selection = selection.write();
            selection.select_all(&order);
            for path in &held {
                selection.toggle(path);
            }
        }
        Command::Refresh => env.reload.call(()),
        Command::Undo => transfer::undo(env),
        Command::EmptyTrash => bin::confirm_empty_trash(env),
        Command::MoveInto { paths, dest, copy } => transfer::move_into(env, paths, dest, copy),
    }
}

/// The entries of the listing that are selected now.
pub(super) fn held_entries(env: &Env) -> Vec<Entry> {
    let selection = env.selection.peek();
    env.listing
        .entries
        .iter()
        .filter(|entry| selection.contains(&entry.path))
        .cloned()
        .collect()
}

fn visible_paths(env: &Env) -> Vec<PathBuf> {
    env.order
        .iter()
        .filter_map(|&index| env.listing.entries.get(index))
        .map(|entry| entry.path.clone())
        .collect()
}

/// Enter on a selection: a folder opens in this view, a file as a double click.
fn open(env: &Env, selected: &[Entry]) {
    for entry in selected {
        let index = env
            .listing
            .entries
            .iter()
            .position(|candidate| candidate.path == entry.path);
        match (selected.len(), entry.is_dir(), index) {
            (1, true, _) => env.open_folder.call(entry.path.clone()),
            (_, _, Some(index)) => env.activate.call(index),
            (_, true, None) => env.open_folder.call(entry.path.clone()),
            _ => {}
        }
    }
}

fn copy_text(env: &Env, text: String, one: &str, many: &str, count: usize) {
    match freya::clipboard::Clipboard::set(text) {
        Ok(()) => notify(env.ui, if count == 1 { one } else { many }, false),
        Err(_) => notify(env.ui, "Could not reach the clipboard.", true),
    }
}

fn bookmark(env: &Env, selected: &[Entry], add: bool) {
    let Some(entry) = selected.first().filter(|entry| entry.is_dir()) else {
        return;
    };
    let path = entry.path.clone();
    let reload = env.bookmarks_changed.clone();
    let ui = env.ui;
    spawn_in_window(async move {
        let outcome = spawn_blocking(move || {
            if add {
                Places::add_bookmark(&path)
            } else {
                Places::remove_bookmark(&path)
            }
        })
        .await;
        match outcome {
            Ok(()) => reload.call(()),
            Err(error) => notify(ui, format!("Could not change the bookmarks: {error}."), true),
        }
    });
}

/// Make "Untitled folder" or "Untitled file" in the folder on show and start
/// naming it.
fn create(env: &Env, folder: bool) {
    let Some(parent) = env.folder.clone() else {
        return;
    };
    let env = env.clone();
    let base = if folder {
        "Untitled folder"
    } else {
        "Untitled file"
    };
    let mut taken: Vec<String> = env
        .listing
        .entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    spawn_in_window(async move {
        let outcome = spawn_blocking(move || {
            // Another program may have made the name since the folder was
            // listed, so try the next one when this one is taken.
            let mut name = names::unused_name(base, |name| taken.iter().any(|t| t == name));
            loop {
                let made = if folder {
                    backend::create_dir(&parent, &name)
                } else {
                    backend::create_file(&parent, &name)
                };
                match made {
                    Err(backend::OpError::Exists) => {
                        taken.push(name);
                        name = names::unused_name(base, |name| taken.iter().any(|t| t == name));
                    }
                    other => return other,
                }
            }
        })
        .await;
        match outcome {
            Ok(path) => {
                undo::push(undo::Step::Create {
                    path: path.clone(),
                    folder,
                });
                let mut selection = env.selection;
                selection.write().select_only(&path);
                set(
                    env.ui.rename,
                    Some(Rename {
                        path,
                        created: true,
                    }),
                );
                env.reload.call(());
            }
            Err(error) => notify(env.ui, format!("Could not create it: {error}"), true),
        }
    });
}

/// Make a copy of `template` in the folder on show, named after it, and start
/// naming it. A name that is taken gets `2`, `3`, and so on before its
/// extension, so `Letter.odt` becomes `Letter 2.odt`.
fn create_from_template(env: &Env, template: PathBuf) {
    let Some(parent) = env.folder.clone() else {
        return;
    };
    let Some(file_name) = template
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
    else {
        return;
    };
    let env = env.clone();
    let split = names::stem_chars(&file_name, false);
    let stem: String = file_name.chars().take(split).collect();
    let extension: String = file_name.chars().skip(split).collect();
    let mut taken: Vec<String> = env
        .listing
        .entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    spawn_in_window(async move {
        let outcome = spawn_blocking(move || {
            let pick = |taken: &[String]| {
                let stem = names::unused_name(&stem, |stem| {
                    taken.iter().any(|name| *name == format!("{stem}{extension}"))
                });
                format!("{stem}{extension}")
            };
            let mut name = pick(&taken);
            loop {
                match backend::create_file(&parent, &name) {
                    Err(backend::OpError::Exists) => {
                        taken.push(name);
                        name = pick(&taken);
                    }
                    Err(error) => return Err(error),
                    Ok(path) => {
                        return match std::fs::copy(&template, &path) {
                            Ok(_) => Ok(path),
                            Err(error) => {
                                let _ = std::fs::remove_file(&path);
                                Err(backend::OpError::from(error))
                            }
                        };
                    }
                }
            }
        })
        .await;
        match outcome {
            Ok(path) => {
                undo::push(undo::Step::Create {
                    path: path.clone(),
                    folder: false,
                });
                let mut selection = env.selection;
                selection.write().select_only(&path);
                set(
                    env.ui.rename,
                    Some(Rename {
                        path,
                        created: true,
                    }),
                );
                env.reload.call(());
            }
            Err(error) => notify(env.ui, format!("Could not create it: {error}"), true),
        }
    });
}

/// Rename `path` to `new_name` on a worker. The rename is remembered for
/// undo and the new name takes the selection if the old name had it; `done`
/// hears how it ended, with the reason as a sentence when the disk refused.
/// The inline field is the caller.
pub(super) fn rename(
    env: &Env,
    path: PathBuf,
    new_name: String,
    done: impl FnOnce(Result<(), String>) + 'static,
) {
    let env = env.clone();
    spawn_in_window(async move {
        let from = path.clone();
        let outcome = spawn_blocking(move || backend::rename(&path, &new_name)).await;
        match outcome {
            Ok(to) => {
                let mut selection = env.selection;
                // The name moves its selection with it. A click on another item
                // is what ended the edit when the selection is elsewhere now, and
                // that choice stands.
                if selection.peek().contains(&from) {
                    selection.write().select_only(&to);
                }
                undo::push(undo::Step::Rename {
                    from,
                    to: to.clone(),
                });
                env.reload.call(());
                done(Ok(()));
            }
            Err(error) => done(Err(error.to_string())),
        }
    });
}

/// The keys of a file view. Returns true when the key was a command that is
/// available, so the caller stops it going further. Select all (Ctrl+A),
/// Enter and the arrows belong to the list, the zoom keys to the zoom.
pub(super) fn handle_key(env: &Env, selected: &[Entry], event: &Event<KeyboardEventData>) -> bool {
    let modifiers = event.modifiers;
    let ctrl = modifiers.contains(Modifiers::CONTROL);
    let shift = modifiers.contains(Modifiers::SHIFT);
    let alt = modifiers.contains(Modifiers::ALT);
    let command = match &event.key {
        Key::Named(NamedKey::F2) if !ctrl && !alt => Command::Rename,
        Key::Named(NamedKey::F5) if !ctrl && !alt => Command::Refresh,
        Key::Named(NamedKey::Delete) if !ctrl && !alt => {
            // Inside the Trash a delete is always permanent (and asks): it is
            // the only delete there is, so no Shift is needed.
            if shift || TrashView::of(env.folder.as_deref()).on() {
                Command::DeletePermanently
            } else {
                Command::Trash
            }
        }
        Key::Named(NamedKey::Enter) if alt && !ctrl && !shift => Command::Properties,
        Key::Character(text) if ctrl && !alt => match text.to_ascii_lowercase().as_str() {
            "c" if shift => Command::CopyPath,
            "c" => Command::Copy,
            "x" if !shift => Command::Cut,
            "v" if !shift => Command::Paste,
            "z" if !shift => Command::Undo,
            "n" if shift => Command::NewFolder,
            "i" if !shift => Command::Properties,
            "a" if shift => Command::InvertSelection,
            "r" if !shift => Command::Refresh,
            "e" if shift => Command::ExtractTo,
            "e" => Command::Compress,
            _ => return false,
        },
        _ => return false,
    };
    if !available(env, selected, &command).is_enabled() {
        return false;
    }
    run(env, selected, command);
    true
}

/// The shortcut shown beside a command in a menu.
pub(super) fn shortcut(command: &Command) -> Option<&'static str> {
    Some(match command {
        Command::Open => "Enter",
        Command::Preview => "Space",
        Command::Cut => "Ctrl+X",
        Command::Copy => "Ctrl+C",
        Command::Paste | Command::PasteInto(_) => "Ctrl+V",
        Command::Rename => "F2",
        Command::NewFolder => "Ctrl+Shift+N",
        Command::Trash => "Delete",
        Command::DeletePermanently => "Shift+Delete",
        Command::CopyPath => "Ctrl+Shift+C",
        Command::Properties => "Alt+Enter",
        Command::Compress => "Ctrl+E",
        Command::ExtractTo => "Ctrl+Shift+E",
        Command::SelectAll => "Ctrl+A",
        Command::InvertSelection => "Ctrl+Shift+A",
        Command::Refresh => "F5",
        Command::Undo => "Ctrl+Z",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(caps: Capabilities, selected: usize, folders: usize) -> Facts {
        Facts {
            caps,
            in_folder: true,
            selected,
            folders,
            can_paste: true,
            bookmarked: false,
            extractable: false,
            can_compress: true,
            shown: 10,
            undo: None,
            trash: TrashView::No,
        }
    }

    fn state(facts: &Facts, command: Command) -> Availability {
        availability(facts, &command)
    }

    #[test]
    fn nothing_selected() {
        let empty = facts(Capabilities::FILES_APP, 0, 0);
        assert_eq!(state(&empty, Command::Open), Availability::Hidden);
        assert_eq!(state(&empty, Command::Copy), Availability::Hidden);
        assert_eq!(state(&empty, Command::Rename), Availability::Hidden);
        assert_eq!(state(&empty, Command::Trash), Availability::Hidden);
        assert!(state(&empty, Command::NewFolder).is_enabled());
        assert!(state(&empty, Command::Paste).is_enabled());
        assert!(state(&empty, Command::SelectAll).is_enabled());
        assert!(state(&empty, Command::Properties).is_enabled());
    }

    #[test]
    fn one_file() {
        let file = facts(Capabilities::FILES_APP, 1, 0);
        assert!(state(&file, Command::Open).is_enabled());
        assert!(state(&file, Command::Rename).is_enabled());
        assert!(state(&file, Command::Trash).is_enabled());
        assert!(state(&file, Command::Compress).is_enabled());
        assert!(
            state(&file, Command::OpenWith(App { name: String::new(), exec: String::new(), icon: String::new(), mimes: vec![] }))
                .is_enabled()
        );
        assert_eq!(state(&file, Command::OpenInNewTab), Availability::Hidden);
        assert_eq!(state(&file, Command::AddBookmark), Availability::Hidden);
    }

    #[test]
    fn one_folder() {
        let mut folder = facts(Capabilities::FILES_APP, 1, 1);
        assert!(state(&folder, Command::OpenInNewTab).is_enabled());
        assert!(state(&folder, Command::AddBookmark).is_enabled());
        assert_eq!(state(&folder, Command::RemoveBookmark), Availability::Hidden);
        folder.bookmarked = true;
        assert_eq!(state(&folder, Command::AddBookmark), Availability::Hidden);
        assert!(state(&folder, Command::RemoveBookmark).is_enabled());
        assert!(state(&folder, Command::PasteInto(PathBuf::from("/x"))).is_enabled());
        assert_eq!(
            state(
                &folder,
                Command::OpenWith(App { name: String::new(), exec: String::new(), icon: String::new(), mimes: vec![] })
            ),
            Availability::Hidden
        );
    }

    #[test]
    fn several_items() {
        let many = facts(Capabilities::FILES_APP, 3, 1);
        assert_eq!(state(&many, Command::Rename), Availability::Hidden);
        assert!(state(&many, Command::Trash).is_enabled());
        assert!(state(&many, Command::Copy).is_enabled());
        assert!(matches!(
            state(&many, Command::OpenInNewTab),
            Availability::Disabled(_)
        ));
        assert_eq!(state(&many, Command::AddBookmark), Availability::Hidden);
        let chooser = facts(Capabilities::CHOOSER, 3, 0);
        assert!(matches!(
            state(&chooser, Command::Open),
            Availability::Disabled(_)
        ));
    }

    #[test]
    fn chooser_cannot_leave_the_dialog() {
        let file = facts(Capabilities::CHOOSER, 1, 0);
        let app = App { name: String::new(), exec: String::new(), icon: String::new(), mimes: vec![] };
        assert_eq!(state(&file, Command::OpenWith(app)), Availability::Hidden);
        let folder = facts(Capabilities::CHOOSER, 1, 1);
        assert_eq!(state(&folder, Command::OpenInNewTab), Availability::Hidden);
        assert!(state(&folder, Command::Trash).is_enabled());
        assert!(state(&folder, Command::AddBookmark).is_enabled());
    }

    #[test]
    fn a_read_only_view_hides_every_change() {
        let caps = Capabilities {
            modify: false,
            ..Capabilities::FILES_APP
        };
        let file = facts(caps, 1, 0);
        for command in [
            Command::Cut,
            Command::Rename,
            Command::Trash,
            Command::DeletePermanently,
            Command::NewFolder,
            Command::NewFile,
            Command::Paste,
            Command::Compress,
            Command::Undo,
        ] {
            assert_eq!(state(&file, command.clone()), Availability::Hidden, "{command:?}");
        }
        assert!(state(&file, Command::Copy).is_enabled());
        assert!(state(&file, Command::Properties).is_enabled());
    }

    #[test]
    fn paste_and_create_need_a_folder_and_a_clipboard() {
        let mut recent = facts(Capabilities::FILES_APP, 0, 0);
        recent.in_folder = false;
        assert!(matches!(state(&recent, Command::NewFolder), Availability::Disabled(_)));
        assert!(matches!(state(&recent, Command::Paste), Availability::Disabled(_)));
        assert!(matches!(state(&recent, Command::Properties), Availability::Disabled(_)));
        let mut empty_clipboard = facts(Capabilities::FILES_APP, 0, 0);
        empty_clipboard.can_paste = false;
        assert!(matches!(state(&empty_clipboard, Command::Paste), Availability::Disabled(_)));
    }

    #[test]
    fn undo_and_extract_follow_their_facts() {
        let mut file = facts(Capabilities::FILES_APP, 1, 0);
        assert!(matches!(state(&file, Command::Undo), Availability::Disabled(_)));
        assert_eq!(state(&file, Command::Extract), Availability::Hidden);
        file.undo = Some("Undo rename");
        file.extractable = true;
        assert!(state(&file, Command::Undo).is_enabled());
        assert!(state(&file, Command::Extract).is_enabled());
        file.can_compress = false;
        assert!(matches!(state(&file, Command::Compress), Availability::Disabled(_)));
    }

    #[test]
    fn describe_names_one_item_and_counts_many() {
        assert_eq!(describe(&[PathBuf::from("/a/b.txt")]), "“b.txt”");
        assert_eq!(
            describe(&[PathBuf::from("/a"), PathBuf::from("/b")]),
            "2 items"
        );
    }
}
