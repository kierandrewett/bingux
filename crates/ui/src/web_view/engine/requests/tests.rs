//! The mapping of requests and answers, from hand-made engine values: no
//! engine is needed to say that a camera request reads as a camera request,
//! that a page's text arrives as text, and that an answer that does not belong
//! to the question is refused.

use std::path::Path;

use super::*;

fn origin(text: &str) -> vv::Origin {
    vv::Origin {
        serialized: text.to_string(),
        ..vv::Origin::default()
    }
}

fn request(payload: vv::RequestPayload) -> vv::Request {
    vv::Request {
        id: vv::RequestId(7),
        kind: vv::RequestKind::Permission,
        session: vv::SessionId::from_raw(3),
        context: None,
        frame_id: vv::FrameTreeId(0),
        document_id: vv::DocumentId(0),
        created_ns: 0,
        deadline_ns: 0,
        flags: vv::RequestFlags::empty(),
        default_answer: vv::Answer::Default,
        origin: origin("https://example.org"),
        top_origin: origin("https://example.org"),
        payload,
    }
}

fn ask(payload: vv::RequestPayload) -> RequestKind {
    match classify(&request(payload), Path::new("/home/u/Downloads")) {
        Incoming::Ask(kind) => kind,
        other => panic!("a question for the person, not {other:?}"),
    }
}

fn item(kind: vv::PermissionType, name: &str) -> vv::PermissionItem {
    vv::PermissionItem {
        kind,
        flags: vv::PermissionItemFlags::empty(),
        type_name: name.to_string(),
        detail: String::new(),
    }
}

#[test]
fn camera_and_microphone_asked_together_are_one_prompt_with_two_things() {
    let kind = ask(vv::RequestPayload::Permission(vv::PermissionRequest {
        items: vec![item(vv::PermissionType::Camera, "camera"), item(vv::PermissionType::Microphone, "microphone")],
        stored_state: vv::Answer::Default,
    }));
    let RequestKind::Permission(permission) = kind else { panic!("a permission") };
    assert_eq!(permission.origin, "https://example.org");
    assert_eq!(permission.kinds, [PermissionKind::Camera, PermissionKind::Microphone]);
}

#[test]
fn a_permission_with_no_word_of_its_own_is_named_by_the_engine() {
    let RequestKind::Permission(permission) = ask(vv::RequestPayload::Permission(vv::PermissionRequest {
        items: vec![item(vv::PermissionType::WebSerial, "web-serial"), item(vv::PermissionType::MidiSysex, "midi-sysex")],
        stored_state: vv::Answer::Default,
    })) else {
        panic!("a permission")
    };
    assert_eq!(permission.kinds, [PermissionKind::Other("web-serial".to_string()), PermissionKind::Midi]);
}

#[test]
fn a_frame_with_no_origin_is_shown_as_the_page_that_holds_it() {
    let mut asked = request(vv::RequestPayload::Permission(vv::PermissionRequest {
        items: vec![item(vv::PermissionType::Geolocation, "geolocation")],
        stored_state: vv::Answer::Default,
    }));
    asked.origin = origin("null");
    asked.top_origin = origin("https://top.example");
    let Incoming::Ask(RequestKind::Permission(permission)) = classify(&asked, Path::new("/d")) else { panic!("a permission") };
    assert_eq!(permission.origin, "https://top.example");
    asked.top_origin = origin("");
    assert_eq!(origin_text(&asked), "", "no origin is no origin, not \"null\"");
}

