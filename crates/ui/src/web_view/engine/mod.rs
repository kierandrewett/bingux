//! The real backend: [`BrowserBackend`] over the `vivarium` crate (ABI 2.0).
//!
//! | ABI 2.0 | Here |
//! | ------- | ---- |
//! | `Engine` | one [`Runtime`](runtime::Runtime): the engine and the thread that listens to it |
//! | `Context` | the default context for tabs, the private context for private tabs, one per container |
//! | `Session` (class `Web`) | a tab; its id is the [`TabId`] |
//! | `Event` | [`BrowserEvent`](crate::web_view::BrowserEvent), by `events.rs` |
//! | `Request` | a prompt, a pop-up tab or the engine's default, by `requests.rs` |
//! | `Frame` | the tab's [`PageSurface`], by `frames.rs`, without a copy |
//! | `Capabilities`, `feature` | [`Capabilities`] and which environment fields are sent, by `caps.rs` |
//! | `SecurityReport` | [`crate::web_view::report::SecurityReport`] on the settings page |
//! | `EngineFailed` | [`EngineEvent::Failed`] and a `restart` that opens the tabs again |
//!
//! Commands go straight to the session from the caller's thread; the library
//! queues them and none waits for the engine. What the engine does not do is
//! [`Capabilities`] `false` and [`BackendError::Unsupported`]: nothing here
//! pretends.
//!
//! Renaming `Session` to `View` later (`docs/api-shape.md`) touches the
//! words `Session`, `session` and `SessionId` in this module and nothing
//! else: the browser knows tabs.

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

use vivarium as vv;

pub use config::{Config, WebrtcChoice, locale_tag};

use runtime::{Runtime, StartFailure};
use state::{Entry, Shared, lock, tab_of};

use crate::web_view::backend::{BackendError, BrowserBackend, Capabilities, Done, EngineInfo, EngineSettings, EngineState, TabConfig};
use crate::web_view::environment::PageEnvironment;
use crate::web_view::events::{BrowserEvent, EngineEvent};
use crate::web_view::ids::{RequestId, TabId};
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
fn failed(error: &vv::Error, tab: TabId, what: &'static str) -> BackendError {
    match error.status() {
        Some(vv::Status::NoSuchSession | vv::Status::Closed) => BackendError::NoSuchTab(tab),
        Some(vv::Status::EngineFailed) => BackendError::EngineGone,
        Some(vv::Status::Unsupported) => BackendError::Unsupported(what),
        _ => BackendError::Engine(error.to_string()),
    }
}

fn report_status(status: vv::CheckStatus) -> CheckStatus {
    match status {
        vv::CheckStatus::Ok => CheckStatus::Ok,
        vv::CheckStatus::Info => CheckStatus::Info,
        vv::CheckStatus::Warn => CheckStatus::Warn,
        vv::CheckStatus::Fail => CheckStatus::Fail,
        _ => CheckStatus::Unknown,
    }
}

/// The engine's report for the settings page; `None` while it has not
/// produced one (`VV_ERR_NOT_READY`) or cannot be asked.
fn security_report(engine: &vv::Engine) -> Option<SecurityReport> {
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
    (!entries.is_empty()).then(|| SecurityReport::from_entries(report_status(report.overall), report.generation, &entries))
}

