//! The mapping of requests and answers, from hand-made engine values: no
//! engine is needed to say that a camera request reads as a camera request,
//! that a page's text arrives as text, and that each answer is the reply its
//! question takes. The answers themselves go through the crate's mock engine
//! in `events/tests.rs`.
//!
//! Failures the private-download tests guard against (a private download
//! must never reach a list or a file that outlives its window):
//!   1. a private download read as a normal one, so the window keeps it;
//!   2. one from the private identity without the flag read as normal;
//!   3. a normal download read as private;
//!   4. the identity lost, so a container's download is attributed to the
//!      default container.

use std::path::Path;

use vivarium::certificates::{CertFlags, CertInfo};
use vivarium::display::Rect;
use vivarium::permissions::{PermissionItemFlags, PermissionType};
use vivarium::prompts::{AuthScheme, SelectOption as EngineOption};
use vivarium::session::SessionId;

use super::*;

fn origin(text: &str) -> Origin {
    Origin {
        serialized: text.to_string(),
        ..Origin::default()
    }
}

fn item(kind: PermissionType, name: &str) -> PermissionItem {
    PermissionItem {
        kind,
        flags: PermissionItemFlags::empty(),
        type_name: name.to_string(),
        detail: String::new(),
    }
}

#[test]
fn camera_and_microphone_asked_together_are_two_things_of_one_prompt() {
    let kinds: Vec<PermissionKind> = [
        item(PermissionType::Camera, "camera"),
        item(PermissionType::Microphone, "microphone"),
    ]
    .iter()
    .map(permission_kind)
    .collect();
    assert_eq!(kinds, [PermissionKind::Camera, PermissionKind::Microphone]);
    assert_eq!(
        permission_kind(&item(
            PermissionType::DesktopNotification,
            "desktop-notification"
        )),
        PermissionKind::Notifications
    );
}

#[test]
fn a_permission_with_no_word_of_its_own_is_named_by_the_engine() {
    assert_eq!(
        permission_kind(&item(PermissionType::WebSerial, "web-serial")),
        PermissionKind::Other("web-serial".to_string())
    );
    assert_eq!(
        permission_kind(&item(PermissionType::MidiSysex, "midi-sysex")),
        PermissionKind::Midi
    );
}

#[test]
fn a_frame_with_no_origin_is_shown_as_the_page_that_holds_it() {
    assert_eq!(
        origin_text(
            &origin("https://example.org"),
            &origin("https://top.example")
        ),
        "https://example.org"
    );
    assert_eq!(
        origin_text(&origin("null"), &origin("https://top.example")),
        "https://top.example"
    );
    assert_eq!(
        origin_text(&origin("null"), &origin("")),
        "",
        "no origin is no origin, not \"null\""
    );
}

#[test]
fn every_dialog_the_engine_has_is_one_the_browser_shows() {
    let js = |kind, message: &str, default: &str| {
        dialog(
            "https://example.org".to_string(),
            &JsDialogRequest {
                dialog: kind,
                can_suppress: true,
                message: message.to_string(),
                default_text: default.to_string(),
            },
        )
    };
    let alert = js(EngineDialog::Alert, "Hello", "");
    assert_eq!(
        (alert.kind, alert.message.as_str(), alert.origin.as_str()),
        (DialogKind::Alert, "Hello", "https://example.org")
    );
    assert_eq!(
        js(EngineDialog::Prompt, "Name?", "Ada").kind,
        DialogKind::Prompt {
            default: "Ada".to_string()
        }
    );
    let leave = js(EngineDialog::BeforeUnload, "", "");
    assert_eq!(
        (leave.kind, leave.message.as_str()),
        (DialogKind::BeforeUnload, "")
    );
    let repost = js(EngineDialog::Repost, "", "");
    assert_eq!(
        repost.kind,
        DialogKind::Confirm,
        "resending a form is a yes or no"
    );
    assert!(
        repost.message.contains("sent again"),
        "and says so when the engine said nothing"
    );
}

#[test]
fn authentication_names_the_server_with_its_port_and_says_who_asks() {
    let asked = |port, flags| {
        auth(&EngineAuth {
            scheme: AuthScheme::Basic,
            flags,
            port,
            host: "intranet.example".to_string(),
            realm: "Staff".to_string(),
            username_hint: String::new(),
        })
    };
    assert_eq!(
        asked(443, AuthFlags::empty()).host,
        "intranet.example",
        "the scheme's own port is not shown"
    );
    let proxy = asked(8080, AuthFlags::PROXY | AuthFlags::RETRY);
    assert_eq!(
        (
            proxy.host.as_str(),
            proxy.realm.as_str(),
            proxy.proxy,
            proxy.retry
        ),
        ("intranet.example:8080", "Staff", true, true)
    );
}

