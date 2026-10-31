//! The file chooser dialog.
//!
//! [`choose`] opens a popup over a window. On a desktop window it is a panel
//! with the places sidebar beside the file area; on a phone it is a sheet
//! with a places menu above the file area. The header carries Cancel and the
//! accept button (Open, Select or Save), as every popup does.
//!
//! Keyboard: Tab walks the toolbar, the list and the footer. In the list the
//! arrows, Home, End and Page keys move; Enter opens. Typing a name, or
//! Ctrl+F, searches the folder in the address bar, which is also the search
//! box (see [`search`](super::search): Ctrl+Enter searches the subfolders,
//! Escape ends the search). Ctrl+L, or typing `/` or `~` in the list, starts
//! typing a path. Alt+Up goes to the parent folder, Alt+Left back, Ctrl+H shows or
//! hides hidden files. Escape cancels.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use bingux_files::find::{Hidden, Invalid, Matcher, Query, Recents, Scope, SortBy};
use bingux_files::request::accept;
use bingux_files::{
    Accept, Action, Choice, ChooserRequest, Chosen, Entry, Filter, Listing, Selection, Sort,
    SortKey, View, ViewMode, location,
};
use freya::prelude::*;

use super::actions::{Capabilities, open_default};
use super::address_bar::AddressBar;
use super::browser::Browse;
use super::directory::{Directory, Load, Reload, Where, home_folder, use_directory};
use super::folder_table::{folder_table, location_of, place_of};
use super::history;
use super::places_view::{PlacesMenu, PlacesSidebar};
use super::reload_button::reload_button;
use super::scroll_memory::measured;
use super::search::{
    Deep, DeepResults, DeepStatus, Find, Marks, RunKey, SearchView, step_selection, use_search_run,
};
use super::search_bar::{
    BarStatus, SearchBar, place_phrase, recents_file, scope_choices, token_label,
};
use super::skeleton::skeleton;
use super::states::{empty_state, error_state, no_results_state, stopped_state};
use super::view::FileView;
use super::view_state;
use super::zoom::{
    REQUESTED_GRID_PX, ViewKind, default_zoom_px, kind_of, nearest_level, switch_kind,
};
use crate::component::ComponentName;
use crate::controls::{SplitButton, checkbox, dropdown, icon_button, text_field};
use crate::group::{Status, status};
use crate::icons::Icon;
use crate::layout::divider;
use crate::motion::{Dwell, use_dwell};
use crate::nav::{HistoryItem, NavModel, Outcome};
use crate::shell::{
    CANCEL_ID, MenuItem, Overlays, ResponseKind, dialog, history_menu_with, menu_item,
    menu_separator, use_overlays,
};
use crate::style::*;
use crate::surface::use_tokens;
use crate::task::{spawn_blocking, use_resource};
use crate::typography::BODY;
use crate::window::{
    PopupAction, PopupActions, PopupFlags, PopupWindow, Window, WindowLike, WindowParent, popup,
    use_popup_actions, use_window,
};

/// Open the chooser for `request` over `window` and return its popup.
///
/// `on_done` is called once, on the UI thread: with `Some(chosen)` when the
/// person accepts, with `None` when they cancel by any route (Cancel, Escape,
/// pulling the sheet down, or the window closing). It never runs twice.
///
/// Modal; the person cannot touch the window under it until it closes.
pub fn choose(
    window: impl WindowParent,
    request: ChooserRequest,
    on_done: impl FnOnce(Option<Chosen>) + 'static,
) -> PopupWindow {
    let title = request.title.clone();
    let done = Done(Rc::new(RefCell::new(Some(Box::new(on_done)))));
    // A phone's sheet takes as much height as it may; the popup clamps both.
    let height = if is_compact() {
        4000.0
    } else {
        Dim::ChooserHeight.px()
    };
    window.open(
        popup(
            title,
            ChooserBody {
                request,
                host: Host::Dialog(done),
            },
        )
        .size(Dim::ChooserWidth.px(), height)
        .flags(PopupFlags::FORM | PopupFlags::FILL | PopupFlags::NO_DISMISS_OUTSIDE),
    )
}

type DoneCall = Box<dyn FnOnce(Option<Chosen>)>;

/// The caller's callback, taken out the first time an answer is given.
#[derive(Clone)]
struct Done(Rc<RefCell<Option<DoneCall>>>);

impl PartialEq for Done {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Done {
    fn finish(&self, outcome: Option<Chosen>) {
        let call = self.0.borrow_mut().take();
        if let Some(call) = call {
            call(outcome);
        }
    }
}

/// What the body is part of: a dialog that answers a request, or a window
/// that browses. The body is one component for both, so every fix to the
/// list, the address bar or the sidebar reaches both.
#[derive(Clone, PartialEq)]
enum Host {
    Dialog(Done),
    Browser(Browse),
}

impl Host {
    /// The request a browsing window stands in for: any file or folder may
    /// be selected, and nothing is asked.
    fn request() -> ChooserRequest {
        ChooserRequest::open().multiple(true)
    }
}

/// The chooser's body as a window's content; see [`super::browse`].
pub(super) fn browse_body(browse: Browse) -> Element {
    ChooserBody {
        request: Host::request(),
        host: Host::Browser(browse),
    }
    .into()
}

/// The places of this system. The key is the bookmark count, so changing a
/// bookmark loads them again.
fn load_places(_: u32) -> Result<bingux_files::Places, String> {
    Ok(bingux_files::Places::load_without_devices())
}

/// Moving between folders. All `State`s, so it is `Copy`.
#[derive(Clone, Copy)]
struct Nav {
    /// The folder shown and the history around it: one place in a linear
    /// stack on [`folder_table`], so Back, Forward and their menus are the
    /// navigation system's.
    model: State<NavModel>,
    selection: State<Selection>,
    editing: State<bool>,
    /// The search; going anywhere ends it.
    find: State<Find>,
    /// The words of the search field.
    words: State<String>,
    problem: State<Option<String>>,
    sort: State<Sort>,
    /// The icon size of the folder shown: its remembered one, or `fallback`.
    level: State<u16>,
    /// What a folder with nothing remembered opens at.
    fallback: u16,
}

impl Nav {
    /// The place shown, without subscribing to it.
    fn here(&self) -> Where {
        place_of(self.model.peek().location())
    }

    /// The place shown; the caller re-renders when it changes.
    fn current(&self) -> Where {
        place_of(self.model.read().location())
    }

    /// Everything about a place that follows the person there.
    fn arrive(mut self, at: Where) {
        // The recent files read newest first; a folder reads by name.
        let recent = matches!(at, Where::Recent);
        // A folder the person has shown a way before shows that way again.
        let remembered = match &at {
            Where::Folder(path) => view_state::folder(path),
            Where::Recent => None,
        };
        match remembered {
            Some(view) => {
                self.sort.set(view.sort.into());
                self.level.set(nearest_level(f32::from(view.icon_px)));
            }
            None => {
                self.level.set(self.fallback);
                self.sort.set(if recent {
                    Sort {
                        key: SortKey::Modified,
                        descending: true,
                        folders_first: true,
                    }
                } else {
                    Sort::default()
                });
            }
        }
        self.selection.write().clear();
        self.editing.set(false);
        self.find.write().clear();
        self.words.set(String::new());
        self.problem.set(None);
    }

    /// Run `op` on a copy of the history; when it moved, keep the copy and
    /// arrive at the new place. A move to where the view already is changes
    /// nothing, not even a re-render.
    fn apply(mut self, op: impl FnOnce(&mut NavModel) -> Outcome) {
        let mut model = self.model.peek().clone();
        if let Outcome::Changed(change) = op(&mut model) {
            self.model.set(model);
            self.arrive(place_of(&change.to.location));
        }
    }

    fn go(self, to: Where) {
        self.apply(move |model| history::visit(model, &to));
    }

    fn back(self) {
        self.apply(NavModel::back);
    }

    fn forward(self) {
        self.apply(NavModel::forward);
    }

    /// A choice of the Back menu: that entry, however many levels back.
    fn back_to(self, entry: u64) {
        self.apply(move |model| model.pop_to(entry));
    }

    /// A choice of the Forward menu.
    fn forward_to(self, entry: u64) {
        self.apply(move |model| model.forward_to(entry));
    }

    /// Keep how this folder is sorted, so it opens that way next time. The
    /// recent files have no folder to remember it in. The icon size is the
    /// folder's own, kept by the view when the person zooms.
    fn remember_sort(self, sort: Sort) {
        let Where::Folder(path) = self.here() else {
            return;
        };
        let icon_px = view_state::folder(&path).map_or(self.fallback, |view| view.icon_px);
        view_state::remember_folder(
            &path,
            view_state::FolderView {
                icon_px,
                sort: sort.into(),
            },
        );
    }

    /// Keep the icon size the toolbar chose for this folder. (The view keeps
    /// the ones the person zooms to.)
    fn remember_level(self, icon_px: u16) {
        let Where::Folder(path) = self.here() else {
            return;
        };
        let sort = view_state::folder(&path).map_or(*self.sort.peek(), |view| view.sort.into());
        view_state::remember_folder(
            &path,
            view_state::FolderView {
                icon_px,
                sort: sort.into(),
            },
        );
    }

