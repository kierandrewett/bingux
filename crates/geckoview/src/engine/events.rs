//! What the engine says, turned into [`BrowserEvent`]s.
//!
//! One function per event kind the browser has a use for; every other kind is
//! dropped here, so the UI sees only what it can show. Events arrive in the
//! order the engine produced them (`Event::seq`); the queue between this
//! thread and the UI keeps that order.
//!
//! The header of an event (`applied_seq`, `navigation_id`, `document_id`) is
//! not used by the browser: it never waits for "everything I sent has taken
//! effect", and the page state it shows is the latest the engine said.

use std::path::Path;

use bingux_ui::frame_view::CaretRect;
use vivarium as vv;

use super::requests::{self, Incoming};
use super::state::{Shared, lock, tab_of};
use super::{convert, frames};
use crate::events::{BrowserEvent, EngineEvent, FailureCause, TabEvent};
use crate::ids::RequestId;
use crate::page::{HistoryEntry, LoadError};
use crate::requests::Request;

/// What an event handler needs besides the event.
pub(super) struct Pump<'a> {
    pub engine: &'a vv::Engine,
    pub shared: &'a Shared,
    /// Where downloads are saved.
    pub downloads: &'a Path,
}

/// The load was cut short by the person or by the next navigation
/// (`NS_BINDING_ABORTED`): the page that was there stays, there is no error
/// to show.
const NS_BINDING_ABORTED: u32 = 0x804B_0002;

/// Progress and the end of a load.
pub(super) fn load(load: &vv::Load) -> TabEvent {
    let stopped = load.state == vv::LoadState::Stopped;
    let error = (stopped && load.nsresult != 0 && load.nsresult != NS_BINDING_ABORTED).then(|| LoadError {
        code: load.nsresult,
        name: load.error_name.clone(),
    });
    TabEvent::Load {
        started: load.state == vv::LoadState::Started,
        loading: !stopped,
        progress: if stopped { 1.0 } else { load.progress.min(1000) as f32 / 1000.0 },
        error,
    }
}

/// Why the engine is gone, as the browser tells the person.
pub(super) fn failure_cause(failure: &vv::EngineFailure) -> FailureCause {
    if failure.is_protocol_violation() {
        FailureCause::ProtocolViolation
    } else if failure.flags.contains(vv::EngineFailedFlags::SANDBOX_FLOOR) {
        FailureCause::SandboxFloor
    } else if failure.flags.contains(vv::EngineFailedFlags::PROFILE_LOCKED) {
        FailureCause::ProfileLocked
    } else {
        FailureCause::Crashed
    }
}

/// The failure as an event. A new engine on the same profile is offered when
/// the engine says it is safe, and also when it simply died (`kill -9` and a
/// crash set no flag: the engine cannot say anything) or found the profile in
/// use: starting again is harmless, and when it does not work the window says
/// why. It is not offered for a protocol violation unless the engine says so,
/// nor when the sandbox floor was not met (the same engine would fail again).
pub(super) fn failed_event(failure: &vv::EngineFailure) -> EngineEvent {
    let cause = failure_cause(failure);
    EngineEvent::Failed {
        message: failure.message.clone(),
        code: failure.exit_code,
        cause,
        restartable: failure.flags.contains(vv::EngineFailedFlags::RESTARTABLE)
            || matches!(cause, FailureCause::ProfileLocked | FailureCause::Crashed),
    }
}

/// The engine is gone. Its sessions are closed already (each said so, with
/// the reason `EngineFailed`, and the browser kept the tabs): take what each
/// had saved, so a new engine can open them again where they were, and let go
/// of every handle of this one.
fn engine_failed(pump: &Pump, failure: &vv::EngineFailure) {
    {
        let mut state = lock(&pump.shared.state);
        let live = std::mem::take(&mut state.live);
        for (tab, entry) in live {
            // Readable after the failure; only a session made with `report_state` has one.
            if let Ok(Some(saved)) = entry.session.saved_state()
                && !saved.data.is_empty()
            {
                state.lost.insert(tab, saved.data);
            }
        }
        state.windows.clear();
        state.requests.clear();
        state.contexts.clear();
    }
    pump.shared.push(BrowserEvent::Engine(failed_event(failure)));
}

