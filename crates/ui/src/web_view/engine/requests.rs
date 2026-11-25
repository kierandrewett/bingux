//! The engine's requests in the control's words, and the control's answers
//! in the engine's.
//!
//! The crate hands each request to a delegate method with the question (an
//! `Ask` of its kind) and a typed `Responder` that answers it. The control
//! already has one model for questions: a [`Request`](crate::web_view::Request)
//! out, a [`Response`] back. This module is the bridge between the two and
//! adds no third: a question for the person becomes a control
//! [`RequestKind`], its responder waits in [`Open`] under the control's
//! [`RequestId`](crate::web_view::RequestId), and [`answer`] turns the
//! control's [`Response`] into the reply type of that responder.
//!
//! **Default-deny.** The backend subscribes only to the kinds in
//! [`super::caps::ANSWERED`] the engine accepts, so every other kind gets the
//! engine's safe default at once: nothing is granted, saved, launched,
//! overridden or remembered, and no page waits on the host. A responder that
//! is dropped (a closed tab, a failed engine, an answer that does not fit)
//! answers the default of its kind.
//!
//! Everything in a request that came from a page is hostile text: it is
//! copied, never interpreted, and the UI shows it as text.

use std::path::{Path, PathBuf};

use vivarium::certificates::{CertErrorFlags, CertErrorReply, CertErrorRequest, CertProblems};
use vivarium::downloads::{DownloadEvent, DownloadState as EngineDownload};
use vivarium::error::Error;
use vivarium::navigation::{NewWindowFlags, NewWindowRequest, TargetWindow};
use vivarium::permissions::{PermissionItem, PermissionReply, PermissionType};
use vivarium::prompts::{
    AuthFlags, AuthReply, AuthRequest as EngineAuth, ContextMenuFlags, ContextMenuReply,
    ContextMenuRequest, DialogKind as EngineDialog, FilePickerReply, FilePickerRequest,
    JsDialogReply, JsDialogRequest, PickedFile, SelectOptionFlags, SelectReply,
    SelectRequest as EngineSelect,
};
use vivarium::request::{Remember, RequestId as EngineRequest, Responder};
use vivarium::security::Origin;

use crate::web_view::events::{Disposition, NewWindow, TabEvent};
use crate::web_view::ids::TabId;
use crate::web_view::requests::{
    AuthRequest, CertError, CertProblem, ContextTarget, DialogKind, DialogRequest, DownloadInfo,
    DownloadState, PermissionKind, PickerMode, PickerRequest, Response, SelectOption,
    SelectRequest,
};

/// The origin to show for a request: the asking frame's, else the page's.
pub(super) fn origin_text(origin: &Origin, top_origin: &Origin) -> String {
    [origin, top_origin]
        .into_iter()
        .map(|origin| origin.serialized.as_str())
        .find(|text| !text.is_empty() && *text != "null")
        .unwrap_or_default()
        .to_string()
}

pub(super) fn permission_kind(item: &PermissionItem) -> PermissionKind {
    use PermissionType as T;
    match item.kind {
        T::Geolocation => PermissionKind::Geolocation,
        T::DesktopNotification => PermissionKind::Notifications,
        T::Camera => PermissionKind::Camera,
        T::Microphone => PermissionKind::Microphone,
        T::Screen => PermissionKind::Screen,
        T::ClipboardRead => PermissionKind::Clipboard,
        T::Midi | T::MidiSysex => PermissionKind::Midi,
        T::AutoplayAudible | T::AutoplayInaudible => PermissionKind::Autoplay,
        T::PersistentStorage => PermissionKind::PersistentStorage,
        T::Popups => PermissionKind::Popups,
        // A type with no word of its own: the engine's name for it, as text.
        _ => PermissionKind::Other(if item.type_name.is_empty() {
            format!("{:?}", item.kind)
        } else {
            item.type_name.clone()
        }),
    }
}

