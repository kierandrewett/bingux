//! The window: a header bar with the tabs, over the file view of the tab on
//! show.
//!
//! Each tab is `bingux_ui::files::browse`, the chooser's body without its
//! question. Only the tab on show is mounted, so a tab out of sight holds no
//! listing and no watch; it keeps where it is (folder, Back and Forward) in a
//! [`TabMemory`] and shows it again from there.

use bingux_ui::files::{self, Browse, TabMemory, Where};
use bingux_ui::prelude::*;
use bingux_ui::shell::{MenuItem, with_window};
use freya::prelude::*;

use crate::target::place;
use crate::window_size::remember_size;

/// One tab: a number that is never reused, the folder it shows and the
/// memory its body keeps its trail in.
#[derive(Clone, PartialEq)]
struct Tab {
    id: u64,
    at: Where,
    memory: TabMemory,
}

/// The tabs of the window. Every field is a `State`, so it is `Copy` and
/// every handler takes its own.
#[derive(Clone, Copy)]
struct Tabs {
    list: State<Vec<Tab>>,
    active: State<u64>,
    next: State<u64>,
    window: Window,
}

impl Tabs {
    /// Open `at` in a new tab beside the one on show and go to it.
    fn open(mut self, at: Where) {
        let id = *self.next.peek();
        self.next.set(id + 1);
        let after = self
            .list
            .peek()
            .iter()
            .position(|tab| tab.id == *self.active.peek())
            .map_or(usize::MAX, |index| index + 1);
        let tab = Tab {
            id,
            memory: TabMemory::new(at.clone()),
            at,
        };
        let mut list = self.list.write();
        let place = after.min(list.len());
        list.insert(place, tab);
        drop(list);
        self.active.set(id);
    }

    fn select(mut self, id: u64) {
        if *self.active.peek() != id && self.list.peek().iter().any(|tab| tab.id == id) {
            self.active.set(id);
        }
    }

    /// Close a tab. The last one closes the window.
    fn close(mut self, id: u64) {
        let len = self.list.peek().len();
        if len <= 1 {
            self.window.close();
            return;
        }
        let Some(index) = self.list.peek().iter().position(|tab| tab.id == id) else {
            return;
        };
        self.list.write().remove(index);
        if *self.active.peek() == id {
            let list = self.list.peek();
            let neighbour = list[index.min(list.len() - 1)].id;
            drop(list);
            self.active.set(neighbour);
        }
    }

    /// Go `by` tabs along, wrapping at either end.
    fn step(mut self, by: isize) {
        let list = self.list.peek();
        let len = list.len() as isize;
        let Some(index) = list.iter().position(|tab| tab.id == *self.active.peek()) else {
            return;
        };
        let next = list[(index as isize + by).rem_euclid(len) as usize].id;
        drop(list);
        self.active.set(next);
    }

    /// Go to the tab at `index` from the start, or the last for `usize::MAX`.
    fn nth(mut self, index: usize) {
        let list = self.list.peek();
        let Some(tab) = list.get(index).or_else(|| list.last()) else {
            return;
        };
        let id = tab.id;
        drop(list);
        self.active.set(id);
    }

    /// A tab was dragged (or moved by its menu) to place `to` in the list,
    /// as if it were removed and inserted there. The active tab stays.
    fn reorder(mut self, id: u64, to: usize) {
        let Some(from) = self.list.peek().iter().position(|tab| tab.id == id) else {
            return;
        };
        let mut list = self.list.write();
        let tab = list.remove(from);
        let to = to.min(list.len());
        list.insert(to, tab);
    }

    /// A tab went to another folder: its title follows.
    fn moved(mut self, id: u64, at: Where) {
        let changed = self
            .list
            .peek()
            .iter()
            .any(|tab| tab.id == id && tab.at != at);
        if changed && let Some(tab) = self.list.write().iter_mut().find(|tab| tab.id == id) {
            tab.at = at;
        }
    }
}

