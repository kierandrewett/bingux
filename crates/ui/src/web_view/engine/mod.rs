//! The real backend: [`BrowserBackend`] over the `vivarium` crate (ABI 2.0).
//!
//! | ABI 2.0 | Here |
//! | ------- | ---- |
//! | `Engine` | one [`Runtime`](runtime::Runtime): the engine, its dispatcher and what it accepted |
//! | Contextual identity | the default one for tabs, the private one for private tabs, one per container |
//! | `Session` | a tab; its id is the [`TabId`] |
//! | Events | delegate methods of `events::Host`, each a [`BrowserEvent`](crate::web_view::BrowserEvent) |
//! | Requests (`Ask` and `Responder`) | a control [`Request`](crate::web_view::Request) answered with a [`Response`], a pop-up tab, or the engine's default, by `requests.rs` |
//! | `Frame` | the tab's [`PageSurface`], by `frames.rs` |
//! | Request subscriptions | [`Capabilities`], by `caps.rs` |
//! | `SecurityReport` | [`crate::web_view::report::SecurityReport`] on the settings page |
//! | `EngineFailed` | [`EngineEvent::Failed`] and a `restart` that opens the tabs again |
//!
//! Commands go straight to the session from the caller's thread; the library
//! queues them and none waits for the engine. Events come back through the
//! engine's wake hook and the crate's dispatcher on the UI thread
//! (`runtime.rs`). What the engine does not do is [`Capabilities`] `false`
//! and [`BackendError::Unsupported`]: nothing here pretends.

mod caps;
mod config;
mod convert;
mod events;
mod frames;
mod requests;
mod runtime;
mod state;

use std::sync::Arc;
use std::task::Waker;

use vivarium::contextual_identities::{ContextualIdentity, ContextualIdentityConfig};
use vivarium::downloads::{DownloadAction, DownloadId as EngineDownloadId};
use vivarium::engine::{Engine, EngineState as State};
use vivarium::environment::ColorScheme;
use vivarium::error::{Error, Result, Status};
use vivarium::navigation::{CachePolicy, NewWindowReply};
use vivarium::operation::OpOptions;
use vivarium::security::CheckStatus as Check;
use vivarium::session::{Session, SessionConfig};

pub use config::{Config, EngineFolders, WebrtcChoice, locale_tag};

use requests::Unanswered;
use runtime::{Runtime, StartFailure};
use state::{Entry, Shared, lock, tab_of};

use crate::web_view::backend::{
    BackendError, BrowserBackend, Capabilities, Done, EngineInfo, EngineState, FindQuery, TabConfig,
};
use crate::web_view::environment::PageEnvironment;
use crate::web_view::events::{BrowserEvent, EngineEvent};
use crate::web_view::ids::{DownloadId, RequestId, TabId};
use crate::web_view::input::PageInput;
use crate::web_view::report::{CheckStatus, ReportEntry, SecurityReport};
use crate::web_view::requests::Response;
use crate::web_view::surface::PageSurface;

/// The engine, behind [`BrowserBackend`].
pub struct GeckoBackend {
    config: Config,
    shared: Arc<Shared>,
}

/// Start the engine. Waits for the engine process to accept its configuration
/// (a second or less), not for Gecko: `EngineEvent::Ready` says when pages
/// run.
///
/// An engine that does not start is not an error of this call: the backend is
/// returned without an engine and its first event is `EngineEvent::Failed`
/// with the cause (a profile that is not safe, a sandbox the engine cannot
/// reach, a library that is not there), so the window can say so and offer
/// `restart`.
pub fn start(config: Config) -> Arc<dyn BrowserBackend> {
    let backend = GeckoBackend {
        config,
        shared: Shared::new(),
    };
    let _ = backend.run();
    Arc::new(backend)
}

/// A start that failed, as the event the UI shows.
fn failed_start(failure: &StartFailure) -> BrowserEvent {
    BrowserEvent::Engine(EngineEvent::Failed {
        message: failure.message.clone(),
        code: 0,
        cause: failure.cause,
        // The engine did not run: another try is safe, and the person may have fixed the cause.
        restartable: true,
    })
}

/// An engine error as the UI says it.
fn failed(error: &Error, tab: TabId, what: &'static str) -> BackendError {
    match error.status() {
        Some(Status::NoSuchSession | Status::Closed) => BackendError::NoSuchTab(tab),
        Some(Status::EngineFailed) => BackendError::EngineGone,
        Some(Status::Unsupported) => BackendError::Unsupported(what),
        _ => BackendError::Engine(error.to_string()),
    }
}

fn report_status(status: Check) -> CheckStatus {
    match status {
        Check::Ok => CheckStatus::Ok,
        Check::Info => CheckStatus::Info,
        Check::Warn => CheckStatus::Warn,
        Check::Fail => CheckStatus::Fail,
        _ => CheckStatus::Unknown,
    }
}

