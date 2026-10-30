//! The places list: Recent, Home, the user folders and the Trash, the user's
//! bookmarks, and the drives with how full each one is.
//!
//! [`PlacesSidebar`] is the column beside a file view. [`PlacesMenu`] is the
//! same list in a sheet behind one button, for a phone. Both draw a
//! `PlaceList`, so the rows, their context menus and the keyboard are the
//! same.
//!
//! Every row is a Tab stop. Up and Down walk the rows, Home and End go to the
//! first and the last, a letter jumps to the next row that starts with it,
//! Enter opens, Alt+Enter shows Properties, F2 renames a bookmark, Delete
//! removes it, Ctrl+Shift+C copies the path, and the Menu key or Shift+F10
//! opens the row's menu. A right click or a long press opens it too. The
//! menu's rows and these keys are one list (`place_menu.rs`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use bingux_files::{Place, PlaceKind, Places};
use freya::prelude::*;

use super::directory::Where;
use super::drives::{Drives, use_drives};
use super::file_icon::{FileIcon, file_icon_view};
use super::place_menu::{PlaceCmd, PlaceEnv, PlaceFacts, Subject, key_command, offered, place_menu};
use super::places_model::{
    Item, ItemKind, Release, Section, UsageLine, current_name, release_for, sections, selected,
    type_ahead, usage_line, volume_text,
};
use crate::component::ComponentName;
use crate::controls::{StopRing, focus_stop_press, focus_stop_pressed, path_label, text_field, usage_bar};
use crate::focus::{focus_node, step_focus};
use crate::icons::{self, Icon};
use crate::shell::panel::{Panel, PanelKind, PanelRect, PanelTrigger};
use crate::shell::{MenuItem, Overlays, ResponseKind, dialog};
use crate::style::*;
use crate::surface::use_tokens;
use crate::task::spawn_blocking;
use crate::typography::{BODY, CAPTION, Ink};

/// How long a pause ends a typed prefix, so the next letter starts again.
const TYPE_AHEAD_RESET: Duration = Duration::from_millis(900);

/// A column of the system's places. The place that is exactly the current
/// location is selected; failing that, the drive the location is on.
///
/// Use it beside any file view. A second user besides the chooser is the
/// Files app's own sidebar.
#[derive(PartialEq)]
pub struct PlacesSidebar {
    places: Places,
    current: Where,
    recent: bool,
    on_open: EventHandler<Where>,
    on_open_new_tab: Option<EventHandler<Where>>,
    on_empty_trash: Option<EventHandler<()>>,
    reload: Option<State<u32>>,
}

impl PlacesSidebar {
    /// `on_open` receives the place a person picks.
    pub fn new(places: Places, current: Where, on_open: impl Into<EventHandler<Where>>) -> Self {
        PlacesSidebar {
            places,
            current,
            recent: true,
            on_open: on_open.into(),
            on_open_new_tab: None,
            on_empty_trash: None,
            reload: None,
        }
    }

    /// Whether the Recent place is listed (default true). A chooser that
    /// saves has nothing to take from it.
    pub fn recent(mut self, recent: bool) -> Self {
        self.recent = recent;
        self
    }

    /// Adds "Open in new tab" to the row menus. The handler receives the
    /// place. Without it the item is left out.
    pub fn on_open_new_tab(mut self, handler: impl Into<EventHandler<Where>>) -> Self {
        self.on_open_new_tab = Some(handler.into());
        self
    }

    /// A counter the list adds one to each time it changes the bookmarks, so
    /// whoever loaded `places` can load them again.
    pub fn reload(mut self, reload: State<u32>) -> Self {
        self.reload = Some(reload);
        self
    }

    /// Adds Empty Trash to the Trash row's menu. The handler empties the Trash
    /// (after its question); the owner decides where the question is asked.
    pub fn on_empty_trash(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_empty_trash = Some(handler.into());
        self
    }
}