#[test]
fn every_dialog_the_engine_has_is_one_the_browser_shows() {
    let js = |dialog, message: &str, default: &str| {
        ask(vv::RequestPayload::JsDialog(vv::JsDialogRequest {
            dialog,
            can_suppress: true,
            message: message.to_string(),
            default_text: default.to_string(),
        }))
    };
    let RequestKind::Dialog(alert) = js(vv::DialogKind::Alert, "Hello", "") else { panic!("a dialog") };
    assert_eq!((alert.kind, alert.message.as_str(), alert.origin.as_str()), (DialogKind::Alert, "Hello", "https://example.org"));
    let RequestKind::Dialog(prompt) = js(vv::DialogKind::Prompt, "Name?", "Ada") else { panic!("a dialog") };
    assert_eq!(prompt.kind, DialogKind::Prompt { default: "Ada".to_string() });
    let RequestKind::Dialog(leave) = js(vv::DialogKind::BeforeUnload, "", "") else { panic!("a dialog") };
    assert_eq!((leave.kind, leave.message.as_str()), (DialogKind::BeforeUnload, ""));
    let RequestKind::Dialog(repost) = js(vv::DialogKind::Repost, "", "") else { panic!("a dialog") };
    assert_eq!(repost.kind, DialogKind::Confirm, "resending a form is a yes or no");
    assert!(repost.message.contains("sent again"), "and says so when the engine said nothing");
}

#[test]
fn authentication_names_the_server_with_its_port_and_says_who_asks() {
    let auth = |port, flags| {
        let RequestKind::Auth(auth) = ask(vv::RequestPayload::Auth(vv::AuthRequest {
            scheme: vv::AuthScheme::Basic,
            flags,
            port,
            host: "intranet.example".to_string(),
            realm: "Staff".to_string(),
            username_hint: String::new(),
        })) else {
            panic!("authentication")
        };
        auth
    };
    assert_eq!(auth(443, vv::AuthFlags::empty()).host, "intranet.example", "the scheme's own port is not shown");
    let proxy = auth(8080, vv::AuthFlags::PROXY | vv::AuthFlags::RETRY);
    assert_eq!((proxy.host.as_str(), proxy.realm.as_str(), proxy.proxy, proxy.retry), ("intranet.example:8080", "Staff", true, true));
}

fn cert(problems: vv::CertProblems, flags: vv::CertErrorFlags) -> CertError {
    let leaf = vv::CertInfo {
        flags: vv::CertFlags::empty(),
        subject: "CN=example.org".to_string(),
        issuer: "CN=Some CA".to_string(),
        serial_hex: "01".to_string(),
        sha256_hex: "ab12cd".to_string(),
        not_before_unix: 0,
        not_after_unix: 1_700_000_000,
        sans: Vec::new(),
        der: Vec::new(),
    };
    let RequestKind::CertError(error) = ask(vv::RequestPayload::CertError(vv::CertErrorRequest {
        flags,
        problems,
        port: 443,
        error_code: 1,
        host: "expired.example.org".to_string(),
        error_name: "SEC_ERROR_EXPIRED_CERTIFICATE".to_string(),
        chain: vec![leaf],
    })) else {
        panic!("a certificate error")
    };
    error
}

#[test]
fn a_certificate_error_says_what_is_wrong_who_issued_it_and_when_it_ran_out() {
    let error = cert(vv::CertProblems::EXPIRED, vv::CertErrorFlags::OVERRIDABLE | vv::CertErrorFlags::TOP_LEVEL);
    assert_eq!(error.problem, CertProblem::Expired);
    assert_eq!(error.code, "SEC_ERROR_EXPIRED_CERTIFICATE");
    assert_eq!(error.issuer.as_deref(), Some("CN=Some CA"));
    assert_eq!(error.fingerprint.as_deref(), Some("AB:12:CD"));
    assert_eq!(error.valid_until.as_deref(), Some("14 November 2023"));
    assert!(error.overridable);
}

