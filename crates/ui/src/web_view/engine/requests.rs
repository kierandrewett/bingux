//! The engine's requests in the browser's words, and the browser's answers in
//! the engine's.
//!
//! **Default-deny.** The backend subscribes (`vv_engine_set_request_handlers`)
//! only to the kinds in [`super::caps::ANSWERED`] that the engine announces, so
//! every other kind gets the engine's safe default at once: nothing is granted,
//! saved, launched, overridden or remembered, and no page waits on the host.
//! Of the kinds it does answer, a request whose payload the browser cannot
//! show is answered with the same default here, not left to its deadline.
//!
//! Everything in a request that came from a page is hostile text: it is
//! copied, never interpreted, and the UI shows it as text.

use std::path::{Path, PathBuf};

use vivarium as vv;

use crate::web_view::events::{Disposition, NewWindow};
use crate::web_view::requests::{
    AuthRequest, CertError, CertProblem, ContextTarget, DialogKind, DialogRequest, DownloadInfo, DownloadState, PermissionKind,
    PermissionRequest, PickerMode, PickerRequest, RequestKind, Response, SelectOption, SelectRequest,
};

/// What a request is to the browser.
#[derive(Debug, PartialEq)]
pub(super) enum Incoming {
    /// A question for the person.
    Ask(RequestKind),
    /// `window.open`: a tab, if the browser takes it.
    Window { url: String, name: String, no_opener: bool, window: NewWindow },
    /// A file the engine is about to save: where it goes.
    Download(PathBuf),
    /// Not one the browser answers (or one it cannot show): the default.
    Default,
}

/// The origin to show for a request: the asking frame's, else the page's.
fn origin_text(request: &vv::Request) -> String {
    [&request.origin, &request.top_origin]
        .into_iter()
        .map(|origin| origin.serialized.as_str())
        .find(|text| !text.is_empty() && *text != "null")
        .unwrap_or_default()
        .to_string()
}

fn permission_kind(item: &vv::PermissionItem) -> PermissionKind {
    use vv::PermissionType as T;
    match item.kind {
        T::Geolocation => PermissionKind::Geolocation,
        T::Notifications => PermissionKind::Notifications,
        T::Camera => PermissionKind::Camera,
        T::Microphone => PermissionKind::Microphone,
        T::Screen => PermissionKind::Screen,
        T::ClipboardRead => PermissionKind::Clipboard,
        T::Midi | T::MidiSysex => PermissionKind::Midi,
        T::AutoplayAudible | T::AutoplayInaudible => PermissionKind::Autoplay,
        T::PersistentStorage => PermissionKind::PersistentStorage,
        T::Popups => PermissionKind::Popups,
        // A type with no word of its own: the engine's name for it, as text.
        _ => PermissionKind::Other(if item.type_name.is_empty() { format!("{:?}", item.kind) } else { item.type_name.clone() }),
    }
}

/// The most telling problem of a certificate: a revoked one before a
/// self-signed one before an unknown issuer, then the rest.
fn cert_problem(problems: vv::CertProblems) -> CertProblem {
    [
        (vv::CertProblems::REVOKED, CertProblem::Revoked),
        (vv::CertProblems::SELF_SIGNED, CertProblem::SelfSigned),
        (vv::CertProblems::UNTRUSTED, CertProblem::UnknownIssuer),
        (vv::CertProblems::EXPIRED, CertProblem::Expired),
        (vv::CertProblems::NOT_YET_VALID, CertProblem::NotYetValid),
        (vv::CertProblems::WRONG_HOST, CertProblem::WrongHost),
    ]
    .into_iter()
    .find(|(flag, _)| problems.contains(*flag))
    .map_or(CertProblem::Other, |(_, problem)| problem)
}

