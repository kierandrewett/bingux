//! What the engine says, turned into state: its events, and the questions it asks.

use bingux_ui::prelude::tr;
use bingux_ui::web_view::requests::{DownloadState, Request, RequestKind};
use bingux_ui::web_view::{BrowserEvent, EngineEvent, Response, TabEvent, TabId};

use crate::browser::{Browser, Notice, PendingMenu, UiRequest};
use crate::commands::Command;
use crate::prompts::Prompt;
use crate::zoom::Zoom;

impl Browser {
    /// Take in what the engine and the profile's store have said since the
    /// last call.
    pub fn poll(&mut self) {
        let mut events = Vec::new();
        self.engine.backend().poll_events(&mut events);
        for event in events {
            self.apply(event);
        }
        self.take_store_events();
    }

    /// Take in one event.
    pub fn apply(&mut self, event: BrowserEvent) {
        match event {
            BrowserEvent::Engine(EngineEvent::Ready) => self.engine_ready(),
            BrowserEvent::Engine(EngineEvent::Failed {
                message,
                code,
                cause,
                restartable,
            }) => {
                self.engine_failed(message, code, cause, restartable);
            }
            // The settings page reads the report again when it is drawn; that
            // it is drawn again is all this has to cause.
            BrowserEvent::Engine(EngineEvent::SecurityChanged) => {}
            // The visits are gone from the pages that are open; only a
            // failure is news.
            BrowserEvent::Engine(EngineEvent::HistoryCleared(Ok(()))) => {}
            BrowserEvent::Engine(EngineEvent::HistoryCleared(Err(reason))) => {
                self.out.notice(Notice {
                    error: true,
                    text: tr!("browser-history-engine-clear-failed", reason = reason),
                    action: None,
                });
            }
            BrowserEvent::Tab { tab, event } => self.apply_tab(tab, event),
            BrowserEvent::Request { tab, request } => self.apply_request(tab, request),
            BrowserEvent::RequestClosed { tab, request } => self.request_closed(tab, request),
            BrowserEvent::Download(info) => {
                // A private download belongs to a private window's list and
                // to no other: a normal window never lists it, nor says
                // its name in a toast.
                if info.private && !self.options.private {
                    return;
                }
                let started = !self
                    .downloads
                    .list()
                    .iter()
                    .any(|known| known.id == info.id);
                let finished = info.state == DownloadState::Finished;
                let name = info.filename.clone();
                self.downloads.update(info);
                if started {
                    self.out.notice(Notice {
                        error: false,
                        text: tr!(
                            "browser-downloads-started-notification",
                            name = name.as_str()
                        ),
                        action: Some((tr!("browser-downloads-show-button"), Command::Downloads)),
                    });
                } else if finished {
                    self.out.notice(Notice {
                        error: false,
                        text: tr!(
                            "browser-downloads-finished-notification",
                            name = name.as_str()
                        ),
                        action: Some((tr!("browser-downloads-show-button"), Command::Downloads)),
                    });
                }
            }
        }
    }

