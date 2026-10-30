//! The window: a header bar with the tabs, over the file view of the tab on
//! show.
//!
//! Each tab is `bingux_ui::files::browse`, the chooser's body without its
//! question. There is one body for all the tabs, mounted once: the toolbar,
//! the places sidebar and the status line stay as they are when a tab is
//! shown, and only the file view and the states behind it are the tab's. A
//! tab keeps where it is (folder, Back and Forward), what is selected, its
//! sort, zoom and the rest in its [`TabMemory`] and in the body's states for
//! it, and shows them again from there in the first frame.
//!
//! The tabs themselves are a [`TabSet`]: plain data with the rules a person
//! feels (where a new tab opens, who is selected after a close, what
//! Ctrl+Shift+T brings back). This file puts them on screen. Closing the last
//! tab closes the window, and asks first only if a file operation is running.

use bingux_ui::files::{self, Browse, Where};
use bingux_ui::prelude::*;
use bingux_ui::shell::{MenuItem, with_window};
use freya::prelude::*;

use crate::tabset::{Closed, Tab, TabSet};
use crate::target::destination;
use crate::tearoff;
use crate::window_size::remember_size;
use crate::windows::{self, Birth, Entry};
use bingux_ui::app::use_window_seed;
use bingux_ui::controls::{Detach, StripProbe};

/// The tabs of the window. Every field is `Copy`, so every handler takes its
/// own.
#[derive(Clone, Copy)]
struct Tabs {
    set: State<TabSet>,
    window: Window,
    overlays: Overlays,
}

impl Tabs {
    /// Open `at` in a new tab beside the one on show and go to it.
    fn open(self, at: Where) {
        self.open_selecting(at, None);
    }

    /// [`Tabs::open`], with `file` (named on the command line) selected in
    /// the folder the tab shows first.
    fn open_selecting(mut self, at: Where, file: Option<std::path::PathBuf>) {
        let id = self.set.write().open(at);
        if let Some(file) = file {
            let set = self.set.peek();
            if let Some(tab) = set.tabs().iter().find(|tab| tab.id == id) {
                tab.memory.select_on_open(file);
            }
        }
    }

    fn select(mut self, id: u64) {
        let known = {
            let set = self.set.peek();
            set.active() != id && set.ids().contains(&id)
        };
        if known {
            self.set.write().select(id);
        }
    }

    /// Close a tab. The tab that takes the selection is the neighbour to its
    /// right, else the one to its left; the last tab closes the window.
    fn close(mut self, id: u64) {
        let closed = {
            let set = self.set.peek();
            set.ids().contains(&id).then_some(set.len() == 1)
        };
        match closed {
            Some(true) => self.close_window(),
            Some(false) => {
                if self.set.write().close(id) == Closed::Window {
                    self.close_window();
                }
            }
            None => {}
        }
    }

    /// Close the window, after asking if a copy, a move or a compression is
    /// still running: closing would stop it half way.
    fn close_window(self) {
        let window = self.window;
        if files::operations_running() {
            self.overlays.show_dialog(
                dialog(
                    "Close Files?",
                    "A file operation is still running. Closing the window stops it.",
                )
                .cancel("Keep open")
                .default_action(CANCEL_ID)
                .icon(DialogIcon::Warning)
                .response("Close window", ResponseKind::Destructive, move |_: ()| window.close()),
            );
        } else {
            window.close();
        }
    }

    /// Close the tabs of a menu's *Close other tabs* or *Close tabs to the
    /// right*.
    fn close_many(mut self, ids: &[u64]) {
        self.set.write().close_many(ids);
    }

    /// Open a copy of a tab beside it.
    fn duplicate(mut self, id: u64) {
        self.set.write().duplicate(id);
    }

    /// Bring back the tab closed last.
    fn reopen(mut self) {
        if self.set.peek().can_reopen() {
            self.set.write().reopen();
        }
    }

    /// Go `by` tabs along, wrapping at either end.
    fn step(mut self, by: isize) {
        self.set.write().step(by);
    }

    /// Go to the tab at `index` from the start, or the last for `usize::MAX`.
    fn nth(mut self, index: usize) {
        self.set.write().nth(index);
    }

    /// A tab was dragged (or moved by its menu) to place `to` in the list.
    fn reorder(mut self, id: u64, to: usize) {
        self.set.write().reorder(id, to);
    }