impl Component for PlacesSidebar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let drives = use_drives();
        let list = drives.list();
        let bookmarks = use_bookmarks(self.reload);
        let shown = bookmarks.current(&self.places.bookmarks);
        let mut groups = sections(&self.places, &shown, &list.mounts, &list.volumes, self.recent);
        // The Trash row follows what the Trash holds, looked at every couple of
        // seconds on a worker (a move to the Trash may come from any window).
        let mut trash_full = use_state(|| false);
        use_hook(move || {
            spawn(async move {
                loop {
                    let full = spawn_blocking(|| !bingux_files::ops::trash_is_empty()).await;
                    trash_full.set_if_modified(full);
                    timer(Duration::from_secs(2)).await;
                }
            });
        });
        super::places_model::trash_state(&mut groups, trash_full());
        let chosen = selected(&groups, &self.current, &list.mounts);
        rect()
            .component("PlacesSidebar")
            .tw(&tokens, &[w_full(), h_full()])
            .child(
                ScrollView::new()
                    .drag_scrolling(false)
                    .width(Size::fill())
                    .height(Size::fill())
                    .child(
                        rect()
                            .tw(&tokens, &[flex_col(), w_full(), p(S2)])
                            .child(PlaceList {
                                sections: groups,
                                selected: chosen,
                                settled: list.settled,
                                drives,
                                bookmarks,
                                bookmark_source: self.places.bookmarks.clone(),
                                actions: Actions {
                                    on_open: self.on_open.clone(),
                                    on_open_new_tab: self.on_open_new_tab.clone(),
                                    dismiss: None,
                                    on_empty_trash: self.on_empty_trash.clone(),
                                    trash_full: trash_full(),
                                },
                                measure: None,
                                on_escape: None,
                            }),
                    ),
            )
    }
}

/// The places on a phone: one button showing where you are, which opens a
/// sheet of the same list the sidebar has, with the drives and how full they
/// are. Put it in the toolbar in place of the sidebar.
#[derive(PartialEq)]
pub struct PlacesMenu {
    places: Places,
    current: Where,
    recent: bool,
    on_open: EventHandler<Where>,
    on_open_new_tab: Option<EventHandler<Where>>,
    reload: Option<State<u32>>,
}

impl PlacesMenu {
    /// `on_open` receives the place a person picks. The sheet closes first.
    pub fn new(places: Places, current: Where, on_open: impl Into<EventHandler<Where>>) -> Self {
        PlacesMenu {
            places,
            current,
            recent: true,
            on_open: on_open.into(),
            on_open_new_tab: None,
            reload: None,
        }
    }

    /// Whether the Recent place is listed (default true).
    pub fn recent(mut self, recent: bool) -> Self {
        self.recent = recent;
        self
    }

    /// See [`PlacesSidebar::on_open_new_tab`].
    pub fn on_open_new_tab(mut self, handler: impl Into<EventHandler<Where>>) -> Self {
        self.on_open_new_tab = Some(handler.into());
        self
    }

    /// See [`PlacesSidebar::reload`].
    pub fn reload(mut self, reload: State<u32>) -> Self {
        self.reload = Some(reload);
        self
    }
}

impl Component for PlacesMenu {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut open = use_state(|| false);
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let mut area = use_state(Area::default);
        let measured = use_state(|| 0.0_f32);
        let scroll = use_scroll_controller(ScrollConfig::default);
        let drives = use_drives();
        let list = drives.list();
        let bookmarks = use_bookmarks(self.reload);
        let shown = bookmarks.current(&self.places.bookmarks);
        let groups = sections(&self.places, &shown, &list.mounts, &list.volumes, self.recent);
        let chosen = selected(&groups, &self.current, &list.mounts);
        let row = chosen.map(|(group, item)| &groups[group].items[item]);
        let name = current_name(&self.current, row);
        let icon = match (row, &self.current) {
            (Some(item), _) => item.icon,
            (None, Where::Recent) => FileIcon::for_place_kind(PlaceKind::Recent),
            (None, Where::Folder(path)) => FileIcon::folder_at(path),
        };

