//! The file view: the list or the grid of a folder, with everything a person
//! does in it.
//!
//! [`FileView`] is what an app mounts. It shows the entries as a list with
//! columns or as a grid of tiles, and adds what both share:
//!
//! - selection with the pointer and the keyboard, a rubber band, and
//!   dragging files onto a folder;
//! - context menus on a file, on several files, on empty space and on the
//!   column headers, and the keyboard commands behind them (see
//!   `actions.rs`);
//! - inline rename, a status line, a detail card on hover;
//! - zoom, which changes the icon size and with it the view: a list at small
//!   icons, a grid at large ones (see `zoom.rs`), with Ctrl and the wheel,
//!   Ctrl and plus or minus, or a pinch;
//! - the scroll position and the selection of a folder, remembered for when
//!   the person comes back.
//!
//! Only the rows in view exist, so a folder of a hundred thousand files
//! scrolls as smoothly as one of ten. A row never holds a closure over the
//! listing: rows read the current state through [`Shared`], so a row that
//! did not change is not drawn again.
//!
//! # Keys
//!
//! These work while the list or the grid has focus. The chooser keeps
//! Ctrl+L, `/`, `~`, Alt+Up, Alt+Left, Ctrl+H and Backspace.
//!
//! | Key | Does |
//! | --- | ---- |
//! | arrows, Home, End, Page Up and Down | move the cursor; Shift extends |
//! | Enter | open the cursor row |
//! | letters | search the folder in the address bar when the owner offers a search ([`FileView::on_search`]); else jump to a name that starts with them |
//! | Ctrl+F | start that search |
//! | Space | Quick Look: preview the selected file (see `preview.rs`) |
//! | Ctrl+A, Ctrl+Space | select all, toggle the cursor row |
//! | F2, Delete, Shift+Delete | rename, move to the Trash, delete permanently (asks first, and cannot be undone) |
//! | Ctrl+C, X, V, Z | copy, cut, paste, undo |
//! | Ctrl+Shift+N, F5 | new folder, refresh |
//! | Alt+Enter, Ctrl+I | properties |
//! | Menu, Shift+F10 | the context menu of the cursor row |
//! | Ctrl+plus, minus, 0 | one icon size up, down; back to 16 px in a list |
//! | Ctrl+1, Ctrl+2 | show a list, show a grid (the nearest icon size of each) |
//! | Ctrl+Alt+1 to 4, Ctrl+Alt+R | sort by name, size, type, date; reverse |
//! | Escape | cancel a rename, a drag or a rubber band |

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use bingux_files::selection::{TypeAhead, type_ahead_target};
use bingux_files::{Entry, Listing, Selection, Sort, SortKey, display};
use freya::prelude::*;

use super::actions::{self, Capabilities, Command, Env, ViewControls, use_ui};
use super::drag::FileDrag;
use super::columns::{ColumnHeaders, ColumnLayout, plan_in, row_padding};
use super::context_menu;
use super::directory::Wants;
use super::file_grid::GridView;
use super::search::SearchView;
use super::scroll_memory::{Owner, SearchTop, came_from, recall, remember};
use super::file_list::ListView;
use super::interaction::{DRAG_SLOP, Hits, Lattice, Span, reveal_offset};
use super::status_line::{SearchLine, StatusInfo, status_line};
use super::view_state;
use super::zoom::{DEFAULT_PX, Pinch, ViewKind, Zoom, cap_for_width, kind_of, use_pinch, use_zoom, zoom_hud};
use crate::component::ComponentName;
use crate::controls::press::{Press, is_touch, point_of, use_press};
use crate::motion::Focusable;
use crate::shell::panel::{PanelRect, PanelTrigger};
use crate::shell::use_overlays;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, CAPTION, Ink};

/// Two presses on one row this close together open it.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// A press this soon after a drag ended is the end of the drag, not a click.
const DRAG_SETTLE: Duration = Duration::from_millis(120);
/// The pointer rests this long on a row before its detail card shows.
const CARD_DELAY: Duration = Duration::from_millis(700);
/// The edge of a view where a band or drag scrolls it, as a share of one row.
const AUTOSCROLL_EDGE: f32 = 1.0;

// ---------------------------------------------------------------------------
// The rows a view shows
// ---------------------------------------------------------------------------

/// A listing and the order its entries appear in.
#[derive(Clone)]
pub(super) struct Rows {
    pub listing: Arc<Listing>,
    /// Indices into `listing.entries`, in display order.
    pub order: Arc<Vec<usize>>,
}

impl PartialEq for Rows {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.listing, &other.listing) && Arc::ptr_eq(&self.order, &other.order)
    }
}

impl Rows {
    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn entry(&self, row: usize) -> &Entry {
        &self.listing.entries[self.order[row]]
    }

    pub fn paths(&self) -> Vec<&Path> {
        (0..self.len())
            .map(|row| self.entry(row).path.as_path())
            .collect()
    }

    pub fn row_of(&self, path: &Path) -> Option<usize> {
        (0..self.len()).find(|row| self.entry(*row).path == path)
    }
}

/// The picture of an entry: a folder in a person's own folders has its own.
pub(super) fn entry_glyph(entry: &Entry) -> crate::icons::Icon {
    if entry.is_dir() {
        let special = entry
            .path
            .parent()
            .and_then(|parent| bingux_files::places::special_folder_in(parent, entry.path.file_name()?));
        super::glyphs::folder_icon(special)
    } else {
        super::glyphs::entry_icon(entry)
    }
}

// ---------------------------------------------------------------------------
// Gestures that outlive one event
// ---------------------------------------------------------------------------

/// A press on a row that may become a drag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Pending {
    pub origin: (f32, f32),
    pub row: usize,
}

/// Files being dragged.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Drag {
    pub paths: Vec<PathBuf>,
    pub label: String,
}

/// A rubber band, in content coordinates.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Band {
    pub start: (f32, f32),
    pub current: (f32, f32),
    /// Ctrl held at the start: the band adds to the selection.
    pub additive: bool,
    pub base: Vec<PathBuf>,
    pub last: Option<Hits>,
    /// The pointer has left the slop circle.
    pub moved: bool,
}

/// The row the pointer rests on, for the detail card.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Hover {
    pub row: usize,
    pub at: (f32, f32),
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct Gestures {
    pub pending: State<Option<Pending>>,
    pub drag: State<Option<Drag>>,
    /// The folder row a drag is over.
    pub over: State<Option<usize>>,
    /// Where the pointer is in the window, while something is dragged.
    pub pointer: State<(f32, f32)>,
    pub band: State<Option<Band>>,
    pub hover: State<Option<Hover>>,
    /// When the last drag ended.
    pub ended: State<Option<Instant>>,
    /// The window's file drag, when its host shares one with drop targets
    /// outside this view (see [`super::drag`]).
    pub session: Option<FileDrag>,
}

fn use_gestures() -> Gestures {
    Gestures {
        session: super::drag::file_drag(),
        pending: use_state(|| None),
        drag: use_state(|| None),
        over: use_state(|| None),
        pointer: use_state(|| (0.0, 0.0)),
        band: use_state(|| None),
        hover: use_state(|| None),
        ended: use_state(|| None),
    }
}

// ---------------------------------------------------------------------------
// The context rows and tiles share
// ---------------------------------------------------------------------------

