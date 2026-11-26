//! The commands of the window, as bingux-ui lists them: its main menu (with
//! About and Keyboard shortcuts), the command palette and the shortcuts sheet.
//! The keys stay the window's own (`Platform::window_command`); this only says
//! what they are.

use bingux_ui::commands::Command as Listed;
use bingux_ui::prelude::*;

use super::Command;
use crate::model::tab::TabKind;
use crate::ui::Ctx;

/// What the window tells bingux-ui about its commands.
pub struct Registry;

impl Registry {
    /// Every command with a key or a menu entry, with what applies now.
    pub fn commands(ctx: Ctx) -> Vec<Listed> {
        let (zoom, can_find, can_download, can_reopen, is_web) = {
            let browser = ctx.browser.read();
            let tab = browser.active();
            (
                tab.map_or(crate::zoom::Zoom::DEFAULT, |tab| tab.zoom)
                    .percent(),
                browser.engine.capabilities().find,
                browser.engine.capabilities().downloads,
                !browser.closed.is_empty(),
                tab.is_some_and(|tab| tab.kind == TabKind::Web && !tab.is_blank()),
            )
        };
        // (id, title, command, keys, in the menu, enabled)
        let rows: Vec<(&str, String, Command, &str, bool, bool)> = vec![
            (
                "browser.new-tab",
                tr!("browser-command-new-tab-title"),
                Command::NewTab,
                "Ctrl+T",
                true,
                true,
            ),
            (
                "browser.new-private-window",
                tr!("browser-command-private-window-title"),
                Command::NewPrivateWindow,
                "Ctrl+Shift+P",
                true,
                true,
            ),
            (
                "browser.reopen-tab",
                tr!("browser-command-reopen-tab-title"),
                Command::ReopenTab,
                "Ctrl+Shift+T",
                true,
                can_reopen,
            ),
            (
                "browser.close-tab",
                tr!("browser-command-close-tab-title"),
                Command::CloseTab,
                "Ctrl+W",
                false,
                true,
            ),
            (
                "browser.next-tab",
                tr!("browser-command-next-tab-title"),
                Command::NextTab,
                "Ctrl+Tab",
                false,
                true,
            ),
            (
                "browser.previous-tab",
                tr!("browser-command-previous-tab-title"),
                Command::PreviousTab,
                "Ctrl+Shift+Tab",
                false,
                true,
            ),
            (
                "browser.address",
                tr!("browser-command-address-title"),
                Command::FocusAddress,
                "Ctrl+L",
                false,
                true,
            ),
            (
                "browser.find",
                tr!("browser-command-find-title"),
                Command::Find,
                "Ctrl+F",
                true,
                can_find,
            ),
            (
                "browser.reload",
                tr!("browser-command-reload-title"),
                Command::Reload,
                "Ctrl+R",
                false,
                true,
            ),
            (
                "browser.back",
                tr!("browser-command-back-title"),
                Command::Back,
                "Alt+Left",
                false,
                true,
            ),
            (
                "browser.forward",
                tr!("browser-command-forward-title"),
                Command::Forward,
                "Alt+Right",
                false,
                true,
            ),
            (
                "browser.zoom-in",
                tr!("browser-command-zoom-in-title"),
                Command::ZoomIn,
                "Ctrl+=",
                true,
                true,
            ),
            (
                "browser.zoom-out",
                tr!("browser-command-zoom-out-title"),
                Command::ZoomOut,
                "Ctrl+-",
                true,
                true,
            ),
            (
                "browser.zoom-reset",
                tr!("browser-command-zoom-reset-title", percent = zoom),
                Command::ZoomReset,
                "Ctrl+0",
                true,
                true,
            ),
            (
                "browser.fullscreen",
                tr!("browser-command-fullscreen-title"),
                Command::Fullscreen,
                "F11",
                false,
                true,
            ),
            (
                "browser.downloads",
                tr!("browser-command-downloads-title"),
                Command::Downloads,
                "Ctrl+Shift+Y",
                true,
                can_download,
            ),
            (
                "browser.settings",
                tr!("browser-command-settings-title"),
                Command::Settings,
                "Ctrl+,",
                true,
                true,
            ),
            (
                "browser.view-source",
                tr!("browser-command-view-source-title"),
                Command::ViewSource,
                "Ctrl+U",
                true,
                is_web,
            ),
            (
                "browser.print",
                tr!("browser-command-print-title"),
                Command::Print,
                "Ctrl+P",
                true,
                true,
            ),
        ];
        let mut listed: Vec<Listed> = rows
            .into_iter()
            .map(|(id, title, command, keys, in_menu, enabled)| {
                let item = Listed::new(id, title, move |_: ()| {
                    ctx.with(|browser| browser.run(command))
                })
                .shortcut(keys)
                .keys_handled_by_view()
                .enabled(enabled);
                if in_menu {
                    item
                } else {
                    item.hidden_from_menu()
                }
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
