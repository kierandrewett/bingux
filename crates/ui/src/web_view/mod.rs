//! `bingux_ui::web_view`: the web as a control any Bingux app can embed.
//!
//! Built like Internet Explorer: a reusable control that shows one page (IE's
//! WebBrowser control, shdocvw hosting MSHTML) and thin frames around it. The
//! browser app (`bingux-browser`, like iexplore.exe) is one such frame; Mail's
//! account sign-in is another. An app speaks to a [`BrowserBackend`], this
//! module's one engine trait, and never to Vivarium:
//!
//! | Module | What it is |
//! | ------ | ---------- |
//! | [`view`] | [`WebView`], the control: the page in a frame view, input to the engine, host hooks |
//! | [`backend`] | [`BrowserBackend`]: sessions, navigation, state events, input, frames, find, zoom, permissions, downloads |
//! | [`environment`] | [`PageEnvironment`]: the host's appearance (scheme, accent, motion, text scale, locales) as pages see it, built from bingux-ui tokens |
//! | [`events`], [`page`], [`requests`] | what the engine says: load, location, security, history, process loss, new windows, and the questions it asks (permission, dialog, auth, certificate error, context menu) |
//! | [`input`] | what the host says to the page: pointer, wheel, key, text, touch, edit commands, focus. DOM names, so converting a [frame view input](crate::frame_view) loses nothing |
//! | [`touch`] | a finger as a click, a scroll and a held press, for an engine without touch input |
//! | [`surface`] | [`PageSurface`], the frame view's [`FrameSource`](crate::frame_view::FrameSource) for one session: newest frame wins, damage folded, pointer shape, caret |
//! | [`queue`] | the event queue between the engine's thread and the UI thread, and the wake-up |
//! | [`report`] | [`SecurityReport`]: the engine's own account of its sandbox and invariants, grouped per kind of process |
//! | `engine` | the real backend over the `vivarium` crate (feature `web-view-vivarium` or `web-view-dynamic-load`) |
//! | `test_double` | `TestDoubleBackend` (features `web-view` and `testing`): an in-memory backend for tests and snapshots, never shipped |
//!
//! The cargo feature `web-view` brings in the control and its types with no
//! engine, so an app that does not embed the web does not link Vivarium.
//!
//! The real backend over the `vivarium` crate (ABI 2.0) maps each
//! [`BrowserBackend`] method onto a crate call and implements the crate's
//! delegate traits, one method per engine event, each turned into a
//! [`BrowserEvent`]. A question the engine asks arrives with a typed
//! responder; the backend keeps it under the [`RequestId`] of the
//! control's [`Request`] and answers it with the control's [`Response`], so
//! there is one model of questions, the control's. What the engine does not
//! do (a request kind it does not accept, a call the crate does not have) is
//! `false` in [`Capabilities`] and answered with
//! [`BackendError::Unsupported`]; the UI shows those controls disabled. The
//! ABI's concepts map onto the control like this (`docs/browser-port.md` of
//! the Vivarium repository has the whole table):
//!
//! | ABI 2.0 | Browser |
//! | ------- | ------- |
//! | `Engine` | one [`BrowserBackend`]; `EngineFailed` is [`EngineEvent::Failed`], and `restart` makes a new one |
//! | Contextual identity | the default one for tabs, the private one for a private window, one per container |
//! | `Session` | a tab, [`TabId`] is its id |
//! | Request (`Ask`, `Responder`) | answered when the engine accepts its kind and the browser has a prompt for it; every other kind gets the engine's default |
//! | Events (wake hook, `Dispatcher`) | [`BrowserEvent`] in the order the engine produced them, dispatched on the UI thread when `poll_events` runs |

pub mod backend;
pub mod environment;
pub mod events;
pub mod ids;
pub mod input;
pub mod page;
pub mod queue;
pub mod report;
pub mod requests;
pub mod surface;
pub mod touch;
pub mod view;

#[cfg(any(feature = "web-view-vivarium", feature = "web-view-dynamic-load"))]
pub mod engine;
#[cfg(feature = "testing")]
pub mod test_double;

pub use backend::{
    BackendError, BrowserBackend, Capabilities, Done, EngineInfo, EngineSettings, EngineState,
    FindQuery, RemoteSettings, TabConfig,
};
pub use environment::{PageEnvironment, PageScheme};
pub use events::{BrowserEvent, Disposition, EngineEvent, FailureCause, NewWindow, TabEvent};
pub use ids::{DownloadId, RequestId, TabId};
pub use page::{Audio, GoneReason, HistoryEntry, LoadError, LoadState, Security, SecurityState};
pub use report::{CheckStatus, ProcessSandbox, SecurityCheck, SecurityReport};
pub use requests::{Request, RequestKind, Response};
pub use surface::PageSurface;
pub use view::{WebView, web_view};
