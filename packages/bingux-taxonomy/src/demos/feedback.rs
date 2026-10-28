//! Controls: progress, tooltips, marks and small widgets.

use bingux_ui::controls::avatar::current_user;
use bingux_ui::controls::sortable::sortable;
use bingux_ui::controls::{EccLevel, elide_middle, elide_name, elide_path, path_label_trace};
use bingux_ui::elide::distinguish;
use bingux_ui::prelude::*;
use freya::prelude::*;

use super::common::{LONG, rows};
use crate::model::Spec;

pub const PROGRESS: &[Spec] = &[
    Spec::live("Determinate, advance it", || {
        let mut progress = use_state(|| 0.35_f64);
        vstack(S3)
            .child(progress_bar(progress()).width(Size::fill()))
            .child(
                action_button("Advance", move |_| {
                    progress.set((progress() + 0.15).min(1.0))
                })
                .compact(),
            )
            .into_element()
    }),
    Spec::live("Indeterminate", || {
        progress_bar(None).width(Size::fill()).into_element()
    }),
    Spec::forced("With text", || {
        progress_bar(0.6)
            .text("Copying 6 of 10 files")
            .width(Size::fill())
            .into_element()
    }),
    Spec::forced("Complete", || {
        progress_bar(1.0).width(Size::fill()).into_element()
    }),
];

pub const SPINNER: &[Spec] = &[
    Spec::live("Small, medium, large", || {
        hstack(S6)
            .child(spinner(SpinnerSize::Small))
            .child(spinner(SpinnerSize::Medium))
            .child(spinner(SpinnerSize::Large))
            .into_element()
    }),
    Spec::live("With text", || {
        spinner(SpinnerSize::Large).text("Loading").into_element()
    }),
    Spec::forced("In a button", || {
        action_button("Saving", |_| {}).loading(true).into_element()
    }),
];

pub const TOOLTIP: &[Spec] = &[
    Spec::live("Hover the icon", || {
        hover_tip(
            icon(
                Icon::DialogInformation,
                IconSize::Row,
                Paint::Muted.color(&use_tokens()),
            ),
            "Shown after a short delay, beside the pointer",
        )
    }),
    Spec::live("Icon with a tooltip", || {
        tooltip_icon(
            Icon::DialogWarning,
            IconSize::Row,
            Paint::Warning,
            "Check the cable",
        )
    }),
    Spec::live("On an icon button", || {
        icon_button(Icon::Delete, |_| {})
            .tooltip("Delete")
            .into_element()
    }),
    Spec::live("Label changes while hovered", || {
        // Press while the tip is up: its text changes in place, centred on the
        // button, and the bubble never jumps.
        let mut shown = use_state(|| false);
        icon_button(
            if shown() { Icon::ViewConceal } else { Icon::ViewReveal },
            move |_| shown.toggle(),
        )
        .tooltip(if shown() { "Hide password" } else { "Show password" })
        .into_element()
    }),
    Spec::live("Neighbours: the second opens at once", || {
        let mut muted = use_state(|| false);
        hstack(S1)
            .child(
                icon_button(
                    if muted() { Icon::AudioVolumeMuted } else { Icon::AudioVolumeMedium },
                    move |_| muted.toggle(),
                )
                .tooltip(if muted() { "Unmute" } else { "Mute" }),
            )
            .child(icon_button(Icon::Refresh, |_| {}).tooltip("Refresh"))
            .child(icon_button(Icon::Delete, |_| {}).tooltip("Delete"))
            .into_element()
    }),
    Spec::live("Long text wraps", || {
        hover_tip(
            icon(
                Icon::DialogInformation,
                IconSize::Row,
                Paint::Muted.color(&use_tokens()),
            ),
            "A tooltip is at most 280 px wide. Longer text wraps onto as many as four lines, and is cut after that.",
        )
    }),
];

pub const AVATAR: &[Spec] = &[
    Spec::forced("Sizes", || {
        hstack(S3)
            .child(avatar("guest", Dim::NavIcon))
            .child(avatar("guest", Dim::Box11))
            .child(avatar("guest", Dim::Box24))
            .into_element()
    }),
    Spec::forced("The signed-in account", || {
        avatar(current_user(), Dim::Box11).into_element()
    }),
];