/// Everything a row, a tile and the handlers of the view need, as of the
/// latest render.
pub(super) struct Ctx {
    pub rows: Rows,
    pub env: Env,
    pub multiple: bool,
    pub folders_only: bool,
    pub on_pick: EventHandler<usize>,
    pub gestures: Gestures,
    pub press: Press,
    pub pinch: Pinch,
    pub zoom: Zoom,
    pub scroll: ScrollController,
    pub focus: AccessibilityId,
    /// The scroll viewport, in window coordinates.
    pub viewport: State<Area>,
    pub lattice: Rc<Cell<Option<Lattice>>>,
    /// The zoom factor, pitch and columns of the last frame, for anchoring.
    pub zoom_seen: Rc<Cell<Option<(f32, f32, usize)>>>,
    pub last_click: State<Option<(Instant, usize)>>,
    /// Thumbnails are allowed in this folder (not on a network mount).
    pub thumbnails: bool,
    /// The Recent place: dates read as how long ago, and a Location column.
    pub recent: bool,
    /// What a search is doing to the list, for marking names and for the
    /// Location column of subfolder results.
    pub search: Option<SearchView>,
    /// A letter typed on the list, or Ctrl+F, starts a search; see
    /// [`FileView::on_search`].
    pub on_search: Option<EventHandler<String>>,
    pub home: PathBuf,
    pub wants: Wants,
}

/// The latest [`Ctx`], shared by the view and its rows. Equality is identity,
/// so a row whose own data did not change is not drawn again.
#[derive(Clone)]
pub(super) struct Shared(Rc<RefCell<Rc<Ctx>>>);

impl PartialEq for Shared {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// A [`Shared`] that does not keep the view alive, for a handler the view's
/// own [`Ctx`] holds (Quick Look, opened from the menu).
pub(super) struct WeakShared(std::rc::Weak<RefCell<Rc<Ctx>>>);

impl WeakShared {
    pub fn upgrade(&self) -> Option<Shared> {
        self.0.upgrade().map(Shared)
    }
}

impl Shared {
    pub fn get(&self) -> Rc<Ctx> {
        self.0.borrow().clone()
    }

    pub fn downgrade(&self) -> WeakShared {
        WeakShared(Rc::downgrade(&self.0))
    }

    fn set(&self, ctx: Ctx) {
        *self.0.borrow_mut() = Rc::new(ctx);
    }
}

impl Ctx {
    /// The list shows what a search of the subfolders found, not a folder.
    pub fn in_results(&self) -> bool {
        self.search.as_ref().is_some_and(|search| search.deep.is_some())
    }

    /// Rows carry a Location column: the recent files, or search results
    /// from many folders.
    pub fn location_column(&self) -> bool {
        self.recent || self.in_results()
    }

    /// The folder of a search result below the folder searched, as the
    /// Location column and the detail card tell it. `None` outside results.
    pub fn result_folder(&self, entry: &Entry) -> Option<String> {
        let deep = self.search.as_ref()?.deep.as_ref()?;
        let folder = bingux_files::search::relative_folder(&entry.path, &deep.root);
        Some(if folder.is_empty() { "This folder".to_string() } else { folder })
    }

    /// The bytes of `name` the search matched, to draw marked.
    pub fn marks(&self, name: &str) -> Vec<std::ops::Range<usize>> {
        self.search
            .as_ref()
            .map(|search| search.query.highlight(name))
            .unwrap_or_default()
    }

    pub fn selectable(&self, entry: &Entry) -> bool {
        !self.folders_only || entry.is_dir()
    }

    pub fn cut_dim(&self, path: &Path) -> bool {
        actions::is_cut(path)
    }

    /// Entries that are selected, in listing order of the selection.
    pub fn selected_refs(&self) -> Vec<&Entry> {
        let selection = self.env.selection.peek();
        let paths = selection.paths();
        if paths.len() <= 4 {
            return paths
                .iter()
                .filter_map(|path| {
                    self.rows
                        .listing
                        .entries
                        .iter()
                        .find(|entry| entry.path == *path)
                })
                .collect();
        }
        let by_path: HashMap<&Path, &Entry> = self
            .rows
            .listing
            .entries
            .iter()
            .map(|entry| (entry.path.as_path(), entry))
            .collect();
        paths
            .iter()
            .filter_map(|path| by_path.get(path.as_path()).copied())
            .collect()
    }

    pub fn selected_owned(&self) -> Vec<Entry> {
        self.selected_refs().into_iter().cloned().collect()
    }

    /// A row was pressed. `touch` is a finger: a tap opens a folder at once
    /// and selects (or toggles) a file.
    pub fn pressed(&self, row: usize, touch: bool) {
        if row >= self.rows.len() || self.pinch.active() {
            return;
        }
        if self
            .gestures
            .ended
            .peek()
            .is_some_and(|ended| ended.elapsed() < DRAG_SETTLE)
        {
            return;
        }
        let entry = self.rows.entry(row);
        if !self.selectable(entry) {
            return;
        }
        let entry_index = self.rows.order[row];
        let path = entry.path.clone();
        let mut selection = self.env.selection;
        if touch {
            if entry.is_dir() {
                selection.write().select_only(&path);
                self.env.activate.call(entry_index);
            } else if self.multiple {
                selection.write().toggle(&path);
                self.on_pick.call(entry_index);
            } else {
                selection.write().select_only(&path);
                self.on_pick.call(entry_index);
            }
            return;
        }
        let held = freya_winit::modifiers::live();
        if self.multiple && held.contains(Modifiers::CONTROL) {
            selection.write().toggle(&path);
            self.on_pick.call(entry_index);
        } else if self.multiple && held.contains(Modifiers::SHIFT) {
            let paths = self.rows.paths();
            selection.write().extend_to(&path, &paths);
            self.on_pick.call(entry_index);
        } else {
            let now = Instant::now();
            let mut last_click = self.last_click;
            let double = last_click.peek().is_some_and(|(then, previous)| {
                previous == row && now.duration_since(then) < DOUBLE_CLICK
            });
            last_click.set(Some((now, row)));
            selection.write().select_only(&path);
            self.on_pick.call(entry_index);
            if double {
                self.env.activate.call(entry_index);
            }
        }
    }

    /// Select the row at `target` (extending with Shift when several are
    /// allowed) and report it.
    pub fn go_to(&self, target: usize, extend: bool) {
        let entry = self.rows.entry(target);
        let mut selection = self.env.selection;
        if extend && self.multiple {
            let paths = self.rows.paths();
            selection.write().extend_to(&entry.path, &paths);
        } else {
            selection.write().select_only(&entry.path);
        }
        self.on_pick.call(self.rows.order[target]);
    }

    /// The row the cursor is on.
    pub fn cursor_row(&self) -> Option<usize> {
        let selection = self.env.selection.peek();
        selection.cursor().and_then(|path| self.rows.row_of(path))
    }

    /// Ctrl+A.
    pub fn select_all(&self) {
        if self.multiple {
            let paths: Vec<&Path> = (0..self.rows.len())
                .filter(|row| self.selectable(self.rows.entry(*row)))
                .map(|row| self.rows.entry(row).path.as_path())
                .collect();
            let mut selection = self.env.selection;
            selection.write().select_all(&paths);
        }
    }

    /// Typing a letter: the row it jumps to.
    pub fn type_ahead(&self, typed: &str) -> Option<usize> {
        type_ahead_target(
            self.rows.len(),
            |row| self.rows.entry(row).name.as_str(),
            self.cursor_row(),
            typed,
        )
        .filter(|row| self.selectable(self.rows.entry(*row)))
    }

