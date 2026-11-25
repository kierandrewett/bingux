//! One engine and the thread that listens to it.
//!
//! [`Runtime::start`] starts the engine, subscribes to the requests the browser
//! answers and the engine announces, and starts the pump: one thread that
//! waits for the engine's events (`vv_engine_wait_event`, which sleeps on the
//! same descriptor `Engine::event_handle` gives), turns each into a
//! [`BrowserEvent`](crate::web_view::BrowserEvent) and pushes it onto the queue whose
//! waker wakes the UI's task. The UI never waits for the engine and never
//! polls a timer; the pump's own timeout only bounds how long it takes to
//! notice that it was asked to stop.
//!
//! A restart is a new `Runtime`: the failed one is dropped first, which joins
//! its pump and lets go of every handle of the engine that died.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use vivarium as vv;

use super::config::Config;
use super::events::{Pump, handle};
use super::state::{Shared, lock};
use super::caps;
use crate::web_view::backend::Capabilities;
use crate::web_view::events::FailureCause;

/// How long the pump sleeps before it looks at whether it was stopped.
const LISTEN: Duration = Duration::from_millis(100);

/// Why an engine did not start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StartFailure {
    pub message: String,
    pub cause: FailureCause,
}

pub(super) struct Runtime {
    pub engine: Arc<vv::Engine>,
    /// What the browser's controls may offer because of it.
    pub capabilities: Capabilities,
    /// The engine took the WebRTC address policy the host asked for.
    pub webrtc_applied: bool,
    stop: Arc<AtomicBool>,
    pump: Mutex<Option<JoinHandle<()>>>,
}

/// An engine error that stopped the start, as the browser says it. A profile
/// that is not safe is its own case: the page names the folder and the rule.
pub(super) fn start_failure(error: &vv::Error) -> StartFailure {
    let cause = if error.is_profile_unsafe() {
        FailureCause::ProfileUnsafe
    } else if error.status() == Some(vv::Status::SandboxFloor) {
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
    pub fn start(config: &Config, shared: &Arc<Shared>) -> Result<Arc<Self>, StartFailure> {
        let engine = config.builder().start().map_err(|error| start_failure(&error))?;
        let engine = Arc::new(engine);
        let fail = |error: vv::Error| start_failure(&error);
        let announced = engine.capabilities().map_err(fail)?;
        // Ordered before the first session: the engine never asks a question
        // the browser has no prompt for, and the ones it does answer only when
        // it announces them.
        engine.set_request_handlers(&caps::subscriptions(&announced)).map_err(fail)?;
        let webrtc_applied = match config.webrtc {
            Some(choice) if engine.feature("net.config") == vv::FeatureState::Supported => engine
                .set_network(&vv::NetConfig {
                    webrtc_ip_policy: Some(choice.policy()),
                    ..vv::NetConfig::default()
                })
                .is_ok(),
            _ => false,
        };
        let capabilities = caps::capabilities(&engine, &announced);
        let stop = Arc::new(AtomicBool::new(false));
        let handle = {
            let (engine, shared, stop) = (engine.clone(), shared.clone(), stop.clone());
            let downloads = config.downloads.clone();
            std::thread::Builder::new()
                .name("vivarium-events".to_string())
                .spawn(move || pump(&engine, &shared, &stop, downloads))
                .map_err(|error| StartFailure {
                    message: format!("the event thread did not start: {error}"),
                    cause: FailureCause::StartFailed,
                })?
        };
        Ok(Arc::new(Self {
            engine,
            capabilities,
            webrtc_applied,
            stop,
            pump: Mutex::new(Some(handle)),
        }))
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = lock(&self.pump).take()
            && handle.thread().id() != std::thread::current().id()
        {
            let _ = handle.join();
        }
    }
}

/// Listens until asked to stop, or until the engine is gone.
fn pump(engine: &Arc<vv::Engine>, shared: &Arc<Shared>, stop: &AtomicBool, downloads: PathBuf) {
    let pump = Pump {
        engine,
        shared,
        downloads: &downloads,
    };
    while !stop.load(Ordering::Acquire) {
        match engine.wait_event(Some(LISTEN)) {
            Ok(Some(event)) => {
                let over = matches!(event.kind, vv::EventKind::EngineFailed(_));
                handle(&pump, event);
                if over {
                    // Every later call answers `EngineFailed`: nothing more to hear.
                    return;
                }
            }
            Ok(None) => {}
            Err(error) => {
                // The engine's own account (`EngineFailed`) comes through the
                // queue before any call fails; an error with none is the
                // library's, and the engine cannot be used.
                shared.push(crate::web_view::BrowserEvent::Engine(crate::web_view::EngineEvent::Failed {
                    message: error.to_string(),
                    code: 0,
                    cause: FailureCause::Crashed,
                    restartable: false,
                }));
                return;
            }
        }
    }
}