#[test]
fn a_pinned_site_cannot_be_overridden_and_the_worst_problem_wins() {
    let error = cert(vv::CertProblems::UNTRUSTED | vv::CertProblems::SELF_SIGNED | vv::CertProblems::EXPIRED, vv::CertErrorFlags::HSTS);
    assert!(!error.overridable, "the engine did not say it may be");
    assert_eq!(error.problem, CertProblem::SelfSigned);
    assert_eq!(error.valid_until, None, "only an expired or not yet valid one has a date");
    assert_eq!(cert(vv::CertProblems::REVOKED | vv::CertProblems::UNTRUSTED, vv::CertErrorFlags::empty()).problem, CertProblem::Revoked);
    assert_eq!(cert(vv::CertProblems::WEAK_KEY, vv::CertErrorFlags::empty()).problem, CertProblem::Other);
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
    use vv::ContextMenuFlags as F;
    let menu = |flags| {
        let RequestKind::ContextMenu(target) = ask(vv::RequestPayload::ContextMenu(vv::ContextMenuRequest {
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
        })) else {
            panic!("a context menu")
        };
        target
    };
    let on_link = menu(F::LINK | F::SELECTION);
    assert_eq!(on_link.position, (12.0, 34.0));
    assert_eq!(on_link.link, Some(("https://l.example/".to_string(), "A link".to_string())));
    assert_eq!(on_link.selection.as_deref(), Some("chosen words"));
    assert_eq!((on_link.image, on_link.media, on_link.frame, on_link.editable, on_link.password), (None, None, None, false, false));
    let video = menu(F::VIDEO | F::FRAME | F::EDITABLE | F::PASSWORD | F::IMAGE);
    assert_eq!(video.media, Some(("https://m.example/v.mp4".to_string(), true)));
    assert_eq!(video.image.as_deref(), Some("https://i.example/a.png"));
    assert_eq!(video.frame.as_deref(), Some("https://f.example/"));
    assert!(video.editable && video.password);
    assert_eq!(menu(F::AUDIO).media.map(|(_, video)| video), Some(false));
    assert_eq!(menu(F::SELECTION | F::empty()).selection.is_some(), true);
    assert_eq!(menu(F::empty()), ContextTarget { position: (12.0, 34.0), ..ContextTarget::default() });
}

#[test]
fn a_select_keeps_every_option_in_place_and_a_group_heading_cannot_be_chosen() {
    let option = |label: &str, flags| vv::SelectOption { label: label.to_string(), flags };
    let RequestKind::Select(list) = ask(vv::RequestPayload::Select(vv::SelectRequest {
        multiple: false,
        rtl: false,
        anchor: vv::Rect { x: 1.0, y: 2.0, w: 3.0, h: 4.0 },
        options: vec![
            option("Fruit", vv::SelectOptionFlags::GROUP),
            option("Apple", vv::SelectOptionFlags::IN_GROUP | vv::SelectOptionFlags::SELECTED),
            option("Pear", vv::SelectOptionFlags::IN_GROUP | vv::SelectOptionFlags::DISABLED),
        ],
        font_size_px: 14.0,
        text_color: 0,
        background_color: 0,
    })) else {
        panic!("a select")
    };
    assert_eq!(list.anchor, (1.0, 2.0, 3.0, 4.0));
    let shown: Vec<(&str, bool, bool)> = list.options.iter().map(|o| (o.label.as_str(), o.selected, o.disabled)).collect();
    assert_eq!(shown, [("Fruit", false, true), ("Apple", true, false), ("Pear", false, true)]);
}

#[test]
fn a_file_picker_says_what_it_is_for() {
    let picker = |mode| {
        let RequestKind::FilePicker(picker) = ask(vv::RequestPayload::FilePicker(vv::FilePickerRequest {
            mode,
            flags: 0,
            accept: vec![".png".to_string(), "image/jpeg".to_string()],
            suggested_name: if mode == vv::PickerMode::Save { "report.pdf".to_string() } else { String::new() },
            title: String::new(),
        })) else {
            panic!("a picker")
        };
        picker
    };
    assert_eq!(picker(vv::PickerMode::Open).mode, PickerMode::Open);
    assert_eq!(picker(vv::PickerMode::OpenMultiple).mode, PickerMode::OpenMany);
    assert_eq!(picker(vv::PickerMode::Folder).mode, PickerMode::Folder);
    let save = picker(vv::PickerMode::Save);
    assert_eq!((save.mode, save.suggested_name.as_deref()), (PickerMode::Save, Some("report.pdf")));
    assert_eq!(save.accept, [".png", "image/jpeg"]);
    assert_eq!(picker(vv::PickerMode::Open).suggested_name, None);
}

