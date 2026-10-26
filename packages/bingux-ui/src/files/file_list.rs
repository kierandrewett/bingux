//! The file list: rows of icon, name, size and modified time.
//!
//! Only the rows in view are built, so a folder of a hundred thousand files
//! scrolls as smoothly as one of ten. The list is one keyboard stop: the arrow
//! keys, Home, End and Page Up and Down move the cursor, Shift extends the
//! selection, Ctrl+A selects everything, Enter opens the cursor row, and
//! typing letters jumps to a name that starts with them. Click selects; a
//! second click, or a tap on a phone, opens. Ctrl-click and Shift-click
//! select several.

use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bingux_files::selection::{TypeAhead, type_ahead_target};
use bingux_files::{Entry, Listing, Selection, Sort, SortKey, display};
use freya::prelude::*;

use super::glyphs::entry_icon;
use crate::component::ComponentName;
use crate::controls::focus_stop;
use crate::icons::{self, Icon};
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, CAPTION};

/// Two presses on one row this close together open it.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// The rows a view shows: a listing and the order its entries appear in.
#[derive(Clone, PartialEq)]
pub(super) struct Rows {
    pub listing: Arc<Listing>,
    /// Indices into `listing.entries`, in display order.
    pub order: Arc<Vec<usize>>,
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

/// How a view reacts to the pointer: shared by the list and the grid so they
/// select, open and navigate the same way.
#[derive(Clone, PartialEq)]
pub(super) struct Behaviour {
    pub rows: Rows,
    pub selection: State<Selection>,
    pub multiple: bool,
    /// Only folders can be selected; files are shown but dimmed.
    pub folders_only: bool,
    pub modifiers: State<Modifiers>,
    pub last_click: State<Option<(Instant, usize)>>,
    pub on_pick: EventHandler<usize>,
    pub on_activate: EventHandler<usize>,
}

impl Behaviour {
    pub fn selectable(&self, entry: &Entry) -> bool {
        !self.folders_only || entry.is_dir()
    }

    /// A row was pressed. `touch` is a finger: a tap opens a folder at once
    /// and selects (or toggles) a file.
    pub fn pressed(&self, row: usize, touch: bool) {
        let entry = self.rows.entry(row);
        if !self.selectable(entry) {
            return;
        }
        let entry_index = self.rows.order[row];
        let path = entry.path.clone();
        let mut selection = self.selection;
        if touch {
            if entry.is_dir() {
                selection.write().select_only(&path);
                self.on_activate.call(entry_index);
            } else if self.multiple {
                selection.write().toggle(&path);
                self.on_pick.call(entry_index);
            } else {
                selection.write().select_only(&path);
                self.on_pick.call(entry_index);
            }
            return;
        }
        let held = *self.modifiers.peek();
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
                self.on_activate.call(entry_index);
            }
        }
    }

    /// Select the row at `target` (extending with Shift when several are
    /// allowed) and report it.
    pub fn go_to(&self, target: usize, extend: bool) {
        let entry = self.rows.entry(target);
        let mut selection = self.selection;
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
        let selection = self.selection.read();
        selection.cursor().and_then(|path| self.rows.row_of(path))
    }

    /// Ctrl+A.
    pub fn select_all(&self) {
        if self.multiple {
            let paths: Vec<&Path> = (0..self.rows.len())
                .filter(|row| self.selectable(self.rows.entry(*row)))
                .map(|row| self.rows.entry(row).path.as_path())
                .collect();
            let mut selection = self.selection;
            selection.write().select_all(&paths);
        }
    }

    /// Typing a letter: the row it jumps to, if a name starts with what has
    /// been typed.
    pub fn type_ahead(&self, typed: &str) -> Option<usize> {
        type_ahead_target(
            self.rows.len(),
            |row| self.rows.entry(row).name.as_str(),
            self.cursor_row(),
            typed,
        )
        .filter(|row| self.selectable(self.rows.entry(*row)))
    }
}

/// The scroll offset that keeps `row` in view, as the scroll controller
/// stores it (negative). `current` is the present offset.
pub(super) fn offset_for(
    row: usize,
    row_height: f32,
    current: i32,
    rows: usize,
    viewport: f32,
) -> i32 {
    let top = row as f32 * row_height;
    let furthest = (rows as f32 * row_height - viewport).max(0.0);
    let shown_top = -(current as f32);
    let want = if top < shown_top {
        top
    } else if top + row_height > shown_top + viewport {
        top + row_height - viewport
    } else {
        shown_top
    };
    -(want.clamp(0.0, furthest) as i32)
}