    /// Scrolls so `row` is fully in view.
    pub fn reveal(&self, row: usize) {
        let Some(lattice) = self.lattice.get() else {
            return;
        };
        let columns = lattice.columns.max(1);
        let pitch = lattice.item_h + lattice.gap_y;
        let lines = self.rows.len().div_ceil(columns);
        let content = lattice.pad_y + lines as f32 * pitch;
        let mut scroll = self.scroll;
        let offset = -(<(i32, i32)>::from(scroll).1 as f32);
        // The first line has its own padding above it.
        let wanted = reveal_offset(
            row / columns,
            pitch,
            offset - lattice.pad_y,
            content,
            self.viewport.peek().height() - lattice.pad_y,
        ) + lattice.pad_y.min(0.0);
        let target = if row / columns == 0 { 0.0 } else { wanted };
        scroll.scroll_to_y(-(target.round() as i32));
    }

    // ---- menus ---------------------------------------------------------

    fn show_menu(&self, trigger: PanelTrigger, items: Vec<crate::shell::MenuItem>, keyboard: bool) {
        if items.is_empty() {
            return;
        }
        self.env
            .overlays
            .show_menu_from(Some(self.focus), trigger, items, keyboard);
    }

    /// The menu of `row`, opened at a point or from the keyboard. A row that
    /// is not selected becomes the selection first.
    pub fn row_menu(&self, row: usize, trigger: PanelTrigger, keyboard: bool) {
        if row >= self.rows.len() {
            return;
        }
        let path = self.rows.entry(row).path.clone();
        if !self.env.selection.peek().contains(&path) {
            let mut selection = self.env.selection;
            selection.write().select_only(&path);
            self.on_pick.call(self.rows.order[row]);
        }
        let selected = self.selected_owned();
        // Shift held while the menu opens adds Delete permanently to it.
        let shift = freya_winit::modifiers::live().contains(Modifiers::SHIFT);
        self.show_menu(
            trigger,
            context_menu::menu_for_entries(&self.env, &selected, shift, keyboard),
            keyboard,
        );
    }

    pub fn empty_menu(&self, trigger: PanelTrigger, keyboard: bool) {
        self.show_menu(trigger, context_menu::menu_for_empty(&self.env), keyboard);
    }

    pub fn header_menu(&self, at: (f32, f32)) {
        self.show_menu(
            PanelTrigger::Pointer(at),
            context_menu::menu_for_header(&self.env),
            false,
        );
    }

    /// Menu key or Shift+F10: the menu of the cursor row, or of empty space.
    pub fn keyboard_menu(&self) {
        let area = *self.viewport.peek();
        let Some(lattice) = self.lattice.get() else {
            return;
        };
        let Some(row) = self.cursor_row().filter(|_| !self.env.selection.peek().is_empty())
        else {
            self.empty_menu(
                PanelTrigger::Keyboard(PanelRect {
                    x: area.min_x(),
                    y: area.min_y(),
                    width: area.width(),
                    height: 0.0,
                }),
                true,
            );
            return;
        };
        let columns = lattice.columns.max(1);
        let scroll = <(i32, i32)>::from(self.scroll).1 as f32;
        let x = area.min_x()
            + lattice.pad_x
            + (row % columns) as f32 * (lattice.item_w + lattice.gap_x);
        let y = area.min_y()
            + lattice.pad_y
            + (row / columns) as f32 * (lattice.item_h + lattice.gap_y)
            + scroll;
        let rect = PanelRect {
            x,
            y: y.clamp(area.min_y(), area.max_y() - lattice.item_h.min(area.height())),
            width: lattice.item_w.min(area.width()),
            height: lattice.item_h,
        };
        self.row_menu(row, PanelTrigger::Keyboard(rect), true);
    }

    // ---- the keyboard --------------------------------------------------

    /// The keys both views share. Returns true when the key was used.
    pub fn common_key(&self, event: &Event<KeyboardEventData>, typed: &RefCell<TypeAhead>) -> bool {
        let ctrl = event.modifiers.contains(Modifiers::CONTROL);
        let alt = event.modifiers.contains(Modifiers::ALT);
        let shift = event.modifiers.contains(Modifiers::SHIFT);
        if self.env.ui.rename.peek().is_some() {
            return false;
        }
        if self.zoom.key(event) {
            return true;
        }
        match &event.key {
            Key::Named(NamedKey::Escape) => {
                let mut gestures = self.gestures;
                let busy = gestures.drag.peek().is_some() || gestures.band.peek().is_some();
                gestures.drag.set_if_modified(None);
                gestures.band.set_if_modified(None);
                gestures.pending.set_if_modified(None);
                gestures.over.set_if_modified(None);
                if let Some(session) = gestures.session {
                    session.end();
                }
                return busy;
            }
            Key::Named(NamedKey::ContextMenu) => {
                self.keyboard_menu();
                return true;
            }
            Key::Named(NamedKey::F10) if shift && !ctrl && !alt => {
                self.keyboard_menu();
                return true;
            }
            Key::Named(NamedKey::Enter) if !alt && !ctrl => {
                if let Some(row) = self.cursor_row() {
                    self.env.activate.call(self.rows.order[row]);
                    return true;
                }
                return false;
            }
            Key::Character(text) if ctrl && !alt && !shift => {
                let view = &self.env.view;
                match text.to_ascii_lowercase().as_str() {
                    "f" if self.on_search.is_some() => {
                        if let Some(start) = &self.on_search {
                            start.call(String::new());
                        }
                        return true;
                    }
                    "a" => {
                        self.select_all();
                        return true;
                    }
                    " " => {
                        if let Some(row) = self.cursor_row() {
                            let path = self.rows.entry(row).path.clone();
                            let mut selection = self.env.selection;
                            selection.write().toggle(&path);
                        }
                        return true;
                    }
                    "1" => {
                        view.zoom.show(ViewKind::List);
                        return true;
                    }
                    "2" => {
                        view.zoom.show(ViewKind::Grid);
                        return true;
                    }
                    _ => {}
                }
            }
            Key::Character(text) if ctrl && alt && !shift => {
                let view = &self.env.view;
                let by = |key: SortKey| {
                    view.set_sort.call(Sort {
                        key,
                        descending: false,
                        ..view.sort
                    });
                };
                match text.to_ascii_lowercase().as_str() {
                    "1" => by(SortKey::Name),
                    "2" => by(SortKey::Size),
                    "3" => by(SortKey::Kind),
                    "4" => by(SortKey::Modified),
                    "r" => view.set_sort.call(Sort {
                        descending: !view.sort.descending,
                        ..view.sort
                    }),
                    _ => return false,
                }
                return true;
            }
            _ => {}
        }
        let selected = self.selected_owned();
        if actions::handle_key(&self.env, &selected, event) {
            return true;
        }
        if let Key::Character(text) = &event.key
            && !ctrl
            && !alt
            && text.chars().count() == 1
            && text != " "
            && text != "/"
            && text != "~"
        {
            // Typing a name searches the folder when the owner offers a
            // search; otherwise it jumps to a name that starts with it.
            if let Some(start) = &self.on_search {
                start.call(text.to_string());
                return true;
            }
            let prefix = typed
                .borrow_mut()
                .push(text.chars().next().unwrap_or(' '), Instant::now())
                .to_string();
            if let Some(target) = self.type_ahead(&prefix) {
                self.go_to(target, false);
                self.reveal(target);
            }
            return true;
        }
        false
    }

    // ---- pointer gestures ---------------------------------------------

    /// The pointer went down on a row with the mouse.
    pub fn row_pointer_down(&self, row: usize, event: &Event<PointerEventData>) {
        let PointerEventData::Mouse(mouse) = event.data() else {
            return;
        };
        // The blank part of a row belongs to the rubber band behind it.
        if mouse.button == Some(MouseButton::Left) && !self.over_name(row, point_of(event).0) {
            return;
        }
        event.stop_propagation();
        self.focus.request_focus();
        match mouse.button {
            Some(MouseButton::Right) => {
                let at = point_of(event);
                self.row_menu(row, PanelTrigger::Pointer(at), false);
            }
            Some(MouseButton::Left) => {
                let mut pending = self.gestures.pending;
                pending.set(Some(Pending {
                    origin: point_of(event),
                    row,
                }));
            }
            // A middle click opens a folder in a tab of its own, as in a
            // browser, where the host has tabs.
            Some(MouseButton::Middle) if self.env.caps.new_tab && row < self.rows.len() => {
                let entry = self.rows.entry(row);
                if entry.is_dir() {
                    self.env.open_new_tab.call(entry.path.clone());
                }
            }
            _ => {}
        }
    }