fn cert(problems: CertProblems, flags: CertErrorFlags) -> CertError {
    let leaf = CertInfo {
        flags: CertFlags::empty(),
        subject: "CN=example.org".to_string(),
        issuer: "CN=Some CA".to_string(),
        serial_hex: "01".to_string(),
        sha256_hex: "ab12cd".to_string(),
        not_before_unix: 0,
        not_after_unix: 1_700_000_000,
        sans: Vec::new(),
        der: Vec::new(),
    };
    cert_error(&CertErrorRequest {
        flags,
        problems,
        port: 443,
        error_code: 1,
        host: "expired.example.org".to_string(),
        error_name: "SEC_ERROR_EXPIRED_CERTIFICATE".to_string(),
        chain: vec![leaf],
    })
}

#[test]
fn a_certificate_error_says_what_is_wrong_who_issued_it_and_when_it_ran_out() {
    let error = cert(
        CertProblems::EXPIRED,
        CertErrorFlags::OVERRIDABLE | CertErrorFlags::TOP_LEVEL,
    );
    assert_eq!(error.problem, CertProblem::Expired);
    assert_eq!(error.code, "SEC_ERROR_EXPIRED_CERTIFICATE");
    assert_eq!(error.issuer.as_deref(), Some("CN=Some CA"));
    assert_eq!(error.fingerprint.as_deref(), Some("AB:12:CD"));
    assert_eq!(error.valid_until.as_deref(), Some("14 November 2023"));
    assert!(error.overridable);
}

#[test]
fn a_pinned_site_cannot_be_overridden_and_the_worst_problem_wins() {
    let error = cert(
        CertProblems::UNTRUSTED | CertProblems::SELF_SIGNED | CertProblems::EXPIRED,
        CertErrorFlags::HSTS,
    );
    assert!(!error.overridable, "the engine did not say it may be");
    assert_eq!(error.problem, CertProblem::SelfSigned);
    assert_eq!(
        error.valid_until, None,
        "only an expired or not yet valid one has a date"
    );
    assert_eq!(
        cert(
            CertProblems::REVOKED | CertProblems::UNTRUSTED,
            CertErrorFlags::empty()
        )
        .problem,
        CertProblem::Revoked
    );
    assert_eq!(
        cert(CertProblems::WEAK_KEY, CertErrorFlags::empty()).problem,
        CertProblem::Other
    );
}

#[test]
fn dates_and_fingerprints_read_as_a_person_reads_them() {
    assert_eq!(date(0), "1 January 1970");
    assert_eq!(date(1_700_000_000), "14 November 2023");
    assert_eq!(date(951_782_400), "29 February 2000", "a leap day");
    assert_eq!(date(-86_400), "31 December 1969");
    assert_eq!(fingerprint("00ff"), Some("00:FF".to_string()));
    assert_eq!(fingerprint(""), None);
}

#[test]
fn the_context_menu_says_what_is_under_the_pointer_and_nothing_else() {
    use ContextMenuFlags as F;
    let menu = |flags| {
        context_target(&ContextMenuRequest {
            flags,
            x: 12.0,
            y: 34.0,
            link_url: "https://l.example/".to_string(),
            link_text: "A link".to_string(),
            image_url: "https://i.example/a.png".to_string(),
            media_url: "https://m.example/v.mp4".to_string(),
            frame_url: "https://f.example/".to_string(),
            page_url: "https://example.org/".to_string(),
            selection_text: "chosen words".to_string(),
            misspelled_word: String::new(),
            suggestions: Vec::new(),
            input_type: String::new(),
        })
    };
    let on_link = menu(F::LINK | F::SELECTION);
    assert_eq!(on_link.position, (12.0, 34.0));
    assert_eq!(
        on_link.link,
        Some(("https://l.example/".to_string(), "A link".to_string()))
    );
    assert_eq!(on_link.selection.as_deref(), Some("chosen words"));
    assert_eq!(
        (
            on_link.image,
            on_link.media,
            on_link.frame,
            on_link.editable,
            on_link.password
        ),
        (None, None, None, false, false)
    );
    let video = menu(F::VIDEO | F::FRAME | F::EDITABLE | F::PASSWORD | F::IMAGE);
    assert_eq!(
        video.media,
        Some(("https://m.example/v.mp4".to_string(), true))
    );
    assert_eq!(video.image.as_deref(), Some("https://i.example/a.png"));
    assert_eq!(video.frame.as_deref(), Some("https://f.example/"));
    assert!(video.editable && video.password);
    assert_eq!(menu(F::AUDIO).media.map(|(_, video)| video), Some(false));
    assert!(menu(F::SELECTION).selection.is_some());
    assert_eq!(
        menu(F::empty()),
        ContextTarget {
            position: (12.0, 34.0),
            ..ContextTarget::default()
        }
    );
}

