//! The window: a header bar with the tabs, over the file view of the tab on
//! show.
//!
//! Each tab is `bingux_ui::files::browse`, the chooser's body without its
//! question. There is one body for all the tabs, mounted once: the toolbar,
//! the places sidebar and the status line stay as they are when a tab is
//! shown, and only the file view and the states behind it are the tab's. A
//! tab keeps where it is (folder, Back and Forward), what is selected, its
//! sort, zoom and the rest in its `TabMemory` and in the body's states for
//! it, and shows them again from there in the first frame.
//!
//! What a tab is, and everything a person does with tabs (the set, the strip,
//! the keys, tearing a tab off into a window of its own and dropping it on
//! another), is `bingux_ui::tabs`; a Files tab is `bingux_ui::files::FileTab`.
//! This file is what Files adds: the folder a tab is titled and iconed by,
//! the folder a new tab opens in, the question before a window with a copy
//! running closes, and the menu.

use bingux_ui::app::use_window_seed;
use bingux_ui::files::launch::{self, destination};
use bingux_ui::files::{self, Browse, FileTab, Where};
use bingux_ui::prelude::*;
use bingux_ui::tabs::{Birth, CloseWarning, Moved, Tab, TabSet, Tabs, tab_items, use_tab_window};
use freya::prelude::*;

use crate::session;
use crate::window_size::remember_size;