    /// Whether a press at window x `x` on a list row is on its icon or name,
    /// where it takes the file, and not on the blank part of the row, where
    /// it starts a rubber band.
    pub fn over_name(&self, row: usize, x: f32) -> bool {
        if self.zoom.kind() == ViewKind::Grid || row >= self.rows.len() {
            return true;
        }
        let left = self.viewport.peek().min_x() + row_padding();
        let text_start = left
            + super::columns::gutter(f32::from(self.zoom.px_now()))
            + super::columns::CELL_PADDING.px();
        let text = crate::controls::path_label::measure_text(&self.rows.entry(row).name, BODY);
        x >= left && x <= text_start + text + S2.px()
    }

    /// The pointer went down on empty space.
    pub fn background_pointer_down(&self, event: &Event<PointerEventData>) {
        let PointerEventData::Mouse(mouse) = event.data() else {
            return;
        };
        let at = point_of(event);
        let area = *self.viewport.peek();
        // The scrollbar belongs to the scroll view.
        if at.0 > area.max_x() - S4.px() {
            return;
        }
        let held = freya_winit::modifiers::live();
        let additive = held.contains(Modifiers::CONTROL) || held.contains(Modifiers::SHIFT);
        match mouse.button {
            Some(MouseButton::Right) => {
                if !additive {
                    let mut selection = self.env.selection;
                    selection.write().clear();
                }
                self.empty_menu(PanelTrigger::Pointer(at), false);
            }
            Some(MouseButton::Left) => {
                self.focus.request_focus();
                let base = if additive {
                    self.env.selection.peek().paths().to_vec()
                } else {
                    let mut selection = self.env.selection;
                    selection.write().clear();
                    Vec::new()
                };
                let start = self.to_content(at);
                let mut band = self.gestures.band;
                band.set(Some(Band {
                    start,
                    current: start,
                    additive,
                    base,
                    last: None,
                    moved: false,
                }));
            }
            _ => {}
        }
    }

    /// A window point in content coordinates.
    fn to_content(&self, at: (f32, f32)) -> (f32, f32) {
        let area = *self.viewport.peek();
        let scroll = <(i32, i32)>::from(self.scroll).1 as f32;
        (at.0 - area.min_x(), at.1 - area.min_y() - scroll)
    }

    /// The pointer moved anywhere in the window.
    pub fn pointer_moved(&self, event: &Event<PointerEventData>) {
        self.press.moved(event);
        if is_touch(event) {
            return;
        }
        let at = point_of(event);
        let mut gestures = self.gestures;
        let pending = *gestures.pending.peek();
        if let Some(pending) = pending {
            let moved = (at.0 - pending.origin.0).hypot(at.1 - pending.origin.1);
            if moved > DRAG_SLOP && pending.row < self.rows.len() {
                self.start_drag(pending.row);
                gestures.pending.set(None);
            }
        }
        if gestures.drag.peek().is_some() {
            gestures.pointer.set(at);
            self.autoscroll(at.1);
        }
        let band = gestures.band.peek().clone();
        if let Some(mut band) = band {
            let current = self.to_content(at);
            let far = (current.0 - band.start.0).hypot(current.1 - band.start.1);
            if !band.moved && far < DRAG_SLOP {
                return;
            }
            band.moved = true;
            band.current = current;
            self.autoscroll(at.1);
            self.apply_band(&mut band);
            gestures.band.set(Some(band));
        }
    }

    fn start_drag(&self, row: usize) {
        let entry = self.rows.entry(row);
        let path = entry.path.clone();
        let mut selection = self.env.selection;
        if !selection.peek().contains(&path) {
            selection.write().select_only(&path);
        }
        let paths: Vec<PathBuf> = selection.peek().paths().to_vec();
        let label = match paths.as_slice() {
            [_] => entry.name.clone(),
            many => format!("{} items", many.len()),
        };
        let mut gestures = self.gestures;
        gestures.drag.set(Some(Drag { paths, label }));
        if let Some(session) = gestures.session {
            session.begin();
        }
    }

    /// Scroll while a band or a drag is held past the top or bottom edge.
    fn autoscroll(&self, pointer_y: f32) {
        let area = *self.viewport.peek();
        let Some(lattice) = self.lattice.get() else {
            return;
        };
        let edge = lattice.item_h * AUTOSCROLL_EDGE;
        let over = if pointer_y < area.min_y() + edge {
            pointer_y - (area.min_y() + edge)
        } else if pointer_y > area.max_y() - edge {
            pointer_y - (area.max_y() - edge)
        } else {
            return;
        };
        let mut scroll = self.scroll;
        let now = <(i32, i32)>::from(scroll).1;
        scroll.scroll_to_y(now - (over * 0.5) as i32);
    }

    fn apply_band(&self, band: &mut Band) {
        let Some(lattice) = self.lattice.get() else {
            return;
        };
        let hits = lattice.hits(Span::between(band.start, band.current));
        if hits == band.last {
            return;
        }
        band.last = hits;
        let mut selection = self.env.selection;
        let picked: Vec<&Path> = hits
            .into_iter()
            .flat_map(Hits::indices)
            .filter(|row| *row < self.rows.len() && self.selectable(self.rows.entry(*row)))
            .map(|row| self.rows.entry(row).path.as_path())
            .collect();
        let mut next = Selection::new();
        if band.additive {
            let have: HashSet<&Path> = band.base.iter().map(PathBuf::as_path).collect();
            let mut all: Vec<&Path> = band.base.iter().map(PathBuf::as_path).collect();
            all.extend(picked.iter().copied().filter(|path| !have.contains(path)));
            next.select_all(&all);
        } else {
            next.select_all(&picked);
        }
        if let Some(last) = picked.last() {
            next.set_cursor(last);
        }
        selection.set(next);
    }

    /// The pointer went up anywhere.
    pub fn pointer_up(&self) {
        self.press.release();
        let mut gestures = self.gestures;
        gestures.pending.set_if_modified(None);
        if gestures.band.peek().as_ref().is_some_and(|band| band.moved) {
            gestures.ended.set(Some(Instant::now()));
        }
        gestures.band.set_if_modified(None);
        let drag = gestures.drag.peek().clone();
        let over = *gestures.over.peek();
        // A folder outside the list that the pointer was released over: a
        // place of the sidebar, a tab.
        let outside = gestures.session.and_then(|session| session.end());
        if let Some(drag) = drag {
            gestures.drag.set(None);
            gestures.over.set_if_modified(None);
            gestures.ended.set(Some(Instant::now()));
            let dest = match over.filter(|row| *row < self.rows.len()) {
                Some(row) if self.rows.entry(row).is_dir() => {
                    Some(self.rows.entry(row).path.clone())
                }
                Some(_) => None,
                None => outside,
            };
            if let Some(dest) = dest {
                let copy = freya_winit::modifiers::live().contains(Modifiers::CONTROL);
                let selected = self.selected_owned();
                actions::run(
                    &self.env,
                    &selected,
                    Command::MoveInto {
                        paths: drag.paths,
                        dest,
                        copy,
                    },
                );
            }
        }
    }