    /// A tab was torn off the strip (see [`crate::tearoff`]). Let go beyond
    /// the strip, it opens a window of its own and this window keeps the
    /// rest. Still held, this window is what follows the pointer, so it keeps
    /// only that tab and the others go to a window of their own.
    fn tear(mut self, detach: Detach) {
        let Some(entry) = windows::find(Platform::window_id()) else {
            return;
        };
        let size = entry.size.get();
        if detach.released {
            let moved = self.set.write().detach(detach.id);
            if let Some(moved) = moved {
                windows::open(vec![moved], 0, size);
            }
            return;
        }
        let others = self.set.write().detach_others(detach.id);
        if let Some((others, on_show)) = others {
            windows::open(others, on_show, size);
        }
        tearoff::begin(&entry, detach.pointer, detach.press);
    }

    /// A tab went to another folder: its title follows.
    fn moved(mut self, id: u64, at: Where) {
        let changed = self.set.peek().differs(id, &at);
        if changed {
            self.set.write().moved(id, at);
        }
    }
}

/// The titles of all the tabs: each folder's name, with the nearest parent
/// folders added to the names that two tabs share (`Documents — kieran`).
fn tab_titles(tabs: &[Tab]) -> Vec<String> {
    let roots = files::Roots::current();
    let folders: Vec<(usize, String)> = tabs
        .iter()
        .enumerate()
        .filter_map(|(index, tab)| match &tab.at {
            // The roots (Home, the Trash and what is in it) have names of
            // their own and are never told apart by a parent folder.
            Where::Folder(path)
                if roots.root_of(&tab.at).is_none()
                    && roots.inside_of(&tab.at).is_none()
                    && path.file_name().is_some() =>
            {
                Some((index, path.to_string_lossy().into_owned()))
            }
            _ => None,
        })
        .collect();
    let paths: Vec<&str> = folders.iter().map(|(_, path)| path.as_str()).collect();
    let labels = bingux_ui::elide::distinguish(&paths);
    let mut titles: Vec<String> = tabs.iter().map(|tab| title_of(&tab.at)).collect();
    for ((index, _), label) in folders.iter().zip(labels) {
        titles[*index] = label.label();
    }
    titles
}

/// The icon a tab shows: its place's, as the sidebar and the crumbs draw it
/// (the Trash's bin, Recent's clock).
fn tab_icon(at: &Where) -> files::FileIcon {
    files::Roots::current().icon(at)
}

/// The name a tab and the window go by: the folder's name, "Home" for the
/// home folder, "Recent" and "Trash" for those places (and the name an item
/// had before it was deleted, inside the Trash), as the crumbs and the
/// sidebar read.
fn title_of(at: &Where) -> String {
    files::Roots::current().title(at)
}

/// The window root. The content is below the overlay host, which provides
/// the window handle and the menus.
#[derive(PartialEq)]
pub struct App;

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        // Files dragged out of the list can drop on a sidebar place or a tab.
        files::provide_file_drag();
        rect()
            .tw(&tokens, &[w_full(), h_full(), ink(Paint::Text)])
            .font_family(bingux_ui::theme::font_family())
            .child(OverlayHost::new(Content))
    }
}

#[derive(PartialEq)]
struct Content;

impl Component for Content {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let window = use_window();
        let overlays = use_overlays();
        let maximized = freya::borderless::use_maximized();
        let location = use_location();
        // A window opened for tabs torn off another is born with them.
        let birth = use_window_seed::<Birth>();
        let set = use_state({
            let location = location.clone();
            move || {
                birth
                    .and_then(|birth| birth.take())
                    .and_then(|(tabs, on_show)| TabSet::adopt(tabs, on_show))
                    .unwrap_or_else(|| {
                        let (at, select) = destination(&location);
                        let set = TabSet::new(at);
                        if let (Some(file), Some(tab)) = (select, set.tabs().first()) {
                            tab.memory.select_on_open(file);
                        }
                        set
                    })
            }
        });
        let drag = files::file_drag();
        let over_tab = use_state(|| None::<u64>);
        let tabs = Tabs { set, window, overlays };

