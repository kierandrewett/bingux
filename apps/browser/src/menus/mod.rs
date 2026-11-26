//! The menus: the main menu (and the sheet that is its phone form), a tab's menu and
//! the page's context menu.

mod host;
mod main;
mod phone;
#[cfg(feature = "snapshot")]
mod snapshot;
#[cfg(test)]
mod tests;
#[cfg(feature = "snapshot")]
pub(crate) use snapshot::scenarios;

pub use host::ContextMenuHost;
pub use main::MainMenuButton;
pub use phone::PhoneMenu;

use bingux_ui::prelude::*;
use bingux_ui::web_view::input::EditCommand;
use bingux_ui::web_view::requests::ContextTarget;

use crate::address::Address;
use crate::browser::PendingMenu;
use crate::commands::Command;
use crate::tabs::Place;
use crate::tabs::{Tab, TabKind};

use crate::ui::Ctx;

/// The menu of a tab in the strip.
pub struct TabMenu;

impl TabMenu {
    /// The items the strip adds to the menu of `tab`.
    pub fn items(ctx: Ctx, tab: &Tab) -> Vec<MenuItem> {
        let key = tab.key;
        let mut items = Vec::new();
        if tab.kind == TabKind::Web {
            items.push(menu_item(tr!("browser-menu-reload-tab"), move |_: ()| {
                ctx.with(|browser| browser.reload(key, false))
            }));
            items.push(menu_item(
                tr!("browser-menu-duplicate-tab"),
                move |_: ()| ctx.with(|browser| browser.duplicate_tab(key)),
            ));
        }
        let pinned = tab.pinned;
        items.push(menu_item(
            if pinned {
                tr!("browser-menu-unpin-tab")
            } else {
                tr!("browser-menu-pin-tab")
            },
            move |_: ()| ctx.with(|browser| browser.set_pinned(key, !pinned)),
        ));
        if tab.audio.playing || tab.audio.muted {
            let muted = tab.audio.muted;
            items.push(menu_item(
                if muted {
                    tr!("browser-menu-unmute-tab")
                } else {
                    tr!("browser-menu-mute-tab")
                },
                move |_: ()| ctx.with(|browser| browser.toggle_mute(key)),
            ));
        }
        items.push(menu_separator());
        items.push(menu_item(tr!("browser-menu-close-others"), move |_: ()| {
            ctx.with(|browser| browser.close_others(key))
        }));
        items.push(menu_item(tr!("browser-menu-close-after"), move |_: ()| {
            ctx.with(|browser| browser.close_after(key))
        }));
        items
    }
}

/// The page's context menu.
pub struct ContextMenu;

impl ContextMenu {
    /// Copy `text` to the clipboard.
    fn copy(ctx: Ctx, text: String, what: &str) {
        match freya::clipboard::Clipboard::set(text) {
            Ok(()) => ctx.overlays.toast(tr!("browser-menu-copied", what = what)),
            Err(_) => ctx
                .overlays
                .toast(tr!("browser-menu-copy-failed", what = what)),
        }
    }

    /// The items of the page's context menu: what is under the pointer first,
    /// then what any page offers.
    pub fn items(ctx: Ctx, menu: &PendingMenu) -> Vec<MenuItem> {
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
                menu_item(tr!("browser-menu-open-link"), open_tab(href.clone())),
                menu_item(tr!("browser-menu-copy-link"), {
                    let href = href.clone();
                    move |_: ()| Self::copy(ctx, href.clone(), "link")
                }),
            ]);
        }
        if let Some(source) = image {
            groups.push(vec![
                menu_item(tr!("browser-menu-open-image"), open_tab(source.clone())),
                menu_item(tr!("browser-menu-copy-image"), move |_: ()| {
                    Self::copy(ctx, source.clone(), "image")
                }),
            ]);
        }
        if let Some((source, video)) = media {
            groups.push(vec![
                menu_item(
                    if video {
                        tr!("browser-menu-open-video")
                    } else {
                        tr!("browser-menu-open-audio")
                    },
                    open_tab(source.clone()),
                ),
                menu_item(tr!("browser-menu-copy-media"), move |_: ()| {
                    Self::copy(ctx, source.clone(), "media")
                }),
            ]);
        }
        let edit = |label: String, command: EditCommand| {
            menu_item(label, move |_: ()| {
                ctx.with(|browser| browser.run(Command::Edit(command)))
            })
        };
        if editable {
            let mut items = vec![
                edit(tr!("browser-menu-undo"), EditCommand::Undo),
                edit(tr!("browser-menu-redo"), EditCommand::Redo),
                menu_separator(),
            ];
            items.push(edit(tr!("browser-menu-cut"), EditCommand::Cut));
            if !password {
                items.push(edit(tr!("browser-menu-copy"), EditCommand::Copy));
            }
            items.push(edit(tr!("browser-menu-paste"), EditCommand::Paste));
            items.push(edit(tr!("browser-menu-select-all"), EditCommand::SelectAll));
            groups.push(items);
        } else if let Some(text) = selection.filter(|text| !text.trim().is_empty()) {
            let shown: String = text.trim().chars().take(24).collect();
            let ellipsis = if text.trim().chars().count() > 24 {
                "\u{2026}"
            } else {
                ""
            };
            groups.push(vec![
                edit(tr!("browser-menu-copy"), EditCommand::Copy),
                menu_item(
                    tr!(
                        "browser-menu-search-for",
                        engine = search_name,
                        text = format!("{shown}{ellipsis}")
                    ),
                    open_tab(text.trim().to_string()),
                ),
            ]);
        }
        let mut page = vec![
            {
                let item = menu_item(tr!("browser-menu-back"), move |_: ()| {
                    ctx.with(|browser| browser.back(key))
                });
                if can_back { item } else { item.disabled() }
            },
            {
                let item = menu_item(tr!("browser-menu-forward"), move |_: ()| {
                    ctx.with(|browser| browser.forward(key))
                });
                if can_forward { item } else { item.disabled() }
            },
            menu_item(tr!("browser-menu-reload"), move |_: ()| {
                ctx.with(|browser| browser.reload(key, false))
            }),
        ];
        let plain_page = groups.is_empty();
        if plain_page {
            page.push(menu_separator());
            page.push(edit(tr!("browser-menu-select-all"), EditCommand::SelectAll));
            let source = format!("view-source:{url}");
            let item = menu_item(tr!("browser-menu-view-source"), open_tab(source));
            page.push(if url.is_empty() || url.starts_with("view-source:") {
                item.disabled()
            } else {
                item
            });
            page.push(menu_item(tr!("browser-menu-copy-page"), {
                let url = url.clone();
                move |_: ()| Self::copy(ctx, Address::new(&url).display(), "page")
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
}