fn new_window(flags: vv::NewWindowFlags, disposition: vv::WindowDisposition, size: (u32, u32)) -> Incoming {
    classify(
        &request(vv::RequestPayload::NewWindow(vv::NewWindowRequest {
            disposition,
            flags,
            new_session: vv::SessionId::from_raw(0x8000_0001),
            position: (0, 0),
            size,
            url: "https://popup.example/".to_string(),
            target_name: "_blank".to_string(),
            referrer: String::new(),
        })),
        Path::new("/d"),
    )
}

#[test]
fn a_new_window_carries_what_the_engine_knows_and_nothing_it_does_not() {
    use vv::NewWindowFlags as F;
    // What the engine sends today (CR-8): no flags, the disposition "window", no size, no origin.
    let Incoming::Window { url, name, no_opener, window } = new_window(F::empty(), vv::WindowDisposition::Window, (0, 0)) else {
        panic!("a window")
    };
    assert_eq!((url.as_str(), name.as_str(), no_opener), ("https://popup.example/", "_blank", false));
    assert_eq!(window.disposition, Disposition::Window);
    assert!(!window.user_gesture && !window.unrequested, "no flag is not knowing, and is not a verdict");
    assert_eq!(window.size, None);
    // What it will send when CR-8 is done.
    let Incoming::Window { no_opener, window, .. } = new_window(
        F::NO_OPENER | F::NO_REFERRER | F::USER_GESTURE | F::HAS_SIZE | F::FROM_SCRIPT_ON_LOAD,
        vv::WindowDisposition::BackgroundTab,
        (400, 300),
    ) else {
        panic!("a window")
    };
    assert!(no_opener && window.no_referrer && window.user_gesture && window.unrequested);
    assert_eq!((window.disposition, window.size, window.origin.as_str()), (Disposition::BackgroundTab, Some((400, 300)), "https://example.org"));
    // A size with no flag for it, or an empty one, is not a size.
    let Incoming::Window { window, .. } = new_window(F::HAS_SIZE, vv::WindowDisposition::Popup, (0, 200)) else { panic!("a window") };
    assert_eq!((window.disposition, window.size), (Disposition::Popup, None));
}

#[test]
fn a_kind_the_browser_has_no_prompt_for_is_left_to_the_engines_default() {
    let fullscreen = vv::RequestPayload::Fullscreen(vv::ModeRequest {
        flags: vv::ModeFlags::ENTER,
        element: vv::Rect::default(),
    });
    assert_eq!(classify(&request(fullscreen), Path::new("/d")), Incoming::Default);
    assert_eq!(classify(&request(vv::RequestPayload::Raw(vec![1, 2])), Path::new("/d")), Incoming::Default);
}

#[test]
fn a_download_goes_into_the_download_folder_under_a_name_that_stays_there() {
    let folder = Path::new("/home/u/Downloads");
    assert_eq!(download_target(folder, "a.zip"), Path::new("/home/u/Downloads/a.zip"));
    assert_eq!(download_target(folder, "../../etc/passwd"), Path::new("/home/u/Downloads/passwd"));
    assert_eq!(download_target(folder, "/etc/shadow"), Path::new("/home/u/Downloads/shadow"));
    assert_eq!(download_target(folder, ""), Path::new("/home/u/Downloads/download"));
    assert_eq!(download_target(folder, ".."), Path::new("/home/u/Downloads/download"));
    let asked = vv::RequestPayload::Download(vv::DownloadRequest {
        flags: vv::DownloadRequestFlags::empty(),
        download_id: vv::DownloadId(5),
        size: None,
        url: "https://e.example/f".to_string(),
        referrer: String::new(),
        suggested_name: "f.bin".to_string(),
        mime: "application/octet-stream".to_string(),
    });
    assert_eq!(classify(&request(asked), folder), Incoming::Download(folder.join("f.bin")));
}

fn debug(response: &vv::Response) -> String {
    format!("{response:?}")
}