pub const QR_CODE: &[Spec] = &[
    Spec::forced("Medium error correction", || {
        qr_code("https://example.com/pair?key=7f3a9c")
            .label("QR code for https://example.com/pair?key=7f3a9c")
            .size(176.0)
            .into_element()
    }),
    Spec::forced("High error correction", || {
        qr_code("WIFI:S:Home;T:WPA;P:correct horse;;")
            .ecc(EccLevel::H)
            .label("QR code for joining Home")
            .size(176.0)
            .into_element()
    }),
];

pub const SIGNAL: &[Spec] = &[Spec::forced("None to full, secured", || {
    let tokens = use_tokens();
    let mut line = hstack(S3);
    for level in 0..=4 {
        line = line.child(signal_bars(level, level == 4, &tokens));
    }
    line.child(signal_bars_labelled(
        3,
        true,
        Some("Signal: good".to_string()),
        &tokens,
    ))
    .into_element()
})];

/// The path of the width slider: seven segments, so every form shows.
const WIDE_PATH: &str = "/home/kieran/Documents/Projects/bingux/design/final-report-2026.pdf";

pub const PATH_LABEL: &[Spec] = &[
    Spec::forced("Fits", || {
        path_label("~/Documents/report.pdf").into_element()
    }),
    Spec::forced("Cut in the middle", || {
        path_label("/home/kieran/Documents/Projects/bingux/design/final-report-2026.pdf")
            .into_element()
    }),
    Spec::live("Width slider: each form in turn", || {
        let tokens = use_tokens();
        let mut width = use_state(|| 160.0_f64);
        let room = 40.0 + width() as f32;
        let (shown, step) = path_label_trace(WIDE_PATH, typography::BODY, room);
        vstack(S2)
            .child(
                slider(width(), move |next| width.set(next))
                    .max(260.0)
                    .step(1.0)
                    .fill(),
            )
            .child(
                rect()
                    .tw(&tokens, &[p(S2), rounded(Radius::Control), bg(Paint::Control)])
                    .child(
                        rect()
                            .width(Size::px(room))
                            .child(path_label(WIDE_PATH)),
                    ),
            )
            .child(caption(format!("{room:.0} px, {step:?}: {shown}")))
            .into_element()
    })
    .script("wait 300; save {out}"),
    Spec::forced("The forms at fixed widths", || {
        let tokens = use_tokens();
        let mut column = vstack(S2);
        for room in [300.0_f32, 250.0, 205.0, 170.0, 140.0, 110.0, 80.0, 50.0] {
            let (_, step) = path_label_trace(WIDE_PATH, typography::BODY, room);
            column = column
                .child(
                    rect()
                        .tw(&tokens, &[p(S1), rounded(Radius::Control), bg(Paint::Control)])
                        .child(rect().width(Size::px(room)).child(path_label(WIDE_PATH))),
                )
                .child(caption(format!("{room:.0} px: {step:?}")));
        }
        column.into_element()
    }),
    Spec::forced("Name only, selectable and copyable", || {
        path_label("final-report-2026-revised-for-review.pdf")
            .name()
            .selectable(true)
            .copyable(true)
            .into_element()
    }),
    Spec::forced("Compound extension stays whole", || {
        rect()
            .tw(&use_tokens(), &[w(Dim::Menu)])
            .child(path_label("backup-of-the-home-folder-2026-10-26.tar.gz").name())
            .into_element()
    }),
    Spec::forced("Same names told apart in a list", || {
        let paths = [
            "/work/app/src/index.ts",
            "/work/lib/src/index.ts",
            "/work/app/test/index.ts",
        ];
        rows(
            distinguish(&paths)
                .iter()
                .map(|label| row(label.name.clone()).subtitle(label.context.clone()).into_element())
                .collect(),
        )
    }),
    Spec::forced("Elide helpers", || {
        // Three eliders measure with one glyph a pixel wide, so the cut is
        // visible in plain text.
        let measure = |text: &str| text.chars().count() as f32;
        rows(vec![
            row("elide_path")
                .subtitle(elide_path("/a/very/long/path/to/file.txt", 18.0, &measure))
                .into_element(),
            row("elide_name")
                .subtitle(elide_name("final-report-2026.pdf", 14.0, &measure))
                .into_element(),
            row("elide_middle")
                .subtitle(elide_middle(LONG, 24.0, &measure))
                .into_element(),
        ])
    }),
];

