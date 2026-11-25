//! What the engine says, turned into [`BrowserEvent`]s: the backend's
//! delegate.
//!
//! [`Host`] implements the crate's delegate traits. The crate's `Dispatcher`
//! calls one of its methods per event, on the UI thread, when the backend's
//! `poll_events` runs (the engine's wake hook woke the UI's task). Every
//! method the browser has no use for keeps its default: an event is dropped,
//! a request gets the default answer of its kind. Events arrive in the order
//! the engine produced them; the queue between this delegate and the UI keeps
//! that order.

use std::path::Path;

use vivarium::certificates::{CertErrorReply, CertErrorRequest, CertificateDelegate};
use vivarium::default_delegates;
use vivarium::diagnostics::{CrashDelegate, ProcessInfo};
use vivarium::display::{DisplayDelegate, FrameAvailable, ZoomChanged};
use vivarium::downloads::{
    Conflict, DownloadDelegate, DownloadEvent, DownloadReply, DownloadRequest,
};
use vivarium::engine::{EngineDelegate, EngineFailedFlags, EngineFailure};
use vivarium::input::{Cursor, InputDelegate};
use vivarium::navigation::{
    FindProgress, Load, LoadState, Location, NavigationDelegate, NewWindowReply, NewWindowRequest,
    ProgressDelegate, SessionHistory,
};
use vivarium::permissions::{PermissionDelegate, PermissionReply, PermissionRequest};
use vivarium::prompts::{
    AuthReply, AuthRequest, ContextMenuReply, ContextMenuRequest, DialogKind as EngineDialog,
    FilePickerReply, FilePickerRequest, JsDialogReply, JsDialogRequest, PromptDelegate,
    SelectReply, SelectRequest,
};
use vivarium::request::{Ask, RequestClosed, Responder};
use vivarium::security::{SecurityDelegate, SecurityInfo, SecurityReport};
use vivarium::session::{
    CloseReason, ContentDelegate, SessionClosed, SessionDelegate, SessionId, SessionReady,
};

use super::requests::{self, Open};
use super::state::{Asked, Shared, lock, tab_of};
use super::{convert, frames};
use crate::web_view::events::{BrowserEvent, EngineEvent, FailureCause, TabEvent};
use crate::web_view::ids::RequestId;
use crate::web_view::page::{HistoryEntry, LoadError};
use crate::web_view::requests::{Request, RequestKind};

/// The backend as the engine's delegate, for one dispatch.
pub(super) struct Host<'a> {
    pub shared: &'a Shared,
    /// Where downloads are saved.
    pub downloads: &'a Path,
    /// The engine said it failed: nothing more will come from it.
    pub failed: bool,
}

/// The load was cut short by the person or by the next navigation
/// (`NS_BINDING_ABORTED`): the page that was there stays, there is no error
/// to show.
const NS_BINDING_ABORTED: u32 = 0x804B_0002;

/// Progress and the end of a load.
pub(super) fn load(load: &Load) -> TabEvent {
    let stopped = load.state == LoadState::Stopped;
    let error =
        (stopped && load.nsresult != 0 && load.nsresult != NS_BINDING_ABORTED).then(|| LoadError {
            code: load.nsresult,
            name: load.error_name.clone(),
        });
    TabEvent::Load {
        started: load.state == LoadState::Started,
        loading: !stopped,
        progress: if stopped {
            1.0
        } else {
            load.progress.min(1000) as f32 / 1000.0
        },
        error,
    }
}