impl GeckoBackend {
    /// Starts an engine, or records why it did not.
    fn run(&self) -> Done {
        match Runtime::start(&self.config, &self.shared) {
            Ok(runtime) => {
                let mut state = lock(&self.shared.state);
                state.webrtc_applied = runtime.webrtc_applied;
                state.runtime = Some(runtime);
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
        lock(&self.shared.state).runtime.clone().ok_or(BackendError::EngineGone)
    }

    fn session(&self, tab: TabId) -> Result<Arc<vv::Session>, BackendError> {
        lock(&self.shared.state)
            .live
            .get(&tab)
            .map(|entry| entry.session.clone())
            .ok_or(BackendError::NoSuchTab(tab))
    }

    fn call(&self, tab: TabId, what: &'static str, run: impl FnOnce(&vv::Session) -> vv::Result<()>) -> Done {
        let session = self.session(tab)?;
        run(&session).map_err(|error| failed(&error, tab, what))
    }

    /// The context a tab lives in: the default one, the private one, or the
    /// one of its container.
    fn context(&self, runtime: &Runtime, config: &TabConfig) -> vv::Result<vv::Context> {
        let engine = &runtime.engine;
        if config.container == 0 {
            return Ok(if config.private { engine.private_context() } else { engine.default_context() });
        }
        let key = (config.container, config.private);
        let mut state = lock(&self.shared.state);
        if let Some(context) = state.contexts.get(&key) {
            return Ok(context.clone());
        }
        let context = engine.create_context(&vv::ContextConfig {
            ephemeral: config.private,
            container_id: config.container,
            ..vv::ContextConfig::default()
        })?;
        state.contexts.insert(key, context.clone());
        Ok(context)
    }

    fn session_config(config: &TabConfig, restore: Vec<u8>) -> vv::SessionConfig {
        let mut out = vv::SessionConfig::new(config.width.max(1), config.height.max(1))
            .scale(config.scale)
            .hidden(config.hidden)
            .color_scheme(if config.dark { vv::ColorScheme::Dark } else { vv::ColorScheme::Light })
            .zoom(config.zoom);
        // What a tab had when the engine failed goes to a new one in memory
        // only; a private tab keeps nothing, not even for that.
        out.report_state = !config.private;
        if restore.is_empty() {
            if !config.url.is_empty() {
                out = out.url(config.url.clone());
            }
        } else {
            out.restore_state = restore;
        }
        out
    }

    fn adopt(&self, session: vv::Session) -> TabId {
        let tab = tab_of(session.id());
        lock(&self.shared.state).live.insert(
            tab,
            Entry {
                session: Arc::new(session),
                surface: PageSurface::new(),
                editing: false,
            },
        );
        tab
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
            state.contexts.clear();
            state.runtime.take()
        };
        drop(runtime);
        self.config.forget();
    }
}

impl BrowserBackend for GeckoBackend {
    fn capabilities(&self) -> Capabilities {
        // What the engine announced, not what the browser hopes for.
        lock(&self.shared.state).runtime.as_ref().map_or_else(Capabilities::default, |runtime| runtime.capabilities)
    }

