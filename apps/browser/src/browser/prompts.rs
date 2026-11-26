//! Answering what the page asked: a permission, a dialog, a password, a
//! certificate error, a list, a file, and the context menu.

use std::path::PathBuf;

use bingux_ui::web_view::Response;

use super::Browser;
use crate::prompts::Prompt;

impl Browser {
    /// The prompt of tab `key` that waits first.
    pub fn prompt(&self, key: u64) -> Option<&Prompt> {
        self.tabs.get(key)?.prompts.front()
    }

    /// Take the first prompt of tab `key` off its queue.
    fn take_prompt(&mut self, key: u64) -> Option<Prompt> {
        self.tabs.get_mut(key)?.prompts.pop_front()
    }

    /// The answer to a permission prompt. `remember` keeps it for the site.
    pub fn answer_permission(&mut self, key: u64, allow: bool, remember: bool) {
        let Some(Prompt::Permission(id, request)) = self.prompt(key).cloned() else {
            return;
        };
        self.take_prompt(key);
        if remember {
            self.permissions
                .remember(&request.origin, &request.kinds, allow);
        }
        if let Some(session) = self.session_of(key) {
            self.engine_call(|backend| {
                backend.respond(session, id, Response::Permission { allow, remember })
            });
        }
    }

    /// The answer to `alert`, `confirm`, `prompt` or `beforeunload`.
    pub fn answer_dialog(&mut self, key: u64, accept: bool, text: Option<String>) {
        let Some(Prompt::Dialog(id, _)) = self.prompt(key).cloned() else {
            return;
        };
        self.take_prompt(key);
        if let Some(session) = self.session_of(key) {
            self.engine_call(|backend| {
                backend.respond(session, id, Response::Dialog { accept, text })
            });
        }
    }

    /// A name and a password, or `None` to cancel.
    pub fn answer_auth(&mut self, key: u64, credentials: Option<(String, String)>) {
        let Some(Prompt::Auth(id, _)) = self.prompt(key).cloned() else {
            return;
        };
        self.take_prompt(key);
        if let Some(session) = self.session_of(key) {
            self.engine_call(|backend| {
                backend.respond(session, id, Response::Auth { credentials })
            });
        }
    }

    /// The item chosen from a `<select>` list, or `None` to close it.
    pub fn answer_select(&mut self, key: u64, index: Option<usize>) {
        let Some(Prompt::Select(id, _)) = self.prompt(key).cloned() else {
            return;
        };
        self.take_prompt(key);
        if let Some(session) = self.session_of(key) {
            self.engine_call(|backend| backend.respond(session, id, Response::Select { index }));
        }
    }

    /// The files chosen for `<input type=file>`; none cancels.
    pub fn answer_picker(&mut self, key: u64, paths: Vec<PathBuf>) {
        let Some(Prompt::FilePicker(id, _)) = self.prompt(key).cloned() else {
            return;
        };
        self.take_prompt(key);
        if let Some(session) = self.session_of(key) {
            self.engine_call(|backend| {
                backend.respond(session, id, Response::FilePicker { paths })
            });
        }
    }

    /// "Go back" on a certificate error page: no exception, and away from the
    /// page that failed.
    pub fn cert_go_back(&mut self, key: u64) {
        let Some((id, _)) = self.tabs.get_mut(key).and_then(|tab| tab.cert.take()) else {
            return;
        };
        let can_back = self.tabs.get(key).is_some_and(|tab| tab.can_back);
        if let Some(session) = self.session_of(key) {
            self.engine_call(|backend| {
                backend.respond(session, id, Response::CertError { proceed: false })
            });
        }
        if can_back {
            self.back(key);
        } else {
            self.go(key, "");
        }
    }

    /// "Proceed" past a certificate error the person may override. The page
    /// loads and the connection is marked as not secure.
    pub fn cert_proceed(&mut self, key: u64) {
        let overridable = self
            .tabs
            .get(key)
            .and_then(|tab| tab.cert.as_ref())
            .is_some_and(|(_, error)| error.overridable);
        if !overridable {
            return;
        }
        let Some((id, _)) = self.tabs.get_mut(key).and_then(|tab| tab.cert.take()) else {
            return;
        };
        if let Some(tab) = self.tabs.get_mut(key) {
            tab.load.loading = true;
            tab.load.error = None;
        }
        if let Some(session) = self.session_of(key) {
            self.engine_call(|backend| {
                backend.respond(session, id, Response::CertError { proceed: true })
            });
        }
    }

    /// The context menu is closed, with an item taken or not. The engine is
    /// told, so it stops waiting.
    pub fn dismiss_menu(&mut self) {
        let Some(menu) = self.menu.take() else {
            return;
        };
        if let (Some(session), Some(request)) = (self.session_of(menu.tab), menu.request) {
            self.engine_call(|backend| backend.respond(session, request, Response::ContextMenu));
        }
    }
}

impl Browser {
    /// The page's menu, opened by the browser at `position` (in the page's
    /// logical pixels) for an engine that does not say what is there: Back,
    /// Forward, Reload, Select all, the page's source.
    pub fn open_local_menu(&mut self, key: u64, position: (f32, f32)) {
        if self.tabs.get(key).is_none() {
            return;
        }
        self.dismiss_menu();
        self.menu = Some(super::PendingMenu {
            tab: key,
            request: None,
            target: bingux_ui::web_view::requests::ContextTarget {
                position,
                ..Default::default()
            },
        });
    }
}
