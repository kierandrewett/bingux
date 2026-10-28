//! The address bar as a trail of folders: crumbs you press to go there, a
//! small chevron after each that lists the folders inside it, a `…` crumb
//! for the middle of a trail that does not fit, and empty space at the end
//! that turns the trail into a text field.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bingux_files::listing::Cancel;
use bingux_files::location::{self, subfolders};
use bingux_files::places::Places;
use bingux_files::{Kind, PlaceKind};
use freya::prelude::*;

use super::address_list::{Lead, ListRow, NoticeRow, Part, rows_height, scroll_offset};
use super::address_target::StopOnDrop;
use super::address_trail::{Item, Metrics, layout};
use super::directory::Where;
use super::file_icon::{FileIcon, FileKind, file_icon_view};
use crate::component::ComponentName;
use crate::controls::path_label::measure_text;
use crate::controls::tooltip::hover_tip;
use crate::controls::{StopRing, focus_stop_menu, focus_stop_with};
use crate::icons::{self, Icon};
use crate::shell::panel::{Panel, PanelKind, PanelRect, PanelTrigger};
use crate::shell::menu_query::{Candidate, Mark, Reaction};
use crate::shell::type_ahead::use_list_query;
use crate::shell::{MenuItem, Overlays, menu_item, menu_separator};
use crate::style::*;
use crate::surface::use_tokens;
use crate::task::{spawn_blocking, use_resource};
use crate::typography::{BODY, Ink};

/// How many subfolders a crumb's list shows. More are cut with a note: a
/// folder of thousands is not browsed in a menu.
const SUBFOLDER_LIMIT: usize = 300;

/// Two presses this close are a double click.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

thread_local! {
    /// The width the trail had last. A bar that opens again starts from it,
    /// so its first frame is already laid out for about the right room.
    static LAST_WIDTH: Cell<f32> = const { Cell::new(0.0) };
}

/// One folder on the way to the current one.
#[derive(Clone, PartialEq)]
pub(super) struct Stop {
    pub label: String,
    /// `None` for Recent, which is not a folder.
    pub path: Option<PathBuf>,
    /// The folder's icon, decided by its path as everywhere else. The trail
    /// draws it on its first crumb only; the compact trail's rows all do.
    pub icon: FileIcon,
}

/// The stops of `location`, outermost first. A path in the home folder starts
/// at Home, any other at Computer. The trail spells the path the text field
/// holds, so the two never disagree about where you are.
pub(super) fn stops(location: &Where, home: &Path) -> Vec<Stop> {
    match location {
        Where::Recent => vec![Stop {
            label: "Recent".into(),
            path: None,
            icon: FileIcon::for_place_kind(PlaceKind::Recent),
        }],
        Where::Folder(path) => location::crumbs(path, home)
            .into_iter()
            .map(|crumb| Stop {
                icon: FileIcon::folder_at(&crumb.path),
                label: crumb.label,
                path: Some(crumb.path),
            })
            .collect(),
    }
}

/// The icon of the folder `name` inside `dir`, in a crumb's list.
pub(super) fn subfolder_icon(dir: &Path, name: &str) -> FileIcon {
    FileIcon::for_path(&dir.join(name), Kind::Directory)
}

/// The trail of a desktop window.
#[derive(PartialEq)]
pub(super) struct Crumbs {
    pub location: Where,
    pub home: PathBuf,
    pub show_hidden: bool,
    pub on_open: EventHandler<Where>,
    /// Turn the trail into the text field.
    pub on_edit: EventHandler<()>,
    /// The bookmarks changed, so the sidebar can list them again.
    pub on_bookmarks: Option<EventHandler<()>>,
}

impl Component for Crumbs {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut width = use_state(|| LAST_WIDTH.with(Cell::get));
        let mut last_press = use_state(|| None::<Instant>);
        let bookmarks = use_resource((), |_: ()| -> Result<Vec<PathBuf>, String> {
            Ok(Places::load_without_devices()
                .bookmarks
                .into_iter()
                .filter_map(|place| place.path)
                .collect())
        });
        let bookmarked: Arc<Vec<PathBuf>> = Arc::new(
            bookmarks
                .with(|resource| resource.value().cloned())
                .unwrap_or_default(),
        );