/// The most telling problem of a certificate: a revoked one before a
/// self-signed one before an unknown issuer, then the rest.
fn cert_problem(problems: CertProblems) -> CertProblem {
    [
        (CertProblems::REVOKED, CertProblem::Revoked),
        (CertProblems::SELF_SIGNED, CertProblem::SelfSigned),
        (CertProblems::UNTRUSTED, CertProblem::UnknownIssuer),
        (CertProblems::EXPIRED, CertProblem::Expired),
        (CertProblems::NOT_YET_VALID, CertProblem::NotYetValid),
        (CertProblems::WRONG_HOST, CertProblem::WrongHost),
    ]
    .into_iter()
    .find(|(flag, _)| problems.contains(*flag))
    .map_or(CertProblem::Other, |(_, problem)| problem)
}

/// `1700000000` as `14 November 2023` (UTC), as the browser writes a date.
pub(super) fn date(unix: i64) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let z = unix.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{day} {} {year}", MONTHS[(month - 1) as usize])
}

/// `ab12cd` as `AB:12:CD`.
fn fingerprint(hex: &str) -> Option<String> {
    (!hex.is_empty()).then(|| {
        hex.as_bytes()
            .chunks(2)
            .map(|pair| String::from_utf8_lossy(pair).to_ascii_uppercase())
            .collect::<Vec<_>>()
            .join(":")
    })
}

pub(super) fn cert_error(request: &CertErrorRequest) -> CertError {
    let problem = cert_problem(request.problems);
    let leaf = request.chain.first();
    CertError {
        host: request.host.clone(),
        code: request.error_name.clone(),
        problem,
        issuer: leaf
            .map(|cert| cert.issuer.clone())
            .filter(|issuer| !issuer.is_empty()),
        fingerprint: leaf.and_then(|cert| fingerprint(&cert.sha256_hex)),
        valid_until: leaf.and_then(|cert| match problem {
            CertProblem::Expired => Some(date(cert.not_after_unix)),
            CertProblem::NotYetValid => Some(date(cert.not_before_unix)),
            _ => None,
        }),
        overridable: request.flags.contains(CertErrorFlags::OVERRIDABLE),
    }
}

pub(super) fn context_target(menu: &ContextMenuRequest) -> ContextTarget {
    use ContextMenuFlags as F;
    let text = |text: &str| (!text.is_empty()).then(|| text.to_string());
    ContextTarget {
        position: (menu.x, menu.y),
        link: menu
            .flags
            .contains(F::LINK)
            .then(|| (menu.link_url.clone(), menu.link_text.clone())),
        image: menu
            .flags
            .contains(F::IMAGE)
            .then(|| menu.image_url.clone()),
        media: menu
            .flags
            .intersects(F::VIDEO | F::AUDIO)
            .then(|| (menu.media_url.clone(), menu.flags.contains(F::VIDEO))),
        selection: menu
            .flags
            .contains(F::SELECTION)
            .then(|| menu.selection_text.clone())
            .and_then(|selected| text(&selected)),
        editable: menu.flags.contains(F::EDITABLE),
        password: menu.flags.contains(F::PASSWORD),
        frame: menu
            .flags
            .contains(F::FRAME)
            .then(|| menu.frame_url.clone())
            .and_then(|url| text(&url)),
    }
}

pub(super) fn dialog(origin: String, dialog: &JsDialogRequest) -> DialogRequest {
    let kind = match dialog.dialog {
        EngineDialog::Alert => DialogKind::Alert,
        EngineDialog::Prompt => DialogKind::Prompt {
            default: dialog.default_text.clone(),
        },
        EngineDialog::BeforeUnload => DialogKind::BeforeUnload,
        // Reload or back to a POST: the question is whether to send it again.
        _ => DialogKind::Confirm,
    };
    let message = if dialog.dialog == EngineDialog::Repost && dialog.message.is_empty() {
        "To show this page again the form must be sent again. Sending it again may repeat what it did the first time.".to_string()
    } else {
        dialog.message.clone()
    };
    DialogRequest {
        origin,
        kind,
        message,
    }
}

pub(super) fn auth(auth: &EngineAuth) -> AuthRequest {
    // `host`, with the port when it is not the scheme's own.
    let host = match auth.port {
        0 | 80 | 443 => auth.host.clone(),
        port => format!("{}:{port}", auth.host),
    };
    AuthRequest {
        host,
        realm: auth.realm.clone(),
        proxy: auth.flags.contains(AuthFlags::PROXY),
        retry: auth.flags.contains(AuthFlags::RETRY),
    }
}