        let content = rect()
            .component("PlacesMenu")
            .tw(
                &tokens,
                &[
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    w_full(),
                    h(Dim::Control),
                    px(S3),
                    gap(S2),
                    rounded(Radius::Control),
                    surface(Paint::Control, hovering() || open(), pressing(), false),
                ],
            )
            .on_sized(move |event: Event<SizedEventData>| area.set_if_modified(event.area))
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            .child(file_icon_view(&tokens, &icon, IconSize::Control, false))
            .child(
                rect().tw(&tokens, &[w_flex()]).child(
                    BODY.in_ink(Ink::Text)
                        .label(&tokens, name)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                ),
            )
            .child(icons::icon(
                Icon::PanDown,
                IconSize::Mark,
                Paint::Muted.color(&tokens),
            ));
        let button = focus_stop_press(
            content,
            "Places",
            AccessibilityRole::Button,
            true,
            Radius::Control,
            move |_: ()| open.set(!open()),
            pressing,
        );

        let close = {
            let on_open = self.on_open.clone();
            EventHandler::new(move |target: Where| {
                open.set(false);
                on_open.call(target);
            })
        };
        let wrap = |handler: &Option<EventHandler<Where>>| {
            handler.clone().map(|handler| {
                EventHandler::new(move |target: Where| {
                    open.set(false);
                    handler.call(target);
                })
            })
        };
        let rows = PlaceList {
            sections: groups.clone(),
            selected: chosen,
            settled: list.settled,
            drives,
            bookmarks,
            bookmark_source: self.places.bookmarks.clone(),
            actions: Actions {
                on_open: close,
                on_open_new_tab: wrap(&self.on_open_new_tab),
                on_empty_trash: None,
                trash_full: false,
                dismiss: Some(EventHandler::new(move |_: ()| open.set(false))),
            },
            measure: Some(measured),
            on_escape: Some(EventHandler::new(move |_: ()| open.set(false))),
        };
        let rows_height = match *measured.read() {
            height if height > 0.0 => height,
            // Until the list has been laid out once, a row's worth each.
            _ => {
                groups.iter().map(|group| group.items.len()).sum::<usize>() as f32
                    * Dim::NavItem.px()
            }
        };
        let panel = Panel::new(
            PanelKind::Choice,
            open(),
            PanelTrigger::Control(PanelRect::from_area(&area.read())),
            vec![Element::from(rows)],
        )
        .size(area.read().width().max(Dim::DropdownMenu.px()), rows_height)
        .scroll(scroll)
        .on_dismiss(move |_: ()| open.set(false))
        .title("Places");

        rect()
            .component("PlacesMenuHost")
            .tw(&tokens, &[w_full()])
            .child(button)
            .child(Element::from(panel))
    }
}

// ---------------------------------------------------------------------------
// Bookmarks
// ---------------------------------------------------------------------------

/// Changing the bookmarks from a menu or the Delete key. The file is written
/// on a worker, and the list shows the result as soon as it is read back.
#[derive(Clone, Copy, PartialEq)]
struct Bookmarks {
    /// The bookmarks as last read back, with the props they were read
    /// against. They count only while the props are still those, so a list
    /// the owner reloads takes over.
    local: State<Option<(Vec<Place>, Vec<Place>)>>,
    reload: Option<State<u32>>,
    overlays: Option<Overlays>,
}

fn use_bookmarks(reload: Option<State<u32>>) -> Bookmarks {
    Bookmarks {
        local: use_state(|| None),
        reload,
        overlays: try_consume_context::<Overlays>(),
    }
}

