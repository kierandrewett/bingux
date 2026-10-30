//! Banner and toast share one code path: these tests hold both to the same
//! geometry, check the colours of every kind in both schemes and every accent,
//! and check that a toast is a banner inside a floating shell.

use std::time::Duration;

use freya::prelude::*;

use crate::contrast::{AA_TEXT, AAA_TEXT, Oklch, ratio};
use crate::shell::banner::{NOTICE_BUTTON, NOTICE_PAD, notice_radii};
use crate::shell::{Banner, BannerKind, OverlayHost, Toast, use_overlays};
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::{Scheme, Tokens, accent_names};

const LONG: &str =
    "The display could not be changed because the monitor rejected the mode that was asked for";

#[derive(Clone, Copy, PartialEq)]
enum Place {
    Inline,
    Toast,
}

#[derive(PartialEq)]
struct Scene {
    place: Place,
    text: &'static str,
    action: bool,
    kind: BannerKind,
}

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let tokens = crate::theme::provide_tokens();
        match self.place {
            Place::Inline => {
                let mut banner = Banner::new(self.text)
                    .kind(self.kind)
                    .dismissible(|_| {});
                if self.action {
                    banner = banner.action("Undo", |_| {});
                }
                Element::from(crate::window::WindowHost::new(
                    rect()
                        .tw(&tokens, &[w_full(), p(S4)])
                        .child(banner),
                ))
            }
            Place::Toast => Element::from(OverlayHost::new(ShowToast {
                text: self.text,
                action: self.action,
                kind: self.kind,
            })),
        }
    }
}

#[derive(PartialEq)]
struct ShowToast {
    text: &'static str,
    action: bool,
    kind: BannerKind,
}

impl Component for ShowToast {
    fn render(&self) -> impl IntoElement {
        let overlays = use_overlays();
        let (text, action, kind) = (self.text, self.action, self.kind);
        use_hook(move || {
            let mut toast = Toast::new(text).kind(kind);
            if action {
                toast = toast.action("Undo", |_| {});
            }
            overlays.show_toast(toast);
        });
        rect()
    }
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.51
}

type Box4 = [f32; 4];

fn boxes(shot: &mut Snapshot) -> Vec<Box4> {
    shot.runner().find_many(|node, _| {
        let a = node.layout().area;
        Some([a.min_x(), a.max_x(), a.min_y(), a.max_y()])
    })
}

fn named(shot: &mut Snapshot, name: &'static str) -> Vec<Box4> {
    shot.runner().find_many(|node, element| {
        (element.accessibility().builder.class_name() == Some(name)).then(|| {
            let a = node.layout().area;
            [a.min_x(), a.max_x(), a.min_y(), a.max_y()]
        })
    })
}

fn scene(place: Place, text: &'static str, action: bool, kind: BannerKind, size: (f32, f32)) -> Snapshot {
    let mut shot = Snapshot::new(move || Scene { place, text, action, kind }.into(), size);
    shot.settle(Duration::from_millis(700));
    shot
}

const SIZES: [(&str, SizeClass, (f32, f32)); 2] = [
    ("desktop", SizeClass::Regular, (900.0, 400.0)),
    ("phone", SizeClass::Compact, (390.0, 800.0)),
];