    /// The pointer entered a row. A drag over a folder makes it the drop
    /// target; otherwise the detail card is armed.
    pub fn row_enter(&self, row: usize, event: &Event<PointerEventData>) {
        if is_touch(event) || row >= self.rows.len() {
            return;
        }
        let mut gestures = self.gestures;
        if let Some(drag) = gestures.drag.peek().as_ref() {
            let entry = self.rows.entry(row);
            if entry.is_dir() && !drag.paths.contains(&entry.path) {
                gestures.over.set_if_modified(Some(row));
            }
            return;
        }
        if gestures.band.peek().is_some() {
            return;
        }
        gestures.hover.set(Some(Hover {
            row,
            at: point_of(event),
        }));
    }

    pub fn row_leave(&self, row: usize) {
        let mut gestures = self.gestures;
        if *gestures.over.peek() == Some(row) {
            gestures.over.set(None);
        }
        if gestures.hover.peek().is_some_and(|hover| hover.row == row) {
            gestures.hover.set(None);
        }
    }
}

// ---------------------------------------------------------------------------
// The public component
// ---------------------------------------------------------------------------

/// The list or the grid of a folder, with its menus, status line and zoom;
/// see the module documentation.
///
/// The caller owns the [`Selection`] and the order: [`Listing::select`] makes
/// the order, [`Sort`] and the show-hidden switch are the caller's.
#[derive(PartialEq)]
pub struct FileView {
    listing: Arc<Listing>,
    order: Arc<Vec<usize>>,
    selection: State<Selection>,
    icon_px: Option<State<u16>>,
    sort: Sort,
    multiple: bool,
    folders_only: bool,
    autofocus: bool,
    show_hidden: bool,
    search: Option<SearchView>,
    on_search: Option<EventHandler<String>>,
    caps: Capabilities,
    new_folder: Option<State<u32>>,
    focus: Option<State<u32>>,
    bookmarks: Arc<Vec<PathBuf>>,
    thumbnails: bool,
    owner: Owner,
    placeholder: Option<Element>,
    on_sort: EventHandler<SortKey>,
    on_sort_set: EventHandler<Sort>,
    on_pick: EventHandler<usize>,
    on_activate: EventHandler<usize>,
    on_show_hidden: EventHandler<bool>,
    on_refresh: EventHandler<()>,
    on_open_folder: EventHandler<PathBuf>,
    on_open_new_tab: EventHandler<PathBuf>,
    on_bookmarks_changed: EventHandler<()>,
    on_problem: EventHandler<String>,
}

impl FileView {
    /// A view of `order` (indices into `listing.entries`).
    pub fn new(listing: Arc<Listing>, order: Arc<Vec<usize>>, selection: State<Selection>) -> Self {
        FileView {
            listing,
            order,
            selection,
            icon_px: None,
            sort: Sort::default(),
            multiple: false,
            folders_only: false,
            autofocus: false,
            show_hidden: false,
            search: None,
            on_search: None,
            caps: Capabilities::CHOOSER,
            new_folder: None,
            focus: None,
            bookmarks: Arc::new(Vec::new()),
            thumbnails: true,
            owner: 0,
            placeholder: None,
            on_sort: EventHandler::new(|_: SortKey| {}),
            on_sort_set: EventHandler::new(|_: Sort| {}),
            on_pick: EventHandler::new(|_: usize| {}),
            on_activate: EventHandler::new(|_: usize| {}),
            on_show_hidden: EventHandler::new(|_: bool| {}),
            on_refresh: EventHandler::new(|_: ()| {}),
            on_open_folder: EventHandler::new(|_: PathBuf| {}),
            on_open_new_tab: EventHandler::new(|_: PathBuf| {}),
            on_bookmarks_changed: EventHandler::new(|_: ()| {}),
            on_problem: EventHandler::new(|_: String| {}),
        }
    }

    /// Where the icon size (the zoom level, one of `zoom::LADDER`) lives.
    /// The view reads it, changes it when the person zooms or picks a list or
    /// a grid, and remembers it for the folder. The owner sets it to the
    /// folder's remembered level, or 16 px, when it opens a folder. Without
    /// one the view keeps its own, at 16 px.
    pub fn icon_px(mut self, level: State<u16>) -> Self {
        self.icon_px = Some(level);
        self
    }

    /// The order the column headers show as sorted.
    pub fn sort(mut self, sort: Sort) -> Self {
        self.sort = sort;
        self
    }

    /// Allow several rows to be selected.
    pub fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }

    /// Dim the files and make only folders selectable.
    pub fn folders_only(mut self, folders_only: bool) -> Self {
        self.folders_only = folders_only;
        self
    }

    /// Take keyboard focus when the view appears, so arrows, letters and
    /// shortcuts work at once; a chooser that opens on a name field leaves it
    /// off.
    pub fn autofocus(mut self, autofocus: bool) -> Self {
        self.autofocus = autofocus;
        self
    }

    /// Hidden files are shown. Only the menu's check mark reads this.
    pub fn show_hidden(mut self, show_hidden: bool) -> Self {
        self.show_hidden = show_hidden;
        self
    }

    /// A search is narrowing the list (the entries in `order` are its
    /// matches) or replacing it with the matches found in the subfolders. The
    /// view marks the matched letters in names, tells the status line
    /// "3 of 120 items match", shows a Location column for subfolder results,
    /// and scrolls to the top when the search changes. Search results are
    /// not a folder: New folder, Paste and the remembered scroll position
    /// do not apply to them.
    pub fn search(mut self, search: Option<SearchView>) -> Self {
        self.search = search;
        self
    }

    /// Typing a name on the list, or Ctrl+F, calls this with the typed
    /// letter (empty for Ctrl+F) so the owner can start a search, instead of
    /// jumping to a name that starts with the letter.
    pub fn on_search(mut self, handler: impl Into<EventHandler<String>>) -> Self {
        self.on_search = Some(handler.into());
        self
    }

    /// What this kind of window may do to files. A chooser changes files and
    /// opens nothing elsewhere; see [`Capabilities`].
    pub fn caps(mut self, caps: Capabilities) -> Self {
        self.caps = caps;
        self
    }

    /// Creates a folder and starts naming it each time the number goes up,
    /// as a toolbar's New folder button does.
    pub fn new_folder(mut self, trigger: State<u32>) -> Self {
        self.new_folder = Some(trigger);
        self
    }

    /// Takes keyboard focus each time the number goes up, as when an address
    /// bar closes and the keys should reach the list again.
    pub fn focus(mut self, trigger: State<u32>) -> Self {
        self.focus = Some(trigger);
        self
    }

    /// The folders that are bookmarks, so a menu offers Add or Remove.
    pub fn bookmarks(mut self, bookmarks: Arc<Vec<PathBuf>>) -> Self {
        self.bookmarks = bookmarks;
        self
    }

    /// Pictures may be drawn as thumbnails. Off for a network folder, where
    /// reading every picture would be slow.
    pub fn thumbnails(mut self, thumbnails: bool) -> Self {
        self.thumbnails = thumbnails;
        self
    }

    /// Whose remembered scroll position and selection this view keeps. A
    /// host with several views of one folder, like the tabs of the Files app,
    /// gives each its own number, so each comes back to its own place.
    /// Without one the view is owner 0.
    pub(super) fn remember_for(mut self, owner: Owner) -> Self {
        self.owner = owner;
        self
    }

