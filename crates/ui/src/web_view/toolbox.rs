//! The handle of a developer tools toolbox.
//!
//! In Firefox the toolbox is a document of its own that is attached to one
//! target tab: it docks under the page, beside it or in a window of its own, and
//! it lives and dies with that tab. The engine makes it a session, like a tab,
//! with its own frames and input. The browser draws that session with the same
//! [`WebView`](crate::web_view::WebView) it uses for a page, so this handle only
//! names the session and says what it is for. It is not a tab: the tab strip
//! never lists it, and the engine closes it after its target.

use crate::web_view::ids::TabId;

/// The toolbox session of one target tab. Copy and cheap: it is only an id.
///
/// A handle stays valid until `BrowserBackend::close_devtools` is called with
/// it or the engine ends the session (the target closed). After that the id is
/// never reused (ids are not), so a stale handle is an error, not another
/// toolbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Toolbox {
    /// The session that shows the tools. Draw it with `web_view`, size it with
    /// `set_size` and show it with `set_visible`, as for any page.
    session: TabId,
}

impl Toolbox {
    /// A handle for the toolbox session `session`. For backends: the browser
    /// gets handles from `BrowserBackend::open_devtools`.
    pub const fn new(session: TabId) -> Self {
        Self { session }
    }

    /// The session that shows the tools.
    pub const fn session(self) -> TabId {
        self.session
    }
}
