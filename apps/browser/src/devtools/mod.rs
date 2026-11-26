//! Developer tools: the toolbox of a tab, docked under its page.
//!
//! This follows Firefox's model, in Gecko's names. A toolbox is attached to one
//! target tab. Its host is the bottom of the tab (`devtools.toolbox.host`
//! defaults to `bottom`), and the height the person drags it to is kept for the
//! next toolbox (`devtools.toolbox.footer.height`). The engine makes the toolbox
//! a browser of its own, a session with frames and input, and closes it after
//! its target; this feature only asks for it, docks it and drops it.
//!
//! The state is explicit: a tab holds `Option<DockedToolbox>`. There are no
//! flags. F12 and Ctrl+Shift+I toggle the toolbox of the tab on show. A tab
//! with no web page (blank, Settings) has nothing to inspect and does nothing.
//! Only a bottom dock exists. The toolbox's own UI may ask for a side dock or a
//! window of its own, but the engine has no event for that yet, so there is no
//! extra UI for it.
//!
//! The pieces: [`DockedToolbox`] is the state, the methods on [`Browser`] here are
//! the transitions (open, close, the engine ending it, a resize, a new split),
//! and `ui::DockedPage` is the layout that puts a page and its toolbox in one
//! tab with a draggable splitter.

use bingux_ui::web_view::{TabId, Toolbox};

use crate::browser::{Browser, ViewSize};
use crate::tabs::TabKind;

/// The share of the content area the page keeps the first time a toolbox opens
/// in a window. The toolbox takes the rest. Firefox opens at a similar split.
pub const DEFAULT_PAGE_SHARE: f32 = 0.6;

/// The toolbox of one tab, docked under its page.
///
/// A tab has at most one: the engine answers `Busy` to a second toolbox for the
/// same target, so the state cannot hold two. It exists from the moment the
/// engine hands the session over until the person toggles it off, the target
/// closes, or the engine fails.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockedToolbox {
    /// The toolbox session. The same web view that draws a page draws it.
    pub handle: Toolbox,
    /// The share of the content area, from 0 to 1, that the page keeps; the
    /// toolbox gets the rest. The splitter moves it.
    pub page_share: f32,
}

impl Browser {
    /// Whether tab `key` shows a web page a toolbox can inspect. A blank tab or
    /// the Settings page has no document to attach to.
    fn inspectable(&self, key: u64) -> bool {
        self.tabs
            .get(key)
            .is_some_and(|tab| tab.kind == TabKind::Web && !tab.is_blank())
    }

    /// F12 and Ctrl+Shift+I: open the toolbox of tab `key` under its page, or
    /// close it if it is open. Does nothing where there is no web page.
    ///
    /// The engine refuses a private tab and says why; that is logged, not shown,
    /// because the engine is a stable target and the person asked for something
    /// it never does in a private window.
    pub fn toggle_devtools(&mut self, key: u64) {
        if !self.inspectable(key) {
            return;
        }
        if self.tabs.get(key).is_some_and(|tab| tab.toolbox.is_some()) {
            self.close_toolbox(key);
        } else {
            self.open_toolbox(key);
        }
    }

    /// Ask the engine for the toolbox of tab `key` and dock it.
    ///
    /// The session starts at the size the dock will probably have, and the web
    /// view corrects it with the real size at its first layout
    /// ([`Browser::resize_toolbox`]), so the first frame is not far off.
    fn open_toolbox(&mut self, key: u64) {
        let Some(session) = self.session_of(key) else {
            return;
        };
        let page_share = self.devtools_page_share;
        let ViewSize {
            width,
            height,
            scale,
        } = self.view;
        let toolbox_height = ((height as f32) * (1.0 - page_share)).round() as u32;
        match self
            .engine
            .backend()
            .open_devtools(session, width, toolbox_height.max(1), scale)
        {
            Ok(handle) => {
                if let Some(tab) = self.tabs.get_mut(key) {
                    tab.toolbox = Some(DockedToolbox { handle, page_share });
                }
            }
            Err(error) => eprintln!("bingux-browser: devtools: {error}"),
        }
    }

    /// Close the toolbox of tab `key` at the person's request and drop the state.
    /// The state goes first, so the session's own `Closed` event, which arrives
    /// later, finds nothing to do.
    pub(crate) fn close_toolbox(&mut self, key: u64) {
        let Some(docked) = self.tabs.get_mut(key).and_then(|tab| tab.toolbox.take()) else {
            return;
        };
        if let Err(error) = self.engine.backend().close_devtools(docked.handle) {
            eprintln!("bingux-browser: devtools: {error}");
        }
    }

    /// The tab whose toolbox is the session `session`, if any. Events for a
    /// toolbox arrive under the toolbox's session id, which no tab owns.
    pub(crate) fn toolbox_owner(&self, session: TabId) -> Option<u64> {
        self.tabs
            .all()
            .iter()
            .find(|tab| {
                tab.toolbox
                    .is_some_and(|docked| docked.handle.session() == session)
            })
            .map(|tab| tab.key)
    }

    /// The engine ended the toolbox of tab `key` (its target closed, or the
    /// person closed the toolbox from inside it). Its session is gone, so no
    /// call goes back to it.
    pub(crate) fn toolbox_ended(&mut self, key: u64) {
        if let Some(tab) = self.tabs.get_mut(key) {
            tab.toolbox = None;
        }
    }

    /// The docked area of tab `key`'s toolbox has a size: tell the session.
    /// Called from the toolbox's web view at layout, in logical pixels.
    pub fn resize_toolbox(&mut self, key: u64, width: u32, height: u32, scale: f32) {
        let Some(docked) = self.tabs.get(key).and_then(|tab| tab.toolbox) else {
            return;
        };
        self.engine.call(&mut self.out, |backend| {
            backend.set_size(docked.handle.session(), width.max(1), height.max(1), scale)
        });
    }

    /// Show or hide the toolbox of tab `key` with its page: a hidden session
    /// makes no frames, so a tab in the background costs nothing.
    pub(crate) fn set_toolbox_visible(&mut self, key: u64, visible: bool) {
        let Some(docked) = self.tabs.get(key).and_then(|tab| tab.toolbox) else {
            return;
        };
        self.engine.call(&mut self.out, |backend| {
            backend.set_visible(docked.handle.session(), visible, visible)
        });
    }

    /// The splitter moved: tab `key`'s page keeps `share` of the area. The
    /// value is also the size the next toolbox opens at, in any tab.
    pub fn set_page_share(&mut self, key: u64, share: f32) {
        self.devtools_page_share = share;
        if let Some(docked) = self.tabs.get_mut(key).and_then(|tab| tab.toolbox.as_mut()) {
            docked.page_share = share;
        }
    }
}

#[cfg(test)]
mod tests;