/// The engine's report for the settings page; `None` while it has not
/// produced one (`VV_ERR_NOT_READY`) or cannot be asked.
fn security_report(engine: &Engine) -> Option<SecurityReport> {
    let report = engine.security_report().ok()?;
    let entries: Vec<ReportEntry> = report
        .entries
        .iter()
        .map(|entry| ReportEntry {
            key: entry.key.clone(),
            value: entry.value.clone(),
            status: report_status(entry.status),
            required: entry.required,
            pid: entry.pid,
        })
        .collect();
    // A report with no entry is the engine's "nothing yet", not an empty healthy one.
    (!entries.is_empty()).then(|| {
        SecurityReport::from_entries(report_status(report.overall), report.generation, &entries)
    })
}

impl GeckoBackend {
    /// Starts an engine, or records why it did not.
    fn run(&self) -> Done {
        match Runtime::start(&self.config, &self.shared) {
            Ok(runtime) => {
                lock(&self.shared.state).runtime = Some(runtime);
                // Whatever the engine queued before the UI listens is dispatched at its first poll.
                self.shared.queue.wake();
                Ok(())
            }
            Err(failure) => {
                lock(&self.shared.state).runtime = None;
                self.shared.push(failed_start(&failure));
                Err(BackendError::Engine(failure.message))
            }
        }
    }

    fn runtime(&self) -> Result<Arc<Runtime>, BackendError> {
        lock(&self.shared.state)
            .runtime
            .clone()
            .ok_or(BackendError::EngineGone)
    }

    fn session(&self, tab: TabId) -> Result<Arc<Session>, BackendError> {
        lock(&self.shared.state)
            .live
            .get(&tab)
            .map(|entry| entry.session.clone())
            .ok_or(BackendError::NoSuchTab(tab))
    }

    fn call(
        &self,
        tab: TabId,
        what: &'static str,
        run: impl FnOnce(&Session) -> Result<()>,
    ) -> Done {
        let session = self.session(tab)?;
        run(&session).map_err(|error| failed(&error, tab, what))
    }

    /// The contextual identity a tab lives in: the default one, the private
    /// one, or the one of its container.
    fn identity(&self, runtime: &Runtime, config: &TabConfig) -> Result<ContextualIdentity> {
        let engine = &runtime.engine;
        if config.container == 0 {
            return Ok(if config.private {
                engine.private_contextual_identity()
            } else {
                engine.default_contextual_identity()
            });
        }
        let key = (config.container, config.private);
        let mut state = lock(&self.shared.state);
        if let Some(identity) = state.identities.get(&key) {
            return Ok(identity.clone());
        }
        let identity = engine.create_contextual_identity(&ContextualIdentityConfig {
            private_browsing: config.private,
            user_context_id: config.container,
            ..ContextualIdentityConfig::default()
        })?;
        state.identities.insert(key, identity.clone());
        Ok(identity)
    }

    fn session_config(config: &TabConfig) -> SessionConfig {
        let out = SessionConfig::new(config.width.max(1), config.height.max(1))
            .scale(config.scale)
            .hidden(config.hidden)
            .color_scheme(if config.dark {
                ColorScheme::Dark
            } else {
                ColorScheme::Light
            })
            .zoom(config.zoom);
        if config.url.is_empty() {
            out
        } else {
            out.url(config.url.clone())
        }
    }

    fn adopt(&self, session: Session) -> TabId {
        let tab = tab_of(session.id());
        self.learn_find(&session);
        lock(&self.shared.state).live.insert(
            tab,
            Entry {
                session: Arc::new(session),
                surface: PageSurface::new(),
            },
        );
        tab
    }

    /// Asks the first session whether the engine has find (a stop with no
    /// search does nothing, so the question changes nothing). An answer that
    /// says nothing of the call leaves it open for the next session.
    fn learn_find(&self, session: &Session) {
        let Some(runtime) = lock(&self.shared.state).runtime.clone() else {
            return;
        };
        if runtime.find.get().is_some() {
            return;
        }
        if let Some(taken) = caps::taken(&session.find_stop(false)) {
            let _ = runtime.find.set(taken);
        }
    }

    /// A call on the engine itself, with an engine's error as the UI says it.
    fn control_download(&self, download: DownloadId, action: DownloadAction) -> Done {
        let runtime = self.runtime()?;
        runtime
            .engine
            .control_download(EngineDownloadId::from_raw(download.0), action)
            .map_err(|error| match error.status() {
                Some(Status::EngineFailed) => BackendError::EngineGone,
                Some(Status::Unsupported) => BackendError::Unsupported("downloads"),
                _ => BackendError::Engine(error.to_string()),
            })
    }
}

