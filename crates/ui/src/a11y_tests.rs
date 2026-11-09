//! What a screen reader is told: the AccessKit nodes bingux-ui builds, read
//! from the tree update a real window would send to AT-SPI.
//!
//! Each case mounts its content after the first frame (a press on a corner
//! box), so the nodes arrive in one update the test can read whole, with the
//! names Freya's tree builder (patched in `vendor/freya-core`) gives them.

use std::collections::HashMap;
use std::time::Duration;

use accesskit::{Live, Node, NodeId, Role, Toggled};
use freya::prelude::*;
use freya_testing::TestingRunner;
use freya_testing::prelude::{MouseEventName, PlatformEvent};

use crate::a11y::{Semantics, SemanticsExt};
use crate::controls::{action_button, checkbox, dropdown, slider};
use crate::group::{Group, section_label};
use crate::icons::Icon;
use crate::rows::{radio_row, radio_rows, row, switch_row, text_row};
use crate::shell::banner::banner;
use crate::snapshot::Snapshot;
use crate::theme::provide_tokens;

#[derive(PartialEq)]
struct Root {
    content: fn() -> Element,
}

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        provide_tokens();
        let mut shown = use_state(|| false);
        rect()
            .expanded()
            .child(
                rect()
                    .width(Size::px(24.0))
                    .height(Size::px(24.0))
                    .on_press(move |_| shown.set(true)),
            )
            .maybe_child(shown().then(|| (self.content)()))
    }
}

/// Every node the content adds, as the window sends them to AT-SPI.
fn nodes(content: fn() -> Element) -> Vec<Node> {
    let mut shot = Snapshot::new(move || Root { content }.into(), (640.0, 480.0));
    shot.settle(Duration::from_millis(50));
    let runner = shot.runner();
    let mut seen: HashMap<NodeId, Node> = HashMap::new();
    let mut take = |runner: &mut TestingRunner| {
        for (id, node) in runner.sync_and_update().nodes {
            seen.insert(id, node);
        }
    };
    for name in [MouseEventName::MouseDown, MouseEventName::MouseUp] {
        runner.send_event(PlatformEvent::Mouse {
            name,
            cursor: (8.0, 8.0).into(),
            button: Some(MouseButton::Left),
        });
        take(runner);
    }
    for _ in 0..4 {
        runner.poll_n(Duration::from_millis(16), 1);
        take(runner);
    }
    seen.into_values().collect()
}

fn with_role(nodes: &[Node], role: Role) -> Vec<&Node> {
    nodes.iter().filter(|node| node.role() == role).collect()
}

fn one(nodes: &[Node], role: Role) -> &Node {
    let found = with_role(nodes, role);
    assert_eq!(found.len(), 1, "one {role:?} expected, found {found:?}");
    found[0]
}

#[test]
fn a_button_is_named_by_its_text_and_says_it_once() {
    let nodes = nodes(|| {
        action_button("Save Changes", |_| {})
            .icon(Icon::Search)
            .into()
    });
    let button = one(&nodes, Role::Button);
    assert_eq!(button.label(), Some("Save Changes"));
    // The text inside it and its icon are not read again.
    for node in nodes
        .iter()
        .filter(|node| matches!(node.role(), Role::Label | Role::SvgRoot))
    {
        assert!(node.is_hidden(), "{node:?} is read a second time");
    }
}

#[test]
fn a_given_name_is_never_replaced_by_the_text_inside() {
    let nodes = nodes(|| {
        rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt("Close")
            .child(label().text("×"))
            .into()
    });
    assert_eq!(one(&nodes, Role::Button).label(), Some("Close"));
}

#[test]
fn a_switch_in_a_row_is_named_by_the_title_and_described_by_the_subtitle() {
    let nodes = nodes(|| {
        Group::new(None)
            .row(switch_row("Wi-Fi", true, |_: bool| {}).subtitle("Connected to Home"))
            .into()
    });
    let switch = one(&nodes, Role::Switch);
    assert_eq!(switch.label(), Some("Wi-Fi"));
    assert_eq!(switch.description(), Some("Connected to Home"));
    assert_eq!(switch.toggled(), Some(Toggled::True));
}