/// The buttons of a notice are as tall as the control, the last is the same
/// padding from the notice's top, bottom and trailing edge, and the action
/// beside it is the same height on the same line. Held for a banner and a
/// toast alike, in every kind.
#[test]
fn buttons_fill_the_notice_with_an_equal_inset_in_a_banner_and_a_toast() {
    for (name, class, size) in SIZES {
        for place in [Place::Inline, Place::Toast] {
            // Every kind with an action; the other texts on two kinds.
            let cases = BannerKind::ALL
                .into_iter()
                .map(|kind| (kind, "Saved", true))
                .chain([
                    (BannerKind::Info, "Saved", false),
                    (BannerKind::Info, LONG, true),
                    (BannerKind::Neutral, LONG, false),
                ]);
            for (kind, text, action) in cases {
                    // An inline banner wraps its whole message: only a toast
                    // keeps it to two lines, so only a toast's long message
                    // keeps the buttons the same inset.
                    if place == Place::Inline && text == LONG {
                        continue;
                    }
                    let mut shot = scene(place, text, action, kind, size);
                    let all = boxes(&mut shot);
                    let button = NOTICE_BUTTON.px_in(class);
                    let pad = NOTICE_PAD.px_in(class);
                    let label = format!("{name} {kind:?} {} {action}", text.len());
                    let dismiss = all
                        .iter()
                        .copied()
                        .filter(|b| near(b[1] - b[0], button) && near(b[3] - b[2], button))
                        .max_by(|a, b| a[1].total_cmp(&b[1]))
                        .unwrap_or_else(|| panic!("{label}: no dismiss button in {all:?}"));
                    let pill = all
                        .iter()
                        .copied()
                        .find(|b| {
                            near(b[1], dismiss[1] + pad)
                                && near(b[2], dismiss[2] - pad)
                                && near(b[3], dismiss[3] + pad)
                        })
                        .unwrap_or_else(|| {
                            panic!("{label}: the dismiss button is not {pad} px from the notice's top, bottom and right: {all:?}")
                        });
                    if action {
                        let undo = all
                            .iter()
                            .copied()
                            .filter(|b| {
                                near(b[3] - b[2], button) && b[1] - b[0] > button && b[1] <= dismiss[0] + 0.51
                            })
                            .max_by(|a, b| a[1].total_cmp(&b[1]))
                            .unwrap_or_else(|| panic!("{label}: no action button"));
                        assert!(near(undo[2], dismiss[2]) && near(undo[3], dismiss[3]), "{label}");
                    }
                    assert!(pill[3] - pill[2] >= button + 2.0 * pad - 0.51, "{label}");
                }
        }
    }
}

/// The action and the dismiss cross abut: no gap between their rects, the
/// same height on the same line, the cross ends one padding from the end (the
/// same as above and below), and only the message is spaced from the group.
/// In every kind, banner and toast, desktop and phone.
#[test]
fn the_action_and_the_dismiss_cross_sit_flush() {
    for (name, class, size) in SIZES {
        let pad = NOTICE_PAD.px_in(class);
        for place in [Place::Inline, Place::Toast] {
            for kind in BannerKind::ALL {
                let label = format!("{name} {kind:?}");
                let mut shot = scene(place, "Saved", true, kind, size);
                let bar = named(&mut shot, "Banner")[0];
                let group = named(&mut shot, "NoticeButtons")[0];
                let action = named(&mut shot, "NoticeAction")[0];
                let dismiss = named(&mut shot, "IconButton")[0];
                assert!(near(action[1], dismiss[0]), "{label}: gap between action and dismiss: {action:?} {dismiss:?}");
                assert!(near(action[2], dismiss[2]) && near(action[3], dismiss[3]), "{label}: equal heights");
                assert!(near(group[0], action[0]) && near(group[1], dismiss[1]), "{label}: the group is just the buttons");
                assert!(near(bar[1] - dismiss[1], pad), "{label}: right inset {pad}");
                assert!(near(dismiss[2] - bar[2], pad), "{label}: top inset equals right inset");
                assert!(action[0] - bar[0] > pad, "{label}: the message stays spaced from the group");
            }
        }
    }
    // Two actions and the cross: one flush run.
    let mut shot = Snapshot::new(|| TwoActions.into(), (900.0, 400.0));
    shot.settle(Duration::from_millis(700));
    let mut actions = named(&mut shot, "NoticeAction");
    actions.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let dismiss = named(&mut shot, "IconButton")[0];
    assert!(near(actions[0][1], actions[1][0]) && near(actions[1][1], dismiss[0]), "two actions and the cross abut");
}