#[test]
fn the_persons_choices_reach_the_engine_as_what_they_chose() {
    let permission = request(vv::RequestPayload::Permission(vv::PermissionRequest { items: vec![item(vv::PermissionType::Camera, "camera")], stored_state: vv::Answer::Default }));
    let allowed = response(&permission, Response::Permission { allow: true, remember: true }).unwrap();
    assert_eq!(allowed.answer(), vv::Answer::Allow);
    assert!(debug(&allowed).contains("Session"), "kept while the window is open, as the settings page says: {}", debug(&allowed));
    let blocked = response(&permission, Response::Permission { allow: false, remember: false }).unwrap();
    assert_eq!(blocked.answer(), vv::Answer::Deny);
    assert!(debug(&blocked).contains("None") && !debug(&blocked).contains("Session"));

    let dialog = |kind| request(vv::RequestPayload::JsDialog(vv::JsDialogRequest { dialog: kind, can_suppress: false, message: String::new(), default_text: String::new() }));
    let prompt = response(&dialog(vv::DialogKind::Prompt), Response::Dialog { accept: true, text: Some("Ada".to_string()) }).unwrap();
    assert_eq!(prompt.answer(), vv::Answer::Allow);
    assert!(debug(&prompt).contains("Ada"));
    assert_eq!(response(&dialog(vv::DialogKind::Prompt), Response::Dialog { accept: false, text: Some("x".into()) }).unwrap().answer(), vv::Answer::Cancel);
    assert_eq!(response(&dialog(vv::DialogKind::Confirm), Response::Dialog { accept: true, text: None }).unwrap().answer(), vv::Answer::Allow);
    assert_eq!(response(&dialog(vv::DialogKind::BeforeUnload), Response::Dialog { accept: false, text: None }).unwrap().answer(), vv::Answer::Cancel, "stay on the page");

    let auth = request(vv::RequestPayload::Auth(vv::AuthRequest { scheme: vv::AuthScheme::Basic, flags: vv::AuthFlags::empty(), port: 0, host: "h".into(), realm: "r".into(), username_hint: String::new() }));
    let signed_in = response(&auth, Response::Auth { credentials: Some(("ada".into(), "secret".into())) }).unwrap();
    assert_eq!(signed_in.answer(), vv::Answer::Allow);
    assert!(!debug(&signed_in).contains("secret"), "a password is not printed: {}", debug(&signed_in));
    assert_eq!(response(&auth, Response::Auth { credentials: None }).unwrap().answer(), vv::Answer::Cancel);

    let error = request(vv::RequestPayload::CertError(vv::CertErrorRequest { flags: vv::CertErrorFlags::OVERRIDABLE, problems: vv::CertProblems::EXPIRED, port: 443, error_code: 1, host: "h".into(), error_name: "n".into(), chain: Vec::new() }));
    let proceed = response(&error, Response::CertError { proceed: true }).unwrap();
    assert_eq!(proceed.answer(), vv::Answer::Allow);
    assert!(debug(&proceed).contains("Session"), "the exception is for this window, not for good");
    assert_eq!(response(&error, Response::CertError { proceed: false }).unwrap().answer(), vv::Answer::Deny);

    let menu = request(vv::RequestPayload::ContextMenu(vv::ContextMenuRequest { flags: vv::ContextMenuFlags::empty(), x: 0.0, y: 0.0, link_url: String::new(), link_text: String::new(), image_url: String::new(), media_url: String::new(), frame_url: String::new(), page_url: String::new(), selection_text: String::new(), misspelled_word: String::new(), suggestions: Vec::new(), input_type: String::new() }));
    assert_eq!(response(&menu, Response::ContextMenu).unwrap().answer(), vv::Answer::Handled);

    let list = request(vv::RequestPayload::Select(vv::SelectRequest { multiple: false, rtl: false, anchor: vv::Rect::default(), options: Vec::new(), font_size_px: 0.0, text_color: 0, background_color: 0 }));
    let chosen = response(&list, Response::Select { index: Some(2) }).unwrap();
    assert_eq!(chosen.answer(), vv::Answer::Allow);
    assert!(chosen.fits(vv::RequestKind::Select));
    assert_eq!(response(&list, Response::Select { index: None }).unwrap().answer(), vv::Answer::Cancel);
}