/// The titles of all the tabs: each folder's name, with the nearest parent
/// folders added to the names that two tabs share (`Documents — kieran`).
fn tab_titles(tabs: &[Tab]) -> Vec<String> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let folders: Vec<(usize, String)> = tabs
        .iter()
        .enumerate()
        .filter_map(|(index, tab)| match &tab.at {
            Where::Folder(path) if Some(path) != home.as_ref() && path.file_name().is_some() => {
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

/// The icon a tab shows: its folder's, as the sidebar and the crumbs draw it.
fn tab_icon(at: &Where) -> files::FileIcon {
    match at {
        Where::Recent => files::FileIcon::for_place_kind(files::model::PlaceKind::Recent),
        Where::Folder(path) => files::FileIcon::folder_at(path),
    }
}

/// The name a tab and the window go by: the folder's name, and "Home" for the
/// home folder, as its crumb and its sidebar place read.
fn title_of(at: &Where) -> String {
    match at {
        Where::Recent => "Recent".to_string(),
        Where::Folder(path) => {
            if std::env::var_os("HOME").is_some_and(|home| path.as_path() == std::path::Path::new(&home)) {
                return "Home".to_string();
            }
            path.file_name()
                .map_or_else(|| "Computer".to_string(), |name| name.to_string_lossy().into_owned())
        }
    }
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
        let list = use_state({
            let location = location.clone();
            move || {
                let at = place(&location);
                vec![Tab {
                    id: 1,
                    memory: TabMemory::new(at.clone()),
                    at,
                }]
            }
        });
        let active = use_state(|| 1_u64);
        let next = use_state(|| 2_u64);
        let drag = files::file_drag();
        let over_tab = use_state(|| None::<u64>);
        let tabs = Tabs {
            list,
            active,
            next,
            window,
        };

        // A second launch changes the location: its folder opens in a tab.
        let handled = use_hook({
            let first = location.clone();
            move || std::rc::Rc::new(std::cell::RefCell::new(first))
        });
        if *handled.borrow() != location {
            *handled.borrow_mut() = location.clone();
            let at = place(&location);
            // Not inside render: the tab list is state the render reads.
            spawn(async move { tabs.open(at) });
        }

        let current = active();
        let shown: Vec<Tab> = list.read().clone();
        let tab = shown
            .iter()
            .find(|tab| tab.id == current)
            .or_else(|| shown.first())
            .cloned();
        let Some(tab) = tab else {
            return rect().into_element();
        };
        let title = title_of(&tab.at);
        // The compositor's title (the dock, the switcher) follows the tab.
        use_side_effect_with_deps(&title, move |title: &String| {
            let title = format!("{title} \u{2013} Files");
            with_window(move |window| window.set_title(&title));
        });

        let body: Element = files::browse(
            Browse::new(tab.memory.clone())
                .on_location({
                    let id = tab.id;
                    move |at: Where| tabs.moved(id, at)
                })
                .on_new_tab(move |path: std::path::PathBuf| tabs.open(Where::Folder(path))),
        );

        // The tabs stand on the file view: the header bar is transparent over
        // the frame's strip, and the strip, from the left, is its title.
        let header = HeaderBar::new(title)
            .fill(Paint::Transparent)
            .trailing(menu_button("Main menu", main_menu(tabs, overlays)).icon(Icon::OpenMenu))
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
                .on_close(move |id: u64| tabs.close(id))
                .on_new(move |_: ()| tabs.open(home()))
                .on_reorder(move |(id, to): (u64, usize)| tabs.reorder(id, to))
                .on_hover(move |(id, entered): (u64, bool)| {
                    let mut over_tab = over_tab;
                    let folder = list.peek().iter().find(|tab| tab.id == id).and_then(|tab| {
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
                Key::Character(text) if ctrl && !alt && !shift && text.eq_ignore_ascii_case("w") => {
                    event.stop_propagation();
                    tabs.close(*active.peek());
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
            .on_key_down(shortcuts)
            .on_sized(move |event: Event<SizedEventData>| {
                // A phone-sized window is a preview or a phone, not a choice.
                if !is_compact() && !*maximized.peek() {
                    remember_size(event.area.width(), event.area.height());
                }
            })
            .child(tab_frame(
                header,
                // Keyed by tab: each tab's body has its own state.
                rect()
                    .key(tab.id)
                    .tw(&tokens, &[w_full(), h_flex(), flex_col()])
                    .child(body),
            ))
            .into_element()
    }
}

fn home() -> Where {
    Where::Folder(files::home_folder())
}

/// The folder a new tab starts in: the one on show, like a new tab beside it.
fn here(tabs: Tabs) -> Where {
    let list = tabs.list.peek();
    list.iter()
        .find(|tab| tab.id == *tabs.active.peek())
        .map_or_else(home, |tab| tab.at.clone())
}

/// What the shortcuts dialog says. The file view's keys are the chooser's.
const SHORTCUTS: &str = "\
Ctrl+T  New tab\n\
Ctrl+W  Close tab\n\
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

fn main_menu(tabs: Tabs, overlays: Overlays) -> Vec<MenuItem> {
    let mut items = vec![
        menu_item("New tab", move |_: ()| tabs.open(here(tabs))).shortcut("Ctrl+T"),
        menu_item("Close tab", move |_: ()| tabs.close(*tabs.active.peek())).shortcut("Ctrl+W"),
        menu_separator(),
        menu_item("Keyboard shortcuts", move |_: ()| {
            overlays.show_dialog(dialog("Keyboard shortcuts", SHORTCUTS).cancel("Close"));
        }),
    ];
    // Every application's menu ends with About.
    items.extend(bingux_ui::about::app_menu_items(overlays));
    items
}