/// `1700000000` as `14 November 2023` (UTC), as the browser writes a date.
pub(super) fn date(unix: i64) -> String {
    const MONTHS: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December",
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

fn cert_error(request: &vv::CertErrorRequest) -> CertError {
    let problem = cert_problem(request.problems);
    let leaf = request.chain.first();
    CertError {
        host: request.host.clone(),
        code: request.error_name.clone(),
        problem,
        issuer: leaf.map(|cert| cert.issuer.clone()).filter(|issuer| !issuer.is_empty()),
        fingerprint: leaf.and_then(|cert| fingerprint(&cert.sha256_hex)),
        valid_until: leaf.and_then(|cert| match problem {
            CertProblem::Expired => Some(date(cert.not_after_unix)),
            CertProblem::NotYetValid => Some(date(cert.not_before_unix)),
            _ => None,
        }),
        overridable: request.flags.contains(vv::CertErrorFlags::OVERRIDABLE),
    }
}

fn context_target(menu: &vv::ContextMenuRequest) -> ContextTarget {
    use vv::ContextMenuFlags as F;
    let text = |text: &str| (!text.is_empty()).then(|| text.to_string());
    ContextTarget {
        position: (menu.x, menu.y),
        link: menu.flags.contains(F::LINK).then(|| (menu.link_url.clone(), menu.link_text.clone())),
        image: menu.flags.contains(F::IMAGE).then(|| menu.image_url.clone()),
        media: menu
            .flags
            .intersects(F::VIDEO | F::AUDIO)
            .then(|| (menu.media_url.clone(), menu.flags.contains(F::VIDEO))),
        selection: menu.flags.contains(F::SELECTION).then(|| menu.selection_text.clone()).and_then(|selected| text(&selected)),
        editable: menu.flags.contains(F::EDITABLE),
        password: menu.flags.contains(F::PASSWORD),
        frame: menu.flags.contains(F::FRAME).then(|| menu.frame_url.clone()).and_then(|url| text(&url)),
    }
}

fn dialog(request: &vv::Request, dialog: &vv::JsDialogRequest) -> DialogRequest {
    let kind = match dialog.dialog {
        vv::DialogKind::Alert => DialogKind::Alert,
        vv::DialogKind::Prompt => DialogKind::Prompt { default: dialog.default_text.clone() },
        vv::DialogKind::BeforeUnload => DialogKind::BeforeUnload,
        // Reload or back to a POST: the question is whether to send it again.
        _ => DialogKind::Confirm,
    };
    let message = if dialog.dialog == vv::DialogKind::Repost && dialog.message.is_empty() {
        "To show this page again the form must be sent again. Sending it again may repeat what it did the first time.".to_string()
    } else {
        dialog.message.clone()
    };
    DialogRequest {
        origin: origin_text(request),
        kind,
        message,
    }
}

/// `host`, with the port when it is not the scheme's own.
fn auth_host(auth: &vv::AuthRequest) -> String {
    match auth.port {
        0 | 80 | 443 => auth.host.clone(),
        port => format!("{}:{port}", auth.host),
    }
}

fn select(select: &vv::SelectRequest) -> SelectRequest {
    SelectRequest {
        options: select
            .options
            .iter()
            .map(|option| SelectOption {
                label: option.label.clone(),
                selected: option.flags.contains(vv::SelectOptionFlags::SELECTED),
                // A group's heading is in the list but cannot be chosen.
                disabled: option.flags.intersects(vv::SelectOptionFlags::DISABLED | vv::SelectOptionFlags::GROUP),
            })
            .collect(),
        anchor: (select.anchor.x, select.anchor.y, select.anchor.w, select.anchor.h),
    }
}

fn picker(picker: &vv::FilePickerRequest) -> PickerRequest {
    PickerRequest {
        mode: match picker.mode {
            vv::PickerMode::OpenMultiple => PickerMode::OpenMany,
            vv::PickerMode::Folder => PickerMode::Folder,
            vv::PickerMode::Save => PickerMode::Save,
            _ => PickerMode::Open,
        },
        accept: picker.accept.clone(),
        suggested_name: (!picker.suggested_name.is_empty()).then(|| picker.suggested_name.clone()),
    }
}

fn disposition(disposition: vv::WindowDisposition) -> Disposition {
    match disposition {
        vv::WindowDisposition::BackgroundTab => Disposition::BackgroundTab,
        vv::WindowDisposition::Window => Disposition::Window,
        vv::WindowDisposition::Popup => Disposition::Popup,
        vv::WindowDisposition::Current => Disposition::Current,
        _ => Disposition::Tab,
    }
}

fn window(request: &vv::Request, window: &vv::NewWindowRequest) -> Incoming {
    use vv::NewWindowFlags as F;
    Incoming::Window {
        url: window.url.clone(),
        name: window.target_name.clone(),
        no_opener: window.flags.contains(F::NO_OPENER),
        window: NewWindow {
            disposition: disposition(window.disposition),
            origin: origin_text(request),
            user_gesture: window.flags.contains(F::USER_GESTURE),
            unrequested: window.flags.contains(F::FROM_SCRIPT_ON_LOAD),
            no_referrer: window.flags.contains(F::NO_REFERRER),
            size: (window.flags.contains(F::HAS_SIZE) && window.size.0 > 0 && window.size.1 > 0).then_some(window.size),
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

/// What a request is to the browser, from what the engine sent.
pub(super) fn classify(request: &vv::Request, downloads: &Path) -> Incoming {
    use vv::RequestPayload as P;
    Incoming::Ask(match &request.payload {
        P::NewWindow(new) => return window(request, new),
        P::Download(download) => return Incoming::Download(download_target(downloads, &download.suggested_name)),
        P::Permission(permission) => RequestKind::Permission(PermissionRequest {
            origin: origin_text(request),
            kinds: permission.items.iter().map(permission_kind).collect(),
        }),
        P::JsDialog(js) => RequestKind::Dialog(dialog(request, js)),
        P::Auth(auth) => RequestKind::Auth(AuthRequest {
            host: auth_host(auth),
            realm: auth.realm.clone(),
            proxy: auth.flags.contains(vv::AuthFlags::PROXY),
            retry: auth.flags.contains(vv::AuthFlags::RETRY),
        }),
        P::CertError(error) => RequestKind::CertError(cert_error(error)),
        P::Select(list) => RequestKind::Select(select(list)),
        P::FilePicker(files) => RequestKind::FilePicker(picker(files)),
        P::ContextMenu(menu) => RequestKind::ContextMenu(context_target(menu)),
        _ => return Incoming::Default,
    })
}

/// A file the person chose, for the engine: opened for reading when it is to
/// be read (the page then never learns the path), and the path alone when it
/// is a place to write or a folder.
fn file_choice(path: &Path, to_read: bool) -> vv::FileChoice {
    let mut choice = vv::FileChoice::new(path);
    if to_read
        && let Ok(file) = std::fs::File::open(path)
        && let Ok(meta) = file.metadata()
        && meta.is_file()
    {
        choice.size = meta.len();
        choice.fd = Some(std::os::fd::OwnedFd::from(file));
    }
    choice
}

/// The browser's answer as the engine's, or why it does not fit the request.
/// A browser answer that does not belong to the kind of the request is a bug
/// of the caller: it is refused, and the engine's own default stands.
pub(super) fn response(request: &vv::Request, answer: Response) -> Result<vv::Response, &'static str> {
    use vv::RequestPayload as P;
    Ok(match (&request.payload, answer) {
        (P::Permission(_), Response::Permission { allow, remember }) => vv::Response::permission(
            if allow { vv::Answer::Allow } else { vv::Answer::Deny },
            // The browser keeps what the person chose while the window is open.
            if remember { vv::Remember::Session } else { vv::Remember::None },
        ),
        (P::JsDialog(js), Response::Dialog { accept, text }) => match (accept, js.dialog) {
            (false, _) => vv::Response::cancel(),
            (true, vv::DialogKind::Prompt) => vv::Response::prompt_text(text.unwrap_or_default()),
            (true, _) => vv::Response::allow(),
        },
        (P::Auth(_), Response::Auth { credentials }) => match credentials {
            Some((user, password)) => vv::Response::credentials(user, password, false),
            None => vv::Response::cancel(),
        },
        (P::CertError(_), Response::CertError { proceed }) => {
            if proceed {
                vv::Response::override_certificate(vv::Remember::Session)
            } else {
                vv::Response::deny()
            }
        }
        // The host drew the menu and finished with it.
        (P::ContextMenu(_), Response::ContextMenu) => vv::Response::handled(),
        (P::Select(_), Response::Select { index }) => match index {
            Some(at) => vv::Response::select(vec![at as u32]),
            None => vv::Response::cancel(),
        },
        (P::FilePicker(files), Response::FilePicker { paths }) => {
            if paths.is_empty() {
                vv::Response::cancel()
            } else {
                let to_read = matches!(files.mode, vv::PickerMode::Open | vv::PickerMode::OpenMultiple);
                vv::Response::files(paths.iter().map(|path| file_choice(path, to_read)).collect())
            }
        }
        _ => return Err("the answer does not belong to the question"),
    })
}

/// The browser's account of a download from the engine's.
pub(super) fn download_info(event: &vv::DownloadEvent) -> Option<DownloadInfo> {
    let state = match event.state {
        vv::DownloadState::Started | vv::DownloadState::Progress => DownloadState::Running,
        vv::DownloadState::Paused => DownloadState::Paused,
        vv::DownloadState::Finished => DownloadState::Finished,
        vv::DownloadState::Failed => DownloadState::Failed,
        vv::DownloadState::Cancelled => DownloadState::Cancelled,
        // Gone from the engine's list: the browser's own list keeps its entry.
        _ => return None,
    };
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