    /// What to draw, centred, when there is nothing to list, such as an
    /// empty-folder message. The menu of empty space, drops and the rubber
    /// band still work, so a person can make the first folder.
    pub fn placeholder(mut self, placeholder: impl Into<Element>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// A column header was pressed.
    pub fn on_sort(mut self, handler: impl Into<EventHandler<SortKey>>) -> Self {
        self.on_sort = handler.into();
        self
    }

    /// A menu or a key chose a whole new order.
    pub fn on_sort_set(mut self, handler: impl Into<EventHandler<Sort>>) -> Self {
        self.on_sort_set = handler.into();
        self
    }

    /// A row was picked by the pointer or the keyboard. Gets the entry's index
    /// in the listing.
    pub fn on_pick(mut self, handler: impl Into<EventHandler<usize>>) -> Self {
        self.on_pick = handler.into();
        self
    }

    /// A row was opened: double-clicked, tapped (a folder), or Enter on the
    /// cursor row. Gets the entry's index in the listing.
    pub fn on_activate(mut self, handler: impl Into<EventHandler<usize>>) -> Self {
        self.on_activate = handler.into();
        self
    }

    /// The menu or Ctrl+H changed whether hidden files show.
    pub fn on_show_hidden(mut self, handler: impl Into<EventHandler<bool>>) -> Self {
        self.on_show_hidden = handler.into();
        self
    }

    /// F5 or the menu asked to read the folder again.
    pub fn on_refresh(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_refresh = handler.into();
        self
    }

    /// A menu asked to go into a folder.
    pub fn on_open_folder(mut self, handler: impl Into<EventHandler<PathBuf>>) -> Self {
        self.on_open_folder = handler.into();
        self
    }

    /// A menu asked to open a folder in a new tab.
    pub fn on_open_new_tab(mut self, handler: impl Into<EventHandler<PathBuf>>) -> Self {
        self.on_open_new_tab = handler.into();
        self
    }

    /// A bookmark was added or removed.
    pub fn on_bookmarks_changed(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_bookmarks_changed = handler.into();
        self
    }

    /// Something went wrong that the status line cannot say well; for a
    /// footer that shows problems.
    pub fn on_problem(mut self, handler: impl Into<EventHandler<String>>) -> Self {
        self.on_problem = handler.into();
        self
    }
}

impl Component for FileView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let compact = is_compact();
        let overlays = use_overlays();
        let ui = use_ui();
        let own_level = use_state(|| DEFAULT_PX);
        let level = self.icon_px.unwrap_or(own_level);
        let viewport = use_state(Area::default);
        let zoom = use_zoom(level, cap_for_width(viewport.read().width()));
        let pinch = use_pinch(zoom);
        // A level the person chooses is remembered for the folder. The one the
        // owner opens it with is not a choice, so it is not written back.
        {
            let folder = (!self.listing.recent).then(|| self.listing.path.clone());
            let sort = self.sort;
            use_side_effect_with_deps(&zoom.chosen(), move |chosen: &Option<u16>| {
                if let (Some(px), Some(folder)) = (*chosen, &folder) {
                    view_state::remember_folder(
                        folder,
                        view_state::FolderView {
                            icon_px: px,
                            sort: sort.into(),
                        },
                    );
                }
            });
        }
        let layout = use_state(ColumnLayout::load);
        let gestures = use_gestures();
        let press = use_press();
        let last_click = use_state(|| None::<(Instant, usize)>);
        let origin = use_state(Area::default);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let typed = use_hook(|| Rc::new(RefCell::new(TypeAhead::new())));
        let lattice = use_hook(|| Rc::new(Cell::new(None::<Lattice>)));
        let zoom_seen = use_hook(|| Rc::new(Cell::new(None::<(f32, f32, usize)>)));
        let home = use_hook(super::directory::home_folder);
        let wants = try_consume_context::<Wants>().unwrap_or_default();
        let recent = self.listing.recent;
        let in_results = self
            .search
            .as_ref()
            .is_some_and(|search| search.deep.is_some());
        let folder = (!recent && !in_results).then(|| self.listing.path.clone());
        // The drop of the view reads this, not the first render's value.
        let results_now = use_hook(|| Rc::new(Cell::new(false)));
        results_now.set(in_results);

        // Where this folder left off, when the person was here before.
        let key = self.listing.path.clone();
        let owner = self.owner;
        let restored = use_hook({
            let key = key.clone();
            move || recall(owner, &key)
        });
        let scroll = use_hook({
            let saved = restored.as_ref().map_or(0, |remembered| remembered.scroll);
            move || ScrollController::new(0, saved)
        });
        let selection = self.selection;
        use_hook({
            let restored = restored.clone();
            let folder = self.listing.path.clone();
            let listing = self.listing.clone();
            move || {
                let mut selection = selection;
                if !selection.peek().is_empty() {
                    return;
                }
                if let Some(remembered) = restored {
                    let present: Vec<&Path> = remembered
                        .selection
                        .iter()
                        .map(PathBuf::as_path)
                        .filter(|path| listing.entries.iter().any(|entry| entry.path == *path))
                        .collect();
                    if !present.is_empty() {
                        selection.write().select_all(&present);
                        return;
                    }
                }
                if let Some(child) = came_from(owner, &folder)
                    && listing.entries.iter().any(|entry| entry.path == child)
                {
                    selection.write().select_only(&child);
                }
            }
        });
        use_drop({
            let path = key.clone();
            let results_now = results_now.clone();
            move || {
                // A scroll position inside search results is not the folder's.
                if results_now.get() {
                    return;
                }
                let paths = selection.peek().paths().to_vec();
                remember(owner, &path, <(i32, i32)>::from(scroll).1, paths);
            }
        });
        // Rows that come and go under a resting pointer leave no hover to
        // end: forget it, so the detail card of a row that is gone hides.
        use_side_effect_with_deps(
            &(
                self.order.len(),
                self.search.as_ref().map(|search| search.query.text().to_string()),
            ),
            move |_| {
                let mut hover = gestures.hover;
                hover.set_if_modified(None);
            },
        );