/// Why the engine is gone, as the browser tells the person.
pub(super) fn failure_cause(failure: &EngineFailure) -> FailureCause {
    if failure.is_protocol_violation() {
        FailureCause::ProtocolViolation
    } else if failure.flags.contains(EngineFailedFlags::SANDBOX_FLOOR) {
        FailureCause::SandboxFloor
    } else if failure.flags.contains(EngineFailedFlags::PROFILE_LOCKED) {
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
pub(super) fn failed_event(failure: &EngineFailure) -> EngineEvent {
    let cause = failure_cause(failure);
    EngineEvent::Failed {
        message: failure.message.clone(),
        code: failure.exit_code,
        cause,
        restartable: failure.flags.contains(EngineFailedFlags::RESTARTABLE)
            || matches!(cause, FailureCause::ProfileLocked | FailureCause::Crashed),
    }
}

impl Host<'_> {
    fn tab(&self, session: SessionId, event: TabEvent) {
        self.shared.push(BrowserEvent::tab(tab_of(session), event));
    }

    /// A question for the person: its responder waits under a new id, and
    /// the UI is told.
    fn ask(&self, session: SessionId, kind: RequestKind, open: Open) {
        let tab = tab_of(session);
        let id = {
            let mut state = lock(&self.shared.state);
            state.next_request += 1;
            let id = RequestId(state.next_request);
            state.requests.insert(id, Asked { tab, open });
            id
        };
        self.shared.push(BrowserEvent::Request {
            tab,
            request: Request { id, kind },
        });
    }
}

impl EngineDelegate for Host<'_> {
    fn on_engine_ready(&mut self) {
        self.shared.push(BrowserEvent::Engine(EngineEvent::Ready));
    }

    /// The engine is gone. Its sessions are closed already (each said so,
    /// with the reason `EngineFailed`, and the browser kept the tabs): let go
    /// of every handle of this engine. A new engine opens the tabs again at
    /// their addresses (ABI 2.0 hands the host no saved session state).
    fn on_engine_failed(&mut self, failure: EngineFailure) {
        {
            let mut state = lock(&self.shared.state);
            state.live.clear();
            state.windows.clear();
            state.requests.clear();
            state.identities.clear();
        }
        self.failed = true;
        self.shared
            .push(BrowserEvent::Engine(failed_event(&failure)));
    }

    /// The engine ended a question itself (its deadline, a navigation): the
    /// UI forgets it.
    fn on_request_closed(&mut self, closed: RequestClosed) {
        let mut state = lock(&self.shared.state);
        let found = state
            .requests
            .iter()
            .find(|(_, asked)| asked.open.id() == closed.request_id)
            .map(|(id, asked)| (*id, asked.tab));
        if let Some((id, tab)) = found {
            state.requests.remove(&id);
            drop(state);
            self.shared
                .push(BrowserEvent::RequestClosed { tab, request: id });
            return;
        }
        // A pop-up the engine gave up on: its session closes by itself.
        state
            .windows
            .retain(|_, reply| reply.id() != closed.request_id);
    }
}

impl SessionDelegate for Host<'_> {
    fn on_session_ready(&mut self, session: SessionId, _: SessionReady) {
        self.tab(session, TabEvent::Ready);
    }

    fn on_session_closed(&mut self, session: SessionId, closed: SessionClosed) {
        let tab = tab_of(session);
        match closed.reason {
            // The engine failed: the tab stays, so a new engine can open it again.
            // `EngineFailed` follows and says it for all of them.
            CloseReason::EngineFailed => {}
            // A window nobody took: it never was a tab.
            CloseReason::WindowRejected => {
                lock(&self.shared.state).windows.remove(&tab);
            }
            CloseReason::Requested | CloseReason::ByPage => {
                lock(&self.shared.state).live.remove(&tab);
                self.tab(session, TabEvent::Closed { error: None });
            }
            _ => {
                lock(&self.shared.state).live.remove(&tab);
                let why = if closed.message.is_empty() {
                    format!("{:?}", closed.reason)
                } else {
                    closed.message
                };
                self.tab(session, TabEvent::Closed { error: Some(why) });
            }
        }
    }

    fn on_close_requested(&mut self, session: SessionId) {
        self.tab(session, TabEvent::CloseRequested);
    }
}