/// A sortable list of files with columns.
///
/// The caller owns the [`Selection`] and the order: [`Listing::select`] makes
/// the order, [`Sort`] and the show-hidden switch are the caller's.
#[derive(PartialEq)]
pub struct FileList {
    listing: Arc<Listing>,
    order: Arc<Vec<usize>>,
    selection: State<Selection>,
    sort: Sort,
    multiple: bool,
    folders_only: bool,
    autofocus: bool,
    on_sort: EventHandler<SortKey>,
    on_pick: EventHandler<usize>,
    on_activate: EventHandler<usize>,
}

impl FileList {
    /// A list of `order` (indices into `listing.entries`).
    pub fn new(listing: Arc<Listing>, order: Arc<Vec<usize>>, selection: State<Selection>) -> Self {
        FileList {
            listing,
            order,
            selection,
            sort: Sort::default(),
            multiple: false,
            folders_only: false,
            autofocus: false,
            on_sort: EventHandler::new(|_: SortKey| {}),
            on_pick: EventHandler::new(|_: usize| {}),
            on_activate: EventHandler::new(|_: usize| {}),
        }
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

    /// Take keyboard focus when the list appears, so arrows, letters and `/`
    /// work at once; a chooser that opens on a name field leaves it off.
    pub fn autofocus(mut self, autofocus: bool) -> Self {
        self.autofocus = autofocus;
        self
    }

    /// A column header was pressed.
    pub fn on_sort(mut self, handler: impl Into<EventHandler<SortKey>>) -> Self {
        self.on_sort = handler.into();
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
}

/// What every row needs to draw. The selection is part of it, so rows redraw
/// when the selection changes.
#[derive(Clone, PartialEq)]
struct RowsData {
    behaviour: Behaviour,
    selection: Selection,
    compact: bool,
    recent: bool,
}

impl Component for FileList {
    fn render_key(&self) -> DiffKey {
        // A different folder is a new list, scrolled to the top.
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.listing.path.hash(&mut hasher);
        self.listing.recent.hash(&mut hasher);
        DiffKey::U64(hasher.finish())
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let compact = is_compact();
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let autofocus = self.autofocus;
        use_side_effect(move || {
            if autofocus {
                a11y_id.request_focus();
            }
        });
        let scroll = use_scroll_controller(ScrollConfig::default);
        let mut viewport = use_state(Area::default);
        let modifiers = use_state(Modifiers::default);
        let last_click = use_state(|| None::<(Instant, usize)>);
        let typed = use_hook(|| Rc::new(RefCell::new(TypeAhead::new())));
        let selection = self.selection;
        let row_height = Dim::NavItem.px();

        let rows = Rows {
            listing: self.listing.clone(),
            order: self.order.clone(),
        };
        let behaviour = Behaviour {
            rows: rows.clone(),
            selection,
            multiple: self.multiple,
            folders_only: self.folders_only,
            modifiers,
            last_click,
            on_pick: self.on_pick.clone(),
            on_activate: self.on_activate.clone(),
        };

        let on_key_down = {
            let behaviour = behaviour.clone();
            let typed = typed.clone();
            let mut scroll = scroll;
            move |event: Event<KeyboardEventData>| {
                let count = behaviour.rows.len();
                let extend = event.modifiers.contains(Modifiers::SHIFT);
                let ctrl = event.modifiers.contains(Modifiers::CONTROL);
                let page = ((viewport.peek().height() / row_height) as usize).max(1);
                let from = behaviour.cursor_row();
                let target = match &event.key {
                    Key::Named(NamedKey::ArrowDown) => Some(from.map_or(0, |at| at + 1)),
                    Key::Named(NamedKey::ArrowUp) => {
                        Some(from.map_or(count.saturating_sub(1), |at| at.saturating_sub(1)))
                    }
                    Key::Named(NamedKey::PageDown) => Some(from.map_or(0, |at| at + page)),
                    Key::Named(NamedKey::PageUp) => {
                        Some(from.map_or(count.saturating_sub(1), |at| at.saturating_sub(page)))
                    }
                    Key::Named(NamedKey::Home) => Some(0),
                    Key::Named(NamedKey::End) => Some(count.saturating_sub(1)),
                    Key::Named(NamedKey::Enter) => {
                        if let Some(row) = from {
                            event.stop_propagation();
                            behaviour.on_activate.call(behaviour.rows.order[row]);
                        }
                        return;
                    }
                    Key::Character(text) if ctrl && text.eq_ignore_ascii_case("a") => {
                        event.stop_propagation();
                        behaviour.select_all();
                        return;
                    }
                    Key::Character(text) if ctrl && text == " " => {
                        if let Some(row) = from {
                            event.stop_propagation();
                            let path = behaviour.rows.entry(row).path.clone();
                            let mut selection = behaviour.selection;
                            selection.write().toggle(&path);
                        }
                        return;
                    }
                    Key::Character(text)
                        if !ctrl
                            && !event.modifiers.contains(Modifiers::ALT)
                            && text.chars().count() == 1
                            && text != " "
                            && text != "/"
                            && text != "~" =>
                    {
                        let prefix = typed
                            .borrow_mut()
                            .push(text.chars().next().unwrap_or(' '), Instant::now())
                            .to_string();
                        event.stop_propagation();
                        behaviour.type_ahead(&prefix)
                    }
                    _ => None,
                };
                let Some(target) = target.filter(|_| count > 0) else {
                    return;
                };
                event.stop_propagation();
                let target = target.min(count - 1);
                if !behaviour.selectable(behaviour.rows.entry(target)) {
                    return;
                }
                behaviour.go_to(target, extend);
                let offset = offset_for(
                    target,
                    row_height,
                    <(i32, i32)>::from(scroll).1,
                    count,
                    viewport.peek().height(),
                );
                scroll.scroll_to_y(offset);
            }
        };

        let data = RowsData {
            behaviour,
            selection: selection.read().clone(),
            compact,
            recent: self.listing.recent,
        };
        let body = VirtualScrollView::new_with_data_controlled(
            data,
            |item, data: &RowsData| {
                let entry = data.behaviour.rows.entry(item.index).clone();
                let selected = data.selection.contains(&entry.path);
                let enabled = data.behaviour.selectable(&entry);
                let behaviour = data.behaviour.clone();
                let row = item.index;
                Element::from(FileRow {
                    entry,
                    row,
                    selected,
                    enabled,
                    compact: data.compact,
                    recent: data.recent,
                    on_press: EventHandler::new(move |touch: bool| behaviour.pressed(row, touch)),
                })
            },
            scroll,
        )
        .length(self.order.len())
        .item_size(row_height)
        .drag_scrolling(compact)
        .on_sized(move |event: Event<SizedEventData>| viewport.set(event.area));

        let header = (!compact).then(|| {
            Element::from(ColumnHeaders {
                sort: self.sort,
                recent: self.listing.recent,
                on_sort: self.on_sort.clone(),
            })
        });

        rect()
            .component("FileList")
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::List)
            .a11y_alt("Files")
            .tw(&tokens, &[flex_col(), w_full(), h_full(), px(S2)])
            .on_key_down(on_key_down)
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                let mut held = modifiers;
                held.set(event.modifiers);
            })
            .on_global_key_up(move |event: Event<KeyboardEventData>| {
                let mut held = modifiers;
                held.set(event.modifiers);
            })
            .maybe_child(header)
            .child(rect().tw(&tokens, &[w_full(), h_flex()]).child(body))
            .focusable(focus(), Radius::Control)
            .tw(&tokens, &[w_full(), h_full()])
    }
}

