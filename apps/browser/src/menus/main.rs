//! The main menu: the rows of the design (Window 07), in its order. Zoom with its steps
//! and Full screen, New tab, the private window, the profile, History, Downloads, the
//! page's tools (Find, Print to PDF, Share, More tools) and Settings. bingux-ui adds its
//! Command palette, Keyboard shortcuts and About after them.
//!
//! A feature that is not built has no row. The row comes with the feature, so nothing
//! here is dimmed to say "not yet" (`docs/browser/parity.md`, "Main menu: entry by
//! entry"). The rows that are dimmed follow the page, not the engine: a page tool has
//! nothing to act on while the tab shows no page.
//!
//! On a phone the same button opens `PhoneMenu`, a sheet, because a finger wants rows
//! the width of the screen and a cascade of submenus does not fit.

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

/// The main menu button: a menu on a desktop, a sheet on a phone. It is a component
/// because the menu's rows are read when the toolbar draws, so a change to the tab
/// (a closed tab, a new zoom level) is in the rows the next time they open.
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

/// What the menu reads of the window when it is built. One read of the browser per
/// build keeps the rows consistent with each other: they are all of the same moment.
pub(super) struct Reading {
    /// The active tab shows a web page.
    pub(super) is_web: bool,
    /// The active tab has an engine session, so zoom and full screen apply.
    pub(super) zoomable: bool,
    pub(super) can_forward: bool,
    /// The address of the active tab, for Copy link.
    pub(super) url: String,
    /// How many pages the history holds: Clear history has nothing to do at zero.
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
        Reading {
            is_web: tab.is_some_and(|tab| tab.kind == TabKind::Web && !tab.is_blank()),
            zoomable: tab.is_some_and(|tab| tab.session.is_some()),
            can_forward: tab.is_some_and(|tab| tab.can_forward),
            url: tab.map(|tab| tab.url.clone()).unwrap_or_default(),
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

/// The rows of the main menu. A type with no state: the rows are built from a `Reading`
/// each time, and each method makes one row or one submenu.
pub struct MainMenu;

impl MainMenu {
    /// The rows, as the window is now. bingux-ui lists its own Command palette,
    /// Keyboard shortcuts and About after them.
    pub fn items(ctx: Ctx) -> Vec<MenuItem> {
        let read = Reading::of(ctx);
        // The order is the design's. Separators fall between its groups: the page's zoom,
        // the windows, the library, the page's tools, Settings.
        let mut items = vec![
            Self::zoom(ctx, &read),
            menu_separator(),
            Self::command(
                ctx,
                Command::NewTab,
                tr!("browser-command-new-tab-title"),
                true,
            ),
            Self::command(
                ctx,
                Command::NewPrivateWindow,
                tr!("browser-command-private-window-title"),
                true,
            ),
        ];
        // A window with no registered profile (a test, a folder named by the environment)
        // has nothing to show, so the row is left out.
        items.extend(Self::profile(ctx, &read));
        items.extend([
            menu_separator(),
            Self::history(ctx, &read),
            Self::command(
                ctx,
                Command::Downloads,
                tr!("browser-command-downloads-title"),
                true,
            ),
            menu_separator(),
            Self::command(ctx, Command::Find, tr!("browser-command-find-title"), true),
            Self::command(
                ctx,
                Command::Print,
                tr!("browser-command-print-title"),
                read.is_web,
            ),
            Self::share(ctx, &read),
            Self::tools(ctx, &read),
            menu_separator(),
            Self::command(
                ctx,
                Command::Settings,
                tr!("browser-command-settings-title"),
                true,
            ),
        ]);
        items
    }

    /// A row that runs `command`, with its shortcut, dimmed when `enabled` is
    /// not: a row for a page is dimmed while there is no page.
    pub(super) fn command(ctx: Ctx, command: Command, title: String, enabled: bool) -> MenuItem {
        let item = menu_item(title, move |_: ()| ctx.run(command));
        let item = match command.keys() {
            Some(keys) => item.shortcut(keys),
            None => item,
        };
        if enabled { item } else { item.disabled() }
    }

    /// Zoom: out, the level (a press resets it), in, and Full screen. The level is read
    /// when the row draws, so it follows the presses while the menu stays open. Full
    /// screen closes the menu first: the menu would otherwise stay over the page.
    fn zoom(ctx: Ctx, read: &Reading) -> MenuItem {
        let keys = |command: Command| command.keys().unwrap_or_default();
        // Zoom is a tab's, and a tab with no engine session (Settings) has none to set.
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
    /// that has it open). A window with no registered profile has no row.
    fn profile(ctx: Ctx, read: &Reading) -> Option<MenuItem> {
        let (own, name) = read.profile.clone()?;
        let mut known = read.profiles.clone();
        if !known.iter().any(|(id, _)| *id == own) {
            known.insert(0, (own.clone(), name.clone()));
        }
        let rows: Vec<MenuItem> = known
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
        Some(submenu(tr!("browser-main-menu-profile"), rows).detail(name))
    }

    /// History ›: Clear history, and the tabs closed in this window, newest
    /// first, each reopened by its row.
    fn history(ctx: Ctx, read: &Reading) -> MenuItem {
        let mut rows = vec![
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

    /// Share ›: copy the address.
    fn share(ctx: Ctx, read: &Reading) -> MenuItem {
        let url = read.url.clone();
        let copy = menu_item(tr!("browser-main-menu-share-copy-link"), move |_: ()| {
            Self::copy_link(ctx, &url)
        });
        submenu(
            tr!("browser-main-menu-share"),
            vec![if read.is_web { copy } else { copy.disabled() }],
        )
    }

    /// More tools ›: what the page offers beyond reading it.
    fn tools(ctx: Ctx, read: &Reading) -> MenuItem {
        submenu(
            tr!("browser-main-menu-more-tools"),
            vec![
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
                Self::command(
                    ctx,
                    Command::DevTools,
                    tr!("browser-command-devtools-title"),
                    read.is_web,
                ),
            ],
        )
    }
}