impl ContentDelegate for Host<'_> {
    fn on_title(&mut self, session: SessionId, title: String) {
        self.tab(session, TabEvent::Title(title));
    }

    fn on_favicon(&mut self, session: SessionId, url: String) {
        self.tab(session, TabEvent::Favicon(url));
    }

    /// The link under the pointer; empty means none.
    fn on_status_text(&mut self, session: SessionId, text: String) {
        self.tab(
            session,
            TabEvent::HoverLink((!text.is_empty()).then_some(text)),
        );
    }

    fn on_zoom(&mut self, session: SessionId, zoom: ZoomChanged) {
        self.tab(session, TabEvent::Zoom(zoom.zoom));
    }

    fn on_fullscreen_changed(&mut self, session: SessionId, entering: bool) {
        self.tab(session, TabEvent::Fullscreen(entering));
    }
}

impl NavigationDelegate for Host<'_> {
    fn on_location(&mut self, session: SessionId, location: Location) {
        self.tab(
            session,
            TabEvent::Location {
                url: location.url,
                can_back: location.can_back,
                can_forward: location.can_forward,
            },
        );
    }

    fn on_session_history(&mut self, session: SessionId, history: SessionHistory) {
        self.tab(
            session,
            TabEvent::History {
                index: history.index as usize,
                entries: history
                    .entries
                    .into_iter()
                    .map(|entry| HistoryEntry {
                        url: entry.url,
                        title: entry.title,
                    })
                    .collect(),
            },
        );
    }

    fn on_find_result(&mut self, session: SessionId, find: FindProgress) {
        self.tab(
            session,
            TabEvent::FindResult {
                current: find.current,
                total: find.total,
            },
        );
    }

    /// `window.open`: the engine made a provisional session; the browser takes
    /// it as a tab (`accept_tab`, which adopts it) or not (`close_tab`, which
    /// refuses it). A request with no provisional session is refused here.
    fn on_new_window_request(
        &mut self,
        session: SessionId,
        ask: Ask<NewWindowRequest>,
        reply: Responder<NewWindowReply>,
    ) {
        let Some(new) = ask.payload.new_session.map(tab_of) else {
            return;
        };
        let event = requests::new_tab(
            new,
            requests::origin_text(&ask.origin, &ask.top_origin),
            &ask.payload,
        );
        lock(&self.shared.state).windows.insert(new, reply);
        self.tab(session, event);
    }
}

impl ProgressDelegate for Host<'_> {
    fn on_load(&mut self, session: SessionId, state: Load) {
        self.tab(session, load(&state));
    }

    fn on_security_changed(&mut self, session: SessionId, security: SecurityInfo) {
        self.tab(
            session,
            TabEvent::Security(convert::security(security.state, security.flags)),
        );
    }
}

impl DisplayDelegate for Host<'_> {
    fn on_frame_available(&mut self, session: SessionId, _: FrameAvailable) {
        if let Some(entry) = lock(&self.shared.state).live.get(&tab_of(session)) {
            frames::take(entry);
        }
    }
}

impl InputDelegate for Host<'_> {
    fn on_cursor(&mut self, session: SessionId, cursor: Cursor) {
        if let Some(entry) = lock(&self.shared.state).live.get(&tab_of(session)) {
            entry.surface.set_cursor(convert::cursor(cursor.kind));
        }
    }
}

