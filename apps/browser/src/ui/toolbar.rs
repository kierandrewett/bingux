//! The toolbar: Back, Forward, Reload or Stop, the connection, the address bar,
//! the zoom, the downloads and the main menu. On a phone the same controls are
//! two rows: the address bar on top and the navigation below.

use bingux_ui::component::ComponentName;
use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::model::downloads::Downloads;
use crate::model::security::{self, Tone};
use crate::model::tab::TabKind;
use crate::model::zoom;

use super::address::AddressBar;
use super::ctx::Ctx;
use super::menus::main_menu;

/// What the toolbar reads of the tab on show.
struct Reading {
    key: u64,
    can_back: bool,
    can_forward: bool,
    loading: bool,
    summary: security::Summary,
    zoom: f64,
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
        summary: tab.map_or_else(
            || security::summarize(&Default::default(), ""),
            |tab| match tab.kind {
                TabKind::Settings => security::summarize(
                    &bingux_ui::web_view::Security {
                        state: bingux_ui::web_view::SecurityState::Internal,
                        ..Default::default()
                    },
                    "bingux:settings",
                ),
                TabKind::Web => security::summarize(&tab.security, &tab.url),
            },
        ),
        zoom: tab.map_or(1.0, |tab| tab.zoom),
        is_web: tab.is_some_and(|tab| tab.kind == TabKind::Web),
        tabs: browser.tabs.len(),
    }
}

fn tone_ink(tone: Tone) -> Option<Ink> {
    match tone {
        Tone::Neutral => None,
        Tone::Good => None,
        Tone::Warning => Some(Ink::Warning),
        Tone::Danger => Some(Ink::Danger),
    }
}

/// Back and Forward.
fn history_buttons(ctx: Ctx, read: &Reading) -> Vec<Element> {
    let key = read.key;
    vec![
        icon_button(Icon::GoPrevious, move |_: ()| {
            ctx.with(|browser| browser.back(key))
        })
        .tooltip("Back")
        .enabled(read.can_back)
        .into(),
        icon_button(Icon::GoNext, move |_: ()| {
            ctx.with(|browser| browser.forward(key))
        })
        .tooltip("Forward")
        .enabled(read.can_forward)
        .into(),
    ]
}

/// Reload, or Stop while the page loads.
fn reload_button(ctx: Ctx, read: &Reading) -> Element {
    let key = read.key;
    if read.loading {
        icon_button(Icon::WindowClose, move |_: ()| {
            ctx.with(|browser| browser.stop(key))
        })
        .tooltip("Stop loading")
        .into()
    } else {
        icon_button(Icon::Refresh, move |_: ()| {
            ctx.with(|browser| browser.reload(key, false))
        })
        .tooltip("Reload")
        .enabled(read.is_web)
        .into()
    }
}

/// The connection of the page: its icon opens the page info.
fn connection_button(ctx: Ctx, read: &Reading) -> Element {
    let mut anchor = ctx.chrome.info_anchor;
    let mut open = ctx.chrome.info_open;
    let mut button =
        icon_button(read.summary.icon, move |_: ()| open.set(true)).tooltip(&read.summary.headline);
    if let Some(ink) = tone_ink(read.summary.tone) {
        button = button.ink(ink);
    }
    rect()
        .on_sized(move |event: Event<SizedEventData>| anchor.set_if_modified(event.area))
        .child(button)
        .into_element()
}

/// The zoom, as a button that puts it back to 100 %, while it is not 100 %.
fn zoom_chip(ctx: Ctx, read: &Reading) -> Option<Element> {
    if zoom::is_default(read.zoom) {
        return None;
    }
    let key = read.key;
    Some(
        action_button(format!("{}%", zoom::percent(read.zoom)), move |_: ()| {
            ctx.with(|browser| browser.set_zoom(key, zoom::DEFAULT));
        })
        .flat()
        .compact()
        .into(),
    )
}

/// The downloads button; busy while something downloads.
fn downloads_button(ctx: Ctx, downloads: &Downloads) -> Element {
    let mut anchor = ctx.chrome.downloads_anchor;
    let mut open = ctx.chrome.downloads_open;
    let active = downloads.active();
    // An engine that does not save downloads has nothing to list: the button
    // stays, so the toolbar keeps its shape, and says why it does nothing.
    let can = ctx.browser.read().capabilities().downloads;
    let tip = match active {
        _ if !can => "Downloads are not available in this engine yet".to_string(),
        0 => "Downloads".to_string(),
        1 => "Downloads: 1 running".to_string(),
        count => format!("Downloads: {count} running"),
    };
    rect()
        .on_sized(move |event: Event<SizedEventData>| anchor.set_if_modified(event.area))
        .child(
            icon_button(Icon::FolderDownload, move |_: ()| {
                open.set(true);
            })
            .tooltip(&tip)
            .working(active > 0)
            .enabled(can),
        )
        .into_element()
}

/// The main menu button.
fn menu_control(ctx: Ctx) -> Element {
    let mut anchor = ctx.chrome.menu_anchor;
    rect()
        .on_sized(move |event: Event<SizedEventData>| anchor.set_if_modified(event.area))
        .child(menu_button("Main menu", main_menu(ctx)).icon(Icon::OpenMenu))
        .into_element()
}

#[derive(PartialEq)]
pub struct Toolbar {
    pub ctx: Ctx,
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
                    px(S2),
                    py(S1),
                    bg(Paint::Bg),
                    border(Line::HairlineBottom),
                ],
            );
        if is_compact() {
            // A phone: the connection, the address, the tabs and the menu. The
            // history is in the bar below.
            return bar
                .a11y_alt("Address")
                .child(connection_button(ctx, &read))
                .child(AddressBar { ctx })
                .child(tabs_button(ctx, read.tabs))
                .child(menu_control(ctx))
                .into_element();
        }
        bar = bar
            .a11y_alt("Navigation")
            .children(history_buttons(ctx, &read))
            .child(reload_button(ctx, &read))
            .child(connection_button(ctx, &read))
            .child(AddressBar { ctx })
            .maybe_child(zoom_chip(ctx, &read));
        if !downloads.list().is_empty() {
            bar = bar.child(downloads_button(ctx, &downloads));
        }
        bar.child(menu_control(ctx)).into_element()
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
            .children(history_buttons(ctx, &read))
            .child(reload_button(ctx, &read))
            .child(downloads_button(ctx, &downloads))
    }
}

/// The tab count of a phone, as a button that opens the list of tabs.
fn tabs_button(ctx: Ctx, count: usize) -> Element {
    // The number of tabs is the whole label, as on every phone browser.
    action_button(count.to_string(), move |_: ()| super::phone::open_tabs(ctx))
        .flat()
        .compact()
        .into()
}
