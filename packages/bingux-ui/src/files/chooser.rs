//! The file chooser dialog.
//!
//! [`choose`] opens a popup over a window. On a desktop window it is a panel
//! with the places sidebar beside the file area; on a phone it is a sheet
//! with a places menu above the file area. The header carries Cancel and the
//! accept button (Open, Select or Save), as every popup does.
//!
//! Keyboard: Tab walks the toolbar, the list and the footer. In the list the
//! arrows, Home, End and Page keys move; typing letters jumps to a name;
//! Enter opens. Ctrl+L, or typing `/` or `~` in the list, starts typing a
//! path. Alt+Up goes to the parent folder, Alt+Left back, Ctrl+H shows or
//! hides hidden files. Escape cancels.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use bingux_files::request::accept;
use bingux_files::{
    Accept, Action, Choice, ChooserRequest, Chosen, Entry, Listing, Selection, Sort, SortKey, View,
    ViewMode, location,
};
use freya::prelude::*;

use super::directory::{Load, Where, home_folder, use_directory, use_places};
use super::file_grid::FileGrid;
use super::file_list::FileList;
use super::glyphs::place_icon;
use super::path_bar::PathBar;
use super::places_view::PlacesSidebar;
use super::states::{empty_state, error_state, loading_state};
use crate::component::ComponentName;
use crate::controls::{checkbox, dropdown, icon_button, text_field};
use crate::group::{Status, status};
use crate::icons::Icon;
use crate::layout::divider;
use crate::shell::{
    MenuItem, Overlays, ResponseKind, dialog, menu_button, menu_item, menu_separator, use_overlays,
};
use crate::style::*;
use crate::surface::use_tokens;
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
        popup(title, ChooserBody { request, done })
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

/// Moving between folders. All `State`s, so it is `Copy`.
#[derive(Clone, Copy)]
struct Nav {
    location: State<Where>,
    history: State<Vec<Where>>,
    selection: State<Selection>,
    editing: State<bool>,
    problem: State<Option<String>>,
    sort: State<Sort>,
}