impl PromptDelegate for Host<'_> {
    fn on_js_dialog_request(
        &mut self,
        session: SessionId,
        ask: Ask<JsDialogRequest>,
        reply: Responder<JsDialogReply>,
    ) {
        let prompt = ask.payload.dialog == EngineDialog::Prompt;
        let kind = RequestKind::Dialog(requests::dialog(
            requests::origin_text(&ask.origin, &ask.top_origin),
            &ask.payload,
        ));
        self.ask(session, kind, Open::Dialog { prompt, reply });
    }

    fn on_auth_request(
        &mut self,
        session: SessionId,
        ask: Ask<AuthRequest>,
        reply: Responder<AuthReply>,
    ) {
        self.ask(
            session,
            RequestKind::Auth(requests::auth(&ask.payload)),
            Open::Auth(reply),
        );
    }

    fn on_select_request(
        &mut self,
        session: SessionId,
        ask: Ask<SelectRequest>,
        reply: Responder<SelectReply>,
    ) {
        self.ask(
            session,
            RequestKind::Select(requests::select(&ask.payload)),
            Open::Select(reply),
        );
    }

    fn on_file_picker_request(
        &mut self,
        session: SessionId,
        ask: Ask<FilePickerRequest>,
        reply: Responder<FilePickerReply>,
    ) {
        self.ask(
            session,
            RequestKind::FilePicker(requests::picker(&ask.payload)),
            Open::FilePicker(reply),
        );
    }

    fn on_context_menu_request(
        &mut self,
        session: SessionId,
        ask: Ask<ContextMenuRequest>,
        reply: Responder<ContextMenuReply>,
    ) {
        self.ask(
            session,
            RequestKind::ContextMenu(requests::context_target(&ask.payload)),
            Open::ContextMenu(reply),
        );
    }
}

impl PermissionDelegate for Host<'_> {
    fn on_permission_request(
        &mut self,
        session: SessionId,
        ask: Ask<PermissionRequest>,
        reply: Responder<PermissionReply>,
    ) {
        let kind = RequestKind::Permission(crate::web_view::requests::PermissionRequest {
            origin: requests::origin_text(&ask.origin, &ask.top_origin),
            kinds: ask
                .payload
                .items
                .iter()
                .map(requests::permission_kind)
                .collect(),
        });
        self.ask(session, kind, Open::Permission(reply));
    }
}

impl CertificateDelegate for Host<'_> {
    fn on_cert_error_request(
        &mut self,
        session: SessionId,
        ask: Ask<CertErrorRequest>,
        reply: Responder<CertErrorReply>,
    ) {
        self.ask(
            session,
            RequestKind::CertError(requests::cert_error(&ask.payload)),
            Open::CertError(reply),
        );
    }
}

impl DownloadDelegate for Host<'_> {
    /// A download belongs to a contextual identity, not to a page: the
    /// engine's event names no session.
    fn on_download(
        &mut self,
        identity: Option<vivarium::contextual_identities::ContextualIdentityId>,
        download: DownloadEvent,
    ) {
        let owner = {
            let state = lock(&self.shared.state);
            requests::download_owner(download.flags, identity, |id| {
                state
                    .identities
                    .iter()
                    .find(|(_, made)| made.id() == id)
                    .map(|(&(container, private), _)| (container, private))
            })
        };
        if let Some(info) = requests::download_info(&download, owner) {
            self.shared.push(BrowserEvent::Download(info));
        }
    }

    /// The browser saves what it is offered, under a name that does not
    /// overwrite another file, in the download folder.
    fn on_download_request(
        &mut self,
        _: SessionId,
        ask: Ask<DownloadRequest>,
        reply: Responder<DownloadReply>,
    ) {
        let target = requests::download_target(self.downloads, &ask.payload.suggested_name);
        let _ = reply.answer(DownloadReply::Save {
            path: target.to_string_lossy().into_owned(),
            conflict: Conflict::Unique,
        });
    }
}

impl SecurityDelegate for Host<'_> {
    fn on_security_report(&mut self, _: SecurityReport) {
        self.shared
            .push(BrowserEvent::Engine(EngineEvent::SecurityChanged));
    }
}

impl CrashDelegate for Host<'_> {
    /// The content process of the tab is gone.
    fn on_process_gone(&mut self, session: SessionId, process: ProcessInfo) {
        self.tab(
            session,
            TabEvent::ProcessGone(convert::gone(process.reason)),
        );
    }
}

default_delegates!(Host<'_>: ExtensionDelegate, LocaleDelegate);

#[cfg(test)]
mod tests;