    fn info(&self) -> EngineInfo {
        let (runtime, webrtc) = {
            let state = lock(&self.shared.state);
            (state.runtime.clone(), state.webrtc_applied)
        };
        let settings: EngineSettings = self.config.settings(webrtc);
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
                    vv::EngineState::Ready => EngineState::Ready,
                    vv::EngineState::Starting => EngineState::Starting,
                    _ => EngineState::Failed,
                },
                if info.gecko_version.is_empty() { String::new() } else { format!("Gecko {}", info.gecko_version) },
                info.gecko_build_id,
                info.user_agent,
            ),
            Err(_) => (EngineState::Failed, String::new(), String::new(), String::new()),
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
        // The engine that failed goes first: its pump is joined and every
        // handle of it let go, then a new one starts on the same profile.
        let old = lock(&self.shared.state).runtime.take();
        drop(old);
        self.run()
    }

    fn set_waker(&self, waker: Waker) {
        self.shared.queue.set_waker(waker);
    }

    fn poll_events(&self, out: &mut Vec<BrowserEvent>) {
        self.shared.queue.drain_into(out);
    }

    fn create_tab(&self, config: &TabConfig) -> Result<TabId, BackendError> {
        let runtime = self.runtime()?;
        let restore = config.restore_from.and_then(|old| lock(&self.shared.state).lost.remove(&old)).unwrap_or_default();
        let context = self.context(&runtime, config).map_err(|error| BackendError::Engine(error.to_string()))?;
        let make = |restore: Vec<u8>| runtime.engine.create_session(Self::session_config(config, restore).context(&context));
        let session = match make(restore.clone()) {
            Ok(session) => session,
            // A saved state the engine refuses must not cost the tab: it opens at its address.
            Err(_) if !restore.is_empty() => make(Vec::new()).map_err(|error| BackendError::Engine(error.to_string()))?,
            Err(error) => return Err(BackendError::Engine(error.to_string())),
        };
        Ok(self.adopt(session))
    }

    fn accept_tab(&self, new: TabId, config: &TabConfig) -> Done {
        let runtime = self.runtime()?;
        let request = lock(&self.shared.state).windows.remove(&new).ok_or(BackendError::NoSuchTab(new))?;
        // The session lives in its opener's context and has its opener; the
        // rest of the configuration is the browser's.
        let session = runtime
            .engine
            .accept_new_window(&request, Self::session_config(config, Vec::new()))
            .map_err(|error| failed(&error, new, "opening a window"))?;
        self.adopt(session);
        Ok(())
    }

    fn close_tab(&self, tab: TabId) -> Done {
        let (closed, window) = {
            let mut state = lock(&self.shared.state);
            (state.live.remove(&tab).is_some(), state.windows.remove(&tab))
        };
        if closed {
            return Ok(());
        }
        match window {
            // A pop-up the browser does not take: the engine closes its provisional session.
            Some(request) => self.runtime()?.engine.deny(&request).map_err(|error| failed(&error, tab, "closing a window")),
            None => Err(BackendError::NoSuchTab(tab)),
        }
    }

    fn surface(&self, tab: TabId) -> Option<Arc<PageSurface>> {
        lock(&self.shared.state).live.get(&tab).map(|entry| entry.surface.clone())
    }

    fn load(&self, tab: TabId, url: &str) -> Done {
        self.call(tab, "loading an address", |session| session.load(url))
    }

    fn reload(&self, tab: TabId, bypass_cache: bool) -> Done {
        self.call(tab, "reloading", |session| session.reload(bypass_cache))
    }

    fn stop(&self, tab: TabId) -> Done {
        self.call(tab, "stopping a load", vv::Session::stop)
    }

    fn back(&self, tab: TabId) -> Done {
        self.call(tab, "going back", vv::Session::back)
    }

    fn forward(&self, tab: TabId) -> Done {
        self.call(tab, "going forward", vv::Session::forward)
    }

    fn go_to_history(&self, tab: TabId, index: usize) -> Done {
        self.call(tab, "going to a history entry", |session| session.goto_history_index(index as u32))
    }

    fn set_size(&self, tab: TabId, width: u32, height: u32, scale: f32) -> Done {
        // The frame that answers carries the number this returns (`resize_id`).
        self.call(tab, "resizing", |session| session.resize(width.max(1), height.max(1), scale).map(|_| ()))
    }

    fn set_visible(&self, tab: TabId, visible: bool, focused: bool) -> Done {
        self.call(tab, "showing a tab", |session| session.set_visible(visible, focused))
    }

    fn set_zoom(&self, tab: TabId, zoom: f64) -> Done {
        self.call(tab, "zooming", |session| session.set_zoom(zoom))
    }

    fn set_environment(&self, environment: &PageEnvironment) -> Done {
        let runtime = self.runtime()?;
        let scheme = if environment.dark() { vv::ColorScheme::Dark } else { vv::ColorScheme::Light };
        // The engine-wide call carries every field the engine announces. A
        // session also holds its own colour scheme (it was made with one), and
        // an engine that does not announce `env.color_scheme` takes the scheme
        // only this way, so each live session is told too.
        for entry in lock(&self.shared.state).live.values() {
            if let Err(error) = entry.session.set_color_scheme(scheme)
                && !matches!(error.status(), Some(vv::Status::NoSuchSession | vv::Status::Closed))
            {
                return Err(BackendError::Engine(error.to_string()));
            }
        }
        let wanted = convert::environment(environment, &|field| caps::environment_supported(&runtime.engine, field));
        if wanted.is_empty() {
            return Ok(());
        }
        runtime.engine.set_environment(&wanted).map_err(|error| match error.status() {
            Some(vv::Status::EngineFailed) => BackendError::EngineGone,
            _ => BackendError::Engine(error.to_string()),
        })
    }

    fn send(&self, tab: TabId, input: PageInput) -> Done {
        self.call(tab, "this kind of input", |session| convert::send(session, &input))
    }

    fn set_muted(&self, tab: TabId, muted: bool) -> Done {
        self.call(tab, "muting a tab", |session| session.set_muted(muted))
    }

    /// ABI 2.0 has no call that tells a page its fullscreen ended: the page
    /// asks (`VV_REQUEST_FULLSCREEN`, which this backend does not subscribe
    /// to, so the engine's default refuses it) and the engine reports
    /// (`VV_EVENT_FULLSCREEN`). The window's own fullscreen is the window's.
    fn set_fullscreen(&self, _tab: TabId, _on: bool) -> Done {
        Ok(())
    }

    fn respond(&self, tab: TabId, request: RequestId, response: Response) -> Done {
        let runtime = self.runtime()?;
        let Some(open) = lock(&self.shared.state).requests.remove(&request) else {
            // The engine ended it first (the deadline, a navigation): what it
            // did is the safe default, and there is nothing left to answer.
            return Ok(());
        };
        let answer = match requests::response(&open, response) {
            Ok(answer) => answer,
            Err(reason) => {
                // A page must not wait for an answer that cannot be made.
                let _ = runtime.engine.answer_default(&open);
                return Err(BackendError::Engine(reason.to_string()));
            }
        };
        match runtime.engine.respond(&open, answer) {
            Ok(()) => Ok(()),
            Err(error) if error.is_request_closed() => Ok(()),
            Err(error) => Err(failed(&error, tab, "answering a question")),
        }
    }
}
