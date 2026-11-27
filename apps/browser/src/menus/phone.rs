//! The main menu of a phone: a sheet with three quick buttons (Forward, Copy link, Find)
//! and the rows of the library that exist (Downloads, Settings), as in the design
//! (Phone 09). The desktop menu is a cascade of submenus, which a finger cannot use, so
//! a phone gets a sheet. A feature that is not built has no tile or row, as on the desktop.

use bingux_ui::prelude::*;
use freya::prelude::*;

use super::main::{MainMenu, Reading};
use crate::commands::Command;
use crate::ui::Ctx;

/// The phone's main menu: a sheet whose presses close it first and then do their work,
/// so what they open (the find bar, the list of downloads) is not left under the sheet.
#[derive(PartialEq)]
pub struct PhoneMenu {
    ctx: Ctx,
}

impl PhoneMenu {
    /// How many rows the sheet lists. The sheet is sized from it.
    const ROWS: f32 = 2.0;

    /// Open the sheet over the window.
    pub fn open(ctx: Ctx) {
        // The popup measures nothing here (a body of rows in a window that already holds
        // the page takes the most the window allows), so the height is asked for: the
        // header, the row of buttons and the rows. The popup limits it to the window.
        let height = Dim::Header.px()
            + Dim::Hairline.px()
            + 2.0 * S4.px()
            + Dim::Button.px()
            + S6.px()
            + Self::ROWS * Dim::RowSubtitle.px();
        ctx.window
            .open(popup(tr!("ui-main-menu"), PhoneMenu { ctx }).size(Dim::PopupWidth.px(), height));
    }

    /// A quick button. It closes the sheet before it acts, so the find bar or the page
    /// it opens is not under the sheet. `window` is passed in, not read here, because
    /// hooks may only be read while the component renders.
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
}

impl Component for PhoneMenu {
    fn render(&self) -> impl IntoElement {
        let ctx = self.ctx;
        let window = use_window();
        let tokens = use_tokens();
        // One read of the browser, so the tiles and rows agree with each other.
        let read = Reading::of(ctx);
        let url = read.url.clone();
        let tiles = vec![
            self.tile(
                window,
                tr!("browser-command-forward-title"),
                Icon::ChevronRight,
                read.can_forward,
                |ctx| ctx.run(Command::Forward),
            ),
            self.tile(
                window,
                tr!("browser-main-menu-share-copy-link"),
                Icon::Link,
                read.is_web,
                move |ctx| MainMenu::copy_link(ctx, &url),
            ),
            self.tile(
                window,
                tr!("browser-phone-menu-find"),
                Icon::Magnifier,
                true,
                |ctx| ctx.run(Command::Find),
            ),
        ];
        // The tiles share the width equally, as the design's row of four does.
        let tiles = rect()
            .tw(&tokens, &[flex_row(), grow_children(), w_full(), gap(S1)])
            .children(
                tiles
                    .into_iter()
                    .map(|tile| rect().width(Size::flex(1.0)).child(tile).into_element()),
            );
        let downloads = {
            let line = row(tr!("browser-command-downloads-title"))
                .leading(Icon::FolderArrowDown)
                .on_press(move |_: ()| {
                    window.close();
                    ctx.run(Command::Downloads);
                });
            // The design's value of this row: how many are running, when any are.
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
            downloads.into(),
            row(tr!("browser-command-settings-title"))
                .leading(Icon::Gear)
                .navigates()
                .on_press(move |_: ()| {
                    window.close();
                    ctx.run(Command::Settings);
                })
                .into(),
        ];
        popup_column()
            .child(tiles)
            .child(Group::new(None::<String>).rows(rows))
    }
}

impl MainMenu {
    /// Copy the address of the page to the clipboard and say so. On the desktop menu and
    /// the phone sheet alike, so both say the same thing.
    pub(super) fn copy_link(ctx: Ctx, url: &str) {
        super::ContextMenu::copy(ctx, crate::address::Address::new(url).display(), "page");
    }
}
