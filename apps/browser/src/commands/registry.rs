//! The commands of the window, as bingux-ui lists them: its command palette and
//! its shortcuts sheet. The main menu is the browser's own (`menus::MainMenu`,
//! in the order of the design), so none of these is listed in it again. The
//! keys stay the window's own (`Platform::window_command`); this only says
//! what they are.

use bingux_ui::commands::Command as Listed;
use bingux_ui::prelude::*;

use super::Command;
use crate::tabs::TabKind;
use crate::ui::Ctx;

/// What the window tells bingux-ui about its commands.
pub struct Registry;

impl Registry {
    /// Every command with a key, with what applies now.
    pub fn commands(ctx: Ctx) -> Vec<Listed> {
        let (
            zoom,
            can_find,
            can_download,
            can_devtools,
            can_print,
            can_reopen,
            is_web,
            has_history,
        ) = {
            let browser = ctx.browser.read();
            let tab = browser.active();
            (
                tab.map_or(crate::zoom::Zoom::DEFAULT, |tab| tab.zoom)
                    .percent(),
                !browser.closed.is_empty(),
                tab.is_some_and(|tab| tab.kind == TabKind::Web && !tab.is_blank()),
                browser.history_len() > 0,
            )
        };
        // (id, title, command, enabled)
        let rows: Vec<(&str, String, Command, bool)> = vec![
            (
                "browser.new-tab",
                tr!("browser-command-new-tab-title"),
                Command::NewTab,
                true,
            ),
            (
                "browser.new-private-window",
                tr!("browser-command-private-window-title"),
                Command::NewPrivateWindow,
                true,
            ),
            (
                "browser.reopen-tab",
                tr!("browser-command-reopen-tab-title"),
                Command::ReopenTab,
                can_reopen,
            ),
            (
                "browser.close-tab",
                tr!("browser-command-close-tab-title"),
                Command::CloseTab,
                true,
            ),
            (
                "browser.next-tab",
                tr!("browser-command-next-tab-title"),
                Command::NextTab,
                true,
            ),
            (
                "browser.previous-tab",
                tr!("browser-command-previous-tab-title"),
                Command::PreviousTab,
                true,
            ),
            (
                "browser.address",
                tr!("browser-command-address-title"),
                Command::FocusAddress,
                true,
            ),
            (
                "browser.find",
                tr!("browser-command-find-title"),
                Command::Find,
                true,
            ),
            (
                "browser.reload",
                tr!("browser-command-reload-title"),
                Command::Reload,
                true,
            ),
            (
                "browser.back",
                tr!("browser-command-back-title"),
                Command::Back,
                true,
            ),
            (
                "browser.forward",
                tr!("browser-command-forward-title"),
                Command::Forward,
                true,
            ),
            (
                "browser.zoom-in",
                tr!("browser-command-zoom-in-title"),
                Command::ZoomIn,
                true,
            ),
            (
                "browser.zoom-out",
                tr!("browser-command-zoom-out-title"),
                Command::ZoomOut,
                true,
            ),
            (
                "browser.zoom-reset",
                tr!("browser-command-zoom-reset-title", percent = zoom),
                Command::ZoomReset,
                true,
            ),
            (
                "browser.fullscreen",
                tr!("browser-command-fullscreen-title"),
                Command::Fullscreen,
                true,
            ),
            (
                "browser.downloads",
                tr!("browser-command-downloads-title"),
                Command::Downloads,
                true,
            ),
            (
                "browser.clear-history",
                tr!("browser-command-clear-history-title"),
                Command::ClearHistory,
                has_history,
            ),
            (
                "browser.settings",
                tr!("browser-command-settings-title"),
                Command::Settings,
                true,
            ),
            (
                "browser.page-info",
                tr!("browser-command-page-info-title"),
                Command::PageInfo,
                is_web,
            ),
            (
                "browser.view-source",
                tr!("browser-command-view-source-title"),
                Command::ViewSource,
                is_web,
            ),
            (
                "browser.print",
                tr!("browser-command-print-title"),
                Command::Print,
                is_web,
            ),
            (
                "browser.devtools",
                tr!("browser-command-devtools-title"),
                Command::DevTools,
                is_web,
            ),
        ];
        let mut listed: Vec<Listed> = rows
            .into_iter()
            .map(|(id, title, command, enabled)| {
                let item = Listed::new(id, title, move |_: ()| {
                    ctx.with(|browser| browser.run(command))
                });
                match command.keys() {
                    Some(keys) => item.shortcut(keys).keys_handled_by_view(),
                    None => item,
                }
                .enabled(enabled)
                .hidden_from_menu()
            })
            .collect();
        listed.push(Listed::key_hint(
            "browser.go-to-tab",
            tr!("browser-command-go-to-tab-title"),
            tr!("browser-command-go-to-tab-keys"),
        ));
        listed
    }
}
