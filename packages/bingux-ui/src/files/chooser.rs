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
use std::rc::Rc;
use std::sync::Arc;

use bingux_files::request::accept;
use bingux_files::{
    Accept, Action, Choice, ChooserRequest, Chosen, Entry, Filter, Listing, Selection, Sort,
    SortKey, View, ViewMode, location,
};
use freya::prelude::*;

use super::actions::{Capabilities, open_default};
use super::address_bar::AddressBar;
use super::browser::Browse;
use super::directory::{Load, Reload, Where, home_folder, use_directory};
use super::folder_table::{folder_table, location_of, place_of};
use super::places_view::{PlacesMenu, PlacesSidebar};
use super::search::{Deep, DeepResults, DeepStatus, Find, SearchLink, SearchView, step_selection, use_subfolder_search};
use super::skeleton::skeleton;
use super::states::{empty_state, error_state, no_matches_state};
use super::view::FileView;
use super::view_state;
use super::zoom::{DEFAULT_PX, REQUESTED_GRID_PX, ViewKind, kind_of, nearest_level, switch_kind};
use crate::component::ComponentName;
use crate::controls::{SplitButton, checkbox, dropdown, icon_button, text_field};
use crate::group::{Status, status};
use crate::icons::Icon;
use crate::layout::divider;
use crate::nav::{NavModel, Outcome};
use crate::shell::{
    CANCEL_ID, MenuItem, Overlays, ResponseKind, dialog, history_menu, menu_item, menu_separator,
    use_overlays,
};
use crate::style::*;
use crate::surface::use_tokens;
use crate::task::use_resource;
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
    /// The search in the address bar; going anywhere ends it.
    find: State<Find>,
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
        self.find.set(Find::default());
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
        let to = location_of(&to);
        self.apply(move |model| model.navigate(&to));
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
        let here = self.here();
        if let Where::Folder(path) = here
            && let Some(parent) = path.parent()
        {
            self.go(Where::Folder(parent.to_path_buf()));
        }
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
    load: State<Load>,
    reload: Reload,
    name: State<String>,
    filter: State<Option<usize>>,
    /// The address bar's text, set when a typed letter starts a search.
    draft: State<String>,
    find: State<Find>,
    /// The subfolder search; its results replace the folder's listing while
    /// they are on show.
    deep: Deep,
    /// Search build and dependency folders too (the default).
    include_noise: State<bool>,
    /// Bumped to give the keyboard back to the list.
    focus: State<u32>,
    /// Counts changes to the bookmarks, so the places load again.
    places_rev: State<u32>,
    choices: State<Vec<(String, String)>>,
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

    /// The listing on show: the subfolder search's matches while they are
    /// shown, else the folder's.
    fn listing(&self) -> Option<Arc<Listing>> {
        if let Some(query) = self.find.peek().deep_query()
            && let Some(results) = self.deep.results_for(&query)
        {
            return Some(results.listing.clone());
        }
        match &*self.load.peek() {
            Load::Ready(listing) => Some(listing.clone()),
            _ => None,
        }
    }

    // ---- search -----------------------------------------------------

    /// A letter typed on the list, or Ctrl+F (`typed` empty): the address bar
    /// becomes a search of this folder.
    fn start_search(&self, typed: &str) {
        if !matches!(self.nav.here(), Where::Folder(_)) {
            return;
        }
        let mut find = self.find;
        let mut draft = self.draft;
        let mut editing = self.nav.editing;
        let selection = self.nav.selection.peek().clone();
        let was = find.peek().scoped;
        // Two letters typed before the field has the keyboard join up.
        let text = if was {
            format!("{}{}", draft.peek(), typed)
        } else {
            typed.to_string()
        };
        {
            let mut find = find.write();
            find.begin(&selection);
            if !typed.is_empty() {
                let query = text.trim().to_string();
                if find.query != query {
                    find.query = query;
                    find.deep = None;
                }
            }
        }
        if !typed.is_empty() || !was {
            draft.set(text);
        }
        // A field that is already there takes the keyboard back.
        find.write().focus += 1;
        editing.set(true);
    }

    /// Escape, the chip's close button: show the whole folder again with the
    /// selection the person had, and give the keyboard back to the list.
    fn clear_search(&self) {
        let mut find = self.find;
        let restored = find.write().clear();
        if let Some(before) = restored {
            let mut selection = self.nav.selection;
            selection.set(before);
        }
        let mut editing = self.nav.editing;
        editing.set(false);
        let mut focus = self.focus;
        let next = *focus.peek() + 1;
        focus.set(next);
    }

    /// Ctrl+Enter, the last row, or Enter with no match: search the
    /// subfolders for what is typed.
    fn start_deep(&self) {
        let mut find = self.find;
        let Some(query) = find.peek().query() else {
            return;
        };
        find.write().deep = Some(query.text().to_string());
    }

    /// Open `path` of the listing on show, as Enter does on its row.
    fn open_match(&self, listing: &Listing, order: &[usize]) {
        let Some(first) = order.first().copied() else {
            self.start_deep();
            return;
        };
        let cursor = self.nav.selection.peek().cursor().map(PathBuf::from);
        let at = cursor
            .and_then(|path| order.iter().copied().find(|index| listing.entries[*index].path == path))
            .unwrap_or(first);
        self.activate(at);
    }

    /// Up or Down in the address field: move the selected match.
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
        self.overlays
            .show_dialog(
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
#[derive(Clone, PartialEq)]
struct Shown {
    listing: Arc<Listing>,
    order: Arc<Vec<usize>>,
    /// Entries the file type filter hides.
    hidden_by_filter: usize,
    /// Entries a search chooses among: what hidden files and the filter let
    /// through.
    of: usize,
    results: Option<Arc<DeepResults>>,
}

#[derive(PartialEq)]
struct ChooserBody {
    request: ChooserRequest,
    host: Host,
}

impl Component for ChooserBody {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let compact = is_compact();
        let window = use_window();
        let overlays = use_overlays();
        let home = use_hook(home_folder);
        let request = use_hook({
            let request = self.request.clone();
            move || Rc::new(request)
        });

        let history = use_hook({
            let host = self.host.clone();
            let start = request.initial_folder(&home);
            move || match host {
                Host::Browser(browse) => browse.memory.model(),
                Host::Dialog(_) => {
                    NavModel::new(Arc::new(folder_table()), &location_of(&Where::Folder(start)))
                }
            }
        });
        // A folder the person has shown a way before opens that way again,
        // the first one too.
        let remembered = match place_of(history.location()) {
            Where::Folder(path) => view_state::folder(&path),
            Where::Recent => None,
        };
        let model = use_state(move || history);
        let selection = use_state(Selection::new);
        let sort = use_state(move || remembered.map_or_else(Sort::default, |view| view.sort.into()));
        let mut show_hidden = use_state(|| false);
        let filters = use_hook({
            let request = request.clone();
            move || Rc::new(request.effective_filters())
        });
        let filter = use_state(|| request.initial_filter());
        let name = use_state(|| request.initial_name());
        let fallback = default_level(&request);
        let level = use_state(move || {
            remembered.map_or(fallback, |view| nearest_level(f32::from(view.icon_px)))
        });
        let new_folder = use_state(|| 0_u32);
        // The sidebar's Empty Trash reaches the view beside it through this.
        let empty_trash = super::actions::provide_empty_trash();
        // Whether the docked preview pane is open; the toolbar button and the
        // file view share it.
        let preview_pane = use_state(|| false);
        let focus = use_state(|| 0_u32);
        let choices = use_state(|| {
            request
                .choices
                .iter()
                .map(|choice| (choice.id.clone(), choice.selected.clone()))
                .collect::<Vec<_>>()
        });
        let mut editing = use_state(|| false);
        let draft = use_state(String::new);
        let find = use_state(Find::default);
        let include_noise = use_state(|| true);
        let problem = use_state(|| None::<String>);
        let places_rev = use_state(|| 0_u32);
        let places = use_resource(*places_rev.read(), load_places).with(Clone::clone);
        let current = place_of(model.read().location());
        let (load, reload) = use_directory(&current);
        // The subfolder search: running exactly while a search of the
        // subfolders is on show for the folder on show.
        let deep = use_subfolder_search(
            match &current {
                Where::Folder(path) => Some(path.clone()),
                Where::Recent => None,
            },
            find.read().deep_query(),
            *show_hidden.read(),
            *include_noise.read(),
            // The file type filter is the chooser's, so the walk leaves out
            // what it would only hide.
            if matches!(request.action, Action::SelectFolder | Action::SaveFiles) {
                None
            } else {
                (*filter.read()).and_then(|index| filters.get(index).cloned())
            },
            match &*load.read() {
                Load::Ready(listing) => listing.mount,
                _ => bingux_files::mounts::MountKind::Other,
            },
        );

        let nav = Nav {
            model,
            selection,
            editing,
            find,
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
            load,
            reload,
            name,
            filter,
            draft,
            find,
            deep: deep.clone(),
            include_noise,
            focus,
            places_rev,
            choices,
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
        // A tab keeps where it is, so it is there when it comes back.
        if let Host::Browser(browse) = &self.host {
            let browse = browse.clone();
            use_side_effect(move || {
                let model = model.read().clone();
                let at = place_of(model.location());
                browse.memory.set(model);
                browse.on_location.call(at);
            });
        }

        let folder_mode = matches!(request.action, Action::SelectFolder | Action::SaveFiles);
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

        // What is shown: hidden files, the filter and the sort applied.
        // The order is computed with the listing it indexes and travels with
        // it, so a view never pairs one folder's rows with another's order.
        let shown = use_memo({
            let filters = filters.clone();
            let deep = deep.clone();
            move || {
                // A subfolder search replaces the folder's listing with its
                // matches while they are on show.
                let find = find.read().clone();
                let results = find.deep_query().and_then(|query| deep.results_for(&query));
                let listing = match &results {
                    Some(results) => results.listing.clone(),
                    None => match &*load.read() {
                        Load::Ready(listing) => listing.clone(),
                        _ => return None,
                    },
                };
                let active = if folder_mode {
                    None
                } else {
                    (*filter.read()).and_then(|index| filters.get(index))
                };
                let shown = listing.select_counted(&View {
                    // Everything in the Trash is shown, dot files included: a
                    // deleted `.bashrc` is an item to restore, not a hidden one.
                    show_hidden: *show_hidden.read()
                        || (!listing.recent
                            && bingux_files::ops::trash_place(&listing.path).is_some_and(|place| place.is_root())),
                    filter: active,
                    directories_only: false,
                    sort: *sort.read(),
                });
                // What the search chooses among: what the hidden-file
                // setting and the file type filter let through.
                let of = shown.order.len();
                let order = match find.query() {
                    Some(query) => bingux_files::search::rank(&listing.entries, &shown.order, &query),
                    None => shown.order,
                };
                Some(Shown {
                    listing,
                    order: Arc::new(order),
                    hidden_by_filter: shown.hidden_by_filter,
                    of,
                    results,
                })
            }
        });
        let shown = shown.read().clone();
        let order = shown
            .as_ref()
            .map_or_else(|| Arc::new(Vec::new()), |shown| shown.order.clone());
        let current_load = match (&*load.read(), &shown) {
            (Load::Ready(_), Some(shown)) => Load::Ready(shown.listing.clone()),
            (Load::Ready(_), None) => Load::Loading,
            (other, _) => other.clone(),
        };
        let search_view = {
            let find = find.read();
            find.query().zip(shown.as_ref()).map(|(query, shown)| SearchView {
                query,
                of: shown.of,
                deep: shown.results.as_ref().map(|results| DeepStatus {
                    // What the list shows: the file type filter has already
                    // taken its share out of what the walker found.
                    found: shown.order.len(),
                    ..results.status()
                }),
            })
        };
        // A new search text selects its best match, so Enter opens it; with
        // no match nothing stays selected that the list does not show.
        let selected_for = use_hook(|| Rc::new(RefCell::new(None::<(String, bool, bool)>)));
        {
            let key = find
                .read()
                .query()
                .map(|query| {
                    (
                        query.text().to_string(),
                        shown.as_ref().is_some_and(|shown| shown.results.is_some()),
                        // Results that start empty and fill select their first.
                        !order.is_empty(),
                    )
                });
            if *selected_for.borrow() != key {
                *selected_for.borrow_mut() = key.clone();
                if key.is_some()
                    && let Some(shown) = shown.clone()
                {
                    spawn(async move {
                        let mut selection = selection;
                        match shown.order.first() {
                            Some(first) => selection
                                .write()
                                .select_only(&shown.listing.entries[*first].path),
                            None => selection.write().clear(),
                        }
                    });
                }
            }
        }
        let address_search = {
            let session = session.clone();
            let listing = shown.as_ref().map(|shown| shown.listing.clone());
            let order = order.clone();
            let folder = match &current {
                Where::Folder(path) => path
                    .file_name()
                    .map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned()),
                Where::Recent => String::new(),
            };
            let on_step = {
                let (session, listing, order) = (session.clone(), listing.clone(), order.clone());
                move |by: i32| {
                    if let Some(listing) = &listing {
                        session.step_match(listing, &order, by);
                    }
                }
            };
            let on_open = {
                let (session, listing, order) = (session.clone(), listing.clone(), order.clone());
                move |_: ()| {
                    if let Some(listing) = &listing {
                        session.open_match(listing, &order);
                    }
                }
            };
            SearchLink::new(find, selection, folder)
                .listing(match &*load.read() {
                    Load::Ready(listing) => Some(listing.clone()),
                    _ => None,
                })
                .deep(shown.as_ref().and_then(|shown| {
                    shown.results.as_ref().map(|results| DeepStatus {
                        found: shown.order.len(),
                        ..results.status()
                    })
                }))
                .on_step(on_step)
                .on_open(on_open)
                .on_deep({
                    let session = session.clone();
                    move |_: ()| session.start_deep()
                })
                .on_stop({
                    let deep = deep.clone();
                    move |_: ()| deep.stop()
                })
                .on_clear(move |_: ()| session.clear_search())
        };

        let active_filter = (!folder_mode)
            .then(|| (*filter.read()).and_then(|index| filters.get(index)))
            .flatten();
        let use_grid = kind_of(*level.read()) == ViewKind::Grid;

        let shortcuts = {
            let session = session.clone();
            move |event: Event<KeyboardEventData>| {
                let mut draft = draft;
                let alt = event.modifiers.contains(Modifiers::ALT);
                let ctrl = event.modifiers.contains(Modifiers::CONTROL);
                match &event.key {
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
                    // Escape ends a search wherever the keyboard is.
                    Key::Named(NamedKey::Escape) if session.find.peek().scoped => {
                        event.stop_propagation();
                        session.clear_search();
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
            use_grid,
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
            address_search,
        );

        // A phone's sheet reaches the screen's bottom edge, so with no footer
        // the list keeps its own clearance.
        let bottom_clearance = compact && footer.is_none();
        rect()
            .component("FileChooser")
            .tw(&tokens, &[flex_col(), grow_children(), w_full(), h_full()])
            .maybe(bottom_clearance, |root| root.tw(&tokens, &[pb(S4)]))
            .on_key_down(shortcuts)
            .child(toolbar)
            .child(divider(&tokens))
            .child(
                rect()
                    .tw(&tokens, &[flex_row(), grow_children(), w_full(), h_flex()])
                    .maybe_child((!compact).then(|| {
                        let places = places.value().cloned().unwrap_or_default();
                        let nav = nav;
                        rect()
                            .tw(&tokens, &[flex_row(), h_full(), w(Dim::PlacesColumn)])
                            .child(
                                rect().tw(&tokens, &[w_flex(), h_full()]).child(
                                    {
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
                                    },
                                ),
                            )
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

/// The icon size a folder opens at when nothing is remembered for it: 16 px,
/// in a list, unless the app asked for a grid, which opens at a large
/// picture size. Neither the filter nor the size of the window decides it.
fn default_level(request: &ChooserRequest) -> u16 {
    match request.view {
        ViewMode::Grid => REQUESTED_GRID_PX,
        ViewMode::List | ViewMode::Auto => DEFAULT_PX,
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
    use_grid: bool,
    folder_mode: bool,
    filtered: bool,
    search: Option<SearchView>,
    corners: Corners,
) -> Element {
    match load {
        Load::Loading => skeleton(use_grid),
        Load::Failed(error) => error_state(error),
        Load::Ready(listing) => {
            let shown_hidden = *show_hidden.peek();
            let placeholder = order.is_empty().then(|| {
                if let Some(search) = &search {
                    return no_matches_state(
                        search.query.text(),
                        search.deep.as_ref().map(|deep| deep.running),
                    );
                }
                if bingux_files::ops::trash_place(&listing.path).is_some_and(|place| place.is_root()) {
                    return super::states::trash_empty_state();
                }
                let hidden_items =
                    !shown_hidden && listing.entries.iter().any(|entry| entry.hidden);
                let others = listing.entries.iter().any(|entry| !entry.hidden);
                empty_state(listing.recent, hidden_items && !others, filtered && others)
            });
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
            let mut view = FileView::new(listing.clone(), order.clone(), selection)
                .icon_px(level)
                .sort(*sort.peek())
                .multiple(session.request.multiple)
                .folders_only(folder_mode)
                .autofocus(
                    !matches!(session.request.action, Action::Save) && !session.find.peek().scoped,
                )
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
                .on_sort(move |key: SortKey| {
                    let next = sort.peek().toggled(key);
                    sort.set(next);
                    nav.remember_sort(next);
                })
                .on_sort_set(move |next: Sort| {
                    sort.set(next);
                    nav.remember_sort(next);
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
    search: SearchLink,
) -> Element {
    let nav = session.nav;
    let mut places_rev = session.places_rev;
    let current = nav.current();
    let (can_go_back, can_go_forward, back_items, forward_items) = {
        let model = nav.model.read();
        (
            model.can_go_back(),
            model.can_forward(),
            history_menu(model.back_menu(), move |entry| nav.back_to(entry)),
            history_menu(model.forward_menu(), move |entry| nav.forward_to(entry)),
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
    .search(search)
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
    let has_parent = matches!(&current, Where::Folder(path) if path.parent().is_some());
    let parent = icon_button(Icon::GoUp, move |_: ()| nav.up())
        .size(Dim::Control)
        .tooltip("Parent folder")
        .enabled(has_parent);
    // Recent has no folder to read again.
    let reload = session.reload.clone();
    let busy = reload.busy();
    let refresh = icon_button(Icon::Refresh, move |_: ()| reload.run())
        .size(Dim::Control)
        .tooltip("Refresh")
        .enabled(matches!(current, Where::Folder(_)))
        .busy(busy);
    let can_create = matches!(current, Where::Folder(_));
    let create = icon_button(Icon::FolderNew, move |_: ()| {
        let next = *new_folder.peek() + 1;
        new_folder.set(next);
    })
    .size(Dim::Control)
    .tooltip("New folder")
    .enabled(can_create);
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
            if use_grid { ViewKind::List } else { ViewKind::Grid },
        );
        level.set(next);
        nav.remember_level(next);
    })
    .menu(view_items(nav, sort, show_hidden, session.include_noise));

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
            .child(parent)
            .child(create)
            .child(view);
        return rect()
            .tw(tokens, &[flex_col(), w_full(), p(S2), gap(S2)])
            .child(first)
            .child(path_bar)
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
        .child(create)
        .child(pane)
        .child(view)
        .into()
}

/// The sort and hidden-file choices of the view menu.
fn view_items(
    nav: Nav,
    mut sort: State<Sort>,
    mut show_hidden: State<bool>,
    mut include_noise: State<bool>,
) -> Vec<MenuItem> {
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
        menu_item("Search build and dependency folders", move |_: ()| {
            let included = *include_noise.peek();
            include_noise.set(!included);
        })
        .checked(*include_noise.read()),
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