#[test]
fn a_select_keeps_every_option_in_place_and_a_group_heading_cannot_be_chosen() {
    let option = |label: &str, flags| EngineOption {
        label: label.to_string(),
        flags,
    };
    let list = select(&EngineSelect {
        rtl: false,
        anchor: Rect {
            x: 1.0,
            y: 2.0,
            w: 3.0,
            h: 4.0,
        },
        options: vec![
            option("Fruit", SelectOptionFlags::GROUP),
            option(
                "Apple",
                SelectOptionFlags::IN_GROUP | SelectOptionFlags::SELECTED,
            ),
            option(
                "Pear",
                SelectOptionFlags::IN_GROUP | SelectOptionFlags::DISABLED,
            ),
        ],
        font_size_px: 14.0,
        text_color: 0,
        background_color: 0,
    });
    assert_eq!(list.anchor, (1.0, 2.0, 3.0, 4.0));
    let shown: Vec<(&str, bool, bool)> = list
        .options
        .iter()
        .map(|o| (o.label.as_str(), o.selected, o.disabled))
        .collect();
    assert_eq!(
        shown,
        [
            ("Fruit", false, true),
            ("Apple", true, false),
            ("Pear", false, true)
        ]
    );
}

#[test]
fn a_file_picker_asks_for_one_file_or_several() {
    let asked = |multiple| {
        picker(&FilePickerRequest {
            multiple,
            accept: vec![".png".to_string(), "image/jpeg".to_string()],
        })
    };
    assert_eq!(asked(false).mode, PickerMode::Open);
    let many = asked(true);
    assert_eq!(
        (many.mode, many.suggested_name),
        (PickerMode::OpenMany, None)
    );
    assert_eq!(many.accept, [".png", "image/jpeg"]);
}

fn new_window(flags: NewWindowFlags, target: TargetWindow, size: (u32, u32)) -> TabEvent {
    new_tab(
        TabId(0x8000_0001),
        "https://example.org".to_string(),
        &NewWindowRequest {
            target_window: target,
            flags,
            new_session: SessionId::from_raw(0x8000_0001),
            position: (0, 0),
            size,
            url: "https://popup.example/".to_string(),
            target_name: "_blank".to_string(),
            referrer: String::new(),
        },
    )
}

#[test]
fn a_new_window_carries_what_the_engine_knows_and_nothing_it_does_not() {
    use NewWindowFlags as F;
    let TabEvent::NewTab {
        new,
        url,
        name,
        no_opener,
        window,
    } = new_window(F::empty(), TargetWindow::NewWindow, (0, 0))
    else {
        panic!("a window")
    };
    assert_eq!(
        (new, url.as_str(), name.as_str(), no_opener),
        (
            TabId(0x8000_0001),
            "https://popup.example/",
            "_blank",
            false
        )
    );
    assert_eq!(window.disposition, Disposition::Window);
    assert!(
        !window.user_gesture && !window.unrequested,
        "no flag is not knowing, and is not a verdict"
    );
    assert_eq!(window.size, None);
    let TabEvent::NewTab {
        no_opener, window, ..
    } = new_window(
        F::NO_OPENER | F::NO_REFERRER | F::USER_GESTURE | F::HAS_SIZE | F::FROM_SCRIPT_ON_LOAD,
        TargetWindow::NewTabBackground,
        (400, 300),
    )
    else {
        panic!("a window")
    };
    assert!(no_opener && window.no_referrer && window.user_gesture && window.unrequested);
    assert_eq!(
        (window.disposition, window.size, window.origin.as_str()),
        (
            Disposition::BackgroundTab,
            Some((400, 300)),
            "https://example.org"
        )
    );
    // A size with no flag for it, or an empty one, is not a size.
    let TabEvent::NewTab { window, .. } = new_window(F::HAS_SIZE, TargetWindow::NewPopup, (0, 200))
    else {
        panic!("a window")
    };
    assert_eq!(
        (window.disposition, window.size),
        (Disposition::Popup, None)
    );
}

#[test]
fn a_download_goes_into_the_download_folder_under_a_name_that_stays_there() {
    let folder = Path::new("/home/u/Downloads");
    assert_eq!(
        download_target(folder, "a.zip"),
        Path::new("/home/u/Downloads/a.zip")
    );
    assert_eq!(
        download_target(folder, "../../etc/passwd"),
        Path::new("/home/u/Downloads/passwd")
    );
    assert_eq!(
        download_target(folder, "/etc/shadow"),
        Path::new("/home/u/Downloads/shadow")
    );
    assert_eq!(
        download_target(folder, ""),
        Path::new("/home/u/Downloads/download")
    );
    assert_eq!(
        download_target(folder, ".."),
        Path::new("/home/u/Downloads/download")
    );
}

