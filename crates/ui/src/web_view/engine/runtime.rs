//! One engine, and the pump that hands its events to the backend's delegate.
//!
//! [`Runtime::start`] starts the engine with a wake hook and subscribes to
//! the requests the browser answers that the engine accepts. The crate starts
//! no thread: the engine's library calls the wake hook on its own thread when
//! its event queue goes from empty to not empty, and the hook only wakes the
//! UI's task (the waker the control gave `set_waker`). That task calls
//! `poll_events`, which runs [`Runtime::dispatch`] on the UI thread: the
//! crate's `Dispatcher` takes at most 256 events, never blocks, and calls one
//! [`Host`] method per event. The UI never waits for the engine and never
//! polls a timer.
//!
//! A restart is a new `Runtime`: the failed one is dropped first, which lets
//! go of every handle of the engine that died.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use vivarium::delegate::Dispatcher;
use vivarium::downloads::{DownloadAction, DownloadId};
use vivarium::engine::Engine;
use vivarium::error::{Error, Status};

use super::caps;
use super::config::Config;
use super::events::Host;
use super::state::Shared;
use crate::web_view::backend::Capabilities;
use crate::web_view::events::{BrowserEvent, EngineEvent, FailureCause};

/// Why an engine did not start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StartFailure {
    pub message: String,
    pub cause: FailureCause,
}

pub(super) struct Runtime {
    pub engine: Engine,
    dispatcher: Dispatcher,
    /// What the browser's controls may offer because of it.
    pub capabilities: Capabilities,
    /// Whether the engine took `Session::find`, once a session has asked it
    /// (`caps::taken`); unset until then.
    pub find: OnceLock<bool>,
    /// Set to `false` when the engine said it has no clear-data call (an
    /// older engine build); unset until a clear has been tried.
    pub clear_history: OnceLock<bool>,
    downloads: PathBuf,
    /// The engine failed: its queue has nothing more to say.
    failed: AtomicBool,
}

/// An engine error that stopped the start, as the browser says it. A profile
/// that is not safe is its own case: the page names the folder and the rule.
pub(super) fn start_failure(error: &Error) -> StartFailure {
    let cause = if error.is_profile_unsafe() {
        FailureCause::ProfileUnsafe
    } else if error.status() == Some(Status::SandboxFloor) {
        FailureCause::SandboxFloor
    } else {
        FailureCause::StartFailed
    };
    StartFailure {
        message: error.to_string(),
        cause,
    }
}

impl Runtime {
    /// Starts the engine for `config`. Returns once the engine accepted its
    /// configuration (a second or less), not when Gecko is up:
    /// `EngineEvent::Ready` says when pages run.
    pub fn start(config: &Config, shared: &Shared) -> Result<Arc<Self>, StartFailure> {
        // The hook holds the queue only, never the backend: the library's
        // thread must not be the one that drops the last handle of the engine.
        let queue = shared.queue.clone();
        let engine = config
            .builder()
            .on_wake(move || queue.wake())
            .start()
            .map_err(|error| start_failure(&error))?;
        // Before the first session: the engine never asks a question the
        // browser has no prompt for.
        let accepted = caps::subscribe(&engine).map_err(|error| start_failure(&error))?;
        // An id no download has is ignored, whatever the action: the answer
        // says only whether the engine has the call.
        let download_control = caps::taken(
            &engine.control_download(DownloadId::from_raw(u64::MAX), DownloadAction::Remove),
        ) == Some(true);
        Ok(Arc::new(Self {
            dispatcher: engine.dispatcher(),
            capabilities: caps::capabilities(&accepted, download_control),
            find: OnceLock::new(),
            clear_history: OnceLock::new(),
            engine,
            downloads: config.downloads.clone(),
            failed: AtomicBool::new(false),
        }))
    }

    /// Hands the engine's queued events to the backend's delegate. Never
    /// blocks; when more are queued than one call takes, the UI is woken
    /// again for the rest.
    pub fn dispatch(&self, shared: &Shared) {
        if self.failed.load(Ordering::Acquire) {
            return;
        }
        let mut host = Host {
            shared,
            downloads: &self.downloads,
            failed: false,
        };
        let dispatched = self.dispatcher.dispatch(&mut host);
        if host.failed {
            // Every later call answers `EngineFailed`: nothing more to hear.
            self.failed.store(true, Ordering::Release);
            return;
        }
        match dispatched {
            Ok(done) if done.more => shared.queue.wake(),
            Ok(_) => {}
            Err(error) => {
                // The engine's own account (`EngineFailed`) comes through the
                // queue before any call fails; an error with none is the
                // library's, and the engine cannot be used.
                self.failed.store(true, Ordering::Release);
                shared.push(BrowserEvent::Engine(EngineEvent::Failed {
                    message: error.to_string(),
                    code: 0,
                    cause: FailureCause::Crashed,
                    restartable: false,
                }));
            }
        }
    }
}
