//! The tab strip: the browser's tabs as bingux-ui's, with what each tab is
//! doing shown on it.

use bingux_ui::prelude::*;

use crate::commands::Command;
use crate::tabs::{Tab, TabKind};

use crate::menus::TabMenu;
use crate::ui::Ctx;

/// The strip of tabs.
pub struct Strip;

impl Strip {
    /// What a tab shows of what it is doing: a crash first, then a question
    /// waiting for the person, then loading, then sound.
    fn status_of(tab: &Tab) -> Option<TabStatus> {
        if tab.gone.is_some() {
            Some(TabStatus::Failed)
        } else if tab.needs_attention() {
            Some(TabStatus::Attention)
        } else if tab.load.loading {
            Some(TabStatus::Loading)
        } else if tab.audio.muted {
            Some(TabStatus::Muted)
        } else if tab.audio.playing {
            Some(TabStatus::Audio)
        } else {
            None
        }
    }

    /// The tabs of the window as a strip. Every handler goes through the browser.
    pub fn build(ctx: Ctx) -> TabStrip {
        let browser = ctx.browser.read();
        let items: Vec<TabItem> = browser
            .tabs
            .all()
            .iter()
            .map(|tab| {
                TabItem::new(tab.key, tab.display_title())
                    .icon(tab.icon(browser.options.private))
                    .pinned(tab.pinned)
                    .status(Self::status_of(tab))
            })
            .collect();
        let mut strip = tab_strip(items, browser.tabs.active_key())
            .on_select(move |key: u64| ctx.with(|browser| browser.select(key)))
            .on_close(move |key: u64| ctx.with(|browser| browser.close_tab(key)))
            .on_new(move |_: ()| ctx.with(|browser| browser.run(Command::NewTab)))
            .on_reorder(move |(key, to): (u64, usize)| {
                ctx.with(|browser| browser.move_tab(key, to))
            })
            .on_status(move |key: u64| ctx.with(|browser| browser.toggle_mute(key)));
        for tab in browser.tabs.all() {
            strip = strip.tab_menu(tab.key, TabMenu::items(ctx, tab));
        }
        strip
    }
}

impl Tab {
    /// The icon a tab has until the engine can hand over the page's own: the
    /// engine reports only the address of a page's icon (`VV_EVENT_FAVICON`), and
    /// the browser fetches nothing on its own.
    /// `private` is whether the window is a private one: every tab of it is.
    pub fn icon(&self, private: bool) -> Icon {
        match self.kind {
            TabKind::Settings => Icon::EmblemSystem,
            TabKind::Web if private => Icon::NetworkWirelessEncrypted,
            TabKind::Web => Icon::WebBrowser,
        }
    }
}