pub(super) fn select(select: &EngineSelect) -> SelectRequest {
    SelectRequest {
        options: select
            .options
            .iter()
            .map(|option| SelectOption {
                label: option.label.clone(),
                selected: option.flags.contains(SelectOptionFlags::SELECTED),
                // A group's heading is in the list but cannot be chosen.
                disabled: option
                    .flags
                    .intersects(SelectOptionFlags::DISABLED | SelectOptionFlags::GROUP),
            })
            .collect(),
        anchor: (
            select.anchor.x,
            select.anchor.y,
            select.anchor.w,
            select.anchor.h,
        ),
    }
}

/// ABI 2.0 asks only for files to open (one or several): a folder or a
/// place to save is not a question it asks.
pub(super) fn picker(picker: &FilePickerRequest) -> PickerRequest {
    PickerRequest {
        mode: if picker.multiple {
            PickerMode::OpenMany
        } else {
            PickerMode::Open
        },
        accept: picker.accept.clone(),
        suggested_name: None,
    }
}

fn disposition(target: TargetWindow) -> Disposition {
    match target {
        TargetWindow::NewTabBackground => Disposition::BackgroundTab,
        TargetWindow::NewWindow => Disposition::Window,
        TargetWindow::NewPopup => Disposition::Popup,
        TargetWindow::Current => Disposition::Current,
        _ => Disposition::Tab,
    }
}

/// `window.open`: a tab, if the browser takes it, in the engine's
/// provisional session `new`.
pub(super) fn new_tab(new: TabId, origin: String, window: &NewWindowRequest) -> TabEvent {
    use NewWindowFlags as F;
    TabEvent::NewTab {
        new,
        url: window.url.clone(),
        name: window.target_name.clone(),
        no_opener: window.flags.contains(F::NO_OPENER),
        window: NewWindow {
            disposition: disposition(window.target_window),
            origin,
            user_gesture: window.flags.contains(F::USER_GESTURE),
            unrequested: window.flags.contains(F::FROM_SCRIPT_ON_LOAD),
            no_referrer: window.flags.contains(F::NO_REFERRER),
            size: (window.flags.contains(F::HAS_SIZE) && window.size.0 > 0 && window.size.1 > 0)
                .then_some(window.size),
        },
    }
}

/// Where a download goes: its suggested name inside the download folder.
/// The engine sanitises the name; a name that is still a path, or nothing,
/// does not escape the folder.
pub(super) fn download_target(folder: &Path, suggested: &str) -> PathBuf {
    let name = Path::new(suggested)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty() && *name != "." && *name != "..")
        .unwrap_or("download");
    folder.join(name)
}

/// A question the engine waits on, by kind: the responder that answers it,
/// and what the answer needs to know of the question.
pub(super) enum Open {
    Permission(Responder<PermissionReply>),
    /// A `prompt()` takes the text the person typed; the others do not.
    Dialog {
        prompt: bool,
        reply: Responder<JsDialogReply>,
    },
    Auth(Responder<AuthReply>),
    CertError(Responder<CertErrorReply>),
    ContextMenu(Responder<ContextMenuReply>),
    Select(Responder<SelectReply>),
    FilePicker(Responder<FilePickerReply>),
}

impl Open {
    /// The engine's id of the request.
    pub(super) fn id(&self) -> EngineRequest {
        match self {
            Self::Permission(reply) => reply.id(),
            Self::Dialog { reply, .. } => reply.id(),
            Self::Auth(reply) => reply.id(),
            Self::CertError(reply) => reply.id(),
            Self::ContextMenu(reply) => reply.id(),
            Self::Select(reply) => reply.id(),
            Self::FilePicker(reply) => reply.id(),
        }
    }
}

/// Why an answer was not sent.
#[derive(Debug)]
pub(super) enum Unanswered {
    /// The answer belongs to another kind of question: a bug of the caller.
    /// The responder is dropped, so the engine's default stands.
    Mismatch,
    /// The engine refused it or the request had ended.
    Engine(Error),
}