#[derive(PartialEq)]
struct ColumnHeaders {
    sort: Sort,
    recent: bool,
    on_sort: EventHandler<SortKey>,
}

impl Component for ColumnHeaders {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let column = |key: SortKey, title: &str, width: Option<Dim>| {
            Element::from(ColumnHeader {
                key,
                title: title.to_string(),
                width,
                sorted: (self.sort.key == key).then_some(self.sort.descending),
                on_sort: self.on_sort.clone(),
            })
        };
        rect()
            .tw(
                &tokens,
                &[
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    w_full(),
                    h(Dim::Control),
                    px(S3),
                    gap(S3),
                ],
            )
            .child(rect().tw(&tokens, &[w(Dim::IconRow)]))
            .child(column(SortKey::Name, "Name", None))
            .child(if self.recent {
                column(SortKey::Kind, "Type", Some(Dim::FileSizeColumn))
            } else {
                column(SortKey::Size, "Size", Some(Dim::FileSizeColumn))
            })
            .child(column(
                SortKey::Modified,
                if self.recent { "Last used" } else { "Modified" },
                Some(Dim::FileModifiedColumn),
            ))
    }
}

#[derive(PartialEq)]
struct ColumnHeader {
    key: SortKey,
    title: String,
    width: Option<Dim>,
    /// `Some(descending)` when the list is sorted by this column.
    sorted: Option<bool>,
    on_sort: EventHandler<SortKey>,
}