    fn up(self) {
        self.apply(history::up);
    }
}

/// Everything accepting needs, cloned into closures.
#[derive(Clone)]
struct Session {
    request: Rc<ChooserRequest>,
    /// The filters the dropdown offers: the request's, or the presets.
    filters: Rc<Vec<Filter>>,
    home: PathBuf,
    nav: Nav,
    directory: Directory,
    reload: Reload,
    name: State<String>,
    filter: State<Option<usize>>,
    find: State<Find>,
    /// The words of the search field.
    words: State<String>,
    /// The search of folders; its results replace the folder's listing while
    /// they are on show.
    deep: Deep,
    /// The listing the rows on show index into, as this render computed it.
    shown: Option<Arc<Listing>>,
    /// Bumped to give the keyboard back to the list.
    focus: State<u32>,
    /// Counts changes to the bookmarks, so the places load again.
    places_rev: State<u32>,
    choices: State<Vec<(String, String)>>,
    /// The toolbar's Stop state is showing (see [`Dwell::LOADING`]): the
    /// folder is being read or a subfolder search runs, and has been for a
    /// moment, or did a moment ago. Escape stops the read while this is set.
    stopping: bool,
    window: Window,
    overlays: Overlays,
    host: Host,
}

impl Session {
    /// The folder shown, or home when the recent files are.
    fn folder(&self) -> PathBuf {
        match self.nav.here() {
            Where::Folder(path) => path,
            Where::Recent => self.home.clone(),
        }
    }

    /// The listing the rows on show index into: the search's results while
    /// they are shown, else the folder's.
    fn listing(&self) -> Option<Arc<Listing>> {
        if let Some(shown) = &self.shown {
            return Some(shown.clone());
        }
        match self.directory.peek(&self.nav.here()) {
            Load::Ready(listing) => Some(listing),
            _ => None,
        }
    }

    // ---- search -----------------------------------------------------

    /// A letter typed on the list, or Ctrl+F (`typed` empty): open the search
    /// bar and give it the keyboard.
    fn start_search(&self, typed: &str) {
        let mut find = self.find;
        let mut words = self.words;
        let selection = self.nav.selection.peek().clone();
        let was = find.peek().open;
        {
            let mut now = find.write();
            now.begin(&selection);
            // A field that is already there takes the keyboard back.
            now.focus += 1;
        }
        if !typed.is_empty() {
            // Two letters typed before the field has the keyboard join up.
            let text = if was {
                format!("{}{}", words.peek(), typed)
            } else {
                typed.to_string()
            };
            words.set(text);
        }
    }

    /// Escape, the field's close: show the whole folder again with the
    /// selection the person had, and give the keyboard back to the list.
    fn clear_search(&self) {
        let mut find = self.find;
        let mut words = self.words;
        let restored = find.write().clear();
        words.set(String::new());
        if let Some(before) = restored {
            let mut selection = self.nav.selection;
            selection.set(before);
        }
        let mut focus = self.focus;
        let next = *focus.peek() + 1;
        focus.set(next);
    }

    /// Open the selected result of the listing on show, as Enter does on its
    /// row: the first one when none is selected.
    fn open_match(&self, listing: &Listing, order: &[usize]) {
        let Some(first) = order.first().copied() else {
            return;
        };
        let cursor = self.nav.selection.peek().cursor().map(PathBuf::from);
        let at = cursor
            .and_then(|path| {
                order
                    .iter()
                    .copied()
                    .find(|index| listing.entries[*index].path == path)
            })
            .unwrap_or(first);
        self.activate(at);
    }

    /// Down in the field: the first result is selected (or the next one when
    /// a result is) and the keyboard goes to the list.
    fn enter_results(&self, listing: &Listing, order: &[usize]) {
        let Some(first) = order.first().copied() else {
            return;
        };
        let mut selection = self.nav.selection;
        let on_a_result = selection.peek().cursor().is_some_and(|path| {
            order
                .iter()
                .any(|index| listing.entries[*index].path == path)
        });
        if on_a_result {
            self.step_match(listing, order, 1);
        } else {
            selection.write().select_only(&listing.entries[first].path);
        }
        let mut focus = self.focus;
        let next = *focus.peek() + 1;
        focus.set(next);
    }

    /// Up in the search field: move the selected match.
    fn step_match(&self, listing: &Listing, order: &[usize], by: i32) {
        let paths: Vec<&std::path::Path> = order
            .iter()
            .map(|index| listing.entries[*index].path.as_path())
            .collect();
        let mut selection = self.nav.selection;
        step_selection(&mut selection.write(), &paths, by);
    }

    fn selected_entries(&self) -> Vec<Entry> {
        let Some(listing) = self.listing() else {
            return Vec::new();
        };
        let selection = self.nav.selection.peek();
        selection
            .paths()
            .iter()
            .filter_map(|path| listing.entries.iter().find(|entry| entry.path == *path))
            .cloned()
            .collect()
    }

    /// Answer the request. A browsing window has none to answer.
    fn finish(&self, paths: Vec<PathBuf>) {
        let Host::Dialog(done) = &self.host else {
            return;
        };
        done.finish(Some(Chosen {
            paths,
            current_filter: *self.filter.peek(),
            choices: self.choices.peek().clone(),
        }));
        self.window.close();
    }

    /// A file was chosen: a dialog answers with it; a browsing window opens
    /// it in the application the system names for its type.
    fn open_file(&self, path: PathBuf) {
        match &self.host {
            Host::Dialog(_) => self.finish(vec![path]),
            Host::Browser(_) => {
                let mut problem = self.nav.problem;
                if let Err(error) = open_default(&path) {
                    problem.set(Some(format!("Could not open the file: {error}.")));
                }
            }
        }
    }

    /// The accept button, Enter in the name field, or a double click.
    fn accept(&self) {
        let mut problem = self.nav.problem;
        let name = self.name.peek().clone();
        let outcome = accept(
            &self.request,
            &self.folder(),
            &self.home,
            &self.selected_entries(),
            &name,
        );
        match outcome {
            Accept::Done(paths) => self.finish(paths),
            Accept::Enter(path) => self.nav.go(Where::Folder(path)),
            Accept::Invalid(message) => problem.set(Some(message)),
            Accept::Confirm(existing) => self.confirm_replace(existing),
        }
    }

    fn confirm_replace(&self, existing: Vec<PathBuf>) {
        let title = if existing.len() == 1 {
            "Replace file?".to_string()
        } else {
            "Replace files?".to_string()
        };
        let body = match existing.as_slice() {
            [only] => format!(
                "A file called “{}” already exists in this folder. Replacing it overwrites its contents.",
                only.file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
            ),
            many => format!(
                "{} files with these names already exist in this folder. Replacing them overwrites their contents.",
                many.len()
            ),
        };
        let session = self.clone();
        let paths = match self.request.action {
            Action::Save | Action::SaveFiles => {
                // The paths that would be written, not only the ones that clash.
                let again =
                    accept_all_paths(&self.request, &self.folder(), &self.home, &self.name.peek());
                if again.is_empty() { existing } else { again }
            }
            _ => existing,
        };
        self.overlays.show_dialog(
            dialog(title, body)
                .cancel("Cancel")
                .default_action(CANCEL_ID)
                .response("Replace", ResponseKind::Destructive, move |_: ()| {
                    session.finish(paths.clone())
                }),
        );
    }

    /// A row was opened by double click, tap or Enter.
    fn activate(&self, entry_index: usize) {
        let Some(listing) = self.listing() else {
            return;
        };
        let Some(entry) = listing.entries.get(entry_index) else {
            return;
        };
        if entry.is_dir() {
            self.nav.go(Where::Folder(entry.path.clone()));
            return;
        }
        let mut selection = self.nav.selection;
        let several = selection.peek().paths().len() > 1 && selection.peek().contains(&entry.path);
        if !several {
            selection.write().select_only(&entry.path);
        }
        if self.request.action == Action::Save {
            let mut name = self.name;
            name.set(entry.name.clone());
        }
        if matches!(self.host, Host::Browser(_)) {
            self.open_file(entry.path.clone());
            return;
        }
        self.accept();
    }

    /// A row was picked: when saving, its name goes in the name field.
    fn picked(&self, entry_index: usize) {
        let mut problem = self.nav.problem;
        problem.set(None);
        if self.request.action != Action::Save {
            return;
        }
        let mut name = self.name;
        if let Some(listing) = self.listing()
            && let Some(entry) = listing.entries.get(entry_index)
            && !entry.is_dir()
        {
            name.set(entry.name.clone());
        }
    }