        let stops = stops(&self.location, &self.home);
        let metrics = Metrics {
            crumb: 2.0 * S2.px(),
            first_extra: IconSize::Control.px() + S1.px(),
            chevron: Dim::Box5.px(),
            widest: Dim::Field.px(),
        };
        // The room for crumbs: the field less its padding and a stretch at the
        // end that always stays empty, to press for the text field.
        let measured = width() > 0.0;
        let room = if measured {
            width() - 2.0 * S1.px() - Dim::Box7.px()
        } else {
            f32::INFINITY
        };
        let labels: Vec<String> = stops.iter().map(|stop| stop.label.clone()).collect();
        let trail = layout(&labels, metrics, room, &|text| measure_text(text, BODY));

        // A second press on a crumb soon after the first starts typing, as a
        // double click anywhere on the bar does.
        let on_open = self.on_open.clone();
        let on_edit = self.on_edit.clone();
        let open_or_edit = EventHandler::new(move |to: Where| {
            let now = Instant::now();
            let double = last_press
                .peek()
                .is_some_and(|before| now.duration_since(before) < DOUBLE_CLICK);
            last_press.set(Some(now));
            if double {
                on_edit.call(());
            } else {
                on_open.call(to);
            }
        });

        let last = stops.len().saturating_sub(1);
        let mut row: Vec<Element> = Vec::new();
        for item in trail {
            match item {
                Item::Crumb { index, text } => {
                    let stop = &stops[index];
                    let current = index == last;
                    let menu = stop
                        .path
                        .as_ref()
                        .map(|path| {
                            crumb_menu(
                                path,
                                bookmarked.contains(path),
                                self.on_open.clone(),
                                self.on_bookmarks.clone(),
                                bookmarks,
                            )
                        })
                        .unwrap_or_default();
                    let target = stop
                        .path
                        .clone()
                        .map(Where::Folder)
                        .unwrap_or(Where::Recent);
                    let press = open_or_edit.clone();
                    row.push(
                        CrumbButton {
                            shown: text,
                            label: stop.label.clone(),
                            icon: (index == 0).then_some(stop.icon),
                            current,
                            menu,
                            on_press: EventHandler::new(move |_: ()| press.call(target.clone())),
                        }
                        .into(),
                    );
                    if let Some(path) = &stop.path {
                        row.push(
                            Chevron {
                                path: path.clone(),
                                title: stop.label.clone(),
                                child: stops.get(index + 1).map(|next| next.label.clone()),
                                show_hidden: self.show_hidden,
                                on_open: self.on_open.clone(),
                            }
                            .into(),
                        );
                    }
                }
                Item::Overflow { hidden } => {
                    let folders: Vec<(String, PathBuf)> = stops[hidden]
                        .iter()
                        .filter_map(|stop| Some((stop.label.clone(), stop.path.clone()?)))
                        .collect();
                    row.push(
                        OverflowButton {
                            folders,
                            on_open: self.on_open.clone(),
                        }
                        .into(),
                    );
                }
            }
        }

        let on_edit = self.on_edit.clone();
        let filler = rect()
            .component("AddressFill")
            .tw(&tokens, &[w_flex(), h_full()])
            .cursor(CursorIcon::Text)
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                on_edit.call(());
            });
        rect()
            .component("AddressCrumbs")
            .a11y_role(AccessibilityRole::Navigation)
            .a11y_alt("Location")
            .tw(
                &tokens,
                &[
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    w_flex(),
                    h(Dim::Control),
                    px(S1),
                    rounded(Radius::Control),
                    bg(Paint::Control),
                    clip(),
                ],
            )
            .on_sized(move |event: Event<SizedEventData>| {
                let next = event.area.width();
                LAST_WIDTH.with(|last| last.set(next));
                width.set_if_modified(next);
            })
            .child(
                rect()
                    .tw(&tokens, &[flex_row(), items(Align::Center), h_full()])
                    // Until the width is known the trail is laid out for no
                    // limit and not drawn, so it never shows a collapse that
                    // is about to change.
                    .opacity(if measured { 1.0 } else { 0.0 })
                    .children(row),
            )
            .child(filler)
    }
}