#[test]
fn a_dropdown_in_a_row_is_named_by_the_row_and_its_value_is_the_choice() {
    let nodes = nodes(|| {
        let options = vec![
            ("light".to_string(), "Light".to_string()),
            ("dark".to_string(), "Dark".to_string()),
        ];
        Group::new(None)
            .row(row("Style").trailing(dropdown(options, "dark", |_: String| {})))
            .into()
    });
    let combo = one(&nodes, Role::ComboBox);
    assert_eq!(combo.label(), Some("Style"));
    assert_eq!(combo.value(), Some("Dark"));
    assert_eq!(combo.is_expanded(), Some(false));
}

#[test]
fn a_slider_and_a_check_box_in_rows_are_named_by_them() {
    let nodes = nodes(|| {
        Group::new(None)
            .row(row("Volume").trailing(slider(40.0, |_: f64| {})))
            .row(row("Mute").trailing(checkbox(false, |_: bool| {})))
            .into()
    });
    assert_eq!(one(&nodes, Role::Slider).label(), Some("Volume"));
    assert_eq!(one(&nodes, Role::CheckBox).label(), Some("Mute"));
}

#[test]
fn a_text_field_is_named_by_its_row_and_its_text_is_the_value() {
    let nodes = nodes(|| {
        let value = State::create("bingux".to_string());
        Group::new(None)
            .row(text_row("Host name", value, |_: String| {}))
            .into()
    });
    let field = one(&nodes, Role::TextInput);
    assert_eq!(field.label(), Some("Host name"));
    assert_eq!(field.value(), Some("bingux"));
}

#[test]
fn titles_are_headings_and_a_titled_group_is_a_named_group() {
    let nodes = nodes(|| {
        rect()
            .child(section_label("Connections"))
            .child(Group::titled("Sound").row(row("Output")))
            .into()
    });
    let headings = with_role(&nodes, Role::Heading);
    assert!(
        headings
            .iter()
            .any(|node| node.label() == Some("Connections") && node.level() == Some(2)),
        "{headings:?}"
    );
    assert!(
        with_role(&nodes, Role::Group)
            .iter()
            .any(|node| node.label() == Some("Sound"))
    );
}

#[test]
fn radio_rows_are_a_radio_group_of_named_radio_buttons() {
    let nodes = nodes(|| {
        radio_rows(
            Some("Text size".to_string()),
            vec![
                radio_row("Small", false, |_| {}).subtitle("Compact"),
                radio_row("Large", true, |_| {}),
            ],
        )
        .into()
    });
    let group = one(&nodes, Role::RadioGroup);
    assert_eq!(group.label(), Some("Text size"));
    let radios = with_role(&nodes, Role::RadioButton);
    let small = radios
        .iter()
        .find(|node| node.label() == Some("Small"))
        .expect("Small");
    assert_eq!(
        small.description(),
        Some("Compact"),
        "the subtitle is the hint, not the name"
    );
    assert!(
        radios
            .iter()
            .any(|node| node.label() == Some("Large") && node.toggled() == Some(Toggled::True))
    );
}

#[test]
fn a_banner_is_read_out_when_it_appears() {
    let nodes = nodes(|| banner("Updates are ready").into());
    let status = one(&nodes, Role::Status);
    assert_eq!(status.label(), Some("Updates are ready"));
    assert_eq!(status.live(), Some(Live::Polite));
}

#[test]
fn semantics_on_any_element_reach_the_tree() {
    let nodes = nodes(|| {
        rect()
            .semantics(
                Semantics::new()
                    .group("Battery")
                    .value("82 %, charging")
                    .hint("Lasts about 5 hours"),
            )
            .child(label().text("82 %"))
            .into()
    });
    let group = one(&nodes, Role::Group);
    assert_eq!(group.label(), Some("Battery"));
    assert_eq!(group.value(), Some("82 %, charging"));
    assert_eq!(group.description(), Some("Lasts about 5 hours"));
}