    /// Whether the accept button is ready. It reads (not peeks) the state it
    /// depends on, so the button follows the selection and the name.
    fn can_accept(&self) -> bool {
        match self.request.action {
            Action::Open => !self.nav.selection.read().is_empty(),
            Action::SelectFolder | Action::SaveFiles => {
                matches!(self.nav.current(), Where::Folder(_))
            }
            Action::Save => !self.name.read().trim().is_empty(),
        }
    }
}

/// The paths a save would write, for the replace dialog to hand back.
fn accept_all_paths(
    request: &ChooserRequest,
    folder: &std::path::Path,
    home: &std::path::Path,
    name: &str,
) -> Vec<PathBuf> {
    match request.action {
        Action::Save => vec![location::resolve(name.trim(), folder, home)],
        Action::SaveFiles => request.files.iter().map(|file| folder.join(file)).collect(),
        _ => Vec::new(),
    }
}

/// What the list shows: the listing (the folder's, or the subfolder
/// search's matches), the order of its rows, and the counts around them.
#[derive(Clone)]
struct Shown {
    listing: Arc<Listing>,
    order: Arc<Vec<usize>>,
    /// Entries a search chooses among: what hidden files and the filter let
    /// through.
    of: usize,
    results: Option<Arc<DeepResults>>,
    /// What marks names and ranks rows, when a search narrows the list.
    matcher: Option<Arc<Matcher>>,
    /// The search cannot run: what is wrong with it.
    invalid: Option<String>,
}

/// What the states of one tab hold when another tab is shown: where it is and
/// its history, what is selected, how it is sorted and zoomed, whether hidden
/// files and the preview pane show, and the path or search being typed. All
/// `State`s, so it is `Copy`.
#[derive(Clone, Copy)]
struct TabStates {
    model: State<NavModel>,
    selection: State<Selection>,
    sort: State<Sort>,
    level: State<u16>,
    show_hidden: State<bool>,
    preview_pane: State<bool>,
    editing: State<bool>,
    draft: State<String>,
    find: State<Find>,
    words: State<String>,
    problem: State<Option<String>>,
}

/// What a tab's states hold, taken out of them, for a tab that moves to
/// another window ([`TabMemory::depart`]).
struct Carried {
    selection: Selection,
    sort: Sort,
    level: u16,
    show_hidden: bool,
    preview_pane: bool,
    editing: bool,
    draft: String,
    find: Find,
    words: String,
    problem: Option<String>,
}

impl TabStates {
    /// The states of a tab that starts where `history` is: with the sort and
    /// the icon size its folder was last shown with, or `fallback`; or as the
    /// window the tab came from left them, when there is a `carried` snapshot.
    /// A `select`ed file (one named on the command line) starts selected.
    fn new(
        history: NavModel,
        fallback: u16,
        carried: Option<Carried>,
        select: Option<PathBuf>,
    ) -> Self {
        if let Some(carried) = carried {
            return TabStates {
                model: State::create(history),
                selection: State::create(carried.selection),
                sort: State::create(carried.sort),
                level: State::create(carried.level),
                show_hidden: State::create(carried.show_hidden),
                preview_pane: State::create(carried.preview_pane),
                editing: State::create(carried.editing),
                draft: State::create(carried.draft),
                find: State::create(carried.find),
                words: State::create(carried.words),
                problem: State::create(carried.problem),
            };
        }
        let remembered = match place_of(history.location()) {
            Where::Folder(path) => view_state::folder(&path),
            Where::Recent => None,
        };
        let mut selection = Selection::new();
        if let Some(file) = select {
            selection.select_only(&file);
        }
        TabStates {
            model: State::create(history),
            selection: State::create(selection),
            sort: State::create(remembered.map_or_else(Sort::default, |view| view.sort.into())),
            level: State::create(
                remembered.map_or(fallback, |view| nearest_level(f32::from(view.icon_px))),
            ),
            show_hidden: State::create(false),
            preview_pane: State::create(false),
            editing: State::create(false),
            draft: State::create(String::new()),
            find: State::create(Find::default()),
            words: State::create(String::new()),
            problem: State::create(None),
        }
    }

    /// What these states hold now.
    fn carry(self) -> Carried {
        Carried {
            selection: self.selection.peek().clone(),
            sort: *self.sort.peek(),
            level: *self.level.peek(),
            show_hidden: *self.show_hidden.peek(),
            preview_pane: *self.preview_pane.peek(),
            editing: *self.editing.peek(),
            draft: self.draft.peek().clone(),
            find: self.find.peek().clone(),
            words: self.words.peek().clone(),
            problem: self.problem.peek().clone(),
        }
    }

    /// Let go of what a closed tab selected and searched for.
    fn clear(self) {
        let (mut selection, mut find, mut draft, mut words, mut problem) = (
            self.selection,
            self.find,
            self.draft,
            self.words,
            self.problem,
        );
        selection.set(Selection::new());
        find.set(Find::default());
        draft.set(String::new());
        words.set(String::new());
        problem.set(None);
    }
}

/// The [`TabStates`] of every tab the body has shown. A switch of tabs is a
/// change of which ones the body reads, in the pass that renders the new tab:
/// nothing is written, nothing is mounted again, and the first frame is the
/// tab as it was left.
pub(super) struct TabBank<T> {
    /// Tab id, what the tab keeps, and a handle that says whether the host
    /// still has the tab.
    tabs: Vec<(u64, T, Option<Weak<()>>)>,
}

impl<T> Default for TabBank<T> {
    fn default() -> Self {
        TabBank { tabs: Vec::new() }
    }
}

impl<T: Copy> TabBank<T> {
    /// What `tab` keeps, made by `create` the first time it is shown.
    pub(super) fn states(
        &mut self,
        tab: u64,
        alive: Option<Weak<()>>,
        create: impl FnOnce() -> T,
    ) -> T {
        if let Some((_, states, _)) = self.tabs.iter().find(|(id, _, _)| *id == tab) {
            return *states;
        }
        let states = create();
        self.tabs.push((tab, states, alive));
        states
    }