fn tab_event(pump: &Pump, event: &vv::Event, tab_event: TabEvent) {
    if let Some(session) = event.session {
        pump.shared.push(BrowserEvent::tab(tab_of(session), tab_event));
    }
}

/// A question or an answer to `window.open`, from the engine.
fn request(pump: &Pump, request: vv::Request) {
    let Some(session) = request.session else {
        let _ = pump.engine.answer_default(&request);
        return;
    };
    match requests::classify(&request, pump.downloads) {
        Incoming::Ask(kind) => {
            let mut state = lock(&pump.shared.state);
            state.next_request += 1;
            let id = RequestId(state.next_request);
            state.requests.insert(id, request);
            drop(state);
            pump.shared.push(BrowserEvent::Request {
                tab: tab_of(session),
                request: Request { id, kind },
            });
        }
        Incoming::Window { url, name, no_opener, window } => {
            let provisional = match &request.payload {
                vv::RequestPayload::NewWindow(new) => new.new_session,
                _ => None,
            };
            let Some(new) = provisional.map(tab_of) else {
                let _ = pump.engine.answer_default(&request);
                return;
            };
            lock(&pump.shared.state).windows.insert(new, request);
            pump.shared.push(BrowserEvent::tab(
                tab_of(session),
                TabEvent::NewTab {
                    new,
                    url,
                    name,
                    no_opener,
                    window,
                },
            ));
        }
        Incoming::Download(target) => {
            // The browser saves what it is offered, under a name that does not
            // overwrite another file, in the download folder.
            let answer = vv::Response::save_download(target.to_string_lossy().into_owned(), vv::Conflict::Unique, false);
            let _ = pump.engine.respond(&request, answer);
        }
        Incoming::Default => {
            let _ = pump.engine.answer_default(&request);
        }
    }
}

/// The engine ended a question itself: the UI forgets it.
fn request_closed(pump: &Pump, closed: &vv::RequestClosed) {
    let mut state = lock(&pump.shared.state);
    let found = state.requests.iter().find(|(_, open)| open.id == closed.request_id).map(|(id, open)| (*id, open.session));
    if let Some((id, session)) = found {
        state.requests.remove(&id);
        drop(state);
        if let Some(session) = session {
            pump.shared.push(BrowserEvent::RequestClosed { tab: tab_of(session), request: id });
        }
        return;
    }
    // A pop-up the engine gave up on: its session closes by itself.
    state.windows.retain(|_, open| open.id != closed.request_id);
}

/// What the engine said about a session closing.
fn session_closed(pump: &Pump, event: &vv::Event, closed: &vv::SessionClosed) {
    let Some(session) = event.session else {
        return;
    };
    let tab = tab_of(session);
    match closed.reason {
        // The engine failed: the tab stays, so a new engine can open it again.
        // `EngineFailed` follows and says it for all of them.
        vv::CloseReason::EngineFailed => {}
        // A window nobody took: it never was a tab.
        vv::CloseReason::WindowRejected => {
            lock(&pump.shared.state).windows.remove(&tab);
        }
        vv::CloseReason::Requested | vv::CloseReason::ByPage => {
            lock(&pump.shared.state).live.remove(&tab);
            pump.shared.push(BrowserEvent::tab(tab, TabEvent::Closed { error: None }));
        }
        _ => {
            lock(&pump.shared.state).live.remove(&tab);
            let why = if closed.message.is_empty() { format!("{:?}", closed.reason) } else { closed.message.clone() };
            pump.shared.push(BrowserEvent::tab(tab, TabEvent::Closed { error: Some(why) }));
        }
    }
}