impl Drop for GeckoBackend {
    fn drop(&mut self) {
        // Sessions go first, then the frames they made, then the engine.
        let runtime = {
            let mut state = lock(&self.shared.state);
            state.live.clear();
            state.windows.clear();
            state.requests.clear();
            state.identities.clear();
            state.runtime.take()
        };
        drop(runtime);
        self.config.forget();
    }
}

impl BrowserBackend for GeckoBackend {
    fn capabilities(&self) -> Capabilities {
        // What the engine accepted, not what the browser hopes for.
        lock(&self.shared.state)
            .runtime
            .as_ref()
            .map_or_else(Capabilities::default, |runtime| Capabilities {
                // Learned from the first session (`learn_find`).
                find: runtime.find.get().copied().unwrap_or(false),
                ..runtime.capabilities
            })
    }

    fn info(&self) -> EngineInfo {
        let runtime = lock(&self.shared.state).runtime.clone();
        let settings = self.config.settings();
        let Some(runtime) = runtime else {
            return EngineInfo {
                state: EngineState::Failed,
                version: String::new(),
                build_id: String::new(),
                user_agent: String::new(),
                settings,
                security: None,
            };
        };
        let (state, version, build_id, user_agent) = match runtime.engine.info() {
            Ok(info) => (
                match info.state {
                    State::Ready => EngineState::Ready,
                    State::Starting => EngineState::Starting,
                    _ => EngineState::Failed,
                },
                if info.gecko_version.is_empty() {
                    String::new()
                } else {
                    format!("Gecko {}", info.gecko_version)
                },
                info.gecko_build_id,
                info.user_agent,
            ),
            Err(_) => (
                EngineState::Failed,
                String::new(),
                String::new(),
                String::new(),
            ),
        };
        EngineInfo {
            state,
            version,
            build_id,
            user_agent,
            settings,
            security: security_report(&runtime.engine),
        }
    }

    fn restart(&self) -> Done {
        // The engine that failed goes first: every handle of it is let go,
        // then a new one starts on the same profile.
        let old = lock(&self.shared.state).runtime.take();
        drop(old);
        self.run()
    }

    fn set_waker(&self, waker: Waker) {
        self.shared.queue.set_waker(waker);
        // The engine may have queued events before anybody listened.
        self.shared.queue.wake();
    }

    fn poll_events(&self, out: &mut Vec<BrowserEvent>) {
        let runtime = lock(&self.shared.state).runtime.clone();
        if let Some(runtime) = runtime {
            runtime.dispatch(&self.shared);
        }
        self.shared.queue.drain_into(out);
    }

    /// ABI 2.0 hands the host no saved session state, so a tab with
    /// `restore_from` opens at its address like any other.
    fn create_tab(&self, config: &TabConfig) -> Result<TabId, BackendError> {
        let runtime = self.runtime()?;
        let identity = self
            .identity(&runtime, config)
            .map_err(|error| BackendError::Engine(error.to_string()))?;
        let session = runtime
            .engine
            .create_session(Self::session_config(config).contextual_identity(&identity))
            .map_err(|error| BackendError::Engine(error.to_string()))?;
        Ok(self.adopt(session))
    }

    fn accept_tab(&self, new: TabId, config: &TabConfig) -> Done {
        let reply = lock(&self.shared.state)
            .windows
            .remove(&new)
            .ok_or(BackendError::NoSuchTab(new))?;
        // The session lives in its opener's contextual identity and has its
        // opener; the rest of the configuration is the browser's.
        let session = reply
            .adopt(Self::session_config(config))
            .map_err(|error| failed(&error, new, "opening a window"))?;
        self.adopt(session);
        Ok(())
    }

    fn close_tab(&self, tab: TabId) -> Done {
        let (closed, window) = {
            let mut state = lock(&self.shared.state);
            (
                state.live.remove(&tab).is_some(),
                state.windows.remove(&tab),
            )
        };
        if closed {
            return Ok(());
        }
        match window {
            // A pop-up the browser does not take: the engine closes its provisional session.
            Some(reply) => match reply.answer(NewWindowReply::Refuse) {
                Err(error) if !error.is_request_closed() => {
                    Err(failed(&error, tab, "closing a window"))
                }
                _ => Ok(()),
            },
            None => Err(BackendError::NoSuchTab(tab)),
        }
    }

    fn surface(&self, tab: TabId) -> Option<Arc<PageSurface>> {
        lock(&self.shared.state)
            .live
            .get(&tab)
            .map(|entry| entry.surface.clone())
    }

    fn load(&self, tab: TabId, url: &str) -> Done {
        self.call(tab, "loading an address", |session| session.load(url))
    }

    fn reload(&self, tab: TabId, bypass_cache: bool) -> Done {
        let cache = if bypass_cache {
            CachePolicy::Bypass
        } else {
            CachePolicy::Normal
        };
        self.call(tab, "reloading", |session| session.reload(cache))
    }

