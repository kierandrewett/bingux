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
                                near(b[3] - b[2], button) && b[1] - b[0] > button && b[1] < dismiss[0]
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

/// The toast's timer waits while the pointer is over it.
#[test]
fn a_toast_stays_up_while_the_pointer_is_over_it() {
    let mut shot = scene(Place::Toast, "Saved", false, BannerKind::Neutral, (900.0, 400.0));
    let bar = named(&mut shot, "Banner")[0];
    shot.hover((bar[0] + bar[1]) / 2.0, (bar[2] + bar[3]) / 2.0);
    shot.settle(Duration::from_secs(6));
    assert_eq!(named(&mut shot, "Banner").len(), 1, "held by the pointer");
    shot.hover(20.0, 20.0);
    shot.settle(Duration::from_secs(6));
    assert!(named(&mut shot, "Banner").is_empty(), "gone once released");
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
                let label = ratio(colours.on_action, colours.action);
                assert!(label >= AA_TEXT, "{name}: action label is {label:.2}");
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