/// A one-line notice is a pill and its buttons are concentric with it: the
/// container's radius (half its height) less the padding is the button's.
#[test]
fn a_one_line_notice_and_its_buttons_have_nested_radii() {
    for (name, class, size) in SIZES {
        for place in [Place::Inline, Place::Toast] {
            let mut shot = scene(place, "Saved", true, BannerKind::Info, size);
            let bars = named(&mut shot, "Banner");
            let bar = bars.first().copied().unwrap_or_else(|| panic!("{name}: no banner"));
            let button = NOTICE_BUTTON.px_in(class);
            let pad = NOTICE_PAD.px_in(class);
            assert!(
                near(bar[3] - bar[2], button + 2.0 * pad),
                "{name}: a one-line notice is the button plus its padding tall"
            );
            assert!(near((bar[3] - bar[2]) / 2.0 - pad, button / 2.0), "{name}: nested pill radii");
        }
    }
    // The tall form: the buttons take the container's radius less the padding.
    let (outer, inner) = notice_radii(true);
    assert_eq!(outer, Radius::Overlay);
    assert!(near(outer.px() - NOTICE_PAD.px(), inner.px()));
    let (outer, inner) = notice_radii(false);
    assert_eq!((outer, inner), (Radius::Pill, Radius::Pill));
}

/// A toast renders a banner inside a floating shell: one `Banner` in it, the
/// shell is named `Toast`, and an inline banner has no shell.
#[test]
fn a_toast_renders_a_banner() {
    let mut toast = scene(Place::Toast, "Saved", true, BannerKind::Success, (900.0, 400.0));
    let shells = named(&mut toast, "Toast");
    let banners = named(&mut toast, "Banner");
    assert_eq!(shells.len(), 1, "one floating shell");
    assert_eq!(banners.len(), 1, "one banner in it");
    let (shell, bar) = (shells[0], banners[0]);
    assert!(
        bar[0] >= shell[0] - 0.5 && bar[1] <= shell[1] + 0.5 && bar[2] >= shell[2] - 0.5 && bar[3] <= shell[3] + 0.5,
        "the banner sits inside the shell"
    );
    // Bottom centre of the window.
    assert!(near((bar[0] + bar[1]) / 2.0, 450.0), "centred: {bar:?}");
    assert!(bar[3] > 400.0 - 80.0, "near the bottom: {bar:?}");

    let mut inline = scene(Place::Inline, "Saved", true, BannerKind::Success, (900.0, 400.0));
    assert_eq!(named(&mut inline, "Banner").len(), 1);
    assert!(named(&mut inline, "Toast").is_empty());
}

/// A scene with one toast that goes after `timeout` (and an Undo button, so
/// the pointer has a button to be over), plus a second queued behind it.
#[derive(PartialEq)]
struct Held {
    timeout: Duration,
    queued: bool,
}

impl Component for Held {
    fn render(&self) -> impl IntoElement {
        let overlays = use_overlays();
        let (timeout, queued) = (self.timeout, self.queued);
        use_hook(move || {
            overlays.show_toast(
                Toast::new("Moved 3 items to Trash")
                    .kind(BannerKind::Neutral)
                    .action("Undo", |_| {})
                    .timeout(timeout),
            );
            if queued {
                overlays.show_toast(Toast::new("Restored").timeout(timeout));
            }
        });
        rect()
    }
}

/// The window the held-toast scene lives in: the theme, then the overlays.
#[derive(PartialEq)]
struct HeldWindow {
    timeout: Duration,
    queued: bool,
}

impl Component for HeldWindow {
    fn render(&self) -> impl IntoElement {
        let _tokens = crate::theme::provide_tokens();
        Element::from(OverlayHost::new(Held {
            timeout: self.timeout,
            queued: self.queued,
        }))
    }
}

fn held(timeout: Duration, queued: bool) -> Snapshot {
    let mut shot = Snapshot::new(move || HeldWindow { timeout, queued }.into(), (900.0, 400.0));
    shot.settle(Duration::from_millis(400));
    shot
}

