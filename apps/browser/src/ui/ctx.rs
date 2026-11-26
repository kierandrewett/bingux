//! What every part of the window holds in common: the browser's state, the
//! handles of the window, and the small states of the chrome itself (what the
//! address bar says, which popover is open).
//!
//! All of it is `Copy`, so a handler takes its own. Commands go through
//! [`Ctx::with`], which changes the browser and then does what only the window
//! can do (focus the address bar, go fullscreen, show a toast).

use bingux_ui::prelude::*;
use bingux_ui::shell::with_window;
use freya::prelude::*;

use crate::address::FieldState;
use crate::browser::{Browser, Notice, UiRequest};
use crate::commands::Command;
use crate::commands::Platform;
use crate::downloads::CardState;
use crate::find::BarState;
use crate::prompts::DialogFields;
use crate::site_info::PopoverState;

/// The states of the chrome that are not the browser's: they belong to the
/// window, not to a tab.
#[derive(Clone, Copy, PartialEq)]
pub struct Chrome {
    /// Where the controls that open popovers are, in the window.
    /// Where the page area starts in the window: a context menu opens at the
    /// pointer's place in the page plus this.
    pub page_origin: State<(f32, f32)>,
}

/// Everything a component of the window needs.
#[derive(Clone, Copy, PartialEq)]
pub struct Ctx {
    pub browser: State<Browser>,
    pub chrome: Chrome,
    pub address: FieldState,
    pub site_info: PopoverState,
    pub find: BarState,
    pub downloads_popover: CardState,
    pub prompts: DialogFields,
    pub overlays: Overlays,
    pub window: Window,
    pub platform: Platform,
}

impl Ctx {
    /// The width of the cards that hang from the toolbar: wide enough for a
    /// sentence, and a phone's own column.
    pub fn popover_width(self) -> f32 {
        if is_compact() {
            // The phone's whole column, less the margin a floating card keeps.
            (self.window.size().0 - 2.0 * S2.px()).max(Dim::MenuWide.px())
        } else {
            Dim::Dialog.px()
        }
    }

    /// Run `change` on the browser, then do what it asked of the window.
    pub fn with<R>(self, change: impl FnOnce(&mut Browser) -> R) -> R {
        let mut browser = self.browser;
        let mut guard = browser.write();
        let result = change(&mut guard);
        let requests = guard.take_ui();
        let notices = guard.take_notices();
        drop(guard);
        self.settle(requests, notices);
        result
    }

    /// Run a command of the browser.
    pub fn run(self, command: Command) {
        self.with(|browser| browser.run(command));
    }

    /// What a change asked of the window: its requests, and its notices as
    /// toasts. Not generic, so a toast's button can run a command again.
    fn settle(self, requests: Vec<UiRequest>, notices: Vec<Notice>) {
        self.act(requests);
        for notice in notices {
            let mut toast = Toast::new(notice.text).kind(if notice.error {
                BannerKind::Error
            } else {
                BannerKind::Neutral
            });
            if let Some((label, command)) = notice.action {
                toast = toast.action(label, move |_: ()| self.run(command));
            }
            self.overlays.show_toast(toast);
        }
    }

    /// Take in what the engine has said.
    pub fn pump(self) {
        self.with(Browser::poll);
    }

    fn act(self, requests: Vec<UiRequest>) {
        for request in requests {
            match request {
                UiRequest::FocusAddress => self.address.focus(),
                UiRequest::SearchTabs => self.address.search_tabs(),
                UiRequest::OpenFind => self.find.focus(),
                UiRequest::OpenDownloads => {
                    self.downloads_popover.open();
                    self.browser.clone().write().downloads.mark_seen();
                }
                UiRequest::OpenPageInfo => self.site_info.open(),
                UiRequest::Fullscreen(on) => with_window(move |window| {
                    window.set_fullscreen(
                        on.then_some(freya::winit::window::Fullscreen::Borderless(None)),
                    );
                }),
                UiRequest::NewPrivateWindow => crate::private::PrivateWindow::open(
                    self.browser.peek().private_window_profile(),
                ),
                UiRequest::OpenProfile(id) => crate::profiles::ProfileWindow::open(id),
                UiRequest::ConfirmClearHistory => crate::history::ClearDialog::open(self),
                UiRequest::CloseWindow => self.window.close(),
            }
        }
    }
}