pub const FOCUS_STOP: &[Spec] = &[
    Spec::live("Tab to the box: the ring animates in", || {
        let tokens = use_tokens();
        focus_stop(
            rect()
                .tw(
                    &tokens,
                    &[p(S3), rounded(Radius::Control), bg(Paint::Control)],
                )
                .child(styled_label(typography::BODY, "A custom control")),
            "A custom control",
            AccessibilityRole::Button,
            true,
            Radius::Control,
            |_| {},
        )
        .into_element()
    })
    .script("key tab; wait 300; save {out}"),
    Spec::forced("Disabled: not a stop", || {
        let tokens = use_tokens();
        focus_stop(
            rect()
                .tw(
                    &tokens,
                    &[p(S3), rounded(Radius::Control), bg(Paint::Control)],
                )
                .child(styled_label(typography::BODY, "Not focusable")),
            "Not focusable",
            AccessibilityRole::Button,
            false,
            Radius::Control,
            |_| {},
        )
        .into_element()
    }),
    Spec::live("With a context menu", || {
        let tokens = use_tokens();
        focus_stop_menu(
            rect()
                .tw(
                    &tokens,
                    &[p(S3), rounded(Radius::Control), bg(Paint::Control)],
                )
                .child(styled_label(typography::BODY, "Right-click, or Shift+F10")),
            "Right-click me",
            AccessibilityRole::Button,
            true,
            Radius::Control,
            |_| {},
            vec![
                menu_item("Rename", |_| {}),
                menu_item("Delete", |_| {}).destructive(),
            ],
        )
        .into_element()
    }),
];

pub const BREADCRUMB: &[Spec] = &[
    Spec::live("Pick a crumb", || {
        let mut at = use_state(|| 3_u32);
        let all = ["Home", "Documents", "Projects", "bingux"];
        breadcrumb(
            all.iter()
                .enumerate()
                .take(at() as usize + 1)
                .map(|(index, name)| (index as u32, name.to_string()))
                .collect(),
            move |key: u32| at.set(key),
        )
        .into_element()
    }),
    Spec::forced("Too long for the width", || {
        breadcrumb(
            (0..8_u32)
                .map(|index| (index, format!("Folder number {index}")))
                .collect(),
            |_| {},
        )
        .into_element()
    }),
    Spec::forced("Code style", || {
        breadcrumb(
            vec![
                (0_u32, "rect".to_string()),
                (1, "ScrollView".to_string()),
                (2, "Label".to_string()),
            ],
            |_| {},
        )
        .code()
        .flat()
        .into_element()
    }),
];

pub const SPLIT_PANE: &[Spec] = &[
    Spec::live("Drag the divider", || {
        let ratio = use_state(|| 0.4_f32);
        let tokens = use_tokens();
        rect()
            .tw(
                &tokens,
                &[w_full(), h(Dim::Box24), rounded(Radius::Card), clip()],
            )
            .child(split_pane(
                rect()
                    .tw(&tokens, &[p(S3), bg(Paint::Card)])
                    .child(caption("First pane")),
                rect()
                    .tw(&tokens, &[p(S3), bg(Paint::Control)])
                    .child(caption("Second pane")),
                ratio,
            ))
            .into_element()
    }),
    Spec::live("Vertical", || {
        let ratio = use_state(|| 0.5_f32);
        let tokens = use_tokens();
        rect()
            .tw(
                &tokens,
                &[w_full(), h(Dim::Box24), rounded(Radius::Card), clip()],
            )
            .child(
                split_pane(
                    rect()
                        .tw(&tokens, &[p(S3), bg(Paint::Card)])
                        .child(caption("Top")),
                    rect()
                        .tw(&tokens, &[p(S3), bg(Paint::Control)])
                        .child(caption("Bottom")),
                    ratio,
                )
                .vertical(),
            )
            .into_element()
    }),
];