    /// What the tabs the host has closed since kept, taken out of the bank.
    pub(super) fn closed(&mut self) -> Vec<T> {
        let mut closed = Vec::new();
        self.tabs.retain(|(_, states, alive)| {
            let gone = alive
                .as_ref()
                .is_some_and(|alive| alive.strong_count() == 0);
            if gone {
                closed.push(*states);
            }
            !gone
        });
        closed
    }
}

/// What [`select_shown`] was given. The same inputs give the same rows, so a
/// render that has them again keeps what it computed.
struct ShownKey {
    listing: Arc<Listing>,
    results: Option<Arc<DeepResults>>,
    filter: Option<usize>,
    show_hidden: bool,
    sort: Sort,
    query: Option<Query>,
}

impl ShownKey {
    fn same(&self, other: &ShownKey) -> bool {
        Arc::ptr_eq(&self.listing, &other.listing)
            && match (&self.results, &other.results) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && self.filter == other.filter
            && self.show_hidden == other.show_hidden
            && self.sort == other.sort
            && self.query == other.query
    }
}

/// The folder sort a search result order stands for.
fn results_sort(by: SortBy) -> Option<Sort> {
    let key = match by {
        SortBy::Relevance => return None,
        SortBy::Name => SortKey::Name,
        SortBy::Modified => SortKey::Modified,
        SortBy::Size => SortKey::Size,
    };
    Some(Sort {
        key,
        // Newest and largest first; names read A to Z.
        descending: key != SortKey::Name,
        folders_first: false,
    })
}

/// The rows of `listing` as the view shows them: hidden files, the filter and
/// the sort applied, then, when a search narrows the list, only what it
/// matches, in the order it asks for. `walking` says the listing is the
/// results of a search of folders, which have been through the search's own
/// filters already.
fn select_shown(
    listing: Arc<Listing>,
    results: Option<Arc<DeepResults>>,
    query: Option<&Query>,
    walking: bool,
    filter: Option<&Filter>,
    show_hidden: bool,
    sort: Sort,
) -> Shown {
    let by = query.map_or(SortBy::Relevance, |query| query.filters.sort);
    let matcher = query.map(Matcher::new);
    if query.is_some() && walking {
        // Results: arrival order while the search runs, so no row moves, then
        // the order asked for.
        let running = results.as_ref().is_some_and(|results| results.running);
        let shown_matcher = matcher
            .as_ref()
            .and_then(|matcher| matcher.as_ref().ok())
            .cloned();
        let identity: Vec<usize> = (0..listing.entries.len()).collect();
        let order = if running {
            identity
        } else {
            match (results_sort(by), &shown_matcher) {
                (Some(sort), _) => {
                    listing
                        .select_counted(&View {
                            show_hidden: true,
                            filter: None,
                            directories_only: false,
                            sort,
                        })
                        .order
                }
                (None, Some(matcher)) => {
                    let mut order = identity;
                    order.sort_by_cached_key(|index| {
                        matcher.rank(&listing.entries[*index], &listing.path)
                    });
                    order
                }
                (None, None) => identity,
            }
        };
        return Shown {
            of: listing.entries.len(),
            listing,
            order: Arc::new(order),
            results,
            matcher: shown_matcher.map(Arc::new),
            invalid: results_invalid(&matcher),
        };
    }
    // The folder on show, narrowed in memory.
    let hidden = match query.map(|query| query.filters.hidden) {
        Some(Hidden::Include | Hidden::Only) => true,
        Some(Hidden::Exclude) => false,
        _ => show_hidden,
    };
    let shown = listing.select_counted(&View {
        // Everything in the Trash is shown, dot files included: a deleted
        // `.bashrc` is an item to restore, not a hidden one.
        show_hidden: hidden
            || (!listing.recent
                && bingux_files::ops::trash_place(&listing.path)
                    .is_some_and(|place| place.is_root())),
        filter,
        directories_only: false,
        sort: results_sort(by).unwrap_or(sort),
    });
    // What the search chooses among: what the hidden-file setting and the file
    // type filter let through.
    let of = shown.order.len();
    let (order, matcher, invalid) = match matcher {
        Some(Ok(matcher)) => {
            let mut order: Vec<usize> = shown
                .order
                .iter()
                .copied()
                .filter(|index| matcher.matches(&listing.entries[*index]))
                .collect();
            if by == SortBy::Relevance {
                // Stable: inside a rank the folder's own sort stands.
                order.sort_by_cached_key(|index| {
                    matcher.rank(&listing.entries[*index], &listing.path)
                });
            }
            (order, Some(Arc::new(matcher)), None)
        }
        Some(Err(invalid)) => (Vec::new(), None, Some(invalid.reason)),
        None => (shown.order, None, None),
    };
    Shown {
        listing,
        order: Arc::new(order),
        of,
        results,
        matcher,
        invalid,
    }
}

/// A press on a column header while a search is on: the results are ordered
/// by that column; a press on the column they are ordered by goes back to
/// relevance. Columns the search has no order for (type) change nothing.
fn set_results_sort(mut find: State<Find>, key: SortKey) {
    let by = match key {
        SortKey::Name => SortBy::Name,
        SortKey::Size => SortBy::Size,
        SortKey::Modified => SortBy::Modified,
        SortKey::Kind | SortKey::Deleted => return,
    };
    let now = find.peek().filters.sort;
    find.write().filters.sort = if now == by { SortBy::Relevance } else { by };
}

/// What to offer when a search finds nothing: take a filter off, look
/// further.
fn search_hints(session: &Session) -> Vec<(String, EventHandler<()>)> {
    let mut find = session.find;
    let now = find.peek().clone();
    let mut hints: Vec<(String, EventHandler<()>)> = now
        .filters
        .active()
        .into_iter()
        .take(2)
        .map(|key| {
            (
                format!("Remove “{}”", token_label(key, &now.filters)),
                EventHandler::new(move |_: ()| {
                    let mut now = find.write();
                    now.filters.clear(key);
                    // The press took the keyboard from the field.
                    now.focus += 1;
                }),
            )
        })
        .collect();
    if now.scope == Scope::Folder {
        hints.push((
            "Search subfolders".to_string(),
            EventHandler::new(move |_: ()| {
                let mut now = find.write();
                now.scope = Scope::Subfolders;
                now.focus += 1;
            }),
        ));
    }
    if now.scope != Scope::Everywhere {
        hints.push((
            "Search everywhere".to_string(),
            EventHandler::new(move |_: ()| {
                let mut now = find.write();
                now.scope = Scope::Everywhere;
                now.focus += 1;
            }),
        ));
    }
    hints
}

/// The mounted drives `Everywhere` adds to the home folder: those a person
/// would list, not the root, not a network share, and not a folder above or
/// below home.
fn search_drives(home: &std::path::Path, scope: &Scope) -> Vec<PathBuf> {
    if *scope != Scope::Everywhere {
        return Vec::new();
    }
    bingux_files::mounts::list_mounts()
        .into_iter()
        .filter(|mount| {
            mount.mount_point != std::path::Path::new("/")
                && !matches!(
                    mount.kind,
                    bingux_files::mounts::MountKind::Network
                        | bingux_files::mounts::MountKind::Optical
                )
                && !home.starts_with(&mount.mount_point)
                && !mount.mount_point.starts_with(home)
        })
        .map(|mount| mount.mount_point)
        .collect()
}

/// Why a search of folders cannot run, if its matcher could not be made.
fn results_invalid(matcher: &Option<Result<Matcher, Invalid>>) -> Option<String> {
    matcher
        .as_ref()
        .and_then(|matcher| matcher.as_ref().err())
        .map(|invalid| invalid.reason.clone())
}

/// The listing of a place that has not been read yet: nothing in it, and the
/// view lays out the same as for the folder once it is.
fn pending_listing(place: &Where) -> Arc<Listing> {
    Arc::new(match place {
        Where::Folder(path) => Listing::new(path.clone()),
        Where::Recent => Listing {
            recent: true,
            ..Listing::new(PathBuf::new())
        },
    })
}

#[derive(PartialEq)]
struct ChooserBody {
    request: ChooserRequest,
    host: Host,
}

impl Component for ChooserBody {
    fn render(&self) -> impl IntoElement {
        let direction = crate::controls::direction::use_direction();
        let tokens = use_tokens();
        let compact = is_compact();
        let window = use_window();
        let overlays = use_overlays();
        let home = use_hook(home_folder);
        let request = use_hook({
            let request = self.request.clone();
            move || Rc::new(request)
        });

        // The states of the tab on show. Every tab keeps its own, so showing
        // another is a change of which ones this body reads, in one pass: the
        // toolbar, the sidebar and the status line stay mounted as they are,
        // and the list is the tab's, as it was left.
        let fallback = default_level(&request);
        let (tab, alive) = match &self.host {
            Host::Browser(browse) => (browse.memory.id(), Some(browse.memory.downgrade())),
            Host::Dialog(_) => (0, None),
        };
        let bank = use_hook(|| Rc::new(RefCell::new(TabBank::default())));
        let states = bank.borrow_mut().states(tab, alive, || {
            let history = match &self.host {
                Host::Browser(browse) => browse.memory.model(),
                Host::Dialog(_) => NavModel::new(
                    Arc::new(folder_table()),
                    &location_of(&Where::Folder(request.initial_folder(&home))),
                ),
            };
            // A tab that came from another window starts as that window
            // left it; any tab can be asked for its states to leave with.
            let carried = match &self.host {
                Host::Browser(browse) => browse.memory.take_view::<Carried>(),
                Host::Dialog(_) => None,
            };
            let select = match &self.host {
                Host::Browser(browse) => browse.memory.take_select(),
                Host::Dialog(_) => None,
            };
            let states = TabStates::new(history, fallback, carried, select);
            if let Host::Browser(browse) = &self.host {
                browse.memory.publish(move || Box::new(states.carry()));
            }
            states
        });
        let closed = bank.borrow_mut().closed();
        if !closed.is_empty() {
            // Not inside render: a write notifies this component.
            spawn(async move {
                for states in closed {
                    states.clear();
                }
            });
        }
        let model = states.model;
        let selection = states.selection;
        let sort = states.sort;
        let level = states.level;
        let mut show_hidden = states.show_hidden;
        let preview_pane = states.preview_pane;
        let mut editing = states.editing;
        let draft = states.draft;
        let find = states.find;
        let words = states.words;
        let problem = states.problem;
        let filters = use_hook({
            let request = request.clone();
            move || Rc::new(request.effective_filters())
        });
        let filter = use_state(|| request.initial_filter());
        let name = use_state(|| request.initial_name());
        let new_folder = use_state(|| 0_u32);
        // The sidebar's Empty Trash reaches the view beside it through this.
        let empty_trash = super::actions::provide_empty_trash();
        let focus = use_state(|| 0_u32);
        let choices = use_state(|| {
            request
                .choices
                .iter()
                .map(|choice| (choice.id.clone(), choice.selected.clone()))
                .collect::<Vec<_>>()
        });
        let include_noise = use_state(|| true);
        let places_rev = use_state(|| 0_u32);
        let places = use_resource(*places_rev.read(), load_places).with(Clone::clone);
        let current = place_of(model.read().location());
        let (directory, reload) = use_directory(&current);
        let load = directory.read(&current);
        // The recent searches are read once, off the UI thread.
        let mut recents = use_state(Recents::default);
        use_hook(move || {
            let file = recents_file();
            spawn(async move {
                let loaded = spawn_blocking(move || Recents::load_from(&file)).await;
                if recents.peek().is_empty() {
                    recents.set(loaded);
                }
            });
        });

        // What is asked: the words of the field and the filters. A search of
        // folders runs exactly while one is on show.
        let find_now = find.read().clone();
        let words_now = words.read().clone();
        let query = find_now.active(&words_now);
        let here = match &current {
            Where::Folder(path) => path.clone(),
            Where::Recent => home.clone(),
        };
        let walking = query.is_some() && find_now.scope.walks();
        // The file type filter is the chooser's, so the search leaves out
        // what it would only hide.
        let chooser_filter = if matches!(request.action, Action::SelectFolder | Action::SaveFiles) {
            None
        } else {
            (*filter.read()).and_then(|index| filters.get(index).cloned())
        };
        let walk_key = query.clone().filter(|_| walking).map(|query| RunKey {
            roots: find_now
                .scope
                .roots(&here, &home, &search_drives(&home, &find_now.scope)),
            home: home.clone(),
            query,
            view_hidden: *show_hidden.read(),
            include_noise: *include_noise.read(),
            filter: chooser_filter.clone(),
            base: find_now.scope.base(&here, &home),
        });
        let mount = match &load {
            Load::Ready(listing) => listing.mount,
            _ => bingux_files::mounts::MountKind::Other,
        };
        let deep = use_search_run(walk_key, mount);
        // The reload button turns into Stop once a read or a search has gone
        // on for a moment, and stays that way for a moment after it ends.
        let stopping = use_dwell(reload.loading() || deep.running(), Dwell::LOADING);

        let folder_mode = matches!(request.action, Action::SelectFolder | Action::SaveFiles);
        // What is shown: hidden files, the filter and the sort applied.
        // The order is computed with the listing it indexes and travels with
        // it, so a view never pairs one folder's rows with another's order.
        // Computed here, from what this render read, and kept while the same
        // inputs come again: a value computed after the render, and read by
        // the next one, would give a switch of tabs a frame of the last tab's
        // rows.
        let shown_cache = use_hook(|| Rc::new(RefCell::new(None::<(ShownKey, Option<Shown>)>)));
        let nothing_yet = use_hook(|| Rc::new(RefCell::new(None::<(PathBuf, Arc<Listing>)>)));
        let shown = {
            // A search of folders replaces the folder's listing with its
            // results while it is on show; until it has found something that
            // is an empty listing, never the folder's.
            let results = if walking { deep.latest() } else { None };
            let listing = match (walking, &results, &load) {
                (true, Some(results), _) => Some(results.listing.clone()),
                (true, None, _) => {
                    let base = find_now.scope.base(&here, &home);
                    let mut held = nothing_yet.borrow_mut();
                    if held.as_ref().is_none_or(|(path, _)| *path != base) {
                        *held = Some((base.clone(), Arc::new(Listing::new(base))));
                    }
                    held.as_ref().map(|(_, listing)| listing.clone())
                }
                (false, _, Load::Ready(listing)) => Some(listing.clone()),
                _ => None,
            };
            let active = if folder_mode {
                None
            } else {
                chooser_filter.as_ref()
            };
            match listing {
                None => None,
                Some(listing) => {
                    let key = ShownKey {
                        listing: listing.clone(),
                        results: results.clone(),
                        filter: if folder_mode { None } else { *filter.read() },
                        show_hidden: *show_hidden.read(),
                        sort: *sort.read(),
                        query: query.clone(),
                    };
                    let mut cache = shown_cache.borrow_mut();
                    match &*cache {
                        Some((last, shown)) if last.same(&key) => shown.clone(),
                        _ => {
                            let shown = Some(select_shown(
                                listing,
                                results,
                                query.as_ref(),
                                walking,
                                active,
                                key.show_hidden,
                                key.sort,
                            ));
                            *cache = Some((key, shown.clone()));
                            shown
                        }
                    }
                }
            }
        };

        let nav = Nav {
            model,
            selection,
            editing,
            find,
            words,
            problem,
            sort,
            level,
            fallback,
        };
        let session = Session {
            request: request.clone(),
            filters: filters.clone(),
            home: home.clone(),
            nav,
            directory,
            reload,
            name,
            filter,
            find,
            words,
            deep: deep.clone(),
            shown: shown.as_ref().map(|shown| shown.listing.clone()),
            focus,
            places_rev,
            choices,
            stopping,
            window,
            overlays,
            host: self.host.clone(),
        };

        // A popup that goes away, however it goes, is a cancel.
        if let Host::Dialog(done) = &self.host
            && let Window::Popup(popup_window) = window
            && let Some(entry) = popup_window.entry()
        {
            let closing = entry.closing;
            let done = done.clone();
            use_side_effect(move || {
                if *closing.read() {
                    done.finish(None);
                }
            });
        }
        // A tab keeps where it is, so it is there when it comes back, and the
        // host learns where it went.
        if let Host::Browser(browse) = &self.host {
            let on_location = use_hook(|| Rc::new(RefCell::new(browse.on_location.clone())));
            *on_location.borrow_mut() = browse.on_location.clone();
            use_side_effect_with_deps(&(model, browse.memory.clone()), move |(model, memory)| {
                let model = model.read().clone();
                let at = place_of(model.location());
                memory.set(model);
                let handler = on_location.borrow().clone();
                handler.call(at);
            });
        }

        let saving = request.action == Action::Save;
        use_popup_actions(PopupActions {
            primary: Some(PopupAction {
                label: request.accept_text(),
                enabled: session.can_accept(),
                on_press: {
                    let session = session.clone();
                    EventHandler::new(move |_: ()| session.accept())
                },
            }),
        });

        let order = shown
            .as_ref()
            .map_or_else(|| Arc::new(Vec::new()), |shown| shown.order.clone());
        let current_load = match (&load, &shown) {
            (Load::Ready(_), Some(shown)) => Load::Ready(shown.listing.clone()),
            (Load::Loading, Some(shown)) if walking => Load::Ready(shown.listing.clone()),
            (other, _) => other.clone(),
        };
        let folder_name = match &current {
            Where::Folder(path) => path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
            Where::Recent => "Recent".to_string(),
        };
        let choices = scope_choices(
            &places.value().cloned().unwrap_or_default(),
            &home,
            &find_now.scope,
        );
        let place = place_phrase(&find_now.scope, &folder_name, &choices);
        let search_view = query
            .as_ref()
            .zip(shown.as_ref())
            .map(|(query, shown)| SearchView {
                query: Marks::new(shown.matcher.clone(), query.to_string()),
                of: shown.of,
                deep: walking.then(|| match &shown.results {
                    Some(results) => DeepStatus {
                        // What the list shows: the file type filter has already
                        // taken its share out of what the search found.
                        found: shown.order.len(),
                        ..results.status()
                    },
                    None => DeepStatus {
                        root: find_now.scope.base(&here, &home),
                        running: deep.running_now(),
                        ..DeepStatus::default()
                    },
                }),
                place: place.clone(),
                invalid: shown.invalid.clone(),
                sort: query.filters.sort,
                words: query.plain_text(),
            });
        // A new search selects its best match, so Enter opens it; with no
        // match nothing stays selected that the list does not show. A search
        // that is still running selects the first row that arrives, and the
        // best one once it ends, unless the person has moved the selection.
        let selected_for = use_hook(|| {
            Rc::new(RefCell::new((
                None::<(String, bool, bool, bool)>,
                None::<PathBuf>,
            )))
        });
        {
            let running = deep.running();
            let key = query
                .as_ref()
                .map(|query| (query.to_string(), walking, !order.is_empty(), running));
            let changed = selected_for.borrow().0 != key;
            if changed {
                selected_for.borrow_mut().0 = key.clone();
                if key.is_some()
                    && let Some(shown) = shown.clone()
                {
                    let held = selected_for.clone();
                    spawn(async move {
                        let mut selection = selection;
                        let mine = selection.peek().cursor().map(PathBuf::from);
                        let auto = held.borrow().1.clone();
                        // Only a selection the person has not touched is ours to move.
                        if mine.is_some() && mine != auto {
                            return;
                        }
                        match shown.order.first() {
                            Some(first) => {
                                let path = shown.listing.entries[*first].path.clone();
                                selection.write().select_only(&path);
                                held.borrow_mut().1 = Some(path);
                            }
                            None => {
                                selection.write().clear();
                                held.borrow_mut().1 = None;
                            }
                        }
                    });
                }
            }
        }
        let bar = find_now.open.then(|| {
            let (listing, order) = (
                shown.as_ref().map(|shown| shown.listing.clone()),
                order.clone(),
            );
            let on_open = {
                let (session, listing, order) = (session.clone(), listing.clone(), order.clone());
                move |_: ()| {
                    if let Some(listing) = &listing {
                        session.open_match(listing, &order);
                    }
                }
            };
            let on_step = {
                let (session, listing, order) = (session.clone(), listing.clone(), order.clone());
                move |by: i32| {
                    if let Some(listing) = &listing {
                        session.step_match(listing, &order, by);
                    }
                }
            };
            let on_list = {
                let (session, listing, order) = (session.clone(), listing.clone(), order.clone());
                move |_: ()| {
                    if let Some(listing) = &listing {
                        session.enter_results(listing, &order);
                    }
                }
            };
            let problem = {
                let (committed, _) = bingux_files::find::split_pending(&words_now);
                Query::parse_checked(committed)
                    .1
                    .first()
                    .map(|problem| format!("{}: {}", problem.key, problem.reason))
            };
            let results = shown.as_ref().and_then(|shown| shown.results.as_ref());
            let status = BarStatus {
                place: place.clone(),
                asked: query.is_some(),
                walking,
                running: walking && deep.running(),
                found: order.len(),
                folders: results.map_or(0, |results| results.folders),
                truncated: results.is_some_and(|results| results.truncated),
                stopped: results.is_some_and(|results| results.stopped),
                matches: (!walking && query.is_some())
                    .then(|| (order.len(), shown.as_ref().map_or(0, |shown| shown.of))),
                invalid: shown
                    .as_ref()
                    .and_then(|shown| shown.invalid.clone())
                    .or_else(|| {
                        results.and_then(|results| {
                            results
                                .invalid
                                .as_ref()
                                .map(|invalid| invalid.reason.clone())
                        })
                    }),
                missing: results.and_then(|results| results.missing.clone()),
                problem,
            };
            SearchBar {
                find,
                draft: words,
                choices: choices.clone(),
                status,
                include_noise,
                recents,
                on_open: EventHandler::new(on_open),
                on_step: EventHandler::new(on_step),
                on_list: EventHandler::new(on_list),
                on_close: {
                    let session = session.clone();
                    EventHandler::new(move |_: ()| session.clear_search())
                },
                on_stop: {
                    let deep = deep.clone();
                    EventHandler::new(move |_: ()| deep.stop())
                },
            }
        });

        let active_filter = (!folder_mode)
            .then(|| (*filter.read()).and_then(|index| filters.get(index)))
            .flatten();
        let use_grid = kind_of(*level.read()) == ViewKind::Grid;

        let filters_open_now = find_now.filters_open;
        let shortcuts = {
            let session = session.clone();
            move |event: Event<KeyboardEventData>| {
                let mut draft = draft;
                let alt = event.modifiers.contains(Modifiers::ALT);
                let ctrl = event.modifiers.contains(Modifiers::CONTROL);
                let shift = event.modifiers.contains(Modifiers::SHIFT);
                // Back and Forward are along the reading direction: Alt+Right goes back in a
                // window that reads from the right.
                let key = direction.reading_key(&event.key);
                match &key {
                    Key::Named(NamedKey::ArrowUp) if alt => {
                        event.stop_propagation();
                        session.nav.up();
                    }
                    Key::Named(NamedKey::ArrowLeft) if alt => {
                        event.stop_propagation();
                        session.nav.back();
                    }
                    Key::Named(NamedKey::ArrowRight) if alt => {
                        event.stop_propagation();
                        session.nav.forward();
                    }
                    Key::Named(NamedKey::Home) if alt => {
                        event.stop_propagation();
                        session.nav.go(Where::Folder(session.home.clone()));
                    }
                    // Backspace goes up a folder, as in a browser's file view.
                    Key::Named(NamedKey::Backspace) if !ctrl && !alt => {
                        event.stop_propagation();
                        session.nav.up();
                    }
                    Key::Named(NamedKey::F5) if !ctrl && !alt => {
                        event.stop_propagation();
                        session.reload.run();
                    }
                    Key::Character(text) if ctrl && !alt && text.eq_ignore_ascii_case("r") => {
                        event.stop_propagation();
                        session.reload.run();
                    }
                    // Escape stops a read that the toolbar offers to stop,
                    // as it stops a page loading in a browser.
                    Key::Named(NamedKey::Escape)
                        if session.stopping && session.reload.loading_now() =>
                    {
                        event.stop_propagation();
                        session.reload.stop();
                    }
                    // Escape in the list ends a search; in the field it first
                    // clears what is typed (the field handles that itself).
                    // The Filters panel hears the same Escape and closes: it
                    // must not end the search as well, so a panel that was open
                    // when this render was made leaves the search alone.
                    Key::Named(NamedKey::Escape)
                        if session.find.peek().open && !filters_open_now =>
                    {
                        event.stop_propagation();
                        session.clear_search();
                    }
                    // Ctrl+Shift+F opens or closes the Filters panel.
                    Key::Character(text)
                        if ctrl && shift && !alt && text.eq_ignore_ascii_case("f") =>
                    {
                        event.stop_propagation();
                        session.start_search("");
                        let mut find = session.find;
                        let open = find.peek().filters_open;
                        find.write().filters_open = !open;
                    }
                    Key::Character(text) if ctrl && !alt && text.eq_ignore_ascii_case("f") => {
                        event.stop_propagation();
                        session.start_search("");
                    }
                    Key::Character(text) if ctrl && text.eq_ignore_ascii_case("l") => {
                        event.stop_propagation();
                        draft.set(AddressBar::text_of(&session.nav.here()));
                        editing.set(true);
                    }
                    Key::Character(text) if ctrl && text.eq_ignore_ascii_case("h") => {
                        event.stop_propagation();
                        let shown = *show_hidden.peek();
                        show_hidden.set(!shown);
                    }
                    // A slash or a tilde starts a path, as in every file chooser.
                    Key::Character(text) if !ctrl && !alt && (text == "/" || text == "~") => {
                        event.stop_propagation();
                        draft.set(text.clone());
                        editing.set(true);
                    }
                    _ => {}
                }
            }
        };

        let footer = footer(&tokens, compact, &session, saving, problem);
        // A browser window's view reaches the window's bottom edge when no
        // footer sits below it, so it paints the window's radius at the corners
        // that touch the window; its focus ring follows. Beside the sidebar
        // only the right corner does.
        let corners = match (&self.host, footer.is_none(), compact) {
            (Host::Browser(_), true, true) => Corners::Bottom,
            (Host::Browser(_), true, false) => Corners::BottomRight,
            _ => Corners::None,
        };
        let main = main_area(
            &session,
            &current_load,
            &order,
            selection,
            sort,
            show_hidden,
            level,
            new_folder,
            preview_pane,
            focus,
            Arc::new(
                places
                    .value()
                    .map(|places| {
                        places
                            .bookmarks
                            .iter()
                            .filter_map(|place| place.path.clone())
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            folder_mode,
            active_filter.is_some(),
            search_view.clone(),
            corners,
        );
        let toolbar = toolbar(
            &tokens,
            compact,
            &session,
            &places.value().cloned().unwrap_or_default(),
            editing,
            draft,
            folder_mode,
            use_grid,
            sort,
            show_hidden,
            level,
            new_folder,
            preview_pane,
            focus,
            (!compact && shows_filters(&session)).then(|| filter_control(&session)),
            find_now.open,
        );

        // A phone's sheet reaches the screen's bottom edge, so with no footer
        // the list keeps its own clearance.
        let bottom_clearance = compact && footer.is_none();
        rect()
            .component("FileChooser")
            .tw(&tokens, &[flex_col(), grow_children(), w_full(), h_full()])
            .maybe(bottom_clearance, |root| root.tw(&tokens, &[pb(S4)]))
            .on_key_down(shortcuts)
            // The mouse's Back and Forward buttons (8 and 9) walk the history.
            .on_global_pointer_down(move |event: Event<PointerEventData>| {
                if let PointerEventData::Mouse(mouse) = event.data() {
                    match mouse.button {
                        Some(MouseButton::Back) => nav.back(),
                        Some(MouseButton::Forward) => nav.forward(),
                        _ => {}
                    }
                }
            })
            .child(toolbar)
            .child(divider(&tokens))
            .maybe_child(bar.map(|bar| Element::from(bar)))
            .maybe_child(find_now.open.then(|| divider(&tokens)))
            .child(
                rect()
                    .tw(&tokens, &[flex_row(), grow_children(), w_full(), h_flex()])
                    .maybe_child((!compact).then(|| {
                        let places = places.value().cloned().unwrap_or_default();
                        let nav = nav;
                        rect()
                            .tw(&tokens, &[flex_row(), h_full(), w(Dim::PlacesColumn)])
                            .child(rect().tw(&tokens, &[w_flex(), h_full()]).child({
                                let sidebar = PlacesSidebar::new(
                                    places,
                                    current.clone(),
                                    move |to: Where| nav.go(to),
                                )
                                .recent(request.action == Action::Open)
                                .reload(places_rev)
                                .on_empty_trash(move |_: ()| {
                                    super::actions::request_empty_trash(empty_trash)
                                });
                                match &self.host {
                                    Host::Browser(browse) => {
                                        let on_new_tab = browse.on_new_tab.clone();
                                        sidebar.on_open_new_tab(move |to: Where| {
                                            if let Where::Folder(path) = to {
                                                on_new_tab.call(path);
                                            }
                                        })
                                    }
                                    Host::Dialog(_) => sidebar,
                                }
                            }))
                            .child(rect().tw(&tokens, SPLIT_HAIRLINE))
                    }))
                    .child(
                        rect()
                            .tw(&tokens, &[flex_col(), w_flex(), h_full(), grow_children()])
                            .child(
                                rect()
                                    .tw(&tokens, &[w_full(), h_flex(), grow_children()])
                                    .child(main),
                            )
                            .maybe_child(footer.map(|footer| {
                                rect()
                                    .tw(&tokens, &[flex_col(), w_full()])
                                    .child(divider(&tokens))
                                    .child(footer)
                            })),
                    ),
            )
    }
}

/// The icon size a folder opens at when nothing is remembered for it: 16 px
/// (32 px on a phone), in a list, unless the app asked for a grid, which opens at a large
/// picture size. Neither the filter nor the size of the window decides it.
fn default_level(request: &ChooserRequest) -> u16 {
    match request.view {
        ViewMode::Grid => REQUESTED_GRID_PX,
        ViewMode::List | ViewMode::Auto => default_zoom_px(),
    }
}

/// The list, the grid or the state that stands in for them.
#[allow(clippy::too_many_arguments)]
fn main_area(
    session: &Session,
    load: &Load,
    order: &Arc<Vec<usize>>,
    selection: State<Selection>,
    sort: State<Sort>,
    show_hidden: State<bool>,
    level: State<u16>,
    new_folder: State<u32>,
    preview_pane: State<bool>,
    focus: State<u32>,
    bookmarks: Arc<Vec<PathBuf>>,
    folder_mode: bool,
    filtered: bool,
    search: Option<SearchView>,
    corners: Corners,
) -> Element {
    // A folder that has not been read yet is the file view of that folder with
    // no rows: the column header, the room of the scrollbar and the status line
    // are those of the folder once it is read, and the rows come into them. A
    // placeholder of grey rows in the rows' own height (after a short delay, so
    // a folder that opens at once never shows it) stands for them.
    let (listing, order, placeholder) = match load {
        Load::Failed(error) => return error_state(error),
        Load::Loading => {
            let owner = match &session.host {
                Host::Browser(browse) => browse.memory.id(),
                Host::Dialog(_) => 0,
            };
            (
                pending_listing(&session.nav.here()),
                Arc::new(Vec::new()),
                Some(skeleton(*level.peek(), measured(owner).viewport)),
            )
        }
        Load::Ready(listing) => {
            let shown_hidden = *show_hidden.peek();
            let placeholder = order.is_empty().then(|| {
                if let Some(search) = &search {
                    return no_results_state(
                        &search.words,
                        &search.place,
                        search.deep.as_ref().map(|deep| deep.running),
                        search.invalid.as_deref(),
                        search_hints(session),
                    );
                }
                if listing.stopped {
                    return stopped_state();
                }
                if bingux_files::ops::trash_place(&listing.path)
                    .is_some_and(|place| place.is_root())
                {
                    return super::states::trash_empty_state();
                }
                let hidden_items =
                    !shown_hidden && listing.entries.iter().any(|entry| entry.hidden);
                let others = listing.entries.iter().any(|entry| !entry.hidden);
                empty_state(listing.recent, hidden_items && !others, filtered && others)
            });
            (listing.clone(), order.clone(), placeholder)
        }
    };
    let shown_hidden = *show_hidden.peek();
    let pick = {
        let session = session.clone();
        move |index: usize| session.picked(index)
    };
    let activate = {
        let session = session.clone();
        move |index: usize| session.activate(index)
    };
    let nav = session.nav;
    let mut places_rev = session.places_rev;
    let mut sort = sort;
    let mut show_hidden = show_hidden;
    let search_active = search.is_some();
    let mut view = FileView::new(listing.clone(), order, selection)
        .icon_px(level)
        .sort(match &search {
            // A search is in the order it asks for. Relevance is no column's
            // sort: nothing in the header is marked.
            Some(search) => results_sort(search.sort).unwrap_or(Sort {
                key: SortKey::Deleted,
                descending: false,
                folders_first: true,
            }),
            None => *sort.peek(),
        })
        .multiple(session.request.multiple)
        .folders_only(folder_mode)
        .autofocus(!matches!(session.request.action, Action::Save) && !session.find.peek().open)
        .search(search)
        .on_search({
            let session = session.clone();
            move |typed: String| session.start_search(&typed)
        })
        .show_hidden(shown_hidden)
        .thumbnails(listing.mount.thumbnails())
        .new_folder(new_folder)
        .preview_pane(preview_pane)
        .focus(focus)
        .bookmarks(bookmarks)
        .on_refresh({
            let reload = session.reload.clone();
            move |_: ()| reload.begin()
        })
        .on_bookmarks_changed(move |_: ()| {
            let next = *places_rev.peek() + 1;
            places_rev.set(next);
        })
        .on_sort({
            let find = session.find;
            move |key: SortKey| {
                if search_active {
                    set_results_sort(find, key);
                    return;
                }
                let next = sort.peek().toggled(key);
                sort.set(next);
                nav.remember_sort(next);
            }
        })
        .on_sort_set({
            let find = session.find;
            move |next: Sort| {
                if search_active {
                    set_results_sort(find, next.key);
                    return;
                }
                sort.set(next);
                nav.remember_sort(next);
            }
        })
        .on_show_hidden(move |on: bool| show_hidden.set(on))
        .on_open_folder(move |path: PathBuf| nav.go(Where::Folder(path)))
        .on_pick(pick)
        .on_activate(activate);
    view = view.corners(corners);
    if let Host::Browser(browse) = &session.host {
        view = view
            .caps(Capabilities::FILES_APP)
            .on_open_new_tab(browse.on_new_tab.clone())
            .remember_for(browse.memory.id());
    }
    if let Some(placeholder) = placeholder {
        view = view.placeholder(placeholder);
    }
    view.into()
}

/// What a middle click on a row of a history menu does: open its folder in a
/// new tab. No action for the recent files (a tab opens on a folder) and
/// where the host has no tabs.
fn middle_click(
    new_tab: Option<EventHandler<PathBuf>>,
) -> impl Fn(&HistoryItem) -> Option<Rc<dyn Fn()>> {
    move |item| {
        let handler = new_tab.clone()?;
        let Where::Folder(path) = place_of(&item.location) else {
            return None;
        };
        Some(Rc::new(move || handler.call(path.clone())) as Rc<dyn Fn()>)
    }
}

/// Back, Forward, Up, the address bar, the view switch and the view menu; on
/// a phone the places menu comes first and the address bar takes a line of
/// its own.
#[allow(clippy::too_many_arguments)]
fn toolbar(
    tokens: &crate::theme::Tokens,
    compact: bool,
    session: &Session,
    places: &bingux_files::Places,
    editing: State<bool>,
    draft: State<String>,
    folder_mode: bool,
    use_grid: bool,
    sort: State<Sort>,
    show_hidden: State<bool>,
    mut level: State<u16>,
    mut new_folder: State<u32>,
    mut preview_pane: State<bool>,
    mut focus: State<u32>,
    filter: Option<Element>,
    search_open: bool,
) -> Element {
    let nav = session.nav;
    let mut places_rev = session.places_rev;
    let current = nav.current();
    // A middle click on a row of a history menu opens that folder in a tab of
    // its own, where the host has tabs.
    let new_tab = match &session.host {
        Host::Browser(browse) => Some(browse.on_new_tab.clone()),
        Host::Dialog(_) => None,
    };
    let (can_go_back, can_go_forward, back_items, forward_items) = {
        let model = nav.model.read();
        let (back, forward) = history::menus(&model);
        (
            model.can_go_back(),
            model.can_forward(),
            history_menu_with(
                back,
                move |entry| nav.back_to(entry),
                middle_click(new_tab.clone()),
            ),
            history_menu_with(
                forward,
                move |entry| nav.forward_to(entry),
                middle_click(new_tab),
            ),
        )
    };
    let path_bar = AddressBar::new(
        current.clone(),
        session.home.clone(),
        editing,
        draft,
        move |to: Where| nav.go(to),
        {
            let session = session.clone();
            move |path: PathBuf| session.open_file(path)
        },
    )
    .folders_only(folder_mode)
    .accepts_file(session.request.action == Action::Open)
    .show_hidden(*show_hidden.read())
    .on_bookmarks_changed(move |_: ()| {
        let next = *places_rev.peek() + 1;
        places_rev.set(next);
    })
    .on_close_edit(move |_: ()| {
        let next = *focus.peek() + 1;
        focus.set(next);
    });

    // A right click or a hold lists the places the button would take.
    let back = icon_button(Icon::GoPrevious, move |_: ()| nav.back())
        .size(Dim::Control)
        .tooltip("Back")
        .enabled(can_go_back)
        .context_menu(back_items);
    let forward = icon_button(Icon::GoNext, move |_: ()| nav.forward())
        .size(Dim::Control)
        .tooltip("Forward")
        .enabled(can_go_forward)
        .context_menu(forward_items);
    let has_parent = super::history::parent(&current).is_some();
    let parent = icon_button(Icon::PanUp, move |_: ()| nav.up())
        .size(Dim::Control)
        .tooltip("Parent folder")
        .enabled(has_parent);
    // Recent has no folder to read again.
    let refresh = reload_button(
        &session.reload,
        &session.deep,
        session.stopping,
        matches!(current, Where::Folder(_)),
    );
    // Nothing is made in the Trash; there, the button that makes a folder
    // gives way to the one that empties it.
    let roots = super::named::Roots::at_home(&session.home);
    let in_trash = roots.inside_of(&current) == Some(super::named::Root::Trash);
    let can_create = matches!(current, Where::Folder(_)) && !in_trash;
    let create = icon_button(Icon::FolderNew, move |_: ()| {
        let next = *new_folder.peek() + 1;
        new_folder.set(next);
    })
    .size(Dim::Control)
    .tooltip(if in_trash {
        "Nothing can be made in the Trash"
    } else {
        "New folder"
    })
    .enabled(can_create);
    let empty_trash = (roots.root_of(&current) == Some(super::named::Root::Trash))
        .then(|| try_consume_root_context::<super::actions::EmptyTrashRequest>())
        .flatten()
        .map(|request| {
            icon_button(Icon::UserTrash, move |_: ()| {
                super::actions::request_empty_trash(request)
            })
            .size(Dim::Control)
            .tooltip("Empty Trash")
        });
    // The switch between a list and a grid, and the menu of the view's other
    // choices (sort, hidden files), in one control.
    let view = SplitButton::new(if use_grid {
        Icon::ViewList
    } else {
        Icon::ViewGrid
    })
    .tooltip(if use_grid {
        "Show a list"
    } else {
        "Show a grid"
    })
    .menu_label("View options")
    .on_press(move |_: ()| {
        let next = switch_kind(
            *level.peek(),
            if use_grid {
                ViewKind::List
            } else {
                ViewKind::Grid
            },
        );
        level.set(next);
        nav.remember_level(next);
    })
    .menu(view_items(nav, sort, show_hidden));

    // The search bar opens under the toolbar; the button is how a person who
    // does not know Ctrl+F finds it.
    let search_button = {
        let session = session.clone();
        icon_button(Icon::Search, move |_: ()| {
            if session.find.peek().open {
                session.clear_search();
            } else {
                session.start_search("");
            }
        })
        .size(Dim::Control)
        .active(search_open)
        .tooltip("Search (Ctrl+F)")
    };

    // The docked preview pane: it shows the selected file beside the list and
    // follows the selection. A phone has no room for it.
    let pane_shown = *preview_pane.read();
    let pane = icon_button(Icon::SidebarShow, move |_: ()| {
        let shown = *preview_pane.peek();
        preview_pane.set(!shown);
    })
    .size(Dim::Control)
    .active(pane_shown)
    .tooltip("Preview pane (Alt+P)");

    if compact {
        let first = rect()
            .tw(
                tokens,
                &[
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    w_full(),
                    gap(S2),
                ],
            )
            .child(
                rect().tw(tokens, &[w_flex()]).child(
                    PlacesMenu::new(places.clone(), current.clone(), move |to: Where| nav.go(to))
                        .recent(session.request.action == Action::Open)
                        .reload(session.places_rev),
                ),
            )
            .child(search_button)
            .maybe_child(empty_trash)
            .child(create)
            .child(view);
        // Up goes beside the address bar: with five buttons in the first row the place's name had
        // 58 px of a phone chooser (and 5 px of the taxonomy's), too little for any of its letters.
        let second = rect()
            .tw(
                tokens,
                &[
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    w_full(),
                    gap(S2),
                ],
            )
            .child(parent)
            .child(rect().tw(tokens, &[w_flex()]).child(path_bar));
        return rect()
            .tw(tokens, &[flex_col(), w_full(), p(S2), gap(S2)])
            .child(first)
            .child(second)
            .into();
    }
    rect()
        .tw(
            tokens,
            &[
                flex_row(),
                grow_children(),
                items(Align::Center),
                w_full(),
                p(S2),
                gap(S2),
            ],
        )
        .child(back)
        .child(forward)
        .child(parent)
        .child(refresh)
        .child(rect().tw(tokens, &[w_flex()]).child(path_bar))
        .maybe_child(filter)
        .child(search_button)
        .maybe_child(empty_trash)
        .child(create)
        .child(pane)
        .child(view)
        .into()
}

/// The sort and hidden-file choices of the view menu. (What a search looks
/// in is the Filters panel's.)
fn view_items(nav: Nav, mut sort: State<Sort>, mut show_hidden: State<bool>) -> Vec<MenuItem> {
    let now = *sort.read();
    let by = |label: &str, key: SortKey| {
        menu_item(label, move |_: ()| {
            let current = *sort.peek();
            let next = if current.key == key {
                current
            } else {
                Sort {
                    key,
                    descending: false,
                    ..current
                }
            };
            sort.set(next);
            nav.remember_sort(next);
        })
        .checked(now.key == key)
    };
    vec![
        by("Sort by name", SortKey::Name),
        by("Sort by size", SortKey::Size),
        by("Sort by type", SortKey::Kind),
        by("Sort by modified", SortKey::Modified),
        menu_item("Reverse order", move |_: ()| {
            let current = *sort.peek();
            let next = Sort {
                descending: !current.descending,
                ..current
            };
            sort.set(next);
            nav.remember_sort(next);
        })
        .checked(now.descending),
        menu_separator(),
        menu_item("Show hidden files", move |_: ()| {
            let shown = *show_hidden.peek();
            show_hidden.set(!shown);
        })
        .checked(*show_hidden.read()),
    ]
}

/// The strip under the content pane: the name to save under, the request's
/// choices and any problem. `None` when there is nothing to show. On a phone
/// it also holds the file type, which a desktop keeps in the toolbar. It sits
/// in the content pane, so the sidebar runs the dialog's full height.
fn footer(
    tokens: &crate::theme::Tokens,
    compact: bool,
    session: &Session,
    saving: bool,
    problem: State<Option<String>>,
) -> Option<Element> {
    let request = &session.request;
    let show_picker = compact && shows_filters(session);
    let message = problem.read().clone();
    if !saving && !show_picker && request.choices.is_empty() && message.is_none() {
        return None;
    }
    let name_field = saving.then(|| {
        Element::from(
            text_field(session.name, |_: String| {})
                .fill_width()
                .placeholder("File name")
                .autofocus(),
        )
    });
    // A phone's sheet reaches the bottom edge of the screen, so its last
    // line keeps clear of the screen's corners.
    let mut column = rect()
        .tw(tokens, &[flex_col(), w_full(), p(S3), gap(S2)])
        .maybe(compact, |column| column.tw(tokens, &[pb(S6)]))
        .maybe_child(message.map(|text| Element::from(status(Status::Error(text)))));
    for choice in &request.choices {
        column = column.child(choice_control(tokens, session, choice));
    }
    column = column.maybe_child(name_field);
    if show_picker {
        column = column.child(
            rect()
                .tw(
                    tokens,
                    &[
                        flex_row(),
                        grow_children(),
                        items(Align::Center),
                        w_full(),
                        px(S2),
                    ],
                )
                .child(BODY.label(tokens, "File type"))
                .child(rect().tw(tokens, &[w_flex()]))
                .child(filter_control(session)),
        );
    }
    Some(column.into())
}

/// Whether the request offers a file type filter to choose from.
fn shows_filters(session: &Session) -> bool {
    matches!(session.host, Host::Dialog(_))
        && !session.filters.is_empty()
        && matches!(session.request.action, Action::Open | Action::Save)
}

/// The file type dropdown: every filter of the request, or the presets.
/// Choosing one clears the selection, which may hold files it now hides.
fn filter_control(session: &Session) -> Element {
    let options: Vec<(String, String)> = session
        .filters
        .iter()
        .enumerate()
        .map(|(index, filter)| (index.to_string(), filter.name.clone()))
        .collect();
    let selected = session.filter.read().unwrap_or(0).to_string();
    let mut filter = session.filter;
    let nav = session.nav;
    dropdown(options, selected, move |key: String| {
        filter.set(key.parse().ok());
        let mut selection = nav.selection;
        selection.write().clear();
    })
    .title("File type")
    .into()
}

/// One of the request's choices: a checkbox, or a menu of options.
fn choice_control(tokens: &crate::theme::Tokens, session: &Session, choice: &Choice) -> Element {
    let mut choices = session.choices;
    let id = choice.id.clone();
    let current = choices
        .read()
        .iter()
        .find(|(key, _)| *key == choice.id)
        .map(|(_, value)| value.clone())
        .unwrap_or_else(|| choice.selected.clone());
    let set = move |value: String| {
        let mut all = choices.write();
        match all.iter_mut().find(|(key, _)| *key == id) {
            Some(entry) => entry.1 = value,
            None => all.push((id.clone(), value)),
        }
    };
    if choice.options.is_empty() {
        let set = set.clone();
        let mut set = set;
        return Element::from(
            checkbox(current == "true", move |on: bool| set(on.to_string()))
                .text(choice.label.clone()),
        );
    }
    let options = choice.options.clone();
    let mut set = set;
    rect()
        .tw(
            tokens,
            &[flex_row(), items(Align::Center), w_full(), gap(S3)],
        )
        .child(BODY.label(tokens, choice.label.clone()))
        .child(
            dropdown(options, current, move |value: String| set(value)).title(choice.label.clone()),
        )
        .into()
}

#[cfg(test)]
mod search_order_tests {
    use super::*;
    use bingux_files::Kind;

    fn entry(path: &str, size: u64) -> Entry {
        let path = PathBuf::from(path);
        let name = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let mut entry = Entry::named(path, name, Kind::File, false, false);
        entry.size = size;
        entry.sized = true;
        entry
    }

    fn results(names: &[(&str, u64)], running: bool) -> (Arc<Listing>, Arc<DeepResults>) {
        let mut listing = Listing::new(PathBuf::from("/r"));
        listing.entries = names
            .iter()
            .map(|(path, size)| entry(path, *size))
            .collect();
        listing.complete = !running;
        let listing = Arc::new(listing);
        let results = Arc::new(DeepResults {
            key: "k".into(),
            listing: listing.clone(),
            folders: 3,
            running,
            stopped: false,
            truncated: false,
            unreadable: 0,
            indexed: 0,
            invalid: None,
            missing: None,
        });
        (listing, results)
    }

    fn names(shown: &Shown) -> Vec<String> {
        shown
            .order
            .iter()
            .map(|index| shown.listing.entries[*index].name.clone())
            .collect()
    }

    const FOUND: &[(&str, u64)] = &[
        ("/r/a/deep/x/notes-old.txt", 5),
        ("/r/zz notes.txt", 50),
        ("/r/notes.txt", 500),
        ("/r/b/notes", 5_000),
    ];

    #[test]
    fn rows_keep_their_places_while_the_search_runs_and_settle_once_when_it_ends() {
        let query = Query::parse("notes");
        let (listing, running) = results(FOUND, true);
        let shown = select_shown(
            listing,
            Some(running),
            Some(&query),
            true,
            None,
            false,
            Sort::default(),
        );
        assert_eq!(
            names(&shown),
            ["notes-old.txt", "zz notes.txt", "notes.txt", "notes"],
            "arrival order: nothing that is on show moves"
        );
        // More arrive: the rows already there keep their places.
        let more: Vec<(&str, u64)> = FOUND
            .iter()
            .copied()
            .chain([("/r/c/best notes", 1)])
            .collect();
        let (listing, running) = results(&more, true);
        let grown = select_shown(
            listing,
            Some(running),
            Some(&query),
            true,
            None,
            false,
            Sort::default(),
        );
        assert_eq!(names(&grown)[..4], names(&shown)[..]);
        // It ends: relevance order once, shallow before deep inside a tier.
        let (listing, done) = results(&more, false);
        let settled = select_shown(
            listing,
            Some(done),
            Some(&query),
            true,
            None,
            false,
            Sort::default(),
        );
        assert_eq!(
            names(&settled),
            [
                "notes",
                "notes.txt",
                "notes-old.txt",
                "zz notes.txt",
                "best notes"
            ]
        );
    }

    #[test]
    fn a_chosen_sort_orders_the_results_when_they_are_done() {
        let (listing, done) = results(FOUND, false);
        let by = |text: &str| {
            let query = Query::parse(text);
            names(&select_shown(
                listing.clone(),
                Some(done.clone()),
                Some(&query),
                true,
                None,
                false,
                Sort::default(),
            ))
        };
        assert_eq!(
            by("notes sort:size"),
            ["notes", "notes.txt", "zz notes.txt", "notes-old.txt"],
            "largest first"
        );
        assert_eq!(
            by("notes sort:name"),
            ["notes", "notes-old.txt", "notes.txt", "zz notes.txt"]
        );
    }

    #[test]
    fn the_folder_on_show_is_narrowed_in_memory_by_the_whole_query() {
        let mut listing = Listing::new(PathBuf::from("/r"));
        listing.entries = vec![
            entry("/r/report.pdf", 3_000_000),
            entry("/r/report.txt", 5),
            entry("/r/photo.png", 10),
        ];
        let listing = Arc::new(listing);
        let run = |text: &str| {
            let query = Query::parse(text);
            let shown = select_shown(
                listing.clone(),
                None,
                Some(&query),
                false,
                None,
                false,
                Sort::default(),
            );
            (names(&shown), shown.of)
        };
        assert_eq!(
            run("report"),
            (vec!["report.pdf".to_string(), "report.txt".into()], 3)
        );
        assert_eq!(run("report size:>1MB").0, ["report.pdf"]);
        assert_eq!(run("ext:png").0, ["photo.png"]);
        assert_eq!(run("-report").0, ["photo.png"]);
        // A pattern that cannot run lists nothing and says why.
        let bad = select_shown(
            listing.clone(),
            None,
            Some(&Query::parse("( match:regex")),
            false,
            None,
            false,
            Sort::default(),
        );
        assert!(bad.order.is_empty() && bad.invalid.is_some());
        // Hidden files follow the view unless the query says.
        let mut with_hidden = Listing::new(PathBuf::from("/r"));
        let mut dot = entry("/r/.report", 1);
        dot.hidden = true;
        with_hidden.entries = vec![dot, entry("/r/report", 1)];
        let with_hidden = Arc::new(with_hidden);
        let count = |text: &str, show: bool| {
            names(&select_shown(
                with_hidden.clone(),
                None,
                Some(&Query::parse(text)),
                false,
                None,
                show,
                Sort::default(),
            ))
            .len()
        };
        assert_eq!((count("report", false), count("report", true)), (1, 2));
        assert_eq!(
            (
                count("report hidden:yes", false),
                count("report hidden:no", true)
            ),
            (2, 1)
        );
        assert_eq!(count("report hidden:only", false), 1);
    }
}
