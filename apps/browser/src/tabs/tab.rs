//! One tab: what the engine last said about its page, and what the browser
//! has to show for it.

use std::collections::VecDeque;

use bingux_ui::web_view::requests::CertError;
use bingux_ui::web_view::{Audio, GoneReason, HistoryEntry, LoadState, RequestId, Security, TabId};

use crate::address::Address;
use crate::find::Find;
use crate::prompts::Prompt;
use crate::zoom::Zoom;
use bingux_ui::prelude::tr;

/// What a tab holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKind {
    /// A web page, shown by an engine session.
    Web,
    /// The browser's settings, drawn by the browser. No session.
    Settings,
}

/// What the tab's content area shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Page<'a> {
    /// The page, from the engine's frames.
    Web,
    /// No address yet: the browser's new tab page.
    NewTab,
    Settings,
    Crashed(GoneReason),
    CertError(&'a CertError),
    /// The load failed.
    Failed(&'a bingux_ui::web_view::LoadError),
}

#[derive(Clone, Debug)]
pub struct Tab {
    /// The tab's name in the strip, never reused in this window.
    pub key: u64,
    pub kind: TabKind,
    pub session: Option<TabId>,
    pub ready: bool,
    pub url: String,
    pub title: String,
    pub favicon: Option<String>,
    pub load: LoadState,
    pub security: Security,
    pub can_back: bool,
    pub can_forward: bool,
    pub history: Vec<HistoryEntry>,
    pub history_index: usize,
    pub zoom: Zoom,
    pub audio: Audio,
    pub pinned: bool,
    pub private: bool,
    pub gone: Option<GoneReason>,
    /// The address of the link under the pointer.
    pub hover: Option<String>,
    pub fullscreen: bool,
    pub cert: Option<(RequestId, CertError)>,
    pub prompts: VecDeque<Prompt>,
    pub find: Find,
    /// The tab this one was opened from, for which tab comes next on close.
    pub opener: Option<u64>,
}

impl Tab {
    /// A tab with nothing in it.
    pub fn blank(key: u64, kind: TabKind) -> Self {
        Self {
            key,
            kind,
            session: None,
            ready: false,
            url: String::new(),
            title: String::new(),
            favicon: None,
            load: LoadState::default(),
            security: Security::default(),
            can_back: false,
            can_forward: false,
            history: Vec::new(),
            history_index: 0,
            zoom: Zoom::default(),
            audio: Audio::default(),
            pinned: false,
            private: false,
            gone: None,
            hover: None,
            fullscreen: false,
            cert: None,
            prompts: VecDeque::new(),
            find: Find::default(),
            opener: None,
        }
    }

    /// What the strip and the window title call it.
    pub fn display_title(&self) -> String {
        match self.kind {
            TabKind::Settings => tr!("browser-tab-settings-title"),
            TabKind::Web if !self.title.is_empty() => self.title.clone(),
            TabKind::Web if self.is_blank() => tr!("browser-tab-new-title"),
            TabKind::Web if self.load.loading => tr!("browser-tab-loading-title"),
            TabKind::Web => Address::new(&self.url).display(),
        }
    }

    /// No address: the new tab page.
    pub fn is_blank(&self) -> bool {
        self.kind == TabKind::Web
            && (self.url.is_empty() || self.url == "about:blank" || self.url == "bingux:newtab")
    }

    /// What the content area shows now.
    pub fn page(&self) -> Page<'_> {
        if self.kind == TabKind::Settings {
            return Page::Settings;
        }
        if let Some(reason) = self.gone {
            return Page::Crashed(reason);
        }
        if let Some((_, cert)) = &self.cert {
            return Page::CertError(cert);
        }
        if let Some(error) = self.load.error.as_ref().filter(|_| !self.load.loading) {
            return Page::Failed(error);
        }
        if self.is_blank() {
            return Page::NewTab;
        }
        Page::Web
    }

    /// Something is waiting for the person.
    pub fn needs_attention(&self) -> bool {
        !self.prompts.is_empty()
    }

    /// The text of the address bar for this tab when it is not being edited.
    pub fn address_text(&self) -> String {
        match self.kind {
            TabKind::Settings => "bingux:settings".to_string(),
            TabKind::Web => Address::new(&self.url).display(),
        }
    }
}

#[cfg(test)]
mod tests {
    use bingux_ui::web_view::LoadError;

    use super::*;

    fn web(url: &str) -> Tab {
        let mut tab = Tab::blank(1, TabKind::Web);
        tab.url = url.to_string();
        tab
    }

    #[test]
    fn a_tab_is_titled_by_its_page_then_by_what_it_is_doing_then_by_its_address() {
        let mut tab = web("https://example.com/");
        assert_eq!(tab.display_title(), "example.com");
        tab.load.loading = true;
        assert_eq!(tab.display_title(), "Loading\u{2026}");
        tab.title = "Example Domain".to_string();
        assert_eq!(tab.display_title(), "Example Domain");
        assert_eq!(web("").display_title(), "New tab");
        assert_eq!(Tab::blank(2, TabKind::Settings).display_title(), "Settings");
    }

    #[test]
    fn the_content_area_shows_the_most_urgent_thing_first() {
        let mut tab = web("https://example.com/");
        assert_eq!(tab.page(), Page::Web);
        tab.load.error = Some(LoadError {
            code: 1,
            name: "NS_ERROR_UNKNOWN_HOST".to_string(),
        });
        assert!(matches!(tab.page(), Page::Failed(_)));
        tab.load.loading = true;
        assert_eq!(tab.page(), Page::Web, "a new load hides the old error");
        tab.load.loading = false;
        tab.cert = Some((RequestId(1), CertError::for_test("expired.example.net")));
        assert!(
            matches!(tab.page(), Page::CertError(_)),
            "a certificate error is before a load error"
        );
        tab.gone = Some(GoneReason::Crashed);
        assert_eq!(
            tab.page(),
            Page::Crashed(GoneReason::Crashed),
            "a crash is before everything"
        );
        assert_eq!(web("").page(), Page::NewTab);
        assert_eq!(Tab::blank(3, TabKind::Settings).page(), Page::Settings);
    }
}