        // Every window is known to the others, so a tab can be handed over.
        let window_id = Platform::window_id();
        let marker = use_state(|| None::<usize>);
        let ghost = use_state(|| false);
        let probe = use_hook(StripProbe::new);
        let title_cell = use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(String::new())));
        let size_cell = use_hook(|| std::rc::Rc::new(std::cell::Cell::new((0.0_f32, 0.0_f32))));
        let ordinal = use_hook(windows::free_ordinal);
        use_hook({
            let entry = Entry {
                id: window_id,
                ordinal,
                set,
                marker,
                ghost,
                probe: probe.clone(),
                title: title_cell.clone(),
                size: size_cell.clone(),
            };
            move || windows::register(entry)
        });
        use_drop(move || windows::unregister(window_id));

        // A second launch changes the location: its folder opens in a tab.
        let handled = use_hook({
            let first = location.clone();
            move || std::rc::Rc::new(std::cell::RefCell::new(first))
        });
        if *handled.borrow() != location {
            *handled.borrow_mut() = location.clone();
            let (at, select) = destination(&location);
            // Not inside render: the tab list is state the render reads.
            spawn(async move { tabs.open_selecting(at, select) });
        }

        let (shown, current, can_reopen, tab) = {
            let set = set.read();
            (set.tabs().to_vec(), set.active(), set.can_reopen(), set.current().cloned())
        };
        let Some(tab) = tab else {
            return rect().into_element();
        };
        let title = title_of(&tab.at);
        // The compositor's title (the dock, the switcher) follows the tab. It
        // also tells this window from another on the same folder.
        use_side_effect_with_deps(&(title.clone(), ordinal), {
            let title_cell = title_cell.clone();
            move |(title, ordinal): &(String, u32)| {
                let title = windows::title(title, *ordinal);
                *title_cell.borrow_mut() = title.clone();
                with_window(move |window| window.set_title(&title));
            }
        });

        let body: Element = files::browse(
            Browse::new(tab.memory.clone())
                .on_location({
                    let id = tab.id;
                    move |at: Where| tabs.moved(id, at)
                })
                .on_new_tab(move |path: std::path::PathBuf| tabs.open(Where::Folder(path))),
        );

        // The tabs are the header bar's title: separate rounded tabs from the
        // left, the window's buttons at the other end. The header bar is the
        // strip's height, so the window has no row above its tabs.
        let header = HeaderBar::new(title)
            .trailing(menu_button("Main menu", main_menu(tabs, overlays, can_reopen)).icon(Icon::OpenMenu))
            .trailing(WindowControls)
            .tab_slot(
                tab_strip(
                    shown
                        .iter()
                        .zip(tab_titles(&shown))
                        .map(|(tab, title)| TabItem::new(tab.id, title).icon(tab_icon(&tab.at).glyph))
                        .collect(),
                    current,
                )
                .on_select(move |id: u64| tabs.select(id))
                .on_detach(move |detach: Detach| tabs.tear(detach))
                .probe(&probe)
                .insertion(*marker.read())
                .on_close(move |id: u64| tabs.close(id))
                .on_close_many(move |ids: Vec<u64>| tabs.close_many(&ids))
                .on_duplicate(move |id: u64| tabs.duplicate(id))
                .on_reopen(can_reopen, move |_: ()| tabs.reopen())
                .on_new(move |_: ()| tabs.open(home()))
                .on_reorder(move |(id, to): (u64, usize)| tabs.reorder(id, to))
                .on_hover(move |(id, entered): (u64, bool)| {
                    let mut over_tab = over_tab;
                    let folder = set.peek().tabs().iter().find(|tab| tab.id == id).and_then(|tab| {
                        match &tab.at {
                            Where::Folder(path) => Some(path.clone()),
                            Where::Recent => None,
                        }
                    });
                    match (drag, folder) {
                        (Some(drag), Some(folder)) if entered => {
                            drag.hover(&folder);
                            over_tab.set_if_modified(Some(id));
                        }
                        (Some(drag), Some(folder)) => {
                            drag.leave(&folder);
                            over_tab.set_if_modified(None);
                        }
                        _ => {}
                    }
                })
                .drop_target(match drag {
                    Some(drag) if drag.active() => *over_tab.read(),
                    _ => None,
                }),
            );

        let shortcuts = move |event: Event<KeyboardEventData>| {
            let ctrl = event.modifiers.contains(Modifiers::CONTROL);
            let alt = event.modifiers.contains(Modifiers::ALT);
            let shift = event.modifiers.contains(Modifiers::SHIFT);
            match &event.key {
                Key::Character(text) if ctrl && !alt && !shift && text.eq_ignore_ascii_case("t") => {
                    event.stop_propagation();
                    tabs.open(here(tabs));
                }
                Key::Character(text) if ctrl && !alt && shift && text.eq_ignore_ascii_case("t") => {
                    event.stop_propagation();
                    tabs.reopen();
                }
                Key::Character(text) if ctrl && !alt && !shift && text.eq_ignore_ascii_case("w") => {
                    event.stop_propagation();
                    let active = set.peek().active();
                    tabs.close(active);
                }
                Key::Named(NamedKey::Tab) if ctrl && !alt => {
                    event.stop_propagation();
                    tabs.step(if shift { -1 } else { 1 });
                }
                Key::Named(NamedKey::PageDown) if ctrl && !alt => {
                    event.stop_propagation();
                    tabs.step(1);
                }
                Key::Named(NamedKey::PageUp) if ctrl && !alt => {
                    event.stop_propagation();
                    tabs.step(-1);
                }
                Key::Character(text) if alt && !ctrl && !shift => {
                    if let Some(digit) = text.chars().next().and_then(|c| c.to_digit(10))
                        && digit >= 1
                    {
                        event.stop_propagation();
                        tabs.nth(if digit == 9 { usize::MAX } else { digit as usize - 1 });
                    }
                }
                _ => {}
            }
        };

        rect()
            .tw(&tokens, &[w_full(), h_full(), flex_col(), bg(Paint::Bg)])
            .maybe(*ghost.read(), |root| root.opacity(GHOST_OPACITY))
            .on_key_down(shortcuts)
            // While the compositor moves a window for a tab torn off it, no window
            // gets the pointer; the first event any gets is the pointer coming
            // back: the button is up.
            .on_global_pointer_move(move |_: Event<PointerEventData>| tearoff::pointer_arrived())
            .on_sized(move |event: Event<SizedEventData>| {
                size_cell.set((event.area.width(), event.area.height()));
                // A phone-sized window is a preview or a phone, not a choice.
                if !is_compact() && !*maximized.peek() {
                    remember_size(event.area.width(), event.area.height());
                }
            })
            .child(header)
            // Not keyed by tab: one body, mounted once, shows whichever tab
            // is on show (its memory is in `body`).
            .child(rect().tw(&tokens, &[w_full(), h_flex(), flex_col()]).child(body))
            .into_element()
    }
}

