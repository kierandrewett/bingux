//! What every part of the window holds in common: the browser's state, the
//! handles of the window, and the small states of the chrome itself (what the
//! address bar says, which popover is open).
//!
//! All of it is `Copy`, so a handler takes its own. Commands go through
//! [`Ctx::with`], which changes the browser and then does what only the window
//! can do (focus the address bar, go fullscreen, show a toast).

use std::sync::Arc;

use bingux_geckoview::BrowserBackend;
use bingux_ui::prelude::*;
use bingux_ui::shell::with_window;
use freya::prelude::*;

use crate::model::browser::{Browser, Notice, UiRequest};
use crate::model::shortcuts::Command;
use crate::model::shortcuts::Platform;

/// The engine, comparable by identity so it can be a prop.
#[derive(Clone)]
pub struct Engine(pub Arc<dyn BrowserBackend>);

impl PartialEq for Engine {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// The states of the chrome that are not the browser's: they belong to the
/// window, not to a tab.
#[derive(Clone, Copy, PartialEq)]
pub struct Chrome {
    /// The text of the address bar's field.
    pub address: State<String>,
    /// The person has changed it: it is theirs until they submit or leave.
    pub typing: State<bool>,
    /// Counts the requests to focus the address bar. A new count is a new
    /// field that takes the focus with its text selected.
    pub focus_address: State<u32>,
    /// The suggestion the keys have reached.
    pub highlight: State<Option<usize>>,
    /// Counts the requests to focus the find field.
    pub focus_find: State<u32>,
    pub info_open: State<bool>,
    pub downloads_open: State<bool>,
    /// Where the controls that open popovers are, in the window.
    pub info_anchor: State<Area>,
    pub downloads_anchor: State<Area>,
    pub menu_anchor: State<Area>,
    /// The address field's box, which the suggestions hang from.
    pub field_area: State<Area>,
    /// Where the page area starts in the window: a context menu opens at the
    /// pointer's place in the page plus this.
    pub page_origin: State<(f32, f32)>,
    /// The text of a page's `prompt` or of an authentication, while its dialog
    /// is open.
    pub dialog_text: State<String>,
    pub dialog_password: State<String>,
}

/// Everything a component of the window needs.
#[derive(Clone, Copy, PartialEq)]
pub struct Ctx {
    pub browser: State<Browser>,
    pub chrome: Chrome,
    pub overlays: Overlays,
    pub window: Window,
    pub platform: Platform,
}

impl Ctx {
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
        let chrome = self.chrome;
        for request in requests {
            match request {
                UiRequest::FocusAddress => {
                    let mut typing = chrome.typing;
                    typing.set(false);
                    let mut focus = chrome.focus_address;
                    let next = *focus.peek() + 1;
                    focus.set(next);
                }
                UiRequest::OpenFind => {
                    let mut focus = chrome.focus_find;
                    let next = *focus.peek() + 1;
                    focus.set(next);
                }
                UiRequest::OpenDownloads => {
                    let mut open = chrome.downloads_open;
                    open.set(true);
                    self.browser.clone().write().downloads.mark_seen();
                }
                UiRequest::OpenPageInfo => {
                    let mut open = chrome.info_open;
                    open.set(true);
                }
                UiRequest::OpenMainMenu => {
                    let anchor = *chrome.menu_anchor.peek();
                    self.overlays.show_menu(
                        (anchor.min_x(), anchor.max_y()),
                        crate::ui::menus::main_menu(self),
                    );
                }
                UiRequest::Fullscreen(on) => with_window(move |window| {
                    window.set_fullscreen(on.then_some(freya::winit::window::Fullscreen::Borderless(None)));
                }),
                UiRequest::NewPrivateWindow => crate::ui::private::open(),
                UiRequest::CloseWindow => self.window.close(),
            }
        }
    }
}