/// The context menu of a crumb.
fn crumb_menu(
    path: &Path,
    bookmarked: bool,
    on_open: EventHandler<Where>,
    on_changed: Option<EventHandler<()>>,
    bookmarks: crate::task::UseResource<Vec<PathBuf>>,
) -> Vec<MenuItem> {
    let overlays = try_consume_context::<Overlays>();
    let open_path = path.to_path_buf();
    let copy_path = path.to_path_buf();
    let mark_path = path.to_path_buf();
    let toast = move |text: &'static str| {
        if let Some(overlays) = overlays {
            overlays.toast(text);
        }
    };
    let bookmark = if bookmarked {
        menu_item("Remove from bookmarks", move |_| {
            let path = mark_path.clone();
            let on_changed = on_changed.clone();
            spawn(async move {
                match spawn_blocking(move || Places::remove_bookmark(&path)).await {
                    Ok(()) => {
                        bookmarks.refresh();
                        if let Some(on_changed) = on_changed {
                            on_changed.call(());
                        }
                    }
                    Err(_) => toast("The bookmark could not be removed."),
                }
            });
        })
    } else {
        menu_item("Add to bookmarks", move |_| {
            let path = mark_path.clone();
            let on_changed = on_changed.clone();
            spawn(async move {
                match spawn_blocking(move || Places::add_bookmark(&path)).await {
                    Ok(()) => {
                        bookmarks.refresh();
                        if let Some(on_changed) = on_changed {
                            on_changed.call(());
                        }
                    }
                    Err(_) => toast("The bookmark could not be saved."),
                }
            });
        })
    };
    vec![
        menu_item("Open", move |_| {
            on_open.call(Where::Folder(open_path.clone()));
        }),
        menu_separator(),
        menu_item("Copy path", move |_| {
            let text = copy_path.display().to_string();
            match freya::clipboard::Clipboard::set(text) {
                Ok(()) => toast("Copied."),
                Err(_) => toast("The path could not be copied."),
            }
        }),
        bookmark,
    ]
}

/// One crumb: a folder's name. Pressing it opens the folder; a right click
/// offers its menu.
#[derive(PartialEq)]
struct CrumbButton {
    /// The name as drawn, which may be cut to fit.
    shown: String,
    /// The whole name.
    label: String,
    icon: Option<FileIcon>,
    /// The last crumb is where you are: normal ink, the others muted.
    current: bool,
    menu: Vec<MenuItem>,
    on_press: EventHandler<()>,
}

impl Component for CrumbButton {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let ink = if self.current { Ink::Text } else { Ink::Muted };
        let cut = self.shown != self.label;
        let content = rect()
            .component("AddressCrumb")
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    h(Dim::Dense),
                    px(S2),
                    gap(S1),
                    rounded(Radius::Inner),
                    surface(Paint::Transparent, hovering(), false, false),
                ],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .maybe_child(
                self.icon
                    .map(|icon| file_icon_view(&tokens, &icon, IconSize::Control, false)),
            )
            .child(
                BODY.in_ink(ink)
                    .label(&tokens, self.shown.clone())
                    .max_lines(1),
            );
        let stop = focus_stop_menu(
            content,
            self.label.clone(),
            AccessibilityRole::Button,
            true,
            Radius::Inner,
            self.on_press.clone(),
            self.menu.clone(),
        );
        if cut {
            hover_tip(stop, self.label.clone())
        } else {
            stop.into()
        }
    }
}

/// The `…` that stands for crumbs that did not fit. It opens a menu of them.
#[derive(PartialEq)]
struct OverflowButton {
    folders: Vec<(String, PathBuf)>,
    on_open: EventHandler<Where>,
}