fn centre(b: Box4) -> (f32, f32) {
    ((b[0] + b[1]) / 2.0, (b[2] + b[3]) / 2.0)
}

/// The toast stays while the pointer is on any part of it: its padding at
/// each side, the gap between the message and the buttons, the action button
/// and the dismiss cross. A pointer on a button must hold it too: those
/// buttons are inside the toast and listen to the pointer themselves.
#[test]
fn a_toast_stays_up_while_the_pointer_is_anywhere_over_it() {
    let timeout = Duration::from_millis(500);
    let mut probe = held(timeout, false);
    let bar = named(&mut probe, "Banner")[0];
    let action = named(&mut probe, "NoticeAction")[0];
    let dismiss = named(&mut probe, "IconButton")[0];
    drop(probe);
    let (mid_x, mid_y) = centre(bar);
    let places = [
        ("the middle", (mid_x, mid_y)),
        ("the leading padding", (bar[0] + 3.0, mid_y)),
        ("the trailing padding", (bar[1] - 1.5, mid_y)),
        ("the top padding", (mid_x, bar[2] + 1.5)),
        ("the bottom padding", (mid_x, bar[3] - 1.5)),
        ("the gap before the action", (action[0] - 2.0, mid_y)),
        ("the action button", centre(action)),
        ("the dismiss cross", centre(dismiss)),
    ];
    for (name, (x, y)) in places {
        let mut shot = held(timeout, false);
        shot.hover(x, y);
        shot.settle(Duration::from_millis(1600));
        assert_eq!(
            named(&mut shot, "Banner").len(),
            1,
            "the pointer on {name} ({x:.0}, {y:.0}) holds the toast"
        );
        // Moving between two of its parts does not let it go either.
        shot.hover(mid_x, mid_y);
        shot.settle(Duration::from_millis(900));
        assert_eq!(named(&mut shot, "Banner").len(), 1, "after {name}, over the middle");
        shot.hover(20.0, 20.0);
        shot.settle(Duration::from_millis(1600));
        assert!(named(&mut shot, "Banner").is_empty(), "gone once the pointer left from {name}");
    }
}

/// Once the pointer leaves, the whole delay starts again: a toast that was
/// hovered for longer than its delay does not close at once.
#[test]
fn leaving_a_toast_restarts_the_full_delay() {
    let timeout = Duration::from_millis(1000);
    let mut shot = held(timeout, false);
    let bar = named(&mut shot, "Banner")[0];
    let (x, y) = centre(bar);
    shot.hover(x, y);
    shot.settle(Duration::from_millis(2500));
    assert_eq!(named(&mut shot, "Banner").len(), 1, "held for longer than its delay");
    shot.hover(20.0, 20.0);
    shot.settle(Duration::from_millis(600));
    assert_eq!(
        named(&mut shot, "Banner").len(),
        1,
        "600 ms after leaving: the delay restarted, it did not resume from where it paused"
    );
    // Entering again pauses again, and leaving again restarts again.
    shot.hover(x, y);
    shot.settle(Duration::from_millis(1500));
    assert_eq!(named(&mut shot, "Banner").len(), 1, "held again");
    shot.hover(20.0, 20.0);
    shot.settle(Duration::from_millis(600));
    assert_eq!(named(&mut shot, "Banner").len(), 1, "restarted again");
    shot.settle(Duration::from_millis(1100));
    assert!(named(&mut shot, "Banner").is_empty(), "gone a full delay after the last leave");
}

