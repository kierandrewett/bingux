//! What the engine can do, read from what it accepts and never from a list
//! of what the browser hopes for.
//!
//! ABI 2.0 announces no lists of request kinds, event kinds or named
//! features any more (`vv_engine_capabilities` holds only the ABI version and
//! the session and identity limits). What is left to read is whether the
//! engine accepts a subscription to a request kind: one it cannot send
//! answers `Unsupported`. A control of the browser is enabled when the
//! request it needs is accepted, or when it needs only events and calls this
//! crate wraps. A call has no subscription to try: an engine that lacks it
//! answers `Unsupported` ([`taken`]). The download control is asked once at
//! start (an id no download has is ignored, so the question changes
//! nothing); find needs a session to be asked and is learned from the first
//! one. The crate has no call for muting, composition or the permission
//! store's lists yet, so those stay disabled (the doc comment of each field
//! says which).

use vivarium::engine::Engine;
use vivarium::error::Result;
use vivarium::request::{RequestDelegate, RequestKind};

use crate::web_view::backend::Capabilities;

/// What the browser has a prompt or a state for, by request kind. A kind the
/// browser does not answer is never subscribed, so the engine gives it its
/// safe default at once.
pub(super) const ANSWERED: &[RequestKind] = &[
    RequestKind::NewWindow,
    RequestKind::Permission,
    RequestKind::JsDialog,
    RequestKind::Auth,
    RequestKind::CertError,
    RequestKind::Select,
    RequestKind::FilePicker,
    RequestKind::ContextMenu,
    RequestKind::Download,
];

/// Subscribes to the kinds the browser answers that the engine can send, and
/// returns those. The whole set is tried first; when the engine refuses it as
/// unsupported, each kind is tried alone, as `Engine::delegate_all` does for
/// every kind.
pub(super) fn subscribe(engine: &Engine) -> Result<Vec<RequestKind>> {
    let all: Vec<RequestDelegate> = ANSWERED
        .iter()
        .map(|kind| RequestDelegate::new(*kind))
        .collect();
    match engine.set_request_delegates(&all) {
        Ok(()) => return Ok(ANSWERED.to_vec()),
        Err(error) if error.is_unsupported() => {}
        Err(error) => return Err(error),
    }
    let mut accepted = Vec::with_capacity(all.len());
    for delegate in all {
        match engine.set_request_delegates(std::slice::from_ref(&delegate)) {
            Ok(()) => accepted.push(delegate),
            Err(error) if error.is_unsupported() => {}
            Err(error) => return Err(error),
        }
    }
    engine.set_request_delegates(&accepted)?;
    Ok(accepted.into_iter().map(|delegate| delegate.kind).collect())
}

/// Whether the engine took a call, from what it answered: `Some(true)` when
/// it did, `Some(false)` when it says it has no such call, `None` when the
/// answer says nothing of the call (the session was gone, the engine failed)
/// and the question is still open.
pub(super) fn taken(answer: &Result<()>) -> Option<bool> {
    match answer {
        Ok(()) => Some(true),
        Err(error) if error.is_unsupported() => Some(false),
        Err(_) => None,
    }
}

/// What the UI may offer, from the request kinds the engine accepted and
/// whether it took the call that controls a download.
pub(super) fn capabilities(accepted: &[RequestKind], download_control: bool) -> Capabilities {
    let asks = |kind: RequestKind| accepted.contains(&kind);
    Capabilities {
        // Learned from the first session (`taken`), which this has none of.
        find: false,
        permissions: asks(RequestKind::Permission),
        dialogs: asks(RequestKind::JsDialog) && asks(RequestKind::Auth),
        cert_errors: asks(RequestKind::CertError),
        context_menus: asks(RequestKind::ContextMenu),
        // The request is answered here and progress comes as events.
        downloads: asks(RequestKind::Download),
        download_control,
        // `ContentDelegate::on_status_text`: an event, nothing to announce.
        hover_links: true,
        // ABI 2.0 has no audio state event and no mute call.
        audio: false,
        // The engine announces no touch input (`docs/rust.md`, "Known engine
        // gaps"): fingers become a click, a scroll and a held press.
        touch: false,
        // The crate has no composition call.
        composition: false,
        form_pickers: asks(RequestKind::Select) && asks(RequestKind::FilePicker),
        // `ContentDelegate::on_favicon`: an event, nothing to announce.
        favicons: true,
    }
}

#[cfg(test)]
mod tests {
    use vivarium::error::{Error, Status};

    use super::*;

    #[test]
    fn only_the_kinds_the_browser_answers_are_subscribed() {
        assert!(ANSWERED.contains(&RequestKind::NewWindow));
        assert!(
            !ANSWERED.contains(&RequestKind::Fullscreen),
            "a kind with no prompt gets the engine's safe default"
        );
        assert!(!ANSWERED.contains(&RequestKind::ExtensionApi));
        assert!(!ANSWERED.contains(&RequestKind::ClipboardRead));
    }

    #[test]
    fn a_control_is_offered_only_when_the_engine_took_its_request() {
        let none = capabilities(&[], false);
        assert!(
            !none.permissions
                && !none.dialogs
                && !none.cert_errors
                && !none.context_menus
                && !none.downloads
                && !none.form_pickers
        );
        let all = capabilities(ANSWERED, true);
        assert!(
            all.permissions
                && all.dialogs
                && all.cert_errors
                && all.context_menus
                && all.downloads
                && all.form_pickers
        );
        assert!(
            !all.audio && !all.touch && !all.composition,
            "no call for them, whatever the engine takes"
        );
        let half = capabilities(&[RequestKind::JsDialog, RequestKind::Select], false);
        assert!(!half.dialogs, "a dialog needs authentication too");
        assert!(!half.form_pickers, "a list needs the file picker too");
    }

    // Failures the find and download-control capabilities guard against:
    //   1. download control on for an engine that has no such call, so a
    //      button does nothing and says nothing;
    //   2. download control off for an engine that has it, so the buttons of
    //      a running download never show;
    //   3. a call the engine lacks read as taken, or one that failed for an
    //      unrelated reason (a closed session) read as lacking, so find stays
    //      off for good after a tab closed under the probe.
    #[test]
    fn download_control_follows_the_call_the_engine_took() {
        assert!(capabilities(ANSWERED, true).download_control);
        assert!(!capabilities(ANSWERED, false).download_control);
        assert!(
            !capabilities(&[], false).download_control && !capabilities(&[], false).downloads,
            "the progress of a download is its own capability"
        );
    }

    #[test]
    fn a_call_is_taken_when_the_engine_answers_it_and_lacking_when_it_says_so() {
        assert_eq!(taken(&Ok(())), Some(true));
        assert_eq!(taken(&Err(Error::Status(Status::Unsupported))), Some(false));
        assert_eq!(
            taken(&Err(Error::Status(Status::NoSuchSession))),
            None,
            "a session that closed under the probe says nothing of the call"
        );
        assert_eq!(taken(&Err(Error::Status(Status::EngineFailed))), None);
    }
}