impl Component for OverflowButton {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut area = use_state(Area::default);
        let id = use_a11y();
        let focus = use_focus(id);
        let folders = self.folders.clone();
        let on_open = self.on_open.clone();
        let open = EventHandler::new(move |_: ()| {
            let Some(overlays) = try_consume_context::<Overlays>() else {
                return;
            };
            let items: Vec<MenuItem> = folders
                .iter()
                .map(|(name, path)| {
                    let to = Where::Folder(path.clone());
                    let on_open = on_open.clone();
                    menu_item(name.clone(), move |_| on_open.call(to.clone()))
                })
                .collect();
            overlays.show_menu_from(
                Some(id),
                PanelTrigger::Control(PanelRect::from_area(&area.read())),
                items,
                focus() == Focus::Keyboard,
            );
        });
        let content = rect()
            .component("AddressOverflow")
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    h(Dim::Dense),
                    px(S2),
                    rounded(Radius::Inner),
                    surface(Paint::Transparent, hovering(), false, false),
                ],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .child(
                BODY.in_ink(Ink::Muted)
                    .label(&tokens, "…".to_string())
                    .max_lines(1),
            );
        let stop = focus_stop_with(
            id,
            content,
            "Hidden folders",
            AccessibilityRole::Button,
            true,
            Radius::Inner,
            open,
            Vec::new(),
            StopRing::Outside,
        );
        rect()
            .tw(&tokens, &[flex_row(), items(Align::Center)])
            .child(
                rect()
                    .on_sized(move |event: Event<SizedEventData>| area.set_if_modified(event.area))
                    .child(stop),
            )
            .child(chevron_glyph(&tokens))
    }
}

/// The chevron after the `…` crumb. The crumbs it stands for have their own.
fn chevron_glyph(tokens: &crate::theme::Tokens) -> Rect {
    rect()
        .tw(tokens, &[w(Dim::Box5), h(Dim::Dense), center()])
        .child(icons::icon(
            Icon::PanEnd,
            IconSize::Mark,
            Ink::Muted.color(tokens),
        ))
}

/// A subfolder in a crumb's list.
#[derive(Clone, PartialEq)]
struct Folder {
    name: String,
}

/// What a crumb's list holds.
#[derive(Clone, PartialEq)]
struct Folders {
    folders: Vec<Folder>,
    /// There are more than the list shows.
    more: bool,
}

/// The subfolders of `dir`, hidden ones only with `show_hidden`. Reads the
/// disk: for a worker.
fn read_folders(dir: &Path, show_hidden: bool, cancel: &Cancel) -> Folders {
    let found = subfolders(dir, SUBFOLDER_LIMIT * 2 + 1, cancel);
    let mut folders: Vec<Folder> = found
        .into_iter()
        .filter(|candidate| candidate.is_dir)
        .filter(|candidate| show_hidden || !candidate.name.starts_with('.'))
        .map(|candidate| Folder {
            name: candidate.name,
        })
        .collect();
    let more = folders.len() > SUBFOLDER_LIMIT;
    folders.truncate(SUBFOLDER_LIMIT);
    Folders { folders, more }
}

/// The chevron after a crumb. It opens the list of the folders inside it,
/// which is on screen at once with a spinner and fills in when the folder
/// has been read. A list is a choice: a panel on a desktop.
#[derive(PartialEq)]
struct Chevron {
    path: PathBuf,
    /// The crumb's name, for the list's header and the chevron's label.
    title: String,
    /// The next crumb's name: the folder the trail goes on in, marked in the
    /// list.
    child: Option<String>,
    show_hidden: bool,
    on_open: EventHandler<Where>,
}

