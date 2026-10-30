//! Renders the keyboard focus ring on every kind of control, one PNG each.
//!
//! One page holds a specimen of each control. The run presses Tab through it and
//! saves the frame after each press, so every control is seen with the real
//! ring (the one that animates in and follows the control's shape), and writes
//! `crops.tsv`: the name and the box of the focused control in each frame, which
//! `scripts/focus-ring-sheet.py` crops into a contact sheet.
//!
//! ```sh
//! cargo run --example focus_ring_sheet --features testing -- OUT_DIR [WIDTH]
//! ```
//!
//! `BINGUX_SETTINGS_SNAPSHOT_SCHEME=prefer-light|prefer-dark` picks the theme and `WIDTH`
//! (default 800; 390 is the phone density) the window width.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Duration;

use bingux_ui::prelude::{
    Group, OverlayHost, WindowHost, action_button, checkbox, dropdown, focus_stop, primary_button, radio_row,
    radio_rows, row, segmented, slider, switch, text_field, use_tokens,
};
use bingux_ui::snapshot::Snapshot;
use bingux_ui::style::*;
use bingux_ui::theme::{SharedTokens, freya_theme, load_tokens};
use bingux_ui::typography;
use freya::prelude::*;
use freya_core::element::ElementExt;

/// The window root: it provides the tokens the window host reads.
#[derive(PartialEq)]
struct Root;

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        // The tokens come from the environment's scheme at once, not from the
        // asynchronous settings load, so a frame is never the default scheme.
        let shared = use_hook(|| SharedTokens(State::create(load_tokens())));
        use_provide_root_context(move || shared);
        let tokens = *shared.0.read();
        let _ = use_init_theme(|| freya_theme(&tokens));
        WindowHost::new(OverlayHost::new(Sheet))
    }
}

/// The specimens, in the order Tab visits them.
const ORDER: [&str; 14] = [
    "switch",
    "button",
    "suggested",
    "destructive",
    "text-field",
    "text-field-invalid",
    "dropdown",
    "segmented",
    "settings-row",
    "slider",
    "checkbox",
    "radio",
    "file-row",
    "sidebar-item",
];

/// One specimen of each control the ring is drawn on.
#[derive(PartialEq)]
struct Sheet;

impl Component for Sheet {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let text = use_state(String::new);
        let invalid = use_state(|| "not an address".to_string());
        let options = || {
            vec![
                ("a".to_string(), "Collabora Office".to_string()),
                ("b".to_string(), "LibreOffice".to_string()),
            ]
        };
        let sidebar_item = rect()
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    gap(S2),
                    px(S3),
                    h(Dim::Control),
                    w_full(),
                    bg(Paint::Selection),
                    rounded(Radius::Control),
                ],
            )
            .child(typography::BODY.label(&tokens, "Sidebar item"));
        let file_row = rect()
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    gap(S2),
                    px(S3),
                    h(Dim::Control),
                    w_full(),
                    rounded(Radius::Inner),
                ],
            )
            .child(typography::BODY.label(&tokens, "report-2026.pdf"));
        rect()
            .tw(&tokens, &[flex_col(), p(S4), gap(S3), w_full(), h_full(), bg(Paint::Bg)])
            .child(switch(true, |_: bool| {}))
            .child(action_button("Button", |_: ()| {}))
            .child(primary_button("Suggested", |_: ()| {}))
            .child(action_button("Destructive", |_: ()| {}).destructive())
            .child(text_field(text, |_: String| {}).placeholder("Text field"))
            .child(text_field(invalid, |_: String| {}).invalid(Some("Not an address".to_string())))
            .child(dropdown(options(), "a", |_: String| {}))
            .child(segmented(options(), "a", |_: String| {}))
            .child(
                Group::new(None::<String>)
                    .row(row("Settings row").subtitle("With a subtitle").on_press(|_: ()| {})),
            )
            .child(slider(0.5, |_: f64| {}))
            .child(checkbox(true, |_: bool| {}).text("Checkbox"))
            .child(radio_rows(
                None,
                vec![
                    radio_row("Radio", true, |_: ()| {}),
                    radio_row("Other", false, |_: ()| {}),
                ],
            ))
            .child(focus_stop(
                file_row,
                "File list row",
                AccessibilityRole::ListBoxOption,
                true,
                Radius::Inner,
                |_: ()| {},
            ))
            .child(focus_stop(
                sidebar_item,
                "Sidebar item",
                AccessibilityRole::Tab,
                true,
                Radius::Control,
                |_: ()| {},
            ))
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().expect("usage: focus_ring_sheet OUT_DIR [WIDTH]"));
    let width: f32 = args.next().map_or(800.0, |width| width.parse().expect("WIDTH is a number"));
    std::fs::create_dir_all(&out).expect("create OUT_DIR");

    let mut shot = Snapshot::new(|| Root.into_element(), (width, 1500.0));
    shot.settle(Duration::from_millis(800));
    let mut crops = String::new();
    let mut seen = Vec::new();
    for press in 0..24 {
        shot.key(Key::Named(NamedKey::Tab));
        shot.settle(Duration::from_millis(450));
        let focused = shot
            .runner()
            .run_in(|| *Platform::get().current_window().focused_accessibility_id.peek());
        if seen.contains(&focused) {
            break;
        }
        seen.push(focused);
        let found = shot.runner().find_many(|node, element| {
            let data = element.accessibility();
            (data.a11y_id == Some(focused)).then(|| node.layout().area)
        });
        let Some(area) = found.into_iter().next() else {
            continue;
        };
        // Tab visits the specimens in the order they are laid out above.
        let name = ORDER.get(press).copied().unwrap_or("extra");
        let file = format!("{press:02}-{name}.png");
        shot.save(out.join(&file));
        writeln!(
            crops,
            "{file}\t{}\t{}\t{}\t{}",
            area.min_x(),
            area.min_y(),
            area.width(),
            area.height()
        )
        .expect("write to a string");
    }
    std::fs::write(out.join("crops.tsv"), crops).expect("write crops.tsv");
}
