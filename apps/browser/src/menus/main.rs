//! The main menu: the rows of the design (Window 07) in its order. Zoom with
//! its steps and Full screen, the three ways to open a window, the profile,
//! the library (History, Bookmarks, Downloads, Passwords), the page's tools
//! (Find, Print, Share, More tools), Settings; bingux-ui adds its Command palette,
//! Keyboard shortcuts and About after them. A row whose feature is not built is disabled and says why in a
//! tooltip (`docs/browser/parity.md`, Window 07): nothing here pretends to work.

use bingux_ui::prelude::MenuItem;
use bingux_ui::prelude::*;
use freya::prelude::*;

use super::PhoneMenu;
use crate::address::Address;
use crate::commands::Command;
use crate::tabs::TabKind;
use crate::ui::Ctx;

/// How many recently closed tabs the History submenu lists.
const CLOSED_SHOWN: usize = 8;

/// The main menu button: a menu on a desktop, a sheet on a phone.
#[derive(PartialEq)]
pub struct MainMenuButton {
    pub ctx: Ctx,
}

impl Component for MainMenuButton {
    fn render(&self) -> impl IntoElement {
        let ctx = self.ctx;
        if is_compact() {
            return icon_button(Icon::OpenMenu, move |_: ()| PhoneMenu::open(ctx))
                .tooltip(tr!("ui-main-menu"))
                .into_element();
        }
        app_menu(MainMenu::items(ctx)).into_element()
    }
}

/// What the menu reads of the window when it is built.
pub(super) struct Reading {
    /// The active tab shows a web page.
    pub(super) is_web: bool,
    /// The active tab has an engine session, so zoom and full screen apply.
    pub(super) zoomable: bool,
    pub(super) can_forward: bool,
    pub(super) url: String,
    pub(super) find: bool,
    pub(super) downloads: bool,
    pub(super) print: bool,
    pub(super) devtools: bool,
    pub(super) private: bool,
    pub(super) pages: usize,
    /// Closed tabs, newest first, each with its index in `Browser::closed`.
    pub(super) closed: Vec<(usize, String)>,
    /// This window's profile: registry id and name.
    pub(super) profile: Option<(String, String)>,
    pub(super) profiles: Vec<(String, String)>,
    /// How many downloads are running.
    pub(super) running: usize,
}

impl Reading {
    pub(super) fn of(ctx: Ctx) -> Self {
        let browser = ctx.browser.read();
        let tab = browser.active();
        let caps = browser.engine.capabilities();
        Reading {
            is_web: tab.is_some_and(|tab| tab.kind == TabKind::Web && !tab.is_blank()),
            zoomable: tab.is_some_and(|tab| tab.session.is_some()),
            can_forward: tab.is_some_and(|tab| tab.can_forward),
            url: tab.map(|tab| tab.url.clone()).unwrap_or_default(),
            find: caps.find,
            downloads: caps.downloads,
            print: caps.print,
            devtools: caps.devtools,
            private: browser.options.private,
            pages: browser.history_len(),
            closed: browser
                .closed
                .iter()
                .enumerate()
                .rev()
                .take(CLOSED_SHOWN)
                .map(|(index, closed)| {
                    let title = if closed.title.trim().is_empty() {
                        Address::new(&closed.url).display()
                    } else {
                        closed.title.clone()
                    };
                    (index, title)
                })
                .collect(),
            profile: browser.profile.clone(),
            profiles: browser.profiles.clone(),
            running: browser.downloads.active(),
        }
    }
}

/// The rows of the main menu.
pub struct MainMenu;

