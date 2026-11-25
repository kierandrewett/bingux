//! What the engine can do, read from what it announces and never from a list
//! of what the browser hopes for.
//!
//! The ABI has three places for it: the bitmaps of request kinds and event
//! kinds (`vv_engine_capabilities`), and named features
//! (`vv_engine_feature`). A control of the browser is enabled when every
//! piece it needs is announced **and** this crate wraps the calls that use it:
//! the safe `vivarium` crate does not wrap find, downloads control or the
//! permission store yet, so those stay disabled whatever the engine announces
//! (the doc comment of each field says which).

use vivarium as vv;
use vivarium_sys as sys;

use crate::backend::Capabilities;

/// What the browser has a prompt or a state for, by request kind. A kind the
/// browser does not answer is never subscribed, so the engine gives it its
/// safe default at once.
pub(super) const ANSWERED: &[vv::RequestKind] = &[
    vv::RequestKind::NewWindow,
    vv::RequestKind::Permission,
    vv::RequestKind::JsDialog,
    vv::RequestKind::Auth,
    vv::RequestKind::CertError,
    vv::RequestKind::Select,
    vv::RequestKind::FilePicker,
    vv::RequestKind::ContextMenu,
    vv::RequestKind::Download,
];

/// The request kinds to subscribe to: the ones the browser answers that the
/// engine announces.
pub(super) fn subscriptions(caps: &vv::Capabilities) -> Vec<vv::RequestHandler> {
    ANSWERED
        .iter()
        .filter(|kind| caps.request_kind(**kind))
        .map(|kind| vv::RequestHandler::new(*kind))
        .collect()
}

/// The environment field names the engine announces (`env.<field>`).
pub(super) fn environment_supported(engine: &vv::Engine, field: &str) -> bool {
    engine.feature(&format!("env.{field}")) == vv::FeatureState::Supported
}

/// What the UI may offer, from what the engine announces.
pub(super) fn capabilities(engine: &vv::Engine, caps: &vv::Capabilities) -> Capabilities {
    let supported = |name: &str| engine.feature(name) == vv::FeatureState::Supported;
    let asks = |kind: vv::RequestKind| caps.request_kind(kind);
    let sends = |event: u32| caps.event_kind(event);
    Capabilities {
        // The safe crate has no `vv_session_find`: never enabled.
        find: false,
        permissions: asks(vv::RequestKind::Permission),
        dialogs: asks(vv::RequestKind::JsDialog) && asks(vv::RequestKind::Auth),
        cert_errors: asks(vv::RequestKind::CertError),
        context_menus: asks(vv::RequestKind::ContextMenu),
        // The request is answered here and progress comes as events; the
        // safe crate has no pause, resume or cancel, so the buttons say so.
        downloads: asks(vv::RequestKind::Download) && sends(sys::VV_EVENT_DOWNLOAD),
        hover_links: sends(sys::VV_EVENT_STATUS_TEXT),
        audio: sends(sys::VV_EVENT_AUDIO_STATE) && supported("session.audio"),
        touch: supported("input.touch"),
        composition: supported("input.composition"),
        form_pickers: asks(vv::RequestKind::Select) && asks(vv::RequestKind::FilePicker),
        favicons: sends(sys::VV_EVENT_FAVICON),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_kinds_the_browser_answers_are_subscribed() {
        assert!(ANSWERED.contains(&vv::RequestKind::NewWindow));
        assert!(!ANSWERED.contains(&vv::RequestKind::Fullscreen), "a kind with no prompt gets the engine's safe default");
        assert!(!ANSWERED.contains(&vv::RequestKind::ExtensionApi));
        assert!(!ANSWERED.contains(&vv::RequestKind::ClipboardRead));
    }
}