impl Nav {
    fn arrive(mut self, at: Where) {
        // The recent files read newest first; a folder reads by name.
        let recent = matches!(at, Where::Recent);
        if recent != matches!(*self.location.peek(), Where::Recent) {
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
        self.location.set(at);
        self.selection.write().clear();
        self.editing.set(false);
        self.problem.set(None);
    }

    fn go(mut self, to: Where) {
        let from = self.location.peek().clone();
        if from == to {
            return;
        }
        self.history.write().push(from);
        self.arrive(to);
    }

    fn back(mut self) {
        let previous = self.history.write().pop();
        if let Some(previous) = previous {
            self.arrive(previous);
        }
    }

    fn up(self) {
        let here = self.location.peek().clone();
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
    home: PathBuf,
    nav: Nav,
    load: State<Load>,
    name: State<String>,
    filter: State<Option<usize>>,
    choices: State<Vec<(String, String)>>,
    window: Window,
    overlays: Overlays,
    done: Done,
}

impl Session {
    /// The folder shown, or home when the recent files are.
    fn folder(&self) -> PathBuf {
        match &*self.nav.location.peek() {
            Where::Folder(path) => path.clone(),
            Where::Recent => self.home.clone(),
        }
    }

    fn listing(&self) -> Option<Arc<Listing>> {
        match &*self.load.peek() {
            Load::Ready(listing) => Some(listing.clone()),
            _ => None,
        }
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

    fn finish(&self, paths: Vec<PathBuf>) {
        self.done.finish(Some(Chosen {
            paths,
            current_filter: *self.filter.peek(),
            choices: self.choices.peek().clone(),
        }));
        self.window.close();
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
            .show_dialog(dialog(title, body).cancel("Cancel").response(
                "Replace",
                ResponseKind::Destructive,
                move |_: ()| session.finish(paths.clone()),
            ));
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

    /// Whether the accept button is ready.
    fn can_accept(&self) -> bool {
        if *self.nav.editing.peek() {
            return false;
        }
        match self.request.action {
            Action::Open => !self.nav.selection.peek().is_empty(),
            Action::SelectFolder | Action::SaveFiles => {
                matches!(*self.nav.location.peek(), Where::Folder(_))
            }
            Action::Save => !self.name.peek().trim().is_empty(),
        }
    }

    /// Open what a person typed in the path bar.
    fn open_typed(&self, text: &str) {
        let mut problem = self.nav.problem;
        let target = location::resolve(text, &self.folder(), &self.home);
        match std::fs::metadata(&target) {
            Ok(meta) if meta.is_dir() => self.nav.go(Where::Folder(target)),
            Ok(_) if self.request.action == Action::Open => {
                // A file typed in full is the answer.
                self.finish(vec![target]);
            }
            Ok(_) => problem.set(Some("That is a file, not a folder.".into())),
            Err(_) => problem.set(Some(format!("{} does not exist.", target.display()))),
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

/// Whether a filter lets only pictures through, so a grid of thumbnails suits
/// it better than a list.
fn only_pictures(filter: &bingux_files::Filter) -> bool {
    !filter.rules.is_empty()
        && filter.rules.iter().all(|rule| match rule {
            bingux_files::Rule::Mime(mime) => mime.starts_with("image/"),
            bingux_files::Rule::Glob(pattern) => {
                bingux_files::mime::for_name(&format!("x{}", pattern.trim_start_matches('*')))
                    .category
                    == bingux_files::Category::Image
            }
        })
}

#[derive(PartialEq)]
struct ChooserBody {
    request: ChooserRequest,
    done: Done,
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

        let location = use_state({
            let start = request.initial_folder(&home);
            move || Where::Folder(start)
        });
        let history = use_state(Vec::<Where>::new);
        let selection = use_state(Selection::new);
        let sort = use_state(Sort::default);
        let mut show_hidden = use_state(|| false);
        let filter = use_state(|| request.initial_filter());
        let name = use_state(|| request.initial_name());
        let grid = use_state(|| None::<bool>);
        let choices = use_state(|| {
            request
                .choices
                .iter()
                .map(|choice| (choice.id.clone(), choice.selected.clone()))
                .collect::<Vec<_>>()
        });
        let mut editing = use_state(|| false);
        let draft = use_state(String::new);
        let problem = use_state(|| None::<String>);
        let places = use_places();
        let current = location.read().clone();
        let load = use_directory(&current);

        let nav = Nav {
            location,
            history,
            selection,
            editing,
            problem,
            sort,
        };
        let session = Session {
            request: request.clone(),
            home: home.clone(),
            nav,
            load,
            name,
            filter,
            choices,
            window,
            overlays,
            done: self.done.clone(),
        };

        // A popup that goes away, however it goes, is a cancel.
        if let Window::Popup(popup_window) = window
            && let Some(entry) = popup_window.entry()
        {
            let closing = entry.closing;
            let done = self.done.clone();
            use_side_effect(move || {
                if *closing.read() {
                    done.finish(None);
                }
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
            let request = request.clone();
            move || match &*load.read() {
                Load::Ready(listing) => {
                    let active = if folder_mode {
                        None
                    } else {
                        (*filter.read()).and_then(|index| request.filters.get(index))
                    };
                    let order = Arc::new(listing.select(&View {
                        show_hidden: *show_hidden.read(),
                        filter: active,
                        directories_only: false,
                        sort: *sort.read(),
                    }));
                    Some((listing.clone(), order))
                }
                _ => None,
            }
        });
        let shown = shown.read().clone();
        let order = shown
            .as_ref()
            .map_or_else(|| Arc::new(Vec::new()), |(_, order)| order.clone());
        let current_load = match (&*load.read(), &shown) {
            (Load::Ready(_), Some((listing, _))) => Load::Ready(listing.clone()),
            (Load::Ready(_), None) => Load::Loading,
            (other, _) => other.clone(),
        };

        let active_filter = (!folder_mode)
            .then(|| (*filter.read()).and_then(|index| request.filters.get(index)))
            .flatten();
        let use_grid = grid.read().unwrap_or_else(|| match request.view {
            ViewMode::Grid => true,
            ViewMode::List => false,
            ViewMode::Auto => active_filter.is_some_and(only_pictures),
        }) && !compact_list_only(compact);

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
                    Key::Character(text) if ctrl && text.eq_ignore_ascii_case("l") => {
                        event.stop_propagation();
                        draft.set(PathBar::text_of(&session.nav.location.peek()));
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

        let main = main_area(
            &session,
            &current_load,
            &order,
            selection,
            sort,
            use_grid,
            folder_mode,
            *show_hidden.read(),
            active_filter.is_some(),
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
            grid,
            history,
        );
        let footer = footer(&tokens, compact, &session, saving, problem);

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
                                    PlacesSidebar::new(
                                        places,
                                        current.clone(),
                                        move |to: Where| nav.go(to),
                                    )
                                    .recent(request.action == Action::Open),
                                ),
                            )
                            .child(rect().tw(&tokens, SPLIT_HAIRLINE))
                    }))
                    .child(rect().tw(&tokens, &[w_flex(), h_full()]).child(main)),
            )
            .maybe_child(footer.map(|footer| {
                rect()
                    .tw(&tokens, &[flex_col(), w_full()])
                    .child(divider(&tokens))
                    .child(footer)
            }))
    }
}

/// On a phone the grid is for thumbnails only when asked; a list reads better
/// at a finger's size. The switch is still the person's.
fn compact_list_only(_compact: bool) -> bool {
    false
}

/// The list, the grid or the state that stands in for them.
#[allow(clippy::too_many_arguments)]
fn main_area(
    session: &Session,
    load: &Load,
    order: &Arc<Vec<usize>>,
    selection: State<Selection>,
    mut sort: State<Sort>,
    use_grid: bool,
    folder_mode: bool,
    show_hidden: bool,
    filtered: bool,
) -> Element {
    match load {
        Load::Loading => loading_state(),
        Load::Failed(error) => error_state(error),
        Load::Ready(listing) if order.is_empty() => {
            let hidden_items = !show_hidden && listing.entries.iter().any(|entry| entry.hidden);
            let others = listing.entries.iter().any(|entry| !entry.hidden);
            empty_state(listing.recent, hidden_items && !others, filtered && others)
        }
        Load::Ready(listing) => {
            let multiple = session.request.multiple;
            let pick = {
                let session = session.clone();
                move |index: usize| session.picked(index)
            };
            let activate = {
                let session = session.clone();
                move |index: usize| session.activate(index)
            };
            if use_grid {
                FileGrid::new(listing.clone(), order.clone(), selection)
                    .multiple(multiple)
                    .folders_only(folder_mode)
                    .autofocus(!matches!(session.request.action, Action::Save))
                    .on_pick(pick)
                    .on_activate(activate)
                    .into()
            } else {
                FileList::new(listing.clone(), order.clone(), selection)
                    .sort(*sort.peek())
                    .multiple(multiple)
                    .folders_only(folder_mode)
                    .autofocus(!matches!(session.request.action, Action::Save))
                    .on_sort(move |key: SortKey| {
                        let next = sort.peek().toggled(key);
                        sort.set(next);
                    })
                    .on_pick(pick)
                    .on_activate(activate)
                    .into()
            }
        }
    }
}

/// Back, the path, the view switch and the view menu; on a phone the places
/// menu comes first and the path moves to a line of its own.
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
    mut grid: State<Option<bool>>,
    history: State<Vec<Where>>,
) -> Element {
    let nav = session.nav;
    let current = nav.location.read().clone();
    let can_go_back = !history.read().is_empty();
    let path_bar = PathBar::new(
        current.clone(),
        session.home.clone(),
        editing,
        draft,
        move |to: Where| nav.go(to),
        {
            let session = session.clone();
            move |text: String| session.open_typed(&text)
        },
    )
    .folders_only(folder_mode);

    let back = icon_button(Icon::GoPrevious, move |_: ()| nav.back())
        .size(Dim::Control)
        .tooltip("Back")
        .enabled(can_go_back);
    let has_parent = matches!(&current, Where::Folder(path) if path.parent().is_some());
    let parent = icon_button(Icon::PanUp, move |_: ()| nav.up())
        .size(Dim::Control)
        .tooltip("Parent folder")
        .enabled(has_parent);
    let view_switch = icon_button(
        if use_grid {
            Icon::ViewList
        } else {
            Icon::ViewGrid
        },
        move |_: ()| grid.set(Some(!use_grid)),
    )
    .size(Dim::Control)
    .tooltip(if use_grid {
        "Show a list"
    } else {
        "Show thumbnails"
    });
    let view_menu = menu_button("View options", view_items(sort, show_hidden)).icon(Icon::OpenMenu);

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
                rect()
                    .tw(tokens, &[w_flex()])
                    .child(places_menu(places, &current, session)),
            )
            .child(parent)
            .child(view_switch)
            .child(view_menu);
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
        .child(parent)
        .child(rect().tw(tokens, &[w_flex()]).child(path_bar))
        .child(view_switch)
        .child(view_menu)
        .into()
}