/// A toast queued while another is hovered waits behind it: the hovered one
/// stays as it is, and the next shows when it is gone.
#[test]
fn a_toast_queued_while_one_is_hovered_waits_behind_it() {
    let timeout = Duration::from_millis(600);
    let mut shot = held(timeout, true);
    let bar = named(&mut shot, "Banner")[0];
    assert_eq!(named(&mut shot, "NoticeAction").len(), 1, "the first toast is up");
    let (x, y) = centre(bar);
    shot.hover(x, y);
    shot.settle(Duration::from_millis(2000));
    assert_eq!(named(&mut shot, "Banner").len(), 1, "only the hovered toast");
    assert_eq!(named(&mut shot, "NoticeAction").len(), 1, "still the first, with its action");
    shot.hover(20.0, 20.0);
    shot.settle(Duration::from_millis(1400));
    assert_eq!(named(&mut shot, "Banner").len(), 1, "the queued toast took its place");
    assert!(named(&mut shot, "NoticeAction").is_empty(), "the second toast has no action");
}

/// The fill each node of component `name` paints now, in tree order, `None`
/// for a node that paints nothing.
fn fills(shot: &mut Snapshot, name: &str) -> Vec<Option<Color>> {
    let tree = crate::inspector::snapshot::Snapshot::capture_headless(&*shot.runner(), 1.0);
    tree.nodes
        .iter()
        .filter(|node| node.a11y.component.as_deref() == Some(name))
        .map(|node| {
            node.props
                .as_ref()
                .and_then(|props| props.background)
                .filter(|colour| colour.a() != 0)
        })
        .collect()
}

/// The action button is quiet: it paints nothing at rest, so the toast reads
/// as one surface; hovering gives it the hover fill and leaving takes it
/// away. It is as tall as the dismiss cross and shares its top inset.
#[test]
fn the_action_button_is_quiet_and_matches_the_dismiss_cross() {
    for kind in BannerKind::ALL {
        let mut shot = scene(Place::Toast, "Saved", true, kind, (900.0, 400.0));
        let action = named(&mut shot, "NoticeAction")[0];
        let dismiss = named(&mut shot, "IconButton")[0];
        assert!(near(action[3] - action[2], dismiss[3] - dismiss[2]), "{kind:?}: same height");
        assert!(near(action[2], dismiss[2]), "{kind:?}: same top inset");
        assert_eq!(fills(&mut shot, "NoticeAction"), [None], "{kind:?}: no fill at rest");
        let (x, y) = centre(action);
        shot.hover(x, y);
        shot.settle(Duration::from_millis(100));
        let hovered = fills(&mut shot, "NoticeAction");
        assert!(hovered[0].is_some(), "{kind:?}: the hover layer shows");
        shot.hover(20.0, 20.0);
        shot.settle(Duration::from_millis(100));
        assert_eq!(fills(&mut shot, "NoticeAction"), [None], "{kind:?}: gone when the pointer leaves");
    }
}

#[derive(PartialEq)]
struct TwoActions;

impl Component for TwoActions {
    fn render(&self) -> impl IntoElement {
        let tokens = crate::theme::provide_tokens();
        Element::from(crate::window::WindowHost::new(
            rect().tw(&tokens, &[w_full(), p(S4)]).child(
                Banner::new("A new version of Bingux Settings is available.")
                    .action("Later", |_| {})
                    .action("Install", |_| {})
                    .dismissible(|_| {}),
            ),
        ))
    }
}

