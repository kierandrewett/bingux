//! Answering what the page asked: a permission, a dialog, a password, a
//! certificate error, a list, a file, and the context menu.

use std::path::PathBuf;

use bingux_ui::web_view::{RequestId, Response};

use super::Browser;
use crate::prompts::Prompt;

impl Browser {
    /// The prompt of tab `key` that waits first: the one on screen.
    pub fn prompt(&self, key: u64) -> Option<&Prompt> {
        self.tabs.get(key)?.prompts.front()
    }

    /// Question `id` of tab `key`, if it still waits. An answer is for one
    /// question, named by the id the dialog was built with. When the engine
    /// ended that question first, there is nothing to answer, and the answer
    /// must not fall on the next question in the queue.
    fn waiting(&self, key: u64, id: RequestId) -> Option<Prompt> {
        self.tabs.get(key)?.prompts.get(id).cloned()
    }

    /// Take question `id` of tab `key` off its queue.
    fn take_prompt(&mut self, key: u64, id: RequestId) -> Option<Prompt> {
        self.tabs.get_mut(key)?.prompts.take(id)
    }

    /// Send `response` to the engine for question `id` of tab `key`.
    fn respond(&mut self, key: u64, id: RequestId, response: Response) {
        if let Some(session) = self.session_of(key) {
            self.engine.call(&mut self.out, |backend| {
                backend.respond(session, id, response)
            });
        }
    }

    /// The answer to the permission question `id`. `remember` keeps it for
    /// the site.
    pub fn answer_permission(&mut self, key: u64, id: RequestId, allow: bool, remember: bool) {
        let Some(Prompt::Permission(_, request)) = self.waiting(key, id) else {
            return;
        };
        self.take_prompt(key, id);
        if remember {
            self.permissions
                .remember(&request.origin, &request.kinds, allow);
        }
        self.respond(key, id, Response::Permission { allow, remember });
    }

    /// The answer to the `alert`, `confirm`, `prompt` or `beforeunload`
    /// question `id`.
    pub fn answer_dialog(&mut self, key: u64, id: RequestId, accept: bool, text: Option<String>) {
        let Some(Prompt::Dialog(..)) = self.waiting(key, id) else {
            return;
        };
        self.take_prompt(key, id);
        self.respond(key, id, Response::Dialog { accept, text });
    }

    /// A name and a password for the sign-in question `id`, or `None` to cancel.
    pub fn answer_auth(&mut self, key: u64, id: RequestId, credentials: Option<(String, String)>) {
        let Some(Prompt::Auth(..)) = self.waiting(key, id) else {
            return;
        };
        self.take_prompt(key, id);
        self.respond(key, id, Response::Auth { credentials });
    }

    /// The item chosen from the `<select>` list `id`, or `None` to close it.
    pub fn answer_select(&mut self, key: u64, id: RequestId, index: Option<usize>) {
        let Some(Prompt::Select(..)) = self.waiting(key, id) else {
            return;
        };
        self.take_prompt(key, id);
        self.respond(key, id, Response::Select { index });
    }

    /// The files chosen for the `<input type=file>` question `id`; none cancels.
    pub fn answer_picker(&mut self, key: u64, id: RequestId, paths: Vec<PathBuf>) {
        let Some(Prompt::FilePicker(..)) = self.waiting(key, id) else {
            return;
        };
        self.take_prompt(key, id);
        self.respond(key, id, Response::FilePicker { paths });
    }

    /// "Go back" on a certificate error page: no exception, and away from the
    /// page that failed.
    pub fn cert_go_back(&mut self, key: u64) {
        let Some((id, _)) = self.tabs.get_mut(key).and_then(|tab| tab.cert.take()) else {
            return;
        };
        let can_back = self.tabs.get(key).is_some_and(|tab| tab.can_back);
        if let Some(session) = self.session_of(key) {
            self.engine.call(&mut self.out, |backend| {
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
            self.engine.call(&mut self.out, |backend| {
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
            self.engine.call(&mut self.out, |backend| {
                backend.respond(session, request, Response::ContextMenu)
            });
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