impl Component for Chevron {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut open = use_state(|| false);
        let mut hovering = use_state(|| false);
        let mut folders = use_state(|| None::<Arc<Folders>>);
        let mut highlighted = use_state(|| None::<usize>);
        let mut hovered = use_state(|| None::<usize>);
        let mut area = use_state(Area::default);
        let mut scroll = use_scroll_controller(ScrollConfig::default);
        let ahead = use_list_query();
        // What the list shows is for this folder; a chevron kept while the
        // trail changes under it starts again.
        let mut shown_for = use_state(|| self.path.clone());
        if *shown_for.peek() != self.path {
            shown_for.set(self.path.clone());
            folders.set(None);
        }

        let path = self.path.clone();
        let child = self.child.clone();
        let show_hidden = self.show_hidden;
        let start = EventHandler::new(move |_: ()| {
            open.set(true);
            hovered.set(None);
            // The list that was read last time is shown while the folder is
            // read again, so a second look is instant.
            highlighted.set(None);
            let path = path.clone();
            let child = child.clone();
            spawn(async move {
                let cancel = Cancel::new();
                let _stop = StopOnDrop(cancel.clone());
                let dir = path.clone();
                let read = spawn_blocking(move || read_folders(&dir, show_hidden, &cancel)).await;
                let at = child
                    .as_ref()
                    .and_then(|name| read.folders.iter().position(|folder| folder.name == *name));
                let count = read.folders.len();
                folders.set(Some(Arc::new(read)));
                if let Some(at) = at {
                    highlighted.set(Some(at));
                    scroll.scroll_to_y(scroll_offset(at, count, 0, true));
                }
            });
        });
        let close = EventHandler::new(move |_: ()| {
            open.set(false);
            highlighted.set(None);
            ahead.clear();
        });
        let list = folders.read().clone();
        let count = list.as_ref().map_or(0, |list| list.folders.len());
        let choose = {
            let on_open = self.on_open.clone();
            let path = self.path.clone();
            let list = list.clone();
            let close = close.clone();
            EventHandler::new(move |index: usize| {
                if let Some(folder) = list.as_ref().and_then(|list| list.folders.get(index)) {
                    on_open.call(Where::Folder(path.join(&folder.name)));
                }
                close.call(());
            })
        };

        let activate = {
            let start = start.clone();
            let close = close.clone();
            let choose = choose.clone();
            EventHandler::new(move |_: ()| {
                if !open() {
                    start.call(());
                } else if let Some(index) = highlighted() {
                    choose.call(index);
                } else {
                    close.call(());
                }
            })
        };

        let on_key_down = {
            let start = start.clone();
            let close = close.clone();
            let list = list.clone();
            let choose = choose.clone();
            move |event: Event<KeyboardEventData>| {
                // Type-ahead first while the list is open: typed letters, and
                // the keys that walk and pick the matches while a query is
                // active.
                if open()
                    && let Some(list) = &list
                {
                    let items: Vec<Candidate> = list
                        .folders
                        .iter()
                        .map(|folder| Candidate {
                            label: &folder.name,
                            selectable: true,
                        })
                        .collect();
                    match ahead.key(&event, &items, highlighted(), true) {
                        Reaction::Ignored => {}
                        Reaction::Handled => {
                            event.stop_propagation();
                            return;
                        }
                        Reaction::Focus(at) => {
                            event.stop_propagation();
                            highlighted.set(Some(at));
                            let current = <(i32, i32)>::from(scroll).1;
                            scroll.scroll_to_y(scroll_offset(at, items.len(), current, false));
                            return;
                        }
                        Reaction::Activate(at) => {
                            event.stop_propagation();
                            choose.call(at);
                            return;
                        }
                    }
                }
                let Key::Named(key) = &event.key else {
                    return;
                };
                if !open() {
                    if *key == NamedKey::ArrowDown {
                        event.stop_propagation();
                        start.call(());
                    }
                    return;
                }
                let last = count.saturating_sub(1);
                let page = ((Dim::MenuList.px() / Dim::MenuItem.px()) as usize).max(1);
                let from = highlighted();
                let next = match key {
                    NamedKey::ArrowDown => from.map_or(0, |at| (at + 1).min(last)),
                    NamedKey::ArrowUp => from.map_or(0, |at| at.saturating_sub(1)),
                    NamedKey::PageDown => from.map_or(0, |at| (at + page).min(last)),
                    NamedKey::PageUp => from.map_or(0, |at| at.saturating_sub(page)),
                    NamedKey::Home => 0,
                    NamedKey::End => last,
                    NamedKey::ArrowLeft => {
                        event.stop_propagation();
                        close.call(());
                        return;
                    }
                    _ => return,
                };
                event.stop_propagation();
                if count > 0 {
                    highlighted.set(Some(next));
                    let current = <(i32, i32)>::from(scroll).1;
                    scroll.scroll_to_y(scroll_offset(next, count, current, false));
                }
            }
        };

