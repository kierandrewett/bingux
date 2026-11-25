//! The places where Gecko would show a window or ask a person (`VV_REQUEST_*`).
//! The engine sends one as an event with a [`RequestId`] and waits for the
//! host's [`Response`]; the host draws all of that UI.
//!
//! ABI 1.1 declares the kinds and produces none of them yet, so these types
//! are the browser's requirement, written down once. When the ABI gives a
//! payload its fields, `engine.rs` maps it onto these; the UI does not change.

use std::path::PathBuf;

use crate::web_view::ids::{DownloadId, RequestId};

/// What a page asks to use.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PermissionKind {
    Geolocation,
    Notifications,
    Camera,
    Microphone,
    /// Screen sharing.
    Screen,
    /// Reading the clipboard.
    Clipboard,
    Midi,
    /// Playing sound without a gesture.
    Autoplay,
    /// Keeping its data when storage runs low.
    PersistentStorage,
    /// Opening windows the person did not ask for.
    Popups,
    Other(String),
}

/// A page (`origin`) asks to use one or more things.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionRequest {
    pub origin: String,
    pub kinds: Vec<PermissionKind>,
}

/// The kind of a page's own dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogKind {
    Alert,
    Confirm,
    /// The text the field starts with.
    Prompt { default: String },
    /// "Leave this page?": the page has unsaved work.
    BeforeUnload,
}

/// `alert`, `confirm`, `prompt` or `beforeunload`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogRequest {
    pub origin: String,
    pub kind: DialogKind,
    pub message: String,
}

/// A server or a proxy wants a name and a password.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthRequest {
    pub host: String,
    pub realm: String,
    /// The proxy asks, not the site.
    pub proxy: bool,
    /// The last attempt was refused.
    pub retry: bool,
}

/// How the certificate fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CertProblem {
    /// Signed by someone the engine does not trust (`SEC_ERROR_UNKNOWN_ISSUER`).
    UnknownIssuer,
    /// Self-signed.
    SelfSigned,
    Expired,
    NotYetValid,
    /// Issued for another name (`SSL_ERROR_BAD_CERT_DOMAIN`).
    WrongHost,
    Revoked,
    Other,
}

/// A secure connection that cannot be trusted. The error page the host draws
/// says what is wrong and offers "Go back", and for an overridable problem
/// "Details" with "Proceed".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertError {
    pub host: String,
    /// The engine's code (`SEC_ERROR_UNKNOWN_ISSUER`).
    pub code: String,
    pub problem: CertProblem,
    pub issuer: Option<String>,
    pub fingerprint: Option<String>,
    /// For an expired or not yet valid certificate, when.
    pub valid_until: Option<String>,
    /// The person may proceed. A pinned (HSTS) site cannot be overridden.
    pub overridable: bool,
}

/// What a right click landed on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContextTarget {
    /// In the view's logical pixels.
    pub position: (f32, f32),
    /// A link: its address and its text.
    pub link: Option<(String, String)>,
    /// An image: its address.
    pub image: Option<String>,
    /// Audio or video: its address and whether it is a video.
    pub media: Option<(String, bool)>,
    /// The selected text.
    pub selection: Option<String>,
    /// A text field or another editor.
    pub editable: bool,
    /// A password field: no Copy, no spelling.
    pub password: bool,
    /// The address of the frame the click was in, when not the top page.
    pub frame: Option<String>,
}

/// One entry of a `<select>` list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectOption {
    pub label: String,
    pub selected: bool,
    pub disabled: bool,
}

/// A `<select>` the page opened: the host draws the list and answers with the
/// index chosen.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectRequest {
    pub options: Vec<SelectOption>,
    /// Where the control is, in the view's logical pixels.
    pub anchor: (f32, f32, f32, f32),
}

/// What a file picker may return.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerMode {
    Open,
    OpenMany,
    Folder,
    Save,
}

/// `<input type=file>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerRequest {
    pub mode: PickerMode,
    /// MIME types and extensions the page accepts.
    pub accept: Vec<String>,
    pub suggested_name: Option<String>,
}

/// What the engine asks, and nothing else.
#[derive(Clone, Debug, PartialEq)]
pub enum RequestKind {
    Permission(PermissionRequest),
    Dialog(DialogRequest),
    Auth(AuthRequest),
    CertError(CertError),
    ContextMenu(ContextTarget),
    Select(SelectRequest),
    FilePicker(PickerRequest),
}

/// The host's answer. The variant must match the request's kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Response {
    Permission {
        allow: bool,
        /// Keep the decision for the origin.
        remember: bool,
    },
    Dialog {
        accept: bool,
        /// The text of a prompt.
        text: Option<String>,
    },
    Auth {
        /// `None` cancels.
        credentials: Option<(String, String)>,
    },
    CertError {
        proceed: bool,
    },
    /// The menu was closed (an item was taken from it, or it was dismissed).
    ContextMenu,
    Select {
        /// `None` closes the list without a choice.
        index: Option<usize>,
    },
    FilePicker {
        /// Empty cancels.
        paths: Vec<PathBuf>,
    },
}

/// What a download is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadState {
    Running,
    Paused,
    Finished,
    Failed,
    Cancelled,
}

/// A file the engine is saving, or has saved.
#[derive(Clone, Debug, PartialEq)]
pub struct DownloadInfo {
    pub id: DownloadId,
    pub url: String,
    /// The file's name, as it is saved.
    pub filename: String,
    /// Where it is saved, once known.
    pub path: Option<PathBuf>,
    pub mime: String,
    pub state: DownloadState,
    pub received: u64,
    /// `None` when the server did not say.
    pub total: Option<u64>,
    /// Bytes a second, smoothed.
    pub speed: u64,
}

/// A question with its answer's name, for the log of what the engine asked.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub id: RequestId,
    pub kind: RequestKind,
}