        let rows = Rows {
            listing: self.listing.clone(),
            order: self.order.clone(),
        };
        // A search that changes the rows starts at the top; one that keeps its
        // text (results streaming in) leaves the scroll alone. Decided here,
        // from what the view is given, and not in an effect: `scroll_to_y`
        // reads the controller's bounds, which the scroll view rewrites on
        // every layout, so an effect calling it ran again after each wheel
        // notch or key and threw the list back to the top.
        let search_key = self
            .search
            .as_ref()
            .map(|search| (search.query.text().to_string(), in_results));
        let search_top = use_hook({
            let (key, rows) = (search_key.clone(), rows.clone());
            move || Rc::new(RefCell::new(SearchTop::new(key, rows)))
        });
        if search_top.borrow_mut().moves(search_key, &rows) {
            spawn(async move {
                let mut scroll = scroll;
                scroll.scroll_to_y(0);
            });
        }
        let on_activate = self.on_activate.clone();
        // Quick Look is opened through the environment, from a menu as well
        // as from the keyboard; the handler reaches the view's context
        // without keeping it alive.
        let preview_target = use_hook(|| Rc::new(RefCell::new(None::<WeakShared>)));
        let env = Env {
            folder,
            home: home.clone(),
            caps: self.caps,
            overlays,
            ui,
            selection,
            listing: self.listing.clone(),
            order: self.order.clone(),
            reload: {
                let on_refresh = self.on_refresh.clone();
                let wants = wants.clone();
                EventHandler::new(move |()| {
                    wants.refresh();
                    on_refresh.call(());
                })
            },
            open_folder: self.on_open_folder.clone(),
            activate: EventHandler::new(move |index: usize| on_activate.call(index)),
            open_new_tab: self.on_open_new_tab.clone(),
            bookmarks: self.bookmarks.clone(),
            bookmarks_changed: self.on_bookmarks_changed.clone(),
            view: ViewControls {
                show_hidden: self.show_hidden,
                set_show_hidden: self.on_show_hidden.clone(),
                sort: self.sort,
                set_sort: self.on_sort_set.clone(),
                zoom,
                layout,
            },
            focus: a11y_id,
            preview: {
                let target = preview_target.clone();
                EventHandler::new(move |()| {
                    let shared = target.borrow().as_ref().and_then(WeakShared::upgrade);
                    if let Some(shared) = shared {
                        super::preview::show(&shared);
                    }
                })
            },
        };
        let shared = use_hook(|| {
            Shared(Rc::new(RefCell::new(Rc::new(Ctx {
                rows: Rows {
                    listing: self.listing.clone(),
                    order: self.order.clone(),
                },
                env: env.clone(),
                multiple: self.multiple,
                folders_only: self.folders_only,
                on_pick: self.on_pick.clone(),
                gestures,
                press,
                pinch,
                zoom,
                scroll,
                focus: a11y_id,
                viewport,
                lattice: lattice.clone(),
                zoom_seen: zoom_seen.clone(),
                last_click,
                thumbnails: self.thumbnails,
                recent,
                search: self.search.clone(),
                on_search: self.on_search.clone(),
                home: home.clone(),
                wants: wants.clone(),
            }))))
        });
        shared.set(Ctx {
            rows: Rows {
                listing: self.listing.clone(),
                order: self.order.clone(),
            },
            env: env.clone(),
            multiple: self.multiple,
            folders_only: self.folders_only,
            on_pick: self.on_pick.clone(),
            gestures,
            press,
            pinch,
            zoom,
            scroll,
            focus: a11y_id,
            viewport,
            lattice: lattice.clone(),
            zoom_seen: zoom_seen.clone(),
            last_click,
            thumbnails: self.thumbnails,
            recent,
            search: self.search.clone(),
            on_search: self.on_search.clone(),
            home: home.clone(),
            wants: wants.clone(),
        });
        *preview_target.borrow_mut() = Some(shared.downgrade());
        // While searching, the arrows are in the address field: keep the
        // selected match in view as it moves.
        let cursor_path = self
            .search
            .is_some()
            .then(|| selection.read().cursor().map(Path::to_path_buf))
            .flatten();
        use_side_effect_with_deps(&cursor_path, {
            let shared = shared.clone();
            move |path: &Option<PathBuf>| {
                if path.is_none() {
                    return;
                }
                let shared = shared.clone();
                spawn(async move {
                    let ctx = shared.get();
                    if let Some(row) = ctx.cursor_row() {
                        ctx.reveal(row);
                    }
                });
            }
        });

        // A toolbar button asked for a new folder.
        let seen = use_hook(|| Rc::new(Cell::new(self.new_folder.map_or(0, |trigger| *trigger.peek()))));
        if let Some(trigger) = self.new_folder {
            let wanted = *trigger.read();
            if seen.get() != wanted {
                seen.set(wanted);
                let shared = shared.clone();
                spawn(async move {
                    let ctx = shared.get();
                    let selected = ctx.selected_owned();
                    actions::run(&ctx.env, &selected, Command::NewFolder);
                });
            }
        }

        let focus_seen = use_hook(|| Rc::new(Cell::new(self.focus.map_or(0, |trigger| *trigger.peek()))));
        if let Some(trigger) = self.focus {
            let wanted = *trigger.read();
            if focus_seen.get() != wanted {
                focus_seen.set(wanted);
                spawn(async move { a11y_id.request_focus() });
            }
        }
        // Sorting by size or date on a drive that is only read for the rows in
        // view needs every entry's size and date.
        let stat_all = {
            let wants = wants.clone();
            let needs = matches!(self.sort.key, SortKey::Size | SortKey::Modified)
                && !self.listing.mount.thumbnails()
                && self.listing.complete;
            move || {
                if needs {
                    wants.stat_all();
                }
            }
        };
        use_side_effect_with_deps(
            &(self.sort.key, self.listing.complete, self.listing.path.clone()),
            move |_| stat_all(),
        );
        let autofocus = self.autofocus;
        use_side_effect(move || {
            if autofocus {
                a11y_id.request_focus();
            }
        });

        let px = zoom.px();
        let grid = kind_of(px) == ViewKind::Grid;
        let compact_list = compact && !grid;
        let with_header = !grid && !compact;
        let header = with_header.then(|| {
            let row_width = viewport.read().width() - 2.0 * row_padding();
            let plan = plan_in(&layout.read(), f32::from(px), row_width, recent || in_results);
            let measure_shared = shared.clone();
            let menu_shared = shared.clone();
            let on_sort = self.on_sort.clone();
            Element::from(
                ColumnHeaders::new(layout, plan, self.sort)
                    .icon_px(f32::from(px))
                    .on_sort(move |key: SortKey| on_sort.call(key))
                    .on_menu(move |at: (f32, f32)| menu_shared.get().header_menu(at))
                    .fit(Rc::new(move |column| {
                        super::file_list::widest(&measure_shared.get(), column)
                    })),
            )
        });

        let on_key_down = {
            let shared = shared.clone();
            let typed = typed.clone();
            move |event: Event<KeyboardEventData>| {
                let ctx = shared.get();
                // Space opens Quick Look, unless a word is being typed.
                if super::preview::wants_open(&ctx, &event, typed.borrow().is_active(Instant::now())) {
                    super::preview::show(&shared);
                    event.stop_propagation();
                    return;
                }
                if ctx.common_key(&event, &typed) {
                    event.stop_propagation();
                    return;
                }
                let handled = if ctx.zoom.kind() == ViewKind::Grid {
                    super::file_grid::navigate(&ctx, &event)
                } else {
                    super::file_list::navigate(&ctx, &event)
                };
                if handled {
                    event.stop_propagation();
                }
            }
        };

        let body: Element = if self.order.is_empty() {
            lattice.set(None);
            rect()
                .tw(&tokens, &[w_full(), h_full(), center()])
                .on_sized(move |event: Event<SizedEventData>| {
                    let mut viewport = viewport;
                    viewport.set_if_modified(event.area);
                })
                .maybe_child(self.placeholder.clone())
                .into()
        } else if grid {
            GridView {
                shared: shared.clone(),
                rows: rows.clone(),
                icon_px: px,
            }
            .into()
        } else {
            ListView {
                shared: shared.clone(),
                rows: rows.clone(),
                icon_px: px,
                layout: layout.read().clone(),
                compact: compact_list,
            }
            .into()
        };

        let info = status_info(self, &shared.get());
        let area = rect()
            .tw(&tokens, &[w_full(), h_flex()])
            .on_sized(move |event: Event<SizedEventData>| {
                let mut origin = origin;
                origin.set_if_modified(event.area);
            })
            .on_wheel({
                let shared = shared.clone();
                move |event: Event<WheelEventData>| {
                    let ctx = shared.get();
                    ctx.zoom.wheel(&event);
                }
            })
            .on_touch_start({
                let shared = shared.clone();
                move |event: Event<TouchEventData>| shared.get().pinch.start(&event)
            })
            .on_touch_move({
                let shared = shared.clone();
                move |event: Event<TouchEventData>| {
                    shared.get().pinch.moved(&event);
                }
            })
            .on_touch_end({
                let shared = shared.clone();
                move |event: Event<TouchEventData>| shared.get().pinch.end(&event)
            })
            .on_touch_cancel({
                let shared = shared.clone();
                move |event: Event<TouchEventData>| shared.get().pinch.end(&event)
            })
            .on_pointer_down({
                let shared = shared.clone();
                move |event: Event<PointerEventData>| {
                    shared.get().background_pointer_down(&event)
                }
            })
            .on_global_pointer_move({
                let shared = shared.clone();
                move |event: Event<PointerEventData>| shared.get().pointer_moved(&event)
            })
            .on_global_pointer_up({
                let shared = shared.clone();
                move |_: Event<PointerEventData>| shared.get().pointer_up()
            })
            .child(body)
            .child(BandOverlay {
                band: gestures.band,
                scroll,
            })
            .child(HoverCard {
                shared: shared.clone(),
                origin,
            })
            .child(zoom_hud(zoom))
            .child(status_line(info, ui));