impl MainMenu {
    /// The rows, as the window is now. bingux-ui lists its own Command palette,
    /// Keyboard shortcuts and About after them.
    pub fn items(ctx: Ctx) -> Vec<MenuItem> {
        let read = Reading::of(ctx);
        vec![
            Self::zoom(ctx, &read),
            menu_separator(),
            Self::command(
                ctx,
                Command::NewTab,
                tr!("browser-command-new-tab-title"),
                true,
            ),
            Self::unavailable(
                tr!("browser-main-menu-new-window"),
                tr!("browser-main-menu-new-window-unavailable"),
            ),
            Self::command(
                ctx,
                Command::NewPrivateWindow,
                tr!("browser-command-private-window-title"),
                true,
            ),
            Self::profile(ctx, &read),
            menu_separator(),
            Self::history(ctx, &read),
            submenu(tr!("browser-main-menu-bookmarks"), Vec::new())
                .disabled()
                .tooltip(tr!("browser-main-menu-bookmarks-unavailable")),
            Self::command(
                ctx,
                Command::Downloads,
                tr!("browser-command-downloads-title"),
                read.downloads,
            ),
            Self::unavailable(
                tr!("browser-main-menu-passwords"),
                tr!("browser-main-menu-passwords-unavailable"),
            ),
            menu_separator(),
            Self::command(
                ctx,
                Command::Find,
                tr!("browser-command-find-title"),
                read.find,
            ),
            Self::print(ctx, &read),
            Self::share(ctx, &read),
            Self::tools(ctx, &read),
            menu_separator(),
            Self::command(
                ctx,
                Command::Settings,
                tr!("browser-command-settings-title"),
                true,
            ),
        ]
    }

    /// A row that runs `command`, with its shortcut, dimmed when `enabled` is
    /// not.
    pub(super) fn command(ctx: Ctx, command: Command, title: String, enabled: bool) -> MenuItem {
        let item = menu_item(title, move |_: ()| ctx.run(command));
        let item = match command.keys() {
            Some(keys) => item.shortcut(keys),
            None => item,
        };
        if enabled { item } else { item.disabled() }
    }

    /// A row of a feature that is not built: disabled, and `reason` is its
    /// tooltip.
    fn unavailable(title: String, reason: String) -> MenuItem {
        menu_item(title, |_: ()| {}).disabled().tooltip(reason)
    }

    /// Zoom: out, the level (a press resets it), in, and Full screen.
    fn zoom(ctx: Ctx, read: &Reading) -> MenuItem {
        let keys = |command: Command| command.keys().unwrap_or_default();
        let on = read.zoomable;
        menu_controls(
            tr!("browser-main-menu-zoom"),
            vec![
                menu_control(
                    Icon::ZoomOut,
                    tr!("browser-main-menu-zoom-out", keys = keys(Command::ZoomOut)),
                    move |_: ()| ctx.run(Command::ZoomOut),
                )
                .enabled(on),
                menu_value_control(
                    move || {
                        let percent = ctx
                            .browser
                            .read()
                            .active()
                            .map_or(100, |tab| tab.zoom.percent());
                        tr!("browser-main-menu-zoom-level", percent = percent)
                    },
                    tr!(
                        "browser-main-menu-zoom-reset",
                        keys = keys(Command::ZoomReset)
                    ),
                    move |_: ()| ctx.run(Command::ZoomReset),
                )
                .enabled(on),
                menu_control(
                    Icon::ZoomIn,
                    tr!("browser-main-menu-zoom-in", keys = keys(Command::ZoomIn)),
                    move |_: ()| ctx.run(Command::ZoomIn),
                )
                .enabled(on),
                menu_control(
                    Icon::ViewFullscreen,
                    tr!(
                        "browser-main-menu-fullscreen",
                        keys = keys(Command::Fullscreen)
                    ),
                    move |_: ()| ctx.run(Command::Fullscreen),
                )
                .divided()
                .closes_menu(),
            ],
        )
    }

