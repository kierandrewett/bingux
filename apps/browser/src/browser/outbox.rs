//! What the browser says to the person and to the window.

use bingux_ui::web_view::BackendError;

use crate::commands::Command;

/// A message for the person that is not part of a page: a toast.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub error: bool,
    pub text: String,
    /// A button on the toast: its label and what it runs.
    pub action: Option<(String, Command)>,
}

/// Why Close duplicate tabs asks before it closes anything: the two questions
/// of Firefox's `warnAboutClosingTabs` that apply to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DuplicatesAsk {
    /// The first time, in a profile: what the command does and that the tab seen
    /// last stays, whatever the number of tabs.
    FirstTime,
    /// Afterwards, only when more tabs than Reopen closed tab can bring back
    /// (`browser.sessionstore.max_tabs_undo`) would go: how many.
    Many(usize),
}

/// Something only the window can do. The UI takes these after a command and
/// does them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiRequest {
    /// Put the cursor in the address bar, with its text selected.
    FocusAddress,
    /// Put the cursor in the address bar to search the open tabs alone.
    SearchTabs,
    /// Open the find bar and focus its field.
    OpenFind,
    OpenDownloads,
    OpenPageInfo,
    /// Make the window fullscreen, or take it out of it.
    Fullscreen(bool),
    /// Start another browser window whose tabs are all private.
    NewPrivateWindow,
    /// Start the window of another profile, by its registry id (it takes the
    /// window already open for it, if there is one).
    OpenProfile(String),
    /// Ask whether to forget the history.
    ConfirmClearHistory,
    /// Ask before closing `keys` as duplicates ([`DuplicatesAsk`]); what the person
    /// answers is [`Browser::close_tabs_as_duplicates`].
    ConfirmCloseDuplicates {
        keys: Vec<u64>,
        ask: DuplicatesAsk,
    },
    /// Close the window: its last tab is gone.
    CloseWindow,
}

/// What has been said and not yet shown: the toasts, and what only the window
/// can do. The UI takes both after each change (`Ctx::with`).
#[derive(Debug, Default)]
pub struct Outbox {
    notices: Vec<Notice>,
    requests: Vec<UiRequest>,
}

impl Outbox {
    /// Tell the person something.
    pub fn notify(&mut self, text: impl Into<String>) {
        self.notice(Notice {
            error: false,
            text: text.into(),
            action: None,
        });
    }

    /// Tell the person something with a notice built by the caller.
    pub fn notice(&mut self, notice: Notice) {
        self.notices.push(notice);
    }

    /// Tell the person something went wrong, unless the same was said last.
    pub fn fail(&mut self, error: &BackendError) {
        let text = error.to_string();
        if self
            .notices
            .last()
            .is_some_and(|last| last.error && last.text == text)
        {
            return;
        }
        self.notice(Notice {
            error: true,
            text,
            action: None,
        });
    }

    /// Ask the window to do something.
    pub fn request(&mut self, request: UiRequest) {
        self.requests.push(request);
    }

    /// The notices not yet shown.
    pub fn take_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }

    /// What only the window can do, not yet done.
    pub fn take_requests(&mut self) -> Vec<UiRequest> {
        std::mem::take(&mut self.requests)
    }
}
