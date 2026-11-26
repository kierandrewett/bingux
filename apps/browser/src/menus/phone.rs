//! The main menu of a phone: a sheet with four quick buttons and the rows of
//! the library and the page's tools (Phone 09). A row whose feature is not
//! built is dimmed and says why under its name, since a finger has no tooltip.

use bingux_ui::prelude::*;
use freya::prelude::*;

use super::main::{MainMenu, Reading};
use crate::address::Address;
use crate::commands::Command;
use crate::ui::Ctx;

/// The phone's main menu.
#[derive(PartialEq)]
pub struct PhoneMenu {
    ctx: Ctx,
}

impl PhoneMenu {
    /// How many rows the sheet lists.
    const ROWS: f32 = 8.0;

    /// Open the sheet over the window.
    pub fn open(ctx: Ctx) {
        // The sheet is as tall as its content: the popup measures nothing here.
        let height = Dim::Header.px()
            + Dim::Hairline.px()
            + 2.0 * S4.px()
            + 2.0 * (Dim::Button.px() + 2.0 * S3.px())
            + S4.px()
            + Self::ROWS * Dim::RowTall.px();
        ctx.window
            .open(popup(tr!("ui-main-menu"), PhoneMenu { ctx }).size(Dim::PopupWidth.px(), height));
    }

    /// A quick button: closes the sheet, then does what it says.
    fn tile(
        &self,
        window: Window,
        label: String,
        icon: Icon,
        enabled: bool,
        on_press: impl Fn(Ctx) + 'static,
    ) -> Element {
        let ctx = self.ctx;
        action_button(label, move |_: ()| {
            window.close();
            on_press(ctx);
        })
        .icon(icon)
        .flat()
        .fill_width()
        .enabled(enabled)
        .into_element()
    }

    /// A row of a feature that is not built: dimmed, with the reason under it.
    fn unavailable(title: String, icon: Icon, reason: String) -> Element {
        row(title)
            .leading(icon)
            .subtitle(reason)
            .enabled(false)
            .into()
    }
}

impl Component for PhoneMenu {
    fn render(&self) -> impl IntoElement {
        let ctx = self.ctx;
        let window = use_window();
        let tokens = use_tokens();
        let read = Reading::of(ctx);
        let url = read.url.clone();
        let tiles = vec![
            self.tile(
                window,
                tr!("browser-command-forward-title"),
                Icon::GoNext,
                read.can_forward,
                |ctx| ctx.run(Command::Forward),
            ),
            self.tile(
                window,
                tr!("browser-main-menu-share-copy-link"),
                Icon::InsertLink,
                read.is_web,
                move |ctx| MainMenu::copy_link(ctx, &url),
            ),
            self.tile(
                window,
                tr!("browser-phone-menu-bookmark"),
                Icon::NonStarred,
                false,
                |_| {},
            ),
            self.tile(
                window,
                tr!("browser-command-find-title"),
                Icon::Search,
                read.find,
                |ctx| ctx.run(Command::Find),
            ),
        ];
        let downloads = {
            let line = row(tr!("browser-command-downloads-title"))
                .leading(Icon::FolderDownload)
                .enabled(read.downloads)
                .on_press(move |_: ()| {
                    window.close();
                    ctx.run(Command::Downloads);
                });
            if read.running > 0 {
                line.value(tr!(
                    "browser-phone-menu-downloads-running",
                    count = read.running
                ))
            } else {
                line
            }
        };
        let rows = vec![
            Self::unavailable(
                tr!("browser-main-menu-history"),
                Icon::DocumentOpenRecent,
                tr!("browser-main-menu-show-history-unavailable"),
            ),
            Self::unavailable(
                tr!("browser-main-menu-bookmarks"),
                Icon::Bookmarks,
                tr!("browser-main-menu-bookmarks-unavailable"),
            ),
            Self::unavailable(
                tr!("browser-phone-menu-reading-list"),
                Icon::DocumentOpen,
                tr!("browser-phone-menu-reading-list-unavailable"),
            ),
            downloads.into(),
            Self::unavailable(
                tr!("browser-main-menu-translate"),
                Icon::Network,
                tr!("browser-main-menu-translate-unavailable"),
            ),
            Self::unavailable(
                tr!("browser-phone-menu-desktop-site"),
                Icon::Desktop,
                tr!("browser-phone-menu-desktop-site-unavailable"),
            ),
            Self::unavailable(
                tr!("browser-phone-menu-extensions"),
                Icon::PreferencesOther,
                tr!("browser-phone-menu-extensions-unavailable"),
            ),
            row(tr!("browser-command-settings-title"))
                .leading(Icon::PreferencesSystem)
                .navigates()
                .on_press(move |_: ()| {
                    window.close();
                    ctx.run(Command::Settings);
                })
                .into(),
        ];
        popup_column()
            .children(tile_rows(&tokens, tiles, Dim::Button.px()))
            .child(Group::new(None::<String>).rows(rows))
    }
}

impl MainMenu {
    /// Copy the address of the page to the clipboard and say so.
    pub(super) fn copy_link(ctx: Ctx, url: &str) {
        super::ContextMenu::copy(ctx, Address::new(url).display(), "page");
    }
}