    /// Profile ›: the profiles of the registry with this window's ticked.
    /// Another profile's row starts its window (or brings forward the one
    /// that has it open).
    fn profile(ctx: Ctx, read: &Reading) -> MenuItem {
        let title = tr!("browser-main-menu-profile");
        let Some((own, name)) = read.profile.clone() else {
            return submenu(title, Vec::new())
                .disabled()
                .tooltip(tr!("browser-main-menu-profile-none"));
        };
        let mut known = read.profiles.clone();
        if !known.iter().any(|(id, _)| *id == own) {
            known.insert(0, (own.clone(), name.clone()));
        }
        let mut rows: Vec<MenuItem> = known
            .into_iter()
            .map(|(id, label)| {
                let ticked = id == own;
                menu_item(label, move |_: ()| {
                    ctx.with(|browser| browser.open_profile(&id))
                })
                .name()
                .checked(ticked)
            })
            .collect();
        rows.push(menu_separator());
        rows.push(Self::unavailable(
            tr!("browser-main-menu-new-profile"),
            tr!("browser-main-menu-new-profile-unavailable"),
        ));
        rows.push(Self::unavailable(
            tr!("browser-main-menu-manage-profiles"),
            tr!("browser-main-menu-manage-profiles-unavailable"),
        ));
        submenu(title, rows).detail(name)
    }

    /// History ›: the history page (not built), Clear history, and the tabs
    /// closed in this window, newest first, each reopened by its row.
    fn history(ctx: Ctx, read: &Reading) -> MenuItem {
        let mut rows = vec![
            Self::unavailable(
                tr!("browser-main-menu-show-history"),
                tr!("browser-main-menu-show-history-unavailable"),
            ),
            Self::command(
                ctx,
                Command::ClearHistory,
                tr!("browser-main-menu-clear-history"),
                read.pages > 0,
            ),
            menu_separator(),
        ];
        if read.closed.is_empty() {
            rows.push(menu_item(tr!("browser-main-menu-no-closed-tabs"), |_: ()| {}).disabled());
        }
        rows.extend(read.closed.iter().map(|(index, title)| {
            let index = *index;
            menu_item(title.clone(), move |_: ()| {
                ctx.with(|browser| browser.reopen_closed_at(index))
            })
            .name()
        }));
        submenu(tr!("browser-main-menu-history"), rows)
    }

    /// The page as a PDF: the one thing the engine prints to.
    fn print(ctx: Ctx, read: &Reading) -> MenuItem {
        let item = Self::command(
            ctx,
            Command::Print,
            tr!("browser-command-print-title"),
            read.print && read.is_web,
        );
        if read.print {
            item
        } else {
            item.tooltip(tr!("browser-main-menu-print-unavailable"))
        }
    }

    /// Share ›: copy the address; sharing to an app needs a portal.
    fn share(ctx: Ctx, read: &Reading) -> MenuItem {
        let url = read.url.clone();
        let copy = menu_item(tr!("browser-main-menu-share-copy-link"), move |_: ()| {
            Self::copy_link(ctx, &url)
        });
        let rows = vec![
            if read.is_web { copy } else { copy.disabled() },
            Self::unavailable(
                tr!("browser-main-menu-share-apps"),
                tr!("browser-main-menu-share-apps-unavailable"),
            ),
        ];
        submenu(tr!("browser-main-menu-share"), rows)
    }

    /// More tools ›: what the page offers beyond reading it.
    fn tools(ctx: Ctx, read: &Reading) -> MenuItem {
        let devtools = Self::command(
            ctx,
            Command::DevTools,
            tr!("browser-command-devtools-title"),
            read.devtools && read.is_web && !read.private,
        );
        let devtools = if !read.devtools {
            devtools.tooltip(tr!("browser-main-menu-devtools-unavailable"))
        } else if read.private {
            devtools.tooltip(tr!("browser-main-menu-devtools-private"))
        } else {
            devtools
        };
        submenu(
            tr!("browser-main-menu-more-tools"),
            vec![
                Self::unavailable(
                    tr!("browser-main-menu-save-page"),
                    tr!("browser-main-menu-save-page-unavailable"),
                ),
                Self::unavailable(
                    tr!("browser-main-menu-translate"),
                    tr!("browser-main-menu-translate-unavailable"),
                ),
                menu_separator(),
                Self::command(
                    ctx,
                    Command::PageInfo,
                    tr!("browser-command-page-info-title"),
                    read.is_web,
                ),
                Self::command(
                    ctx,
                    Command::ViewSource,
                    tr!("browser-command-view-source-title"),
                    read.is_web,
                ),
                devtools,
            ],
        )
    }
}