/// A file the person chose, for the page: opened for reading, under its
/// name. The page never learns the path. A folder, a file that vanished or
/// cannot be read, or a name the page may not see is left out.
pub(super) fn picked(path: &Path) -> Option<PickedFile> {
    let file = std::fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let name = path.file_name()?.to_string_lossy().into_owned();
    PickedFile::new(name, file).ok()
}

fn remember(remember: bool) -> Remember {
    // The browser keeps what the person chose while the window is open.
    if remember {
        Remember::Session
    } else {
        Remember::None
    }
}

/// Sends the control's answer through the responder of its question.
pub(super) fn answer(open: Open, response: Response) -> Result<(), Unanswered> {
    let sent = match (open, response) {
        (
            Open::Permission(reply),
            Response::Permission {
                allow,
                remember: keep,
            },
        ) => reply.answer(if allow {
            PermissionReply::allow_all(remember(keep))
        } else {
            PermissionReply::deny_all(remember(keep))
        }),
        (Open::Dialog { prompt, reply }, Response::Dialog { accept, text }) => {
            reply.answer(match (accept, prompt) {
                // A dialog has no "deny": closing it is the page's `false` or `null`.
                (false, _) => JsDialogReply::dismiss(),
                (true, true) => JsDialogReply::accept_text(text.unwrap_or_default()),
                (true, false) => JsDialogReply::accept(),
            })
        }
        (Open::Auth(reply), Response::Auth { credentials }) => reply.answer(match credentials {
            Some((user, password)) => AuthReply::credentials(user, password, false),
            None => AuthReply::Cancel,
        }),
        (Open::CertError(reply), Response::CertError { proceed }) => {
            // The exception is for this window, not for good.
            reply.answer(if proceed {
                CertErrorReply::Proceed(Remember::Session)
            } else {
                CertErrorReply::Refuse
            })
        }
        // The host drew the menu and finished with it.
        (Open::ContextMenu(reply), Response::ContextMenu) => {
            reply.answer(ContextMenuReply::Dismiss)
        }
        (Open::Select(reply), Response::Select { index }) => reply.answer(match index {
            Some(at) => SelectReply::Choose(vec![at as u32]),
            None => SelectReply::Cancel,
        }),
        (Open::FilePicker(reply), Response::FilePicker { paths }) => {
            // Opening the files can wait on a slow disk: it is done on a
            // thread of its own, never on the UI thread that answers. A
            // thread that does not start drops the responder: the default.
            let _ = std::thread::Builder::new()
                .name("web-view-picker".to_string())
                .spawn(move || {
                    let files: Vec<PickedFile> =
                        paths.iter().filter_map(|path| picked(path)).collect();
                    let _ = reply.answer(if files.is_empty() {
                        FilePickerReply::Cancel
                    } else {
                        FilePickerReply::Files(files)
                    });
                });
            Ok(())
        }
        _ => return Err(Unanswered::Mismatch),
    };
    sent.map_err(Unanswered::Engine)
}

/// A download's state in the browser's words; `None` for one gone from the
/// engine's list (the browser's own list keeps its entry).
pub(super) fn download_state(state: EngineDownload) -> Option<DownloadState> {
    Some(match state {
        EngineDownload::Started | EngineDownload::Progress => DownloadState::Running,
        EngineDownload::Paused => DownloadState::Paused,
        EngineDownload::Succeeded => DownloadState::Finished,
        EngineDownload::Failed => DownloadState::Failed,
        EngineDownload::Canceled => DownloadState::Cancelled,
        _ => return None,
    })
}

/// The browser's account of a download from the engine's.
pub(super) fn download_info(event: &DownloadEvent) -> Option<DownloadInfo> {
    let state = download_state(event.state)?;
    Some(DownloadInfo {
        id: crate::web_view::ids::DownloadId(event.download_id.get()),
        url: event.url.clone(),
        filename: event.filename.clone(),
        path: (!event.target_path.is_empty()).then(|| PathBuf::from(&event.target_path)),
        mime: event.mime.clone(),
        state,
        received: event.received,
        total: event.total,
        speed: event.speed_bps,
    })
}

#[cfg(test)]
mod tests;