#[test]
fn a_download_state_becomes_a_line_of_the_list_and_a_removed_one_does_not() {
    for (engine, browser) in [
        (EngineDownload::Started, DownloadState::Running),
        (EngineDownload::Progress, DownloadState::Running),
        (EngineDownload::Paused, DownloadState::Paused),
        (EngineDownload::Succeeded, DownloadState::Finished),
        (EngineDownload::Failed, DownloadState::Failed),
        (EngineDownload::Canceled, DownloadState::Cancelled),
    ] {
        assert_eq!(download_state(engine), Some(browser), "{engine:?}");
    }
    assert_eq!(download_state(EngineDownload::Removed), None);
}

#[test]
fn a_file_the_person_chose_is_opened_for_the_page_under_its_name_only() {
    let dir = std::env::temp_dir().join(format!("bingux-picker-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("a.txt");
    std::fs::write(&file, b"hello").unwrap();
    let chosen = picked(&file).expect("a file is opened for the page");
    assert_eq!(
        chosen.name(),
        "a.txt",
        "the page sees the name, and never the path"
    );
    assert!(picked(&dir).is_none(), "a folder is not a file to read");
    assert!(
        picked(&dir.join("gone")).is_none(),
        "a file that vanished is left out"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

fn download_event(flags: u32) -> DownloadEvent {
    DownloadEvent {
        state: EngineDownload::Started,
        download_id: vivarium::downloads::DownloadId::from_raw(7),
        flags,
        error_code: 0,
        received: 10,
        total: Some(100),
        speed_bps: 5,
        url: "https://example.com/a.zip".to_string(),
        target_path: "/home/u/Downloads/a.zip".to_string(),
        filename: "a.zip".to_string(),
        mime: "application/zip".to_string(),
        error: String::new(),
    }
}

fn identity(raw: u32) -> ContextualIdentityId {
    ContextualIdentityId::from_raw(raw).expect("a non-zero id")
}

fn no_containers(_: ContextualIdentityId) -> Option<(u32, bool)> {
    None
}

#[test]
fn a_private_flag_makes_a_private_download() {
    let owner = download_owner(vivarium_sys::VV_DOWNLOAD_IS_PRIVATE, None, no_containers);
    assert_eq!(
        owner,
        DownloadOwner {
            container: 0,
            private: true
        }
    );
    let owner = download_owner(
        vivarium_sys::VV_DOWNLOAD_CAN_PAUSE | vivarium_sys::VV_DOWNLOAD_IS_PRIVATE,
        Some(ContextualIdentityId::DEFAULT),
        no_containers,
    );
    assert!(owner.private, "the flag counts among the other bits");
    let info = download_info(&download_event(0), owner).expect("a running download");
    assert!(info.private, "and reaches the list's line");
}

#[test]
fn the_private_identity_makes_a_download_private() {
    let owner = download_owner(0, Some(ContextualIdentityId::PRIVATE), no_containers);
    assert_eq!(
        owner,
        DownloadOwner {
            container: 0,
            private: true
        }
    );
}

#[test]
fn a_normal_download_stays_normal() {
    for flags in [
        0,
        vivarium_sys::VV_DOWNLOAD_CAN_PAUSE | vivarium_sys::VV_DOWNLOAD_CAN_RESUME,
    ] {
        for who in [None, Some(ContextualIdentityId::DEFAULT)] {
            let owner = download_owner(flags, who, no_containers);
            assert_eq!(owner, DownloadOwner::default(), "{flags} {who:?}");
        }
    }
    let info =
        download_info(&download_event(0), DownloadOwner::default()).expect("a running download");
    assert_eq!((info.container, info.private), (0, false));
    assert_eq!(info.filename, "a.zip");
    assert_eq!(info.received, 10);
    assert_eq!(info.total, Some(100));
}

#[test]
fn a_container_download_names_its_container() {
    let known = |id: ContextualIdentityId| match id.get() {
        40 => Some((3, false)),
        41 => Some((3, true)),
        _ => None,
    };
    let owner = download_owner(0, Some(identity(40)), known);
    assert_eq!(
        owner,
        DownloadOwner {
            container: 3,
            private: false
        }
    );
    let owner = download_owner(0, Some(identity(41)), known);
    assert_eq!(
        owner,
        DownloadOwner {
            container: 3,
            private: true
        },
        "a private container's is private"
    );
    let info = download_info(&download_event(0), owner).expect("a running download");
    assert_eq!((info.container, info.private), (3, true));
    let unknown = download_owner(0, Some(identity(99)), known);
    assert_eq!(
        unknown,
        DownloadOwner::default(),
        "an identity the browser did not make is the default's"
    );
}