        let content = rect()
            .component("AddressChevron")
            .tw(
                &tokens,
                &[
                    w(Dim::Box5),
                    h(Dim::Dense),
                    center(),
                    rounded(Radius::Inner),
                    surface(Paint::Transparent, hovering() || open(), false, false),
                ],
            )
            .a11y_builder(move |builder| builder.set_expanded(open()))
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .child(icons::icon(
                if open() { Icon::PanDown } else { Icon::PanEnd },
                IconSize::Mark,
                if hovering() || open() {
                    Ink::Text
                } else {
                    Ink::Muted
                }
                .color(&tokens),
            ));
        let stop = crate::controls::focus_stop(
            content,
            format!("Folders in {}", self.title),
            AccessibilityRole::Button,
            true,
            Radius::Inner,
            activate,
        );

        let rows: Vec<Element> = match &list {
            None => vec![NoticeRow::busy("Loading…").into()],
            Some(list) if list.folders.is_empty() => {
                vec![NoticeRow::quiet("No subfolders").into()]
            }
            Some(list) => {
                let mut rows: Vec<Element> = list
                    .folders
                    .iter()
                    .enumerate()
                    .map(|(index, folder)| {
                        let choose = choose.clone();
                        Element::from(ListRow {
                            icon: Lead::File(subfolder_icon(&self.path, &folder.name)),
                            parts: vec![Part::plain(folder.name.clone())],
                            current: self.child.as_deref() == Some(folder.name.as_str()),
                            mark: ahead.mark(index),
                            highlighted: hovered().is_none() && highlighted() == Some(index),
                            on_press: EventHandler::new(move |_: ()| choose.call(index)),
                            on_hover: EventHandler::new(move |inside: bool| {
                                hovered.set(inside.then_some(index));
                            }),
                        })
                    })
                    .collect();
                if list.more {
                    rows.push(
                        NoticeRow::quiet("More folders. Type the path to reach them.").into(),
                    );
                }
                rows
            }
        };
        let height = rows_height(rows.len());
        let panel = Panel::new(
            PanelKind::Choice,
            open(),
            PanelTrigger::Control(PanelRect::from_area(&area.read())),
            rows,
        )
        .size(Dim::DropdownMenu.px(), height)
        .scroll(scroll)
        .title(self.title.clone())
        .pill(ahead.pill())
        .on_escape({
            // Escape takes back a typed query first, then closes the list.
            let close = close.clone();
            EventHandler::new(move |_: ()| {
                if !ahead.escape() {
                    close.call(());
                }
            })
        })
        .on_dismiss(close);

        // The stop sets its own size handler, so the panel's anchor is read
        // from the box around it.
        rect()
            .on_key_down(on_key_down)
            .child(
                rect()
                    .on_sized(move |event: Event<SizedEventData>| area.set_if_modified(event.area))
                    .child(stop),
            )
            .child(Element::from(panel))
    }
}

/// The trail on a phone: one row with the name of the folder you are in and
/// a pencil. The name opens a sheet of the whole trail, and the pencil turns
/// the row into the text field.
#[derive(PartialEq)]
pub(super) struct CompactTrail {
    pub location: Where,
    pub home: PathBuf,
    pub on_open: EventHandler<Where>,
    pub on_edit: EventHandler<()>,
}