/// One event of the engine.
pub(super) fn handle(pump: &Pump, event: vv::Event) {
    use vv::EventKind as K;
    let tab = |e: &vv::Event, state: TabEvent| tab_event(pump, e, state);
    match &event.kind {
        K::EngineReady => pump.shared.push(BrowserEvent::Engine(EngineEvent::Ready)),
        K::EngineFailed(failure) => engine_failed(pump, failure),
        K::SecurityReport(_) => pump.shared.push(BrowserEvent::Engine(EngineEvent::SecurityChanged)),
        K::SessionReady(_) => tab(&event, TabEvent::Ready),
        K::SessionClosed(closed) => session_closed(pump, &event, closed),
        K::FrameAvailable(_) => {
            if let Some(session) = event.session
                && let Some(entry) = lock(&pump.shared.state).live.get(&tab_of(session))
            {
                frames::take(entry);
            }
        }
        K::Location(location) => tab(
            &event,
            TabEvent::Location {
                url: location.url.clone(),
                can_back: location.can_back,
                can_forward: location.can_forward,
            },
        ),
        K::Title(title) => tab(&event, TabEvent::Title(title.clone())),
        K::Load(state) => tab(&event, load(state)),
        K::Security(security) => tab(&event, TabEvent::Security(convert::security(security.state, security.flags))),
        K::Favicon(url) => tab(&event, TabEvent::Favicon(url.clone())),
        K::Cursor(cursor) => {
            if let Some(session) = event.session
                && let Some(entry) = lock(&pump.shared.state).live.get(&tab_of(session))
            {
                entry.surface.set_cursor(convert::cursor(cursor.kind));
            }
        }
        K::History(history) => tab(
            &event,
            TabEvent::History {
                index: history.index as usize,
                entries: history
                    .entries
                    .iter()
                    .map(|entry| HistoryEntry { url: entry.url.clone(), title: entry.title.clone() })
                    .collect(),
            },
        ),
        K::Fullscreen { entering } => tab(&event, TabEvent::Fullscreen(*entering)),
        // Only the process that shows the tab is the tab's business.
        K::ProcessGone(info) if info.kind == vv::ProcessKind::Content => tab(&event, TabEvent::ProcessGone(convert::gone(info.reason))),
        K::CloseRequested => tab(&event, TabEvent::CloseRequested),
        K::Zoom(zoom) => tab(&event, TabEvent::Zoom(zoom.zoom)),
        // The link under the pointer; empty means none.
        K::StatusText(text) => tab(&event, TabEvent::HoverLink((!text.is_empty()).then(|| text.clone()))),
        K::AudioState(audio) => tab(
            &event,
            TabEvent::Audio(crate::page::Audio {
                playing: audio.audible,
                muted: audio.muted,
            }),
        ),
        K::FindResult(find) => tab(&event, TabEvent::FindResult { current: find.current, total: find.total }),
        K::Download(download) => {
            if let Some(info) = requests::download_info(download) {
                pump.shared.push(BrowserEvent::Download(info));
            }
        }
        K::ImeState(state) => {
            if let Some(session) = event.session
                && let Some(entry) = lock(&pump.shared.state).live.get_mut(&tab_of(session))
            {
                entry.editing = state.focused && state.state != vv::ImeStateKind::Disabled;
                if !entry.editing {
                    entry.surface.set_caret(None);
                }
            }
        }
        K::ImeRects(rects) => {
            if let Some(session) = event.session
                && let Some(entry) = lock(&pump.shared.state).live.get(&tab_of(session))
                && entry.editing
            {
                entry.surface.set_caret(Some(CaretRect {
                    x: rects.caret.x,
                    y: rects.caret.y,
                    width: rects.caret.w,
                    height: rects.caret.h,
                }));
            }
        }
        K::Request(open) => request(pump, (**open).clone()),
        K::RequestClosed(closed) => request_closed(pump, closed),
        _ => {}
    }
}

#[cfg(test)]
mod tests;