/// The sort and hidden-file choices of the view menu.
fn view_items(mut sort: State<Sort>, mut show_hidden: State<bool>) -> Vec<MenuItem> {
    let now = *sort.read();
    let by = |label: &str, key: SortKey| {
        menu_item(label, move |_: ()| {
            let current = *sort.peek();
            sort.set(if current.key == key {
                current
            } else {
                Sort {
                    key,
                    descending: false,
                    ..current
                }
            });
        })
        .checked(now.key == key)
    };
    vec![
        by("Sort by name", SortKey::Name),
        by("Sort by size", SortKey::Size),
        by("Sort by modified", SortKey::Modified),
        menu_item("Reverse order", move |_: ()| {
            let current = *sort.peek();
            sort.set(Sort {
                descending: !current.descending,
                ..current
            });
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

/// A phone's replacement for the sidebar: a menu of every place.
fn places_menu(places: &bingux_files::Places, current: &Where, session: &Session) -> Element {
    let all: Vec<&bingux_files::Place> = places
        .all()
        .filter(|place| {
            session.request.action == Action::Open || place.kind != bingux_files::PlaceKind::Recent
        })
        .collect();
    let target_of = |place: &bingux_files::Place| match &place.path {
        Some(path) => Where::Folder(path.clone()),
        None => Where::Recent,
    };
    let mut options: Vec<(String, String)> = all
        .iter()
        .enumerate()
        .map(|(index, place)| (index.to_string(), place.name.clone()))
        .collect();
    let mut glyphs: Vec<Icon> = all.iter().map(|place| place_icon(place.kind)).collect();
    let selected = match all.iter().position(|place| target_of(place) == *current) {
        Some(index) => index.to_string(),
        None => {
            // Somewhere that is not a place: name it, so the menu says where you are.
            let label = match current {
                Where::Folder(path) => path.file_name().map_or_else(
                    || "Computer".to_string(),
                    |n| n.to_string_lossy().into_owned(),
                ),
                Where::Recent => "Recent".to_string(),
            };
            options.push(("here".into(), label));
            glyphs.push(Icon::Folder);
            "here".into()
        }
    };
    let targets: Vec<Where> = all.iter().map(|place| target_of(place)).collect();
    let nav = session.nav;
    Element::from(
        dropdown(options, selected, move |key: String| {
            if let Some(target) = key
                .parse::<usize>()
                .ok()
                .and_then(|index| targets.get(index))
            {
                nav.go(target.clone());
            }
        })
        .icons(glyphs)
        .title("Places"),
    )
}

/// The bottom strip: the name to save under, the file type filter, the
/// request's choices and any problem. `None` when there is nothing to show.
fn footer(
    tokens: &crate::theme::Tokens,
    compact: bool,
    session: &Session,
    saving: bool,
    problem: State<Option<String>>,
) -> Option<Element> {
    let request = &session.request;
    let show_filters =
        request.filters.len() > 1 && matches!(request.action, Action::Open | Action::Save);
    let message = problem.read().clone();
    if !saving && !show_filters && request.choices.is_empty() && message.is_none() {
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
    let picker = show_filters.then(|| {
        let options: Vec<(String, String)> = request
            .filters
            .iter()
            .enumerate()
            .map(|(index, filter)| (index.to_string(), filter.name.clone()))
            .collect();
        let selected = session.filter.read().unwrap_or(0).to_string();
        let mut filter = session.filter;
        let nav = session.nav;
        let picker = dropdown(options, selected, move |key: String| {
            filter.set(key.parse().ok());
            let mut selection = nav.selection;
            selection.write().clear();
        })
        .title("File type");
        Element::from(if compact {
            picker
        } else {
            picker.min_width(Dim::Field)
        })
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
    if compact {
        column = column
            .maybe_child(name_field)
            .maybe_child(picker.map(|picker| {
                Element::from(
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
                        .child(picker),
                )
            }));
    } else if name_field.is_some() || picker.is_some() {
        column = column.child(
            rect()
                .tw(
                    tokens,
                    &[
                        flex_row(),
                        grow_children(),
                        items(Align::Center),
                        w_full(),
                        gap(S3),
                    ],
                )
                .child(rect().tw(tokens, &[w_flex()]).maybe_child(name_field))
                .maybe_child(picker),
        );
    }
    Some(column.into())
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