impl Component for CompactTrail {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut open = use_state(|| false);
        let mut area = use_state(Area::default);
        let mut scroll = use_scroll_controller(ScrollConfig::default);
        let stops = stops(&self.location, &self.home);
        let last = stops.len().saturating_sub(1);
        let current = stops.last().cloned();
        let name = current
            .as_ref()
            .map(|stop| stop.label.clone())
            .unwrap_or_default();
        let current_icon = current
            .as_ref()
            .map_or(FileIcon::for_kind(FileKind::Folder), |stop| stop.icon);

        let close = EventHandler::new(move |_: ()| open.set(false));
        let mut rows: Vec<Element> = stops
            .iter()
            .enumerate()
            .map(|(index, stop)| {
                let to = stop
                    .path
                    .clone()
                    .map(Where::Folder)
                    .unwrap_or(Where::Recent);
                let on_open = self.on_open.clone();
                let close = close.clone();
                Element::from(ListRow {
                    icon: Lead::File(stop.icon),
                    parts: vec![Part::plain(stop.label.clone())],
                    current: index == last,
                    highlighted: false,
                    mark: Mark::Plain,
                    on_press: EventHandler::new(move |_: ()| {
                        close.call(());
                        on_open.call(to.clone());
                    }),
                    on_hover: EventHandler::new(|_: bool| {}),
                })
            })
            .collect();
        let on_edit = self.on_edit.clone();
        let type_path = {
            let close = close.clone();
            EventHandler::new(move |_: ()| {
                close.call(());
                on_edit.call(());
            })
        };
        rows.push(
            ListRow {
                icon: Lead::Action(Icon::DocumentEdit),
                parts: vec![Part::plain("Type a path")],
                current: false,
                highlighted: false,
                mark: Mark::Plain,
                on_press: type_path,
                on_hover: EventHandler::new(|_: bool| {}),
            }
            .into(),
        );
        let height = rows_height(rows.len());
        let panel = Panel::new(
            PanelKind::Choice,
            open(),
            PanelTrigger::Control(PanelRect::from_area(&area.read())),
            rows,
        )
        .size(Dim::DropdownMenu.px(), height)
        .scroll(scroll)
        .title("Location")
        .on_dismiss(close);

        let content = rect()
            .component("AddressCompact")
            .tw(
                &tokens,
                &[
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    gap(S2),
                    w_full(),
                    h_full(),
                    px(S3),
                    rounded(Radius::Control),
                ],
            )
            .child(file_icon_view(&tokens, &current_icon, IconSize::Control, false))
            .child(
                BODY.in_ink(Ink::Text)
                    .label(&tokens, name.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .width(Size::flex(1.0)),
            )
            .child(icons::icon(
                Icon::PanDown,
                IconSize::Row,
                Ink::Muted.color(&tokens),
            ));
        let trail = crate::controls::focus_stop(
            content,
            format!("{name}. Show the path"),
            AccessibilityRole::Button,
            true,
            Radius::Control,
            EventHandler::new(move |_: ()| {
                open.set(true);
                // The trail opens at the folder you are in, the end of it.
                // The rows are laid out a moment after the sheet opens.
                spawn(async move {
                    timer(Duration::from_millis(60)).await;
                    scroll.scroll_to_y(-1_000_000);
                });
            }),
        );
        let on_edit = self.on_edit.clone();
        rect()
            .component("AddressCompactBar")
            .tw(
                &tokens,
                &[
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    w_full(),
                    h(Dim::Control),
                    rounded(Radius::Control),
                    bg(Paint::Control),
                    clip(),
                ],
            )
            .on_sized(move |event: Event<SizedEventData>| area.set_if_modified(event.area))
            .child(rect().tw(&tokens, &[w_flex(), h_full()]).child(trail))
            .child(
                crate::controls::button::icon_button(Icon::DocumentEdit, move |_: ()| {
                    on_edit.call(())
                })
                .size(Dim::Control)
                .tooltip("Type a path"),
            )
            .child(Element::from(panel))
    }
}