        rect()
            .component("FileView")
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::List)
            .a11y_alt("Files")
            .tw(&tokens, &[flex_col(), grow_children(), w_full(), h_full()])
            .on_key_down(on_key_down)
            .maybe_child(header)
            .child(area)
            .child(DragChip {
                drag: gestures.drag,
                pointer: gestures.pointer,
                origin,
            })
            // The view fills its pane up to the window's edge, so the ring is
            // drawn inside it: outside, the window would cut it off.
            .focusable_inside(focus(), Radius::Control)
    }
}

/// What the status line says about the view.
fn status_info(view: &FileView, ctx: &Ctx) -> StatusInfo {
    let selected = ctx.selected_refs();
    let selected_bytes = selected
        .iter()
        .filter(|entry| !entry.is_dir())
        .map(|entry| entry.size)
        .sum();
    StatusInfo {
        shown: view.order.len(),
        total_listed: view.listing.entries.len(),
        selected: selected.len(),
        selected_bytes,
        search: view.search.as_ref().map(|search| match &search.deep {
            Some(deep) => SearchLine::Subfolders {
                found: deep.found,
                folders: deep.folders,
                running: deep.running,
                truncated: deep.truncated,
            },
            None => SearchLine::Folder {
                matches: view.order.len(),
                of: search.of,
            },
        }),
        loading: false,
        complete: true,
        unresponsive: false,
    }
}

// ---------------------------------------------------------------------------
// Overlays
// ---------------------------------------------------------------------------

/// The rubber band, drawn over the content.
#[derive(Clone, PartialEq)]
struct BandOverlay {
    band: State<Option<Band>>,
    scroll: ScrollController,
}

impl Component for BandOverlay {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let band = self.band.read().clone();
        let Some(band) = band.filter(|band| band.moved) else {
            return rect().interactive(false).into_element();
        };
        let scroll = <(i32, i32)>::from(self.scroll).1 as f32;
        let span = Span::between(band.start, band.current);
        rect()
            .interactive(false)
            .layer(Layer::Overlay)
            .position(
                Position::new_absolute()
                    .left(span.x0)
                    .top(span.y0 + scroll),
            )
            .width(Size::px(span.x1 - span.x0))
            .height(Size::px(span.y1 - span.y0))
            .tw(
                &tokens,
                &[
                    bg(Paint::AccentSoft),
                    border(Line::Stroke(Dim::Hairline, Paint::Accent)),
                ],
            )
            .into_element()
    }
}

/// The files being dragged, beside the pointer.
#[derive(Clone, PartialEq)]
struct DragChip {
    drag: State<Option<Drag>>,
    pointer: State<(f32, f32)>,
    origin: State<Area>,
}

impl Component for DragChip {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let drag = self.drag.read().clone();
        let Some(drag) = drag else {
            return rect().interactive(false).into_element();
        };
        let at = *self.pointer.read();
        let origin = *self.origin.read();
        let copy = freya_winit::modifiers::live().contains(Modifiers::CONTROL);
        let label = if copy {
            format!("Copy {}", drag.label)
        } else {
            drag.label
        };
        rect()
            .interactive(false)
            .layer(Layer::Overlay)
            .position(
                Position::new_absolute()
                    .left(at.0 - origin.min_x() + S4.px())
                    .top(at.1 - origin.min_y() + S4.px()),
            )
            .tw(&tokens, TOOLTIP)
            .tw(
                &tokens,
                &[border(Line::Stroke(Dim::Hairline, Paint::FloatingOutline))],
            )
            .child(
                BODY.in_ink(Ink::OnFloating)
                    .label(&tokens, label)
                    .max_lines(1),
            )
            .into_element()
    }
}

/// The detail card of the row the pointer rests on: type, size and date.
#[derive(Clone, PartialEq)]
struct HoverCard {
    shared: Shared,
    origin: State<Area>,
}

impl Component for HoverCard {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ctx = self.shared.get();
        let hover = *ctx.gestures.hover.read();
        let mut shown = use_state(|| None::<Hover>);
        let mut size = use_state(|| None::<(f32, f32)>);
        use_side_effect_with_deps(&hover, move |hover: &Option<Hover>| {
            // Moving to another row hides the card and starts the wait again.
            shown.set_if_modified(None);
            size.set_if_modified(None);
            if let Some(hover) = *hover {
                spawn(async move {
                    timer(CARD_DELAY).await;
                    shown.set(Some(hover));
                });
            }
        });
        let busy = ctx.gestures.drag.read().is_some()
            || ctx.gestures.band.read().is_some()
            || ctx.env.ui.rename.read().is_some()
            // A preview or a dialog is over the view; the pointer rests under it.
            || ctx.env.overlays.dialog_open();
        let Some(hover) = (*shown.read()).filter(|_| !busy) else {
            return rect().interactive(false).into_element();
        };
        if hover.row >= ctx.rows.len() {
            return rect().interactive(false).into_element();
        }
        let entry = ctx.rows.entry(hover.row);
        let now = SystemTime::now();
        let mut lines: Vec<String> = vec![entry.name.clone()];
        let kind = if entry.is_dir() {
            "Folder".to_string()
        } else {
            format!("{} · {}", entry.mime.description, display::size(entry.size))
        };
        lines.push(kind);
        if entry.modified.is_some() {
            let modified = super::columns::cell_text_styled(
                super::columns::Column::Modified,
                entry,
                now,
                &ctx.home,
                super::columns::DateStyle::Absolute,
            );
            lines.push(format!("Modified {modified}"));
        }
        if ctx.recent
            && let Some(parent) = entry.path.parent()
        {
            lines.push(parent.to_string_lossy().into_owned());
        } else if let Some(folder) = ctx.result_folder(entry) {
            lines.push(format!("In {folder}"));
        }
        let origin = *self.origin.read();
        let (width, height) = (*size.read()).unwrap_or((0.0, 0.0));
        let measured = width > 0.0;
        // Below and right of the pointer; flipped when it would leave the view.
        let mut left = hover.at.0 - origin.min_x() + S4.px();
        let mut top = hover.at.1 - origin.min_y() + S5.px();
        if left + width > origin.width() {
            left = (hover.at.0 - origin.min_x() - width - S2.px()).max(0.0);
        }
        if top + height > origin.height() {
            top = (hover.at.1 - origin.min_y() - height - S2.px()).max(0.0);
        }
        let mut card = rect()
            .interactive(false)
            .layer(Layer::Overlay)
            .position(Position::new_absolute().left(left).top(top))
            .opacity(if measured { 1.0 } else { 0.0 })
            .on_sized(move |event: Event<SizedEventData>| {
                size.set_if_modified(Some((event.area.width(), event.area.height())));
            })
            .tw(&tokens, TOOLTIP)
            .tw(
                &tokens,
                &[
                    flex_col(),
                    border(Line::Stroke(Dim::Hairline, Paint::FloatingOutline)),
                ],
            );
        for (index, line) in lines.into_iter().enumerate() {
            let style = if index == 0 { BODY } else { CAPTION }.in_ink(Ink::OnFloating);
            card = card.child(style.label(&tokens, line).max_lines(1));
        }
        card.into_element()
    }
}