/// Two actions share the row with the message on a desktop. On a phone they
/// would crush it, so they take a row of their own under the message, both
/// as tall as a touch target, and the dismiss cross stays at the top corner.
#[test]
fn several_actions_stack_under_the_message_on_a_phone() {
    for (name, class, size, stacked) in [
        ("desktop", SizeClass::Regular, (900.0, 400.0), false),
        ("phone", SizeClass::Compact, (390.0, 800.0), true),
    ] {
        let mut shot = Snapshot::new(|| TwoActions.into(), size);
        shot.settle(Duration::from_millis(700));
        let all = boxes(&mut shot);
        let button = NOTICE_BUTTON.px_in(class);
        let pad = NOTICE_PAD.px_in(class);
        let dismiss = all
            .iter()
            .copied()
            .filter(|b| near(b[1] - b[0], button) && near(b[3] - b[2], button))
            .max_by(|a, b| a[1].total_cmp(&b[1]))
            .unwrap_or_else(|| panic!("{name}: no dismiss button"));
        let mut actions: Vec<Box4> = all
            .iter()
            .copied()
            .filter(|b| near(b[3] - b[2], button) && b[1] - b[0] > button && b[1] <= dismiss[1] + 0.5)
            .collect();
        actions.sort_by(|a, b| a[0].total_cmp(&b[0]));
        assert!(actions.len() >= 2, "{name}: two action buttons in {all:?}");
        let (later, install) = (actions[actions.len() - 2], actions[actions.len() - 1]);
        assert!(near(later[2], install[2]), "{name}: the actions share a row");
        if stacked {
            assert!(later[2] >= dismiss[3] - 0.5, "{name}: the actions sit under the dismiss row");
            assert!(near(install[1], dismiss[1]), "{name}: the last action ends at the trailing inset");
        } else {
            assert!(near(later[2], dismiss[2]), "{name}: one row on a desktop");
        }
        let bar = named(&mut shot, "Banner")[0];
        assert!(near(dismiss[1] + pad, bar[1]), "{name}: the dismiss cross is {pad} px from the end");
        assert!(
            near(bar[3], install[3] + pad) || !stacked,
            "{name}: the banner ends {pad} px under the action row (bar {bar:?}, install {install:?})"
        );
    }
}

fn hue_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// Every kind in both schemes and every accent: the ink is a readable shade of
/// the kind's own hue (darker than the fill on a light scheme, lighter on a
/// dark one), and the action label reads on its button.
#[test]
fn every_kind_reads_in_both_schemes_and_every_accent() {
    for accent in accent_names() {
        for scheme in [Scheme::PreferLight, Scheme::PreferDark] {
            let tokens = Tokens::new(accent, scheme, false);
            let dark = scheme.is_dark();
            for kind in BannerKind::ALL {
                let colours = tokens.notice(kind);
                let name = format!("{accent} {scheme:?} {kind:?}");
                let text = ratio(colours.ink, colours.fill);
                assert!(text >= AA_TEXT, "{name}: ink on fill is {text:.2}");
                // The edge is a faint hairline, never a coloured outline; the
                // saturated hue is the leading icon, at 3:1 on the fill.
                let edge = ratio(colours.edge, colours.fill);
                assert!(edge > 1.02 && edge < 1.6, "{name}: the edge is faint, got {edge:.2}");
                assert!(Oklch::from_colour(colours.edge).c <= 0.05, "{name}: the edge is not a saturated outline");
                if kind != BannerKind::Neutral {
                    assert!(ratio(colours.glyph, colours.fill) >= 3.0 - 0.05, "{name}: the icon reaches 3:1");
                }
                if kind == BannerKind::Neutral {
                    continue;
                }
                assert!(text >= AAA_TEXT - 0.05, "{name}: tinted ink reaches AAA, got {text:.2}");
                let hue_of = match kind {
                    BannerKind::Info => Paint::Accent,
                    BannerKind::Success => Paint::Success,
                    BannerKind::Warning => Paint::Warning,
                    _ => Paint::Danger,
                }
                .color(&tokens);
                let (fill, ink, hue) = (
                    Oklch::from_colour(colours.fill),
                    Oklch::from_colour(colours.ink),
                    Oklch::from_colour(hue_of),
                );
                assert!(
                    hue_distance(ink.h, hue.h) < 25.0 && ink.c > 0.02,
                    "{name}: the ink keeps the hue ({:.0} vs {:.0}, chroma {:.3})",
                    ink.h,
                    hue.h,
                    ink.c
                );
                if dark {
                    assert!(ink.l > fill.l, "{name}: lighter than its fill");
                } else {
                    assert!(ink.l < fill.l, "{name}: darker than its fill");
                }
            }
        }
    }
}