/// The titles of all the tabs: each folder's name, with the nearest parent
/// folders added to the names that two tabs share (`Documents — kieran`).
fn tab_titles(tabs: &[Tab<FileTab>]) -> Vec<String> {
    let roots = files::Roots::current();
    let folders: Vec<(usize, String)> = tabs
        .iter()
        .enumerate()
        .filter_map(|(index, tab)| match &tab.content.at {
            // The roots (Home, the Trash and what is in it) have names of
            // their own and are never told apart by a parent folder.
            Where::Folder(path)
                if roots.root_of(&tab.content.at).is_none()
                    && roots.inside_of(&tab.content.at).is_none()
                    && path.file_name().is_some() =>
            {
                Some((index, path.to_string_lossy().into_owned()))
            }
            _ => None,
        })
        .collect();
    let paths: Vec<&str> = folders.iter().map(|(_, path)| path.as_str()).collect();
    let labels = bingux_ui::elide::distinguish(&paths);
    let mut titles: Vec<String> = tabs.iter().map(|tab| title_of(&tab.content.at)).collect();
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

/// Closing the window while a copy, a move or a compression is running would
/// stop it half way: ask first.
fn close_guard() -> Option<CloseWarning> {
    files::operations_running().then(|| CloseWarning {
        title: "Close Files?".into(),
        message: "A file operation is still running. Closing the window stops it.".into(),
    })
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
        // A window opened for tabs torn off another is born with them. Files
        // started with no folder named opens the tabs of the window that was
        // closed last.
        let birth = use_window_seed::<Birth<FileTab>>();
        let set = use_state({
            let location = location.clone();
            move || {
                if let Some(set) = birth.and_then(|birth| birth.into_set()) {
                    return set;
                }
                let restored = (!launch::names_a_place(&location))
                    .then(session::remembered)
                    .flatten()
                    .and_then(|(places, on_show)| {
                        let tabs = places
                            .into_iter()
                            .map(|at| Moved {
                                content: FileTab::new(at),
                            })
                            .collect();
                        TabSet::adopt(tabs, on_show)
                    });
                restored.unwrap_or_else(|| {
                    let (at, select) = destination(&location);
                    TabSet::new(FileTab::new(at).selecting(select))
                })
            }
        });
        let drag = files::file_drag();
        let over_tab = use_state(|| None::<u64>);
        let tabs = Tabs::new(set, window, overlays).guard(close_guard);

        // A second launch changes the location: its folder opens in a tab.
        let handled = use_hook({
            let first = location.clone();
            move || std::rc::Rc::new(std::cell::RefCell::new(first))
        });
        if *handled.borrow() != location {
            *handled.borrow_mut() = location.clone();
            let (at, select) = destination(&location);
            // Not inside render: the tab list is state the render reads.
            spawn(async move {
                tabs.open(FileTab::new(at).selecting(select));
            });
        }

        let (shown, tab) = {
            let set = set.read();
            (set.tabs().to_vec(), set.current().cloned())
        };
        // The window's tabs are written down as they change, so the window is
        // kept as it was when it closed, however it was closed.
        let places: Vec<Where> = shown.iter().map(|tab| tab.content.at.clone()).collect();
        let on_show = tab
            .as_ref()
            .and_then(|tab| shown.iter().position(|shown| shown.id == tab.id))
            .unwrap_or(0);
        use_side_effect_with_deps(
            &(places, on_show),
            |(places, on_show): &(Vec<Where>, usize)| {
                session::remember(&session::Session::of(places, *on_show));
            },
        );
        let title = tab
            .as_ref()
            .map(|tab| title_of(&tab.content.at))
            .unwrap_or_default();
        // Every window is known to the others, so a tab can be handed over,
        // and the compositor's title follows the tab on show.
        let tab_window = use_tab_window(set, &title);
        let Some(tab) = tab else {
            return rect().into_element();
        };

        // The tab bar shows once the window has a second tab, as in Finder
        // and Safari, and while a tab from another window is held over this
        // one, so there is somewhere to drop it. Its tabs share the width.
        let titles = tab_titles(&shown);
        use_window_commands(window_commands(tabs));
        let commands = use_commands();
        let strip = (shown.len() > 1 || tab_window.insertion().is_some()).then(|| {
            Element::from(
                tabs.strip(
                    tab_items(&shown, titles, |tab| tab_icon(&tab.at).glyph),
                    &tab_window,
                    move |_: ()| {
                        tabs.open(FileTab::new(here(tabs)));
                    },
                )
                .fill(true)
                .on_hover(move |(id, entered): (u64, bool)| {
                    let mut over_tab = over_tab;
                    let folder = set.peek().content(id).and_then(|tab| match &tab.at {
                        Where::Folder(path) => Some(path.clone()),
                        Where::Recent => None,
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
            )
        });

        // The header bar, the sidebar and the tab bar are the file view's:
        // the window is laid out as Settings is, with the places down the
        // whole left side and the header over the files.
        let body: Element = files::browse(
            Browse::new(tab.content.memory.clone())
                .on_location({
                    let id = tab.id;
                    move |at: Where| {
                        let now = at.clone();
                        tabs.update_if(id, move |tab| tab.at != now, move |tab| tab.at = at);
                    }
                })
                .on_new_tab(move |path: std::path::PathBuf| {
                    tabs.open(FileTab::new(Where::Folder(path)));
                })
                .menu({
                    let mut items = commands.main_menu_items();
                    items.extend(bingux_ui::about::app_menu_items(overlays));
                    items
                })
                .tab_bar(strip),
        );

        let shortcuts = tabs.shortcuts(move |_: ()| {
            tabs.open(FileTab::new(here(tabs)));
        });

        tab_window
            .root(rect())
            .tw(&tokens, &[w_full(), h_full(), flex_col(), bg(Paint::Bg)])
            .on_key_down(shortcuts)
            .on_sized(move |event: Event<SizedEventData>| {
                tab_window.sized(event.area.width(), event.area.height());
                // A phone-sized window is a preview or a phone, not a choice.
                if !is_compact() && !*maximized.peek() {
                    remember_size(event.area.width(), event.area.height());
                }
            })
            // Not keyed by tab: one body, mounted once, shows whichever tab
            // is on show (its memory is in `body`).
            .child(
                rect()
                    .tw(&tokens, &[w_full(), h_flex(), flex_col()])
                    .child(body),
            )
            .into_element()
    }
}

fn home() -> Where {
    Where::Folder(files::home_folder())
}

/// The folder a new tab starts in: the one on show, like a new tab beside it.
fn here(tabs: Tabs<FileTab>) -> Where {
    tabs.set()
        .peek()
        .current()
        .map_or_else(home, |tab| tab.content.at.clone())
}

/// The window's commands: New window and the tab commands, for the main
/// menu, the palette and the shortcuts sheet (Ctrl+?). The browser's (Back,
/// Edit the path, Show hidden files) and the file view's (Rename, Duplicate,
/// Copy) are their own scopes' (`bingux_ui::files`), so the sheet lists
/// every key the window answers without a list kept here.
fn window_commands(tabs: Tabs<FileTab>) -> Vec<Command> {
    // Ctrl+N is a new window at the folder on show, as in Finder and GNOME
    // Files.
    let mut commands = vec![
        Command::new("files.new-window", "New window", move |_| {
            tabs.open_window(FileTab::new(here(tabs)));
        })
        .shortcut("Ctrl+N")
        .group(CommandGroup::File)
        .section("tabs"),
    ];
    commands.extend(tabs.commands(move |_: ()| {
        tabs.open(FileTab::new(here(tabs)));
    }));
    commands
}