#[test]
fn a_file_the_person_chose_is_opened_for_the_page_and_a_save_target_is_a_path() {
    let dir = std::env::temp_dir().join(format!("bingux-picker-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("a.txt");
    std::fs::write(&file, b"hello").unwrap();
    let read = file_choice(&file, true);
    assert_eq!(read.size, 5);
    assert!(read.fd.is_some(), "the page reads the descriptor, and never learns the path");
    let folder = file_choice(&dir, true);
    assert!(folder.fd.is_none() && folder.size == 0, "a folder is a path");
    let save = file_choice(&dir.join("new.pdf"), false);
    assert!(save.fd.is_none(), "a place to write is a path");
    let missing = file_choice(&dir.join("gone"), true);
    assert!(missing.fd.is_none(), "a file that vanished is left to the engine to refuse");
    std::fs::remove_dir_all(&dir).unwrap();

    let picker = request(vv::RequestPayload::FilePicker(vv::FilePickerRequest { mode: vv::PickerMode::Open, flags: 0, accept: Vec::new(), suggested_name: String::new(), title: String::new() }));
    assert_eq!(response(&picker, Response::FilePicker { paths: Vec::new() }).unwrap().answer(), vv::Answer::Cancel, "no file is a cancel");
    assert_eq!(response(&picker, Response::FilePicker { paths: vec![dir.join("missing")] }).unwrap().answer(), vv::Answer::Allow);
}

#[test]
fn an_answer_that_does_not_belong_to_the_question_is_refused() {
    let permission = request(vv::RequestPayload::Permission(vv::PermissionRequest { items: Vec::new(), stored_state: vv::Answer::Default }));
    assert!(response(&permission, Response::CertError { proceed: true }).is_err(), "a certificate answer must not grant a permission");
    assert!(response(&permission, Response::Auth { credentials: None }).is_err());
    let fullscreen = request(vv::RequestPayload::Fullscreen(vv::ModeRequest { flags: vv::ModeFlags::ENTER, element: vv::Rect::default() }));
    assert!(response(&fullscreen, Response::Permission { allow: true, remember: false }).is_err());
}

#[test]
fn a_download_event_becomes_a_line_of_the_list_and_a_removed_one_does_not() {
    let event = |state| vv::DownloadEvent {
        state,
        download_id: vv::DownloadId(9),
        flags: 0,
        error_code: 0,
        received: 512,
        total: Some(1024),
        speed_bps: 100,
        url: "https://e.example/f.bin".to_string(),
        target_path: "/home/u/Downloads/f.bin".to_string(),
        filename: "f.bin".to_string(),
        mime: "application/octet-stream".to_string(),
        error: String::new(),
    };
    let info = download_info(&event(vv::DownloadState::Progress)).unwrap();
    assert_eq!((info.id, info.state, info.received, info.total, info.speed), (crate::web_view::ids::DownloadId(9), DownloadState::Running, 512, Some(1024), 100));
    assert_eq!(info.path.as_deref(), Some(Path::new("/home/u/Downloads/f.bin")));
    for (engine, browser) in [
        (vv::DownloadState::Started, DownloadState::Running),
        (vv::DownloadState::Paused, DownloadState::Paused),
        (vv::DownloadState::Finished, DownloadState::Finished),
        (vv::DownloadState::Failed, DownloadState::Failed),
        (vv::DownloadState::Cancelled, DownloadState::Cancelled),
    ] {
        assert_eq!(download_info(&event(engine)).unwrap().state, browser);
    }
    assert_eq!(download_info(&event(vv::DownloadState::Removed)), None);
    let mut unknown_size = event(vv::DownloadState::Started);
    unknown_size.total = None;
    unknown_size.target_path.clear();
    let info = download_info(&unknown_size).unwrap();
    assert_eq!((info.total, info.path), (None, None));
}