impl Bookmarks {
    /// The bookmarks to show, given the ones the owner passed in.
    fn current(&self, from_props: &[Place]) -> Vec<Place> {
        match &*self.local.read() {
            Some((source, loaded)) if source == from_props => loaded.clone(),
            _ => from_props.to_vec(),
        }
    }

    /// Add or remove the bookmark `path`. `source` is the bookmarks the list
    /// was drawn from; `name` is what to call it in a message.
    fn change(self, source: Vec<Place>, path: PathBuf, add: bool, name: String) {
        let mut local = self.local;
        let mut reload = self.reload;
        let overlays = self.overlays;
        spawn(async move {
            let target = path.clone();
            let outcome = spawn_blocking(move || {
                let done = if add {
                    Places::add_bookmark(&target)
                } else {
                    Places::remove_bookmark(&target)
                };
                done.map(|()| Places::load_without_devices().bookmarks)
            })
            .await;
            match outcome {
                Ok(loaded) => {
                    local.set(Some((source, loaded.clone())));
                    if let Some(reload) = &mut reload {
                        *reload.write() += 1;
                    }
                    if let (false, Some(overlays)) = (add, overlays) {
                        overlays.toast_with_action(
                            format!("Removed {name} from the bookmarks."),
                            "Undo",
                            move |_: ()| {
                                self.change(loaded.clone(), path.clone(), true, name.clone())
                            },
                        );
                    }
                }
                Err(error) => {
                    if let Some(overlays) = overlays {
                        overlays.toast(format!("The bookmarks could not be changed: {error}."));
                    }
                }
            }
        });
    }