    fn apply_tab(&mut self, session: TabId, event: TabEvent) {
        // A page the engine opened for `window.open` is not a tab yet.
        if let TabEvent::NewTab {
            new,
            url,
            name: _,
            no_opener: _,
            window,
        } = &event
        {
            self.accept_new_tab(session, *new, url, window);
            return;
        }
        let Some(key) = self.key_of(session) else {
            return;
        };
        let private = self.options.private;
        match event {
            TabEvent::Ready => {
                if let Some(tab) = self.tabs.get_mut(key) {
                    tab.ready = true;
                }
            }
            TabEvent::Closed { error } => {
                // The session is gone already: no call back to it.
                if let Some(tab) = self.tabs.get_mut(key) {
                    tab.session = None;
                }
                if let Some(error) = error {
                    self.out.notice(Notice {
                        error: true,
                        text: tr!(
                            "browser-tabs-closed-by-engine-notification",
                            error = error.to_string()
                        ),
                        action: None,
                    });
                }
                self.close_tab(key);
            }
            TabEvent::Location {
                url,
                can_back,
                can_forward,
            } => {
                if let Some(tab) = self.tabs.get_mut(key) {
                    tab.url = url;
                    tab.can_back = can_back;
                    tab.can_forward = can_forward;
                    tab.hover = None;
                }
            }
            TabEvent::Title(title) => {
                let url = self
                    .tabs
                    .get(key)
                    .map(|tab| tab.url.clone())
                    .unwrap_or_default();
                if let Some(tab) = self.tabs.get_mut(key) {
                    tab.title = title.clone();
                }
                if !private && let Some(store) = &self.store {
                    store.set_title(&url, &title);
                }
            }
            TabEvent::Load {
                started,
                loading,
                progress,
                error,
            } => {
                let Some(tab) = self.tabs.get_mut(key) else {
                    return;
                };
                if started {
                    tab.gone = None;
                    tab.cert = None;
                    tab.find.clear_result();
                    tab.hover = None;
                    tab.audio.playing = false;
                }
                tab.load.loading = loading;
                tab.load.progress = progress.clamp(0.0, 1.0);
                tab.load.error = error;
                let finished_page = !loading && tab.load.error.is_none() && !tab.is_blank();
                let (url, title) = (tab.url.clone(), tab.title.clone());
                if finished_page
                    && !private
                    && let Some(store) = &self.store
                {
                    store.record_visit(&url, &title);
                }
            }
            TabEvent::Security(security) => {
                if let Some(tab) = self.tabs.get_mut(key) {
                    tab.security = security;
                }
            }
            TabEvent::Favicon(url) => {
                if let Some(tab) = self.tabs.get_mut(key) {
                    tab.favicon = Some(url);
                }
            }
            TabEvent::History { index, entries } => {
                if let Some(tab) = self.tabs.get_mut(key) {
                    tab.history_index = index;
                    tab.history = entries;
                }
            }
            TabEvent::Printed(saved) => {
                let notice = match saved {
                    Ok(path) => Notice {
                        error: false,
                        text: tr!("browser-print-saved", path = path.display().to_string()),
                        action: None,
                    },
                    Err(reason) => Notice {
                        error: true,
                        text: tr!("browser-print-failed", reason = reason),
                        action: None,
                    },
                };
                self.out.notice(notice);
            }
            TabEvent::Fullscreen(entering) => {
                if let Some(tab) = self.tabs.get_mut(key) {
                    tab.fullscreen = entering;
                }
                if key == self.tabs.active_key() && self.fullscreen != entering {
                    self.fullscreen = entering;
                    self.out.request(UiRequest::Fullscreen(entering));
                }
            }
            TabEvent::ProcessGone(reason) => {
                if let Some(tab) = self.tabs.get_mut(key) {
                    tab.gone = Some(reason);
                    tab.load.loading = false;
                    tab.audio.playing = false;
                    tab.hover = None;
                    // Its questions can no longer be answered.
                    tab.prompts.clear();
                }
                if self.menu.as_ref().is_some_and(|menu| menu.tab == key) {
                    self.menu = None;
                }
            }
            TabEvent::NewTab { .. } => {}
            TabEvent::CloseRequested => self.close_tab(key),
            TabEvent::Zoom(zoom) => {
                if let Some(tab) = self.tabs.get_mut(key) {
                    tab.zoom = Zoom::reported(zoom);
                }
            }
            TabEvent::HoverLink(url) => {
                if let Some(tab) = self.tabs.get_mut(key) {
                    tab.hover = url;
                }
            }
            TabEvent::Audio(audio) => {
                if let Some(tab) = self.tabs.get_mut(key) {
                    tab.audio = audio;
                }
            }
            TabEvent::FindResult { current, total } => {
                if let Some(tab) = self.tabs.get_mut(key) {
                    tab.find.on_result(current, total);
                }
            }
        }
    }

    fn apply_request(&mut self, session: TabId, request: Request) {
        let Request { id, kind } = request;
        let Some(key) = self.key_of(session) else {
            // The tab is gone: tell the engine so it does not wait.
            self.refuse(session, id, &kind);
            return;
        };
        match kind {
            RequestKind::Permission(permission) => {
                match self
                    .permissions
                    .decided(&permission.origin, &permission.kinds)
                {
                    Some(allow) => {
                        self.engine.call(&mut self.out, |backend| {
                            backend.respond(
                                session,
                                id,
                                Response::Permission {
                                    allow,
                                    remember: false,
                                },
                            )
                        });
                    }
                    None => self.queue_prompt(key, Prompt::Permission(id, permission)),
                }
            }
            RequestKind::Dialog(dialog) => self.queue_prompt(key, Prompt::Dialog(id, dialog)),
            RequestKind::Auth(auth) => self.queue_prompt(key, Prompt::Auth(id, auth)),
            RequestKind::Select(select) => self.queue_prompt(key, Prompt::Select(id, select)),
            RequestKind::FilePicker(picker) => {
                self.queue_prompt(key, Prompt::FilePicker(id, picker))
            }
            RequestKind::CertError(error) => {
                if let Some(tab) = self.tabs.get_mut(key) {
                    tab.cert = Some((id, error));
                    tab.load.loading = false;
                }
            }
            RequestKind::ContextMenu(target) => {
                if key == self.tabs.active_key() {
                    // A menu already open is replaced: the engine is told it is closed.
                    self.dismiss_menu();
                    self.menu = Some(PendingMenu {
                        tab: key,
                        request: Some(id),
                        target,
                    });
                } else {
                    self.engine.call(&mut self.out, |backend| {
                        backend.respond(session, id, Response::ContextMenu)
                    });
                }
            }
        }
    }

    fn queue_prompt(&mut self, key: u64, prompt: Prompt) {
        if let Some(tab) = self.tabs.get_mut(key) {
            tab.prompts.push_back(prompt);
        }
    }

    /// A request for a tab that is gone gets the answer that does the least.
    fn refuse(&mut self, session: TabId, id: bingux_ui::web_view::RequestId, kind: &RequestKind) {
        let response = match kind {
            RequestKind::Permission(_) => Response::Permission {
                allow: false,
                remember: false,
            },
            RequestKind::Dialog(_) => Response::Dialog {
                accept: false,
                text: None,
            },
            RequestKind::Auth(_) => Response::Auth { credentials: None },
            RequestKind::CertError(_) => Response::CertError { proceed: false },
            RequestKind::ContextMenu(_) => Response::ContextMenu,
            RequestKind::Select(_) => Response::Select { index: None },
            RequestKind::FilePicker(_) => Response::FilePicker { paths: Vec::new() },
        };
        self.engine.call(&mut self.out, |backend| {
            backend.respond(session, id, response)
        });
    }
}
