//! The toolbar: Back, Forward, Reload or Stop, the connection, the address bar,
//! the zoom, the downloads and the main menu. On a phone the same controls are
//! two rows: the address bar on top and the navigation below.

use bingux_ui::component::ComponentName;
use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::downloads::button::downloads_button;
use crate::navigation::NavButtons;
use crate::site_info::ConnectionButton;
use crate::tabs::TabKind;
use crate::tabs::TabsSheet;
use crate::zoom::{Zoom, ZoomChip};

use super::ctx::Ctx;
use crate::address::AddressBar;

/// What the toolbar reads of the tab on show.
struct Reading {
    key: u64,
    can_back: bool,
    can_forward: bool,
    loading: bool,
    zoom: Zoom,
    is_web: bool,
    tabs: usize,
}

fn reading(ctx: Ctx) -> Reading {
    let browser = ctx.browser.read();
    let tab = browser.active();
    Reading {
        key: browser.tabs.active_key(),
        can_back: tab.is_some_and(|tab| tab.can_back),
        can_forward: tab.is_some_and(|tab| tab.can_forward),
        loading: tab.is_some_and(|tab| tab.load.loading),
        zoom: tab.map_or(Zoom::DEFAULT, |tab| tab.zoom),
        is_web: tab.is_some_and(|tab| tab.kind == TabKind::Web),
        tabs: browser.tabs.len(),
    }
}

/// The main menu button.
fn menu_control() -> Element {
    app_menu(Vec::new()).into()
}

#[derive(PartialEq)]
pub struct Toolbar {
    pub ctx: Ctx,
    /// The toolbar is the window's header (`Screen` paints its tone and the
    /// hairline under it); otherwise it is a bar of its own over the page.
    pub in_header: bool,
}

impl Component for Toolbar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ctx = self.ctx;
        let read = reading(ctx);
        let downloads = ctx.browser.read().downloads.clone();
        let mut bar = rect()
            .component("Toolbar")
            .a11y_role(AccessibilityRole::Toolbar)
            .tw(
                &tokens,
                &[
                    w_full(),
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    gap(S1),
                ],
            );
        if !self.in_header {
            bar = bar.tw(
                &tokens,
                &[px(S2), py(S1), bg(Paint::Bg), border(Line::HairlineBottom)],
            );
        }
        if is_compact() {
            // A phone: the connection, the address, the tabs and the menu. The
            // history is in the bar below.
            return bar
                .a11y_alt("Address")
                .child(ConnectionButton::element(ctx))
                .child(AddressBar { ctx })
                .child(tabs_button(ctx, read.tabs))
                .child(menu_control())
                .into_element();
        }
        bar = bar
            .a11y_alt("Navigation")
            .children(NavButtons::history(
                ctx,
                read.key,
                read.can_back,
                read.can_forward,
            ))
            .child(NavButtons::reload(ctx, read.key, read.loading, read.is_web))
            .child(ConnectionButton::element(ctx))
            .child(AddressBar { ctx })
            .maybe_child((!read.zoom.is_default()).then_some(ZoomChip {
                ctx,
                key: read.key,
                zoom: read.zoom,
            }));
        if !downloads.list().is_empty() {
            bar = bar.child(downloads_button(ctx, &downloads));
        }
        bar.child(menu_control()).into_element()
    }
}

/// The bottom bar of a phone: history, reload and downloads, within a
/// thumb's reach.
#[derive(PartialEq)]
pub struct PhoneBottom {
    pub ctx: Ctx,
}

impl Component for PhoneBottom {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ctx = self.ctx;
        let read = reading(ctx);
        let downloads = ctx.browser.read().downloads.clone();
        rect()
            .component("BottomBar")
            .a11y_role(AccessibilityRole::Toolbar)
            .a11y_alt("Navigation")
            .tw(
                &tokens,
                &[
                    w_full(),
                    flex_row(),
                    items(Align::Center),
                    grow_children(),
                    justify(Align::Around),
                    px(S2),
                    py(S1),
                    bg(Paint::Bg),
                    border(Line::HairlineTop),
                ],
            )
            .children(NavButtons::history(
                ctx,
                read.key,
                read.can_back,
                read.can_forward,
            ))
            .child(NavButtons::reload(ctx, read.key, read.loading, read.is_web))
            .child(downloads_button(ctx, &downloads))
    }
}

/// The tab count of a phone, as a button that opens the list of tabs.
fn tabs_button(ctx: Ctx, count: usize) -> Element {
    // The number of tabs is the whole label, as on every phone browser.
    action_button(count.to_string(), move |_: ()| TabsSheet::open(ctx))
        .flat()
        .compact()
        .into()
}