    /// Give the bookmark `path` the label `name`. An empty name takes the
    /// folder's own name back.
    fn rename(self, source: Vec<Place>, path: PathBuf, name: String) {
        let mut local = self.local;
        let mut reload = self.reload;
        let overlays = self.overlays;
        spawn(async move {
            let outcome = spawn_blocking(move || {
                Places::rename_bookmark(&path, &name)
                    .map(|()| Places::load_without_devices().bookmarks)
            })
            .await;
            match outcome {
                Ok(loaded) => {
                    local.set(Some((source, loaded)));
                    if let Some(reload) = &mut reload {
                        *reload.write() += 1;
                    }
                }
                Err(error) => {
                    if let Some(overlays) = overlays {
                        overlays.toast(format!("The bookmark could not be renamed: {error}."));
                    }
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// The list
// ---------------------------------------------------------------------------

/// What a row can ask of the app around it.
#[derive(Clone, PartialEq)]
struct Actions {
    on_open: EventHandler<Where>,
    on_open_new_tab: Option<EventHandler<Where>>,
    /// Empty Trash on the Trash row's menu.
    on_empty_trash: Option<EventHandler<()>>,
    /// The Trash has something in it, so Empty Trash is not greyed.
    trash_full: bool,
    /// Closes the sheet the list is in, before a dialog opens over it.
    dismiss: Option<EventHandler<()>>,
}

/// The groups and rows, and the keys that walk them.
#[derive(PartialEq)]
struct PlaceList {
    sections: Vec<Section>,
    selected: Option<(usize, usize)>,
    settled: bool,
    drives: Drives,
    bookmarks: Bookmarks,
    /// The bookmarks the owner passed in, which `bookmarks` is read against.
    bookmark_source: Vec<Place>,
    actions: Actions,
    /// Receives the height of the list, for a sheet that has to know it.
    measure: Option<State<f32>>,
    on_escape: Option<EventHandler<()>>,
}

impl PlaceList {
    /// What a row can be asked to do: the facts its menu is chosen from and
    /// the handler that does it. The menu and the keys of the row both send
    /// their commands here, so they cannot disagree (see `place_menu.rs`).
    /// `id` is the row's focus node, which a dialog gives the focus back to.
    fn commands(
        &self,
        item: &Item,
        open: EventHandler<()>,
        release: Option<Release>,
        busy: bool,
        draft: State<String>,
        id: AccessibilityId,
    ) -> (PlaceFacts, EventHandler<PlaceCmd>) {
        let actions = &self.actions;
        let facts = PlaceFacts::of(
            item,
            actions.on_open_new_tab.is_some(),
            release,
            busy,
            actions.on_empty_trash.is_some(),
            actions.trash_full,
        );
        let target = item.target.clone();
        let path = item.path().map(Path::to_path_buf);
        let name = item.label.clone();
        // What the Properties dialog calls the place: the folder's own name,
        // except where that is not a name (the Trash's folder is `files`, the
        // root has none) and a drive, which has its label.
        let properties_name = match Subject::of(item) {
            Subject::Trash | Subject::Computer | Subject::Drive => item.label.clone(),
            _ => path
                .as_deref()
                .and_then(Path::file_name)
                .map_or_else(|| item.label.clone(), |name| name.to_string_lossy().into_owned()),
        };
        let (bookmarks, source, drives) = (self.bookmarks, self.bookmark_source.clone(), self.drives);
        let mount = item.mount.clone();
        let on_new_tab = actions.on_open_new_tab.clone();
        let on_empty_trash = actions.on_empty_trash.clone();
        let on_open = actions.on_open.clone();
        let dismiss = actions.dismiss.clone();
        let act = EventHandler::new(move |cmd: PlaceCmd| {
            let overlays = bookmarks.overlays;
            match cmd {
                PlaceCmd::Open => open.call(()),
                PlaceCmd::OpenInNewTab => {
                    if let Some(handler) = &on_new_tab {
                        handler.call(target.clone());
                    }
                }
                PlaceCmd::Rename => {
                    let (Some(overlays), Some(path)) = (overlays, path.clone()) else {
                        return;
                    };
                    let mut draft = draft;
                    draft.set(name.clone());
                    let source = source.clone();
                    overlays.show_dialog(
                        dialog("Rename bookmark", "")
                            .content(Element::from(
                                text_field(draft, |_: String| {})
                                    .fill_width()
                                    .placeholder("Bookmark name")
                                    .autofocus(),
                            ))
                            .cancel("Cancel")
                            .response("Rename", ResponseKind::Suggested, move |_: ()| {
                                bookmarks.rename(source.clone(), path.clone(), draft.peek().clone())
                            }),
                    );
                }
                PlaceCmd::Release(how) => {
                    if let Some(mount) = &mount {
                        drives.release(mount.clone(), how);
                    }
                }
                PlaceCmd::EmptyTrash => {
                    if let Some(handler) = &on_empty_trash {
                        handler.call(());
                    }
                }
                PlaceCmd::CopyPath => {
                    let Some(path) = &path else {
                        return;
                    };
                    let said = match freya::clipboard::Clipboard::set(path.display().to_string()) {
                        Ok(()) => "Copied.",
                        Err(_) => "The path could not be copied.",
                    };
                    if let Some(overlays) = overlays {
                        overlays.toast(said);
                    }
                }
                PlaceCmd::AddBookmark | PlaceCmd::RemoveBookmark => {
                    if let Some(path) = &path {
                        let add = cmd == PlaceCmd::AddBookmark;
                        bookmarks.change(source.clone(), path.clone(), add, name.clone());
                    }
                }
                PlaceCmd::Properties => {
                    let (Some(overlays), Some(path)) = (overlays, &path) else {
                        return;
                    };
                    if let Some(dismiss) = &dismiss {
                        dismiss.call(());
                    }
                    let on_open = on_open.clone();
                    super::properties::show_folder_properties(
                        path.clone(),
                        properties_name.clone(),
                        overlays,
                        EventHandler::new(move |folder: PathBuf| on_open.call(Where::Folder(folder))),
                        EventHandler::new(move |()| focus_node(id)),
                    );
                }
            }
        });
        (facts, act)
    }
}

/// What names a row across renders: what it is and where it goes.
fn row_key(item: &Item) -> String {
    format!("{:?}|{:?}", item.kind, item.target)
}

impl Component for PlaceList {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let total: usize = self.sections.iter().map(|group| group.items.len()).sum();
        // One focus node per row, kept while the row exists, so Home, End and
        // type-ahead can ask for a row and a row that moves keeps its focus.
        let pool = use_hook(|| Rc::new(RefCell::new(HashMap::<String, AccessibilityId>::new())));
        let ids: Vec<AccessibilityId> = {
            let mut pool = pool.borrow_mut();
            let keys: Vec<String> = self
                .sections
                .iter()
                .flat_map(|group| group.items.iter().map(row_key))
                .collect();
            pool.retain(|key, _| keys.contains(key));
            keys.into_iter()
                .map(|key| *pool.entry(key).or_insert_with(AccessibilityId::new_unique))
                .collect()
        };
        let typed = use_hook(|| Rc::new(RefCell::new((String::new(), Instant::now()))));
        // The name typed into the rename dialog of a bookmark.
        let rename_draft = use_state(String::new);

        let mut column = rect()
            .component("PlaceList")
            .tw(&tokens, &[flex_col(), w_full(), gap(S0_5)])
            .on_sized({
                let measure = self.measure;
                move |event: Event<SizedEventData>| {
                    if let Some(mut measure) = measure {
                        measure.set_if_modified(event.area.height());
                    }
                }
            });
        let mut labels = Vec::with_capacity(total);
        let mut commands: Vec<(PlaceFacts, EventHandler<PlaceCmd>)> = Vec::with_capacity(total);
        let mut index = 0;
        for (group_index, group) in self.sections.iter().enumerate() {
            if let Some(title) = group.title {
                column = column.child(
                    rect()
                        .tw(&tokens, NAV_LABEL)
                        .child(CAPTION.label(&tokens, title)),
                );
            }
            for (item_index, item) in group.items.iter().enumerate() {
                let release = item.mount.as_ref().and_then(release_for);
                let busy = item
                    .mount
                    .as_ref()
                    .map(|mount| mount.id)
                    .or_else(|| item.volume.as_ref().map(|volume| volume.busy_id()))
                    .is_some_and(|id| self.drives.is_busy(id));
                labels.push(item.label.clone());
                let on_open = {
                    let handler = self.actions.on_open.clone();
                    let target = item.target.clone();
                    match &item.volume {
                        // A drive that is not mounted is mounted, then opened.
                        Some(volume) => {
                            let (drives, volume) = (self.drives, volume.clone());
                            EventHandler::new(move |_: ()| {
                                drives.mount(volume.clone(), handler.clone())
                            })
                        }
                        None => EventHandler::new(move |_: ()| handler.call(target.clone())),
                    }
                };
                let (facts, act) = self.commands(item, on_open.clone(), release, busy, rename_draft, ids[index]);
                let menu = place_menu(PlaceEnv {
                    facts: facts.clone(),
                    act: act.clone(),
                });
                commands.push((facts, act));
                column = column.child(Element::from(PlaceRow {
                    key: row_key(item),
                    id: ids[index],
                    icon: item.icon,
                    label: item.label.clone(),
                    kind: item.kind,
                    selected: self.selected == Some((group_index, item_index)),
                    usage: match &item.volume {
                        Some(volume) => UsageLine::Text(volume_text(volume)),
                        None => usage_line(item.mount.as_ref(), self.settled),
                    },
                    menu,
                    on_open,
                    drop: item.path().map(Path::to_path_buf),
                }));
                index += 1;
            }
        }

        let on_escape = self.on_escape.clone();
        column.on_key_down(move |event: Event<KeyboardEventData>| {
            let focused = *Platform::get()
                .current_window()
                .focused_accessibility_id
                .peek();
            let at = ids.iter().position(|id| *id == focused);
            // A key of the menu does what its row does, when the row has it.
            if let Some(at) = at
                && let Some(cmd) = key_command(&event.key, event.modifiers)
                && let Some((facts, act)) = commands.get(at)
                && offered(facts, cmd)
            {
                event.stop_propagation();
                // Focus stays in the list: on the row that follows, or the
                // one before when the last row goes.
                if cmd == PlaceCmd::RemoveBookmark
                    && let Some(next) = ids
                        .get(at + 1)
                        .or_else(|| at.checked_sub(1).and_then(|before| ids.get(before)))
                {
                    focus_node(*next);
                }
                act.call(cmd);
                return;
            }
            match &event.key {
                Key::Named(NamedKey::ArrowDown) => {
                    event.stop_propagation();
                    step_focus(false);
                }
                Key::Named(NamedKey::ArrowUp) => {
                    event.stop_propagation();
                    step_focus(true);
                }
                Key::Named(NamedKey::Home) => {
                    if let Some(id) = ids.first() {
                        event.stop_propagation();
                        focus_node(*id);
                    }
                }
                Key::Named(NamedKey::End) => {
                    if let Some(id) = ids.last() {
                        event.stop_propagation();
                        focus_node(*id);
                    }
                }
                Key::Named(NamedKey::Escape) => {
                    if let Some(on_escape) = &on_escape {
                        event.stop_propagation();
                        on_escape.call(());
                    }
                }
                Key::Character(text) => {
                    let plain = !(event.modifiers.contains(Modifiers::CONTROL)
                        || event.modifiers.contains(Modifiers::ALT)
                        || event.modifiers.contains(Modifiers::META));
                    if !plain || text.trim().is_empty() {
                        return;
                    }
                    let mut typed = typed.borrow_mut();
                    if typed.1.elapsed() > TYPE_AHEAD_RESET {
                        typed.0.clear();
                    }
                    typed.0.push_str(text);
                    typed.1 = Instant::now();
                    if let Some(found) = type_ahead(&labels, at, &typed.0) {
                        event.stop_propagation();
                        focus_node(ids[found]);
                    }
                }
                _ => {}
            }
        })
    }
}

// ---------------------------------------------------------------------------
// One row
// ---------------------------------------------------------------------------

#[derive(PartialEq)]
struct PlaceRow {
    /// Names the row across renders, so a row keeps its place in the tree,
    /// and its focus node, when another row above it comes or goes.
    key: String,
    id: AccessibilityId,
    icon: FileIcon,
    label: String,
    kind: ItemKind,
    selected: bool,
    usage: UsageLine,
    menu: Vec<MenuItem>,
    on_open: EventHandler<()>,
    /// The folder files dragged onto the row go into, if it takes any.
    drop: Option<PathBuf>,
}

impl Component for PlaceRow {
    fn render_key(&self) -> DiffKey {
        let mut hasher = std::hash::DefaultHasher::new();
        std::hash::Hash::hash(&self.key, &mut hasher);
        DiffKey::U64(std::hash::Hasher::finish(&hasher))
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let session = use_hook(super::drag::file_drag);
        let drop_here = match (&session, &self.drop) {
            (Some(session), Some(folder)) => session.is_over(folder),
            _ => false,
        };
        let silent = matches!(self.usage, UsageLine::NotResponding);
        let icon = if silent { self.icon.dimmed() } else { self.icon };
        let ink = if silent { Ink::Muted } else { Ink::Text };
        let name: Element = match self.kind {
            ItemKind::Place => BODY
                .in_ink(ink)
                .label(&tokens, self.label.clone())
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis)
                .into(),
            // Names people gave, which can be long.
            ItemKind::Bookmark | ItemKind::Drive => {
                path_label(self.label.clone()).ink(ink).middle().into()
            }
        };

        let (line, spoken): (Option<Element>, Option<String>) = match &self.usage {
            UsageLine::Nothing => (None, None),
            // The bar's track and a blank line, so the row is as tall as it
            // will be and does not jump when the figures arrive.
            UsageLine::Reading => (
                Some(
                    rect()
                        .tw(&tokens, &[flex_col(), w_full(), gap(S0_5)])
                        .child(usage_bar(None))
                        .child(CAPTION.label(&tokens, " "))
                        .into(),
                ),
                None,
            ),
            UsageLine::Known { fraction, text, .. } => (
                Some(
                    rect()
                        .tw(&tokens, &[flex_col(), w_full(), gap(S0_5)])
                        .child(usage_bar(*fraction))
                        .child(
                            CAPTION
                                .label(&tokens, text.clone())
                                .max_lines(1)
                                .text_overflow(TextOverflow::Ellipsis),
                        )
                        .into(),
                ),
                Some(text.clone()),
            ),
            UsageLine::NotResponding => (
                Some(CAPTION.label(&tokens, "Not responding").max_lines(1).into()),
                Some("Not responding".to_string()),
            ),
            UsageLine::Text(text) => (
                Some(
                    CAPTION
                        .label(&tokens, text.clone())
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis)
                        .into(),
                ),
                Some(text.clone()),
            ),
        };
        let tall = line.is_some();

        let content = rect()
            .component("PlaceRow")
            .tw(&tokens, LIST_ITEM)
            .tw(&tokens, &[grow_children()])
            .maybe(!tall, |row| row.tw(&tokens, &[h(Dim::NavItem)]))
            .maybe(tall, |row| row.tw(&tokens, &[py(S1)]))
            .tw(
                &tokens,
                &[surface(
                    if drop_here {
                        Paint::AccentSoft
                    } else {
                        Paint::Transparent
                    },
                    hovering(),
                    pressing(),
                    self.selected,
                )],
            )
            .on_pointer_enter({
                let (session, folder) = (session, self.drop.clone());
                move |_| {
                    hovering.set(true);
                    if let (Some(session), Some(folder)) = (session, &folder) {
                        session.hover(folder);
                    }
                }
            })
            .on_pointer_leave({
                let (session, folder) = (session, self.drop.clone());
                move |_| {
                    hovering.set(false);
                    pressing.set_if_modified(false);
                    if let (Some(session), Some(folder)) = (session, &folder) {
                        session.leave(folder);
                    }
                }
            })
            .a11y_builder({
                let selected = self.selected;
                move |builder| builder.set_selected(selected)
            })
            .child(
                rect()
                    .tw(&tokens, NAV_ICON)
                    .child(file_icon_view(&tokens, &icon, IconSize::Row, self.selected)),
            )
            .child(
                rect()
                    .tw(&tokens, &[flex_col(), w_flex(), gap(S0_5)])
                    .child(rect().tw(&tokens, &[w_full()]).child(name))
                    .maybe_child(line),
            );

        let spoken = match spoken {
            Some(text) => format!("{}, {text}", self.label),
            None => self.label.clone(),
        };
        let on_open = self.on_open.clone();
        focus_stop_pressed(
            self.id,
            content,
            spoken,
            AccessibilityRole::Tab,
            true,
            Radius::Control,
            move |_: ()| on_open.call(()),
            self.menu.clone(),
            StopRing::Inside,
            Some(pressing),
        )
        .tw(&tokens, &[w_full()])
    }
}

/// One sidebar row on its own, for the test that every surface draws a path
/// with the same icon.
#[cfg(test)]
pub(super) fn probe_row(icon: FileIcon, selected: bool) -> Element {
    PlaceRow {
        key: "probe".into(),
        id: AccessibilityId::new_unique(),
        icon,
        label: "probe".into(),
        kind: ItemKind::Bookmark,
        selected,
        usage: UsageLine::Nothing,
        menu: Vec::new(),
        on_open: EventHandler::new(|_: ()| {}),
        drop: None,
    }
    .into()
}