    fn stop(&self, tab: TabId) -> Done {
        self.call(tab, "stopping a load", Session::stop)
    }

    fn back(&self, tab: TabId) -> Done {
        self.call(tab, "going back", Session::go_back)
    }

    fn forward(&self, tab: TabId) -> Done {
        self.call(tab, "going forward", Session::go_forward)
    }

    fn go_to_history(&self, tab: TabId, index: usize) -> Done {
        self.call(tab, "going to a history entry", |session| {
            session.goto_history_index(index as u32)
        })
    }

    fn set_size(&self, tab: TabId, width: u32, height: u32, scale: f32) -> Done {
        // The frame that answers carries the number this returns (`resize_id`).
        self.call(tab, "resizing", |session| {
            session
                .resize(width.max(1), height.max(1), scale)
                .map(|_| ())
        })
    }

    fn set_visible(&self, tab: TabId, visible: bool, focused: bool) -> Done {
        self.call(tab, "showing a tab", |session| {
            session.set_window_state(convert::window_state(visible, focused))
        })
    }

    fn set_zoom(&self, tab: TabId, zoom: f64) -> Done {
        self.call(tab, "zooming", |session| session.set_zoom(zoom))
    }

    fn set_environment(&self, environment: &PageEnvironment) -> Done {
        let runtime = self.runtime()?;
        let scheme = if environment.dark() {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        };
        // A session holds its own colour scheme (it was made with one), and
        // an engine that does not take the engine-wide environment takes the
        // scheme only this way, so each live session is told too.
        for entry in lock(&self.shared.state).live.values() {
            if let Err(error) = entry.session.set_color_scheme(scheme)
                && !error.is_session_gone()
            {
                return Err(BackendError::Engine(error.to_string()));
            }
        }
        match runtime
            .engine
            .set_environment(&convert::environment(environment))
        {
            Ok(()) => Ok(()),
            // The engine of today announces no environment: the sessions have their scheme.
            Err(error) if error.is_unsupported() => Ok(()),
            Err(error) if error.is_engine_failed() => Err(BackendError::EngineGone),
            Err(error) => Err(BackendError::Engine(error.to_string())),
        }
    }

    fn send(&self, tab: TabId, input: PageInput) -> Done {
        self.call(tab, "this kind of input", |session| {
            convert::send(session, &input)
        })
    }

    /// A new search or a step in the one that runs. The result settles
    /// later as an operation, which carries the tab as its tag
    /// (`events::on_operation_result`); the live counts arrive as events.
    fn find(&self, tab: TabId, query: &FindQuery) -> Done {
        self.call(tab, "find in page", |session| {
            session
                .find(
                    &requests::find_options(query),
                    OpOptions::tagged(u64::from(tab.0)),
                )
                .map(|_| ())
        })
    }

    fn find_stop(&self, tab: TabId) -> Done {
        self.call(tab, "find in page", |session| session.find_stop(false))
    }

    fn pause_download(&self, download: DownloadId) -> Done {
        self.control_download(download, DownloadAction::Pause)
    }

    fn resume_download(&self, download: DownloadId) -> Done {
        self.control_download(download, DownloadAction::Resume)
    }

    fn cancel_download(&self, download: DownloadId) -> Done {
        self.control_download(download, DownloadAction::Cancel)
    }

    fn remove_download(&self, download: DownloadId) -> Done {
        self.control_download(download, DownloadAction::Remove)
    }

    /// ABI 2.0 has no call that tells a page its fullscreen ended: the page
    /// asks (`VV_REQUEST_FULLSCREEN`, which this backend does not subscribe
    /// to, so the engine's default refuses it) and the engine reports
    /// (`ContentDelegate::on_fullscreen_changed`). The window's own fullscreen
    /// is the window's.
    fn set_fullscreen(&self, _tab: TabId, _on: bool) -> Done {
        Ok(())
    }

    fn respond(&self, tab: TabId, request: RequestId, response: Response) -> Done {
        let Some(asked) = lock(&self.shared.state).requests.remove(&request) else {
            // The engine ended it first (the deadline, a navigation): what it
            // did is the safe default, and there is nothing left to answer.
            return Ok(());
        };
        match requests::answer(asked.open, response) {
            Ok(()) => Ok(()),
            // A page must not wait for an answer that cannot be made: the
            // responder is gone, and with it the engine's default was sent.
            Err(Unanswered::Mismatch) => Err(BackendError::Engine(
                "the answer does not belong to the question".to_string(),
            )),
            Err(Unanswered::Engine(error)) if error.is_request_closed() => Ok(()),
            Err(Unanswered::Engine(error)) => Err(failed(&error, tab, "answering a question")),
        }
    }
}
