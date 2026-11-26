//! What a site was told and what it may do, and the prompt a page's request becomes.

use bingux_ui::web_view::Response;
use bingux_ui::web_view::requests::{PermissionRequest, RequestKind};
use bingux_ui::web_view::test_double::Call;

use super::*;
use crate::browser::testing::{browser, go};

const SITE: &str = "https://maps.example";

#[test]
fn a_prompt_names_the_site_and_what_it_wants() {
    assert_eq!(
        prompt_text(SITE, &[PermissionKind::Geolocation]),
        "maps.example wants to know your location"
    );
    assert_eq!(
        prompt_text(SITE, &[PermissionKind::Camera, PermissionKind::Microphone]),
        "maps.example wants to use your camera and use your microphone"
    );
    assert_eq!(
        prompt_text(
            SITE,
            &[
                PermissionKind::Camera,
                PermissionKind::Microphone,
                PermissionKind::Screen
            ]
        ),
        "maps.example wants to use your camera, use your microphone and share your screen"
    );
}

#[test]
fn a_decision_is_only_reused_when_it_covers_everything_asked() {
    let mut permissions = Permissions::new();
    let both = [PermissionKind::Camera, PermissionKind::Microphone];
    assert_eq!(permissions.decided(SITE, &both), None);
    permissions.remember(SITE, &[PermissionKind::Camera], true);
    assert_eq!(
        permissions.decided(SITE, &both),
        None,
        "the microphone was never asked about"
    );
    permissions.remember(SITE, &[PermissionKind::Microphone], true);
    assert_eq!(permissions.decided(SITE, &both), Some(true));
    permissions.remember(SITE, &[PermissionKind::Microphone], false);
    assert_eq!(
        permissions.decided(SITE, &both),
        Some(false),
        "one refusal refuses the set"
    );
    assert_eq!(
        permissions.decided("https://other.example", &both),
        None,
        "a decision belongs to a site"
    );
    assert_eq!(permissions.decided(SITE, &[]), None);
}

#[test]
fn a_decision_can_be_forgotten_and_listed() {
    let mut permissions = Permissions::new();
    permissions.remember(
        SITE,
        &[PermissionKind::Geolocation, PermissionKind::Notifications],
        true,
    );
    permissions.remember("https://b.example", &[PermissionKind::Camera], false);
    assert_eq!(permissions.for_origin(SITE).len(), 2);
    assert_eq!(
        permissions.origins(),
        [("https://b.example".to_string(), 1), (SITE.to_string(), 2)]
    );
    permissions.forget(SITE, "Know your location");
    assert_eq!(
        permissions.for_origin(SITE),
        [("Show notifications".to_string(), true)]
    );
    permissions.forget_all();
    assert!(permissions.origins().is_empty());
}

#[test]
fn a_permission_prompt_waits_and_a_remembered_answer_is_reused() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "perm.example.net");
    let Some(crate::model::tab::Prompt::Permission(_, request)) = browser.prompt(key).cloned()
    else {
        panic!("a permission prompt");
    };
    assert_eq!(request.kinds, [PermissionKind::Geolocation]);
    double.clear_calls();
    browser.answer_permission(key, true, true);
    assert!(browser.prompt(key).is_none());
    assert!(double.calls().iter().any(|call| matches!(
        call,
        Call::Respond(
            _,
            _,
            Response::Permission {
                allow: true,
                remember: true
            }
        )
    )));
    // The same site asks again: it is answered without a prompt.
    double.clear_calls();
    let session = browser.session_of(key).unwrap();
    double.ask(session, RequestKind::Permission(request));
    browser.poll();
    assert!(
        browser.prompt(key).is_none(),
        "no prompt for a decided site"
    );
    assert!(double.calls().iter().any(|call| matches!(
        call,
        Call::Respond(_, _, Response::Permission { allow: true, .. })
    )));
}

#[test]
fn a_refused_permission_that_is_not_remembered_asks_again() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "perm.example.net");
    browser.answer_permission(key, false, false);
    let session = browser.session_of(key).unwrap();
    double.ask(
        session,
        RequestKind::Permission(PermissionRequest {
            origin: "https://perm.example.net".to_string(),
            kinds: vec![PermissionKind::Geolocation],
        }),
    );
    browser.poll();
    assert!(browser.prompt(key).is_some());
}