/// Move the item at `from` to place `to`, as `on_reorder` says.
fn reordered<T>(list: &mut Vec<T>, from: usize, to: usize) {
    let item = list.remove(from);
    list.insert(to, item);
}

/// The four pages of the first specimen, as rows with their icon.
fn page_row(name: &str) -> Element {
    let icon = match name {
        "Network" => Icon::Network,
        "Sound" => Icon::Sound,
        "Power" => Icon::Power,
        _ => Icon::Keyboard,
    };
    row(name).leading(icon).into_element()
}

pub const SORTABLE: &[Spec] = &[
    Spec::live(
        "Drag the grip, or focus it and press Up or Down; Alt+Up or Down on a row",
        || {
            let mut order = use_state(|| vec!["Network", "Sound", "Power", "Keyboard"]);
            let names = order.read().clone();
            sortable(
                names.iter().map(|name| page_row(name)).collect(),
                move |(from, to): (usize, usize)| reordered(&mut order.write(), from, to),
            )
            .labels(names.iter().map(|name| name.to_string()).collect())
            .into_element()
        },
    ),
    Spec::live("Rows with their own controls", || {
        let mut order = use_state(|| vec!["Compressor", "Equaliser", "Limiter"]);
        let mut on = use_state(|| vec![true, true, false]);
        let names = order.read().clone();
        let enabled = on.read().clone();
        let rows: Vec<Element> = names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                row(*name)
                    .trailing(switch(enabled[index], move |value: bool| {
                        on.write()[index] = value;
                    }))
                    .into_element()
            })
            .collect();
        sortable(rows, move |(from, to): (usize, usize)| {
            reordered(&mut order.write(), from, to);
            reordered(&mut on.write(), from, to);
        })
        .labels(names.iter().map(|name| name.to_string()).collect())
        .into_element()
    }),
    Spec::live("A long list: a line shows where it lands", || {
        let mut order = use_state(|| (1..=12).collect::<Vec<u32>>());
        let names: Vec<String> = order.read().iter().map(|n| format!("Profile {n}")).collect();
        sortable(
            names.iter().map(|name| row(name.clone()).into_element()).collect(),
            move |(from, to): (usize, usize)| reordered(&mut order.write(), from, to),
        )
        .labels(names.clone())
        .drop_indicator()
        .into_element()
    }),
];

/// A part that fails on purpose. Its first render is fine; once `broken` it
/// calls one more hook than the render before, which is the panic Freya's
/// "Hook functions must follow these rules" is. Nothing here fails until a
/// button is pressed: it is a test of the error boundary, not of the page.
#[derive(PartialEq)]
struct DeliberateFailure {
    broken: bool,
}

impl Component for DeliberateFailure {
    fn render(&self) -> impl IntoElement {
        let _first = use_state(|| 0);
        if self.broken {
            let _extra = use_state(|| 1);
        }
        styled_label(
            typography::BODY,
            "This part is fine. Retry mounted it again with fresh hooks.",
        )
    }
}

pub const ERROR_BOUNDARY: &[Spec] = &[
    Spec::live("A test of the boundary: press to make the part fail", || {
        let mut broken = use_state(|| false);
        vstack(S3)
            .child(action_button(
                "Break this part (a test of the error boundary)",
                move |_| broken.set(true),
            ))
            .child(error_boundary(
                "Boundary test",
                DeliberateFailure { broken: broken() },
            ))
            .into_element()
    }),
    Spec::live("As one row of a list", || {
        let mut broken = use_state(|| false);
        let tokens = use_tokens();
        vstack(S3)
            .child(action_button(
                "Break the row (a test of the error boundary)",
                move |_| broken.set(true),
            ))
            .child(
                rect().tw(&tokens, BOX).child(
                    error_boundary("Boundary test row", DeliberateFailure { broken: broken() })
                        .look(BoundaryLook::Row),
                ),
            )
            .into_element()
    }),
    Spec::live("An event handler that panics: a toast, no card", || {
        action_button(
            "Panic in an event handler (a test of the guard)",
            |_| panic!("deliberate panic in an event handler, a test of the guard"),
        )
        .into_element()
    }),
];