impl Component for ColumnHeader {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let key = self.key;
        let on_sort = self.on_sort.clone();
        let arrow = self.sorted.map(|descending| {
            icons::icon(
                if descending {
                    Icon::PanDown
                } else {
                    Icon::PanUp
                },
                IconSize::Mark,
                Paint::Muted.color(&tokens),
            )
        });
        let content = rect()
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    h_full(),
                    px(S1),
                    gap(S1),
                    rounded(Radius::Mark),
                    surface(Paint::Transparent, hovering(), false, false),
                ],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .child(
                CAPTION
                    .in_ink(if self.sorted.is_some() {
                        crate::typography::Ink::Text
                    } else {
                        crate::typography::Ink::Muted
                    })
                    .label(&tokens, self.title.clone())
                    .max_lines(1),
            )
            .maybe_child(arrow);
        let button = focus_stop(
            content,
            format!("Sort by {}", self.title),
            AccessibilityRole::Button,
            true,
            Radius::Mark,
            move |_: ()| on_sort.call(key),
        );
        match self.width {
            Some(width) => button.tw(&tokens, &[w(width), h_full()]),
            None => button.tw(&tokens, &[w_flex(), h_full()]),
        }
    }
}

#[derive(PartialEq)]
struct FileRow {
    entry: Entry,
    row: usize,
    selected: bool,
    enabled: bool,
    compact: bool,
    recent: bool,
    /// `true` when the press came from a finger.
    on_press: EventHandler<bool>,
}

impl Component for FileRow {
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(self.row as u64)
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let entry = &self.entry;
        let on_press = self.on_press.clone();
        let muted = Paint::Muted.color(&tokens);
        let icon = icons::icon(
            entry_icon(entry),
            IconSize::Row,
            if entry.is_dir() {
                Paint::Accent.color(&tokens)
            } else {
                muted
            },
        );
        let modified = entry.modified.map(display::modified).unwrap_or_default();
        let size = if entry.is_dir() {
            String::new()
        } else if self.recent {
            entry.mime.description.to_string()
        } else {
            display::size(entry.size)
        };
        let name = BODY
            .label(&tokens, entry.name.clone())
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis);
        let text: Element = if self.compact {
            let detail = [size.as_str(), modified.as_str()]
                .iter()
                .filter(|part| !part.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" · ");
            rect()
                .tw(&tokens, &[flex_col(), w_flex()])
                .child(name)
                .maybe_child((!detail.is_empty()).then(|| {
                    CAPTION
                        .label(&tokens, detail)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis)
                }))
                .into()
        } else {
            rect()
                .tw(
                    &tokens,
                    &[flex_row(), items(Align::Center), w_flex(), gap(S2)],
                )
                .child(rect().tw(&tokens, &[w_flex()]).child(name))
                .into()
        };
        let mut row = rect()
            .component("FileRow")
            .tw(
                &tokens,
                &[
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    w_full(),
                    h(Dim::NavItem),
                    px(S3),
                    gap(S3),
                    rounded(Radius::Control),
                    surface(
                        Paint::Transparent,
                        hovering() && self.enabled,
                        false,
                        self.selected,
                    ),
                ],
            )
            .a11y_role(AccessibilityRole::ListBoxOption)
            .a11y_alt(entry.name.clone())
            .a11y_builder({
                let selected = self.selected;
                move |builder| builder.set_selected(selected)
            })
            .maybe(!self.enabled, |row| row.opacity(DISABLED_OPACITY))
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                on_press.call(matches!(event.data(), PressEventData::Touch(_)));
            })
            .child(icon)
            .child(text);
        if !self.compact {
            row = row
                .child(
                    rect().tw(&tokens, &[w(Dim::FileSizeColumn)]).child(
                        CAPTION
                            .label(&tokens, size)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    ),
                )
                .child(
                    rect().tw(&tokens, &[w(Dim::FileModifiedColumn)]).child(
                        CAPTION
                            .label(&tokens, modified)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    ),
                );
        }
        row
    }
}