/// How faint the window is while it is carried over another window's strip.
const GHOST_OPACITY: f32 = 0.4;

fn home() -> Where {
    Where::Folder(files::home_folder())
}

/// The folder a new tab starts in: the one on show, like a new tab beside it.
fn here(tabs: Tabs) -> Where {
    tabs.set.peek().current().map_or_else(home, |tab| tab.at.clone())
}

/// What the shortcuts dialog says. The file view's keys are the chooser's.
const SHORTCUTS: &str = "\
Ctrl+T  New tab\n\
Ctrl+W  Close tab (a middle click on a tab too)\n\
Ctrl+Shift+T  Reopen the tab closed last\n\
Ctrl+Tab  Next tab\n\
Alt+1 to 9  Go to a tab\n\
Alt+Left, Alt+Right, Alt+Up  Back, Forward, parent folder\n\
Ctrl+L  Edit the path\n\
Ctrl+F  Search\n\
Ctrl+H  Show hidden files\n\
Ctrl+Shift+N  New folder\n\
F2  Rename\n\
Delete  Move to the Trash\n\
Shift+Delete  Delete permanently\n\
Ctrl+C, Ctrl+X, Ctrl+V  Copy, cut, paste\n\
Ctrl+Z  Undo\n\
Space  Preview\n\
Alt+Enter  Properties\n\
Ctrl+Shift+?  About Files";

fn main_menu(tabs: Tabs, overlays: Overlays, can_reopen: bool) -> Vec<MenuItem> {
    let reopen = menu_item("Reopen closed tab", move |_: ()| tabs.reopen()).shortcut("Ctrl+Shift+T");
    let mut items = vec![
        menu_item("New tab", move |_: ()| tabs.open(here(tabs))).shortcut("Ctrl+T"),
        menu_item("Close tab", move |_: ()| {
            let active = tabs.set.peek().active();
            tabs.close(active);
        })
        .shortcut("Ctrl+W"),
        if can_reopen { reopen } else { reopen.disabled() },
        menu_separator(),
        menu_item("Keyboard shortcuts", move |_: ()| {
            overlays.show_dialog(dialog("Keyboard shortcuts", SHORTCUTS).cancel("Close"));
        }),
    ];
    // Every application's menu ends with About.
    items.extend(bingux_ui::about::app_menu_items(overlays));
    items
}
