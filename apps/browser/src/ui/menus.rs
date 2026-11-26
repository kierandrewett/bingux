//! The menus: the main menu, a tab's menu and the page's context menu.

use bingux_ui::prelude::*;
use bingux_ui::web_view::input::EditCommand;
use bingux_ui::web_view::requests::ContextTarget;

use crate::browser::PendingMenu;
use crate::model::address;
use crate::model::shortcuts::{self, Command};
use crate::model::tab::{Tab, TabKind};
use crate::model::tabs::Place;
use crate::zoom::Zoom;

use super::ctx::Ctx;

/// A menu entry that runs `command`, with its shortcut beside it.
fn command_item(ctx: Ctx, label: &str, command: Command) -> MenuItem {
    let item = menu_item(label, move |_: ()| ctx.with(|browser| browser.run(command)));
    // A phone has no keyboard to learn the keys from, and no room for them.
    match shortcuts::label(ctx.platform, command) {
        Some(keys) if !is_compact() => item.shortcut(keys),
        _ => item,
    }
}

/// The main menu, from the button at the right of the toolbar.
pub fn main_menu(ctx: Ctx) -> Vec<MenuItem> {
    let (zoom_text, can_find, can_download, can_reopen, is_web) = {
        let browser = ctx.browser.read();
        let tab = browser.active();
        (
            tab.map_or(Zoom::DEFAULT, |tab| tab.zoom).percent(),
            browser.engine.capabilities().find,
            browser.engine.capabilities().downloads,
            !browser.closed.is_empty(),
            tab.is_some_and(|tab| tab.kind == TabKind::Web && !tab.is_blank()),
        )
    };
    let mut items = vec![
        command_item(ctx, "New tab", Command::NewTab),
        command_item(ctx, "Private window", Command::NewPrivateWindow),
    ];
    let reopen = command_item(ctx, "Reopen closed tab", Command::ReopenTab);
    items.push(if can_reopen {
        reopen
    } else {
        reopen.disabled()
    });
    items.push(menu_separator());
    items.push(command_item(ctx, "Zoom in", Command::ZoomIn));
    items.push(command_item(ctx, "Zoom out", Command::ZoomOut));
    items.push(command_item(
        ctx,
        &format!("Reset zoom ({zoom_text}%)"),
        Command::ZoomReset,
    ));
    items.push(menu_separator());
    let find = command_item(ctx, "Find in page", Command::Find);
    items.push(if can_find { find } else { find.disabled() });
    let downloads = command_item(ctx, "Downloads", Command::Downloads);
    items.push(if can_download {
        downloads
    } else {
        downloads.disabled()
    });
    let source = command_item(ctx, "View page source", Command::ViewSource);
    items.push(if is_web { source } else { source.disabled() });
    items.push(command_item(ctx, "Print\u{2026}", Command::Print));
    items.push(menu_separator());
    items.push(command_item(ctx, "Settings", Command::Settings));
    let overlays = ctx.overlays;
    let platform = ctx.platform;
    items.push(menu_item("Keyboard shortcuts", move |_: ()| {
        overlays.show_dialog(dialog("Keyboard shortcuts", shortcut_text(platform)).cancel("Close"));
    }));
    items.extend(bingux_ui::about::app_menu_items(overlays));
    items
}

/// What the shortcuts dialog says.
pub fn shortcut_text(platform: shortcuts::Platform) -> String {
    let rows = [
        ("New tab", Command::NewTab),
        ("Close tab", Command::CloseTab),
        ("Reopen closed tab", Command::ReopenTab),
        ("Next tab", Command::NextTab),
        ("Previous tab", Command::PreviousTab),
        ("New private window", Command::NewPrivateWindow),
        ("Address bar", Command::FocusAddress),
        ("Find in page", Command::Find),
        ("Reload", Command::Reload),
        ("Back", Command::Back),
        ("Forward", Command::Forward),
        ("Zoom in", Command::ZoomIn),
        ("Zoom out", Command::ZoomOut),
        ("Reset zoom", Command::ZoomReset),
        ("Fullscreen", Command::Fullscreen),
        ("Downloads", Command::Downloads),
        ("Settings", Command::Settings),
    ];
    let mut lines: Vec<String> = rows
        .iter()
        .filter_map(|(name, command)| {
            shortcuts::label(platform, *command).map(|keys| format!("{keys}  {name}"))
        })
        .collect();
    let digit = if platform == shortcuts::Platform::Linux {
        "Alt+1 to 9"
    } else {
        "Ctrl+1 to 9"
    };
    lines.insert(5, format!("{digit}  Go to a tab"));
    lines.join("\n")
}

/// The items the strip adds to the menu of `tab`.
pub fn tab_items(ctx: Ctx, tab: &Tab) -> Vec<MenuItem> {
    let key = tab.key;
    let mut items = Vec::new();
    if tab.kind == TabKind::Web {
        items.push(menu_item("Reload tab", move |_: ()| {
            ctx.with(|browser| browser.reload(key, false))
        }));
        items.push(menu_item("Duplicate tab", move |_: ()| {
            ctx.with(|browser| browser.duplicate_tab(key))
        }));
    }
    let pinned = tab.pinned;
    items.push(menu_item(
        if pinned { "Unpin tab" } else { "Pin tab" },
        move |_: ()| ctx.with(|browser| browser.set_pinned(key, !pinned)),
    ));
    if tab.audio.playing || tab.audio.muted {
        let muted = tab.audio.muted;
        items.push(menu_item(
            if muted { "Unmute tab" } else { "Mute tab" },
            move |_: ()| ctx.with(|browser| browser.toggle_mute(key)),
        ));
    }
    items.push(menu_separator());
    items.push(menu_item("Close other tabs", move |_: ()| {
        ctx.with(|browser| browser.close_others(key))
    }));
    items.push(menu_item("Close tabs to the right", move |_: ()| {
        ctx.with(|browser| browser.close_after(key))
    }));
    items
}

/// Copy `text` to the clipboard.
fn copy(ctx: Ctx, text: String, what: &str) {
    match freya::clipboard::Clipboard::set(text) {
        Ok(()) => ctx.overlays.toast(format!("{what} copied")),
        Err(_) => ctx
            .overlays
            .toast(format!("Could not copy the {}", what.to_lowercase())),
    }
}

/// The items of the page's context menu: what is under the pointer first,
/// then what any page offers.
pub fn context_items(ctx: Ctx, menu: &PendingMenu) -> Vec<MenuItem> {
    let ContextTarget {
        link,
        image,
        media,
        selection,
        editable,
        password,
        ..
    } = menu.target.clone();
    let key = menu.tab;
    let (can_back, can_forward, url, search_name) = {
        let browser = ctx.browser.read();
        let tab = browser.tabs.get(key);
        (
            tab.is_some_and(|tab| tab.can_back),
            tab.is_some_and(|tab| tab.can_forward),
            tab.map(|tab| tab.url.clone()).unwrap_or_default(),
            browser.search().name,
        )
    };
    let mut groups: Vec<Vec<MenuItem>> = Vec::new();
    let open_tab = move |target: String| {
        move |_: ()| {
            ctx.with(|browser| browser.open_tab(&target, Place::After(key), false));
        }
    };
    if let Some((href, _)) = link {
        groups.push(vec![
            menu_item("Open link in new tab", open_tab(href.clone())),
            menu_item("Copy link address", {
                let href = href.clone();
                move |_: ()| copy(ctx, href.clone(), "Link address")
            }),
        ]);
    }
    if let Some(source) = image {
        groups.push(vec![
            menu_item("Open image in new tab", open_tab(source.clone())),
            menu_item("Copy image address", move |_: ()| {
                copy(ctx, source.clone(), "Image address")
            }),
        ]);
    }
    if let Some((source, video)) = media {
        groups.push(vec![
            menu_item(
                if video {
                    "Open video in new tab"
                } else {
                    "Open audio in new tab"
                },
                open_tab(source.clone()),
            ),
            menu_item("Copy media address", move |_: ()| {
                copy(ctx, source.clone(), "Media address")
            }),
        ]);
    }
    let edit = |label: &str, command: EditCommand| {
        menu_item(label, move |_: ()| {
            ctx.with(|browser| browser.run(Command::Edit(command)))
        })
    };
    if editable {
        let mut items = vec![
            edit("Undo", EditCommand::Undo),
            edit("Redo", EditCommand::Redo),
            menu_separator(),
        ];
        items.push(edit("Cut", EditCommand::Cut));
        if !password {
            items.push(edit("Copy", EditCommand::Copy));
        }
        items.push(edit("Paste", EditCommand::Paste));
        items.push(edit("Select all", EditCommand::SelectAll));
        groups.push(items);
    } else if let Some(text) = selection.filter(|text| !text.trim().is_empty()) {
        let shown: String = text.trim().chars().take(24).collect();
        let ellipsis = if text.trim().chars().count() > 24 {
            "\u{2026}"
        } else {
            ""
        };
        groups.push(vec![
            edit("Copy", EditCommand::Copy),
            menu_item(
                format!("Search {search_name} for \u{201c}{shown}{ellipsis}\u{201d}"),
                open_tab(text.trim().to_string()),
            ),
        ]);
    }
    let mut page = vec![
        {
            let item = menu_item("Back", move |_: ()| ctx.with(|browser| browser.back(key)));
            if can_back { item } else { item.disabled() }
        },
        {
            let item = menu_item("Forward", move |_: ()| {
                ctx.with(|browser| browser.forward(key))
            });
            if can_forward { item } else { item.disabled() }
        },
        menu_item("Reload", move |_: ()| {
            ctx.with(|browser| browser.reload(key, false))
        }),
    ];
    let plain_page = groups.is_empty();
    if plain_page {
        page.push(menu_separator());
        page.push(edit("Select all", EditCommand::SelectAll));
        let source = format!("view-source:{url}");
        let item = menu_item("View page source", open_tab(source));
        page.push(if url.is_empty() || url.starts_with("view-source:") {
            item.disabled()
        } else {
            item
        });
        page.push(menu_item("Copy page address", {
            let url = url.clone();
            move |_: ()| copy(ctx, address::display(&url), "Page address")
        }));
    }
    groups.push(page);
    let mut items = Vec::new();
    for group in groups {
        if !items.is_empty() {
            items.push(menu_separator());
        }
        items.extend(group);
    }
    items
}
