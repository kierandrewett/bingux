//! A split button: a primary action beside a chevron that opens a menu, for a
//! command with a few variants (a view button whose menu picks the view, a
//! save button whose menu offers "Save as").
//!
//! At rest it is a bare icon, a hairline and a chevron with no fill. Under the
//! pointer the whole control becomes one `Control`-radius box with a hover fill
//! and the hairline fades out; the segment under the pointer takes a second
//! hover layer of its own. The segments sit `S1` inside the box, so their
//! corners are `Inner`, the box radius less that inset. Each segment is its own
//! tab stop with its own hover, press and focus ring.
//!
//! The primary segment runs `on_press`. The chevron opens `menu` against the
//! whole control, below it (above when there is no room), its edge aligned
//! with the control's: the same placement as [`menu_button`] and
//! [`Dropdown`](super::Dropdown). Down on either segment also opens it, with
//! the first item highlighted; Escape closes it and focus returns to the
//! segment that opened it. A press on the chevron while the menu is open
//! closes it. With [`SplitButton::on_menu`] the chevron runs a handler instead,
//! for an app that opens its own popup.
//!
//! [`menu_button`]: crate::shell::menu_button

use super::press::{is_touch, use_press};
use super::tooltip::hover_tip;
use freya::prelude::*;

use crate::component::ComponentName;
use crate::focus::{press_focus, use_focus_core};
use crate::icons::{self, Icon};
use crate::motion::{Focusable, MOVE_MS, use_tween};
use crate::shell::panel::{PanelRect, PanelTrigger};
use crate::shell::{MenuItem, use_overlays};
use crate::style::*;
use crate::surface::{is_activation_key, use_enabled, use_tokens};
use crate::typography::{BODY, Ink};

/// The chevron segment's accessible name when none is given.
const DEFAULT_MENU_LABEL: &str = "More options";

/// A primary action and a menu in one control. Build with
/// [`SplitButton::new`] or [`split_button`].
#[derive(PartialEq)]
pub struct SplitButton {
    icon: Icon,
    label: Option<String>,
    tooltip: Option<String>,
    menu_label: String,
    on_press: EventHandler<()>,
    items: Vec<MenuItem>,
    on_menu: Option<EventHandler<()>>,
    enabled: bool,
}

/// A split button showing `icon`. Give it `on_press`, a `tooltip` or `label`
/// (its accessible name) and a `menu`.
pub fn split_button(icon: Icon) -> SplitButton {
    SplitButton::new(icon)
}

impl SplitButton {
    pub fn new(icon: Icon) -> Self {
        Self {
            icon,
            label: None,
            tooltip: None,
            menu_label: DEFAULT_MENU_LABEL.to_string(),
            on_press: EventHandler::new(|_: ()| {}),
            items: Vec::new(),
            on_menu: None,
            enabled: true,
        }
    }

    /// What the primary segment does. It runs on a click, a touch, and Enter
    /// or Space.
    pub fn on_press(mut self, on_press: impl Into<EventHandler<()>>) -> Self {
        self.on_press = on_press.into();
        self
    }

    /// The primary segment's tooltip. It is also the segment's accessible
    /// name unless a [`label`](Self::label) is set.
    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    /// Show `text` after the icon in the primary segment. It is the segment's
    /// accessible name.
    pub fn label(mut self, text: impl Into<String>) -> Self {
        self.label = Some(text.into());
        self
    }

    /// The chevron segment's accessible name: what the menu is
    /// for, such as "View options". It is "More options" by default.
    pub fn menu_label(mut self, text: impl Into<String>) -> Self {
        self.menu_label = text.into();
        self
    }

    /// The items of the menu the chevron opens.
    pub fn menu(mut self, items: Vec<MenuItem>) -> Self {
        self.items = items;
        self.on_menu = None;
        self
    }

    /// Run `on_menu` when the chevron is activated, in place of a menu: for
    /// an app that opens its own popup. The control draws no open state.
    pub fn on_menu(mut self, on_menu: impl Into<EventHandler<()>>) -> Self {
        self.on_menu = Some(on_menu.into());
        self.items = Vec::new();
        self
    }

    /// Dim the control and ignore presses.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// The two tab stops of the control.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Segment {
    Primary,
    Menu,
}

/// What a key does on a focused segment.
#[derive(Clone, Copy, Debug, PartialEq)]
enum KeyAction {
    /// Run the primary action.
    Press,
    /// Open the menu with its first item highlighted.
    OpenMenu,
}

/// Enter and Space press the primary segment and open the menu from the
/// chevron; Down opens the menu from either. Any other key is not ours, so it
/// keeps travelling (Tab moves focus, Escape reaches the popup around us).
fn route_key(segment: Segment, key: &Key, has_menu: bool) -> Option<KeyAction> {
    let down = *key == Key::Named(NamedKey::ArrowDown);
    match segment {
        Segment::Primary if is_activation_key(key) => Some(KeyAction::Press),
        Segment::Primary | Segment::Menu if down && has_menu => Some(KeyAction::OpenMenu),
        Segment::Menu if is_activation_key(key) && has_menu => Some(KeyAction::OpenMenu),
        _ => None,
    }
}

impl Component for SplitButton {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = use_overlays();
        let core = use_focus_core();
        let usable = use_enabled(self.enabled);
        let primary_id = use_a11y();
        let menu_id = use_a11y();
        let primary_focus = use_focus(primary_id);
        let menu_focus = use_focus(menu_id);
        let primary_press = use_press();
        let menu_press = use_press();
        let mut control_hover = use_state(|| false);
        let mut primary_hover = use_state(|| false);
        let mut primary_down = use_state(|| false);
        let mut menu_hover = use_state(|| false);
        let mut menu_down = use_state(|| false);
        let mut area = use_state(Area::default);

        let has_items = !self.items.is_empty();
        let has_menu = has_items || self.on_menu.is_some();
        let open =
            has_items && (overlays.menu_owned_by(primary_id) || overlays.menu_owned_by(menu_id));
        let lit = usable && (control_hover() || open);
        // The hairline only shows at rest, and fades as the box fills.
        let divider = use_tween(if lit { 0.0 } else { 1.0 }, tokens.motion(MOVE_MS));
        // Open the menu against the whole control, owned by the segment that
        // asked so focus returns to it.
        let open_menu = {
            let items = self.items.clone();
            let on_menu = self.on_menu.clone();
            move |owner: AccessibilityId, keyboard: bool| {
                if let Some(on_menu) = &on_menu {
                    on_menu.call(());
                } else if !items.is_empty() {
                    overlays.show_menu_from(
                        Some(owner),
                        PanelTrigger::Control(PanelRect::from_area(&area.read())),
                        items.clone(),
                        keyboard,
                    );
                }
            }
        };

        // The segments are as tall as the box less its inset, and the
        // primary one is square unless it carries a label.
        let segment_height = Dim::Button.px() - 2.0 * S1.px();
        let name = self.label.clone().or_else(|| self.tooltip.clone());

        let primary = {
            let on_press = self.on_press.clone();
            let core = core.clone();
            let on_key = on_press.clone();
            let open_menu = open_menu.clone();
            let has_label = self.label.is_some();
            rect()
                .component("SplitButtonPrimary")
                .a11y_id(primary_id)
                .a11y_focusable(usable)
                .a11y_enabled(usable)
                .a11y_role(AccessibilityRole::Button)
                .maybe(name.is_some(), |element| {
                    element.a11y_alt(name.clone().unwrap_or_default())
                })
                .height(Size::px(segment_height))
                .maybe(!has_label, |element| {
                    element.width(Size::px(segment_height))
                })
                .tw(
                    &tokens,
                    &[
                        flex_row(),
                        center(),
                        gap(S2),
                        rounded(Radius::Inner),
                        surface(Paint::Transparent, primary_hover(), primary_down(), false),
                    ],
                )
                .maybe(has_label, |element| element.tw(&tokens, &[px(S3)]))
                .maybe(usable, |element| {
                    element
                        .on_pointer_enter(move |_| primary_hover.set(true))
                        .on_pointer_leave(move |_| {
                            primary_hover.set(false);
                            primary_down.set_if_modified(false);
                        })
                        .on_pointer_down(move |event: Event<PointerEventData>| {
                            event.stop_propagation();
                            primary_down.set(true);
                            primary_press.begin(&event, None);
                        })
                        .on_global_pointer_move(move |event: Event<PointerEventData>| {
                            primary_press.moved(&event);
                        })
                        .on_global_pointer_up(move |_| {
                            primary_press.release();
                            primary_down.set_if_modified(false);
                        })
                        .on_press(move |event: Event<PressEventData>| {
                            event.stop_propagation();
                            if !primary_press.tap(&event) {
                                return;
                            }
                            on_press.call(());
                            press_focus(&core, primary_id);
                        })
                        .on_key_down(move |event: Event<KeyboardEventData>| {
                            match route_key(Segment::Primary, &event.key, has_menu) {
                                Some(KeyAction::Press) => {
                                    event.stop_propagation();
                                    on_key.call(());
                                }
                                Some(KeyAction::OpenMenu) => {
                                    event.stop_propagation();
                                    open_menu(primary_id, true);
                                }
                                None => {}
                            }
                        })
                })
                .child(icons::icon(
                    self.icon,
                    IconSize::Control,
                    Paint::Text.color(&tokens),
                ))
                .maybe_child(
                    self.label
                        .clone()
                        .map(|text| BODY.in_ink(Ink::Text).label(&tokens, text).max_lines(1)),
                )
                .focusable(primary_focus(), Radius::Inner)
        };
        let primary = match &self.tooltip {
            Some(text) => hover_tip(primary, text.clone()),
            None => Element::from(primary),
        };

        let menu = has_menu.then(|| {
            let down_open = open_menu.clone();
            let key_open = open_menu.clone();
            let press_open = open_menu.clone();
            let button = rect()
                .component("SplitButtonMenu")
                .a11y_id(menu_id)
                .a11y_focusable(usable)
                .a11y_enabled(usable)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(self.menu_label.clone())
                .a11y_builder(move |builder| {
                    if has_items {
                        builder.set_expanded(open);
                    }
                })
                .height(Size::px(segment_height))
                .tw(
                    &tokens,
                    &[
                        w(Dim::StepperHalf),
                        center(),
                        rounded(Radius::Inner),
                        surface(Paint::Transparent, menu_hover() || open, menu_down(), false),
                    ],
                )
                .maybe(usable, |element| {
                    element
                        .on_pointer_enter(move |_| menu_hover.set(true))
                        .on_pointer_leave(move |_| {
                            menu_hover.set(false);
                            menu_down.set_if_modified(false);
                        })
                        .on_pointer_down(move |event: Event<PointerEventData>| {
                            event.stop_propagation();
                            menu_down.set(true);
                            menu_press.begin(&event, None);
                            // A mouse opens the menu on the way down, as a
                            // menu button does; a touch waits for the lift,
                            // so a finger that scrolls past does not open it.
                            if has_items && !is_touch(&event) {
                                down_open(menu_id, false);
                            }
                        })
                        .on_global_pointer_move(move |event: Event<PointerEventData>| {
                            menu_press.moved(&event);
                        })
                        .on_global_pointer_up(move |_| {
                            menu_press.release();
                            menu_down.set_if_modified(false);
                        })
                        .on_press(move |event: Event<PressEventData>| {
                            event.stop_propagation();
                            let mouse_opened =
                                has_items && !matches!(event.data(), PressEventData::Touch(_));
                            if mouse_opened || !menu_press.tap(&event) {
                                return;
                            }
                            press_open(menu_id, false);
                            if !has_items {
                                press_focus(&core, menu_id);
                            }
                        })
                        .on_key_down(move |event: Event<KeyboardEventData>| {
                            if route_key(Segment::Menu, &event.key, has_menu)
                                == Some(KeyAction::OpenMenu)
                            {
                                event.stop_propagation();
                                key_open(menu_id, true);
                            }
                        })
                })
                .child(icons::icon(
                    Icon::PanDown,
                    IconSize::Control,
                    Paint::Text.color(&tokens),
                ))
                .focusable(
                    // The open menu owns the keyboard; the ring would run under it.
                    if open { Focus::Not } else { menu_focus() },
                    Radius::Inner,
                );
            // No tooltip: the shield over an open menu hides the pointer's
            // leave from it, so it would stay on screen beside the menu.
            Element::from(button)
        });

        rect()
            .component("SplitButton")
            .on_sized(move |event: Event<SizedEventData>| {
                area.set_if_modified(event.area);
            })
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    p(S1),
                    h(Dim::Button),
                    rounded(Radius::Control),
                    surface(Paint::Transparent, lit, false, false),
                ],
            )
            .maybe(!usable, |element| element.opacity(DISABLED_OPACITY))
            .maybe(usable, |element| {
                element
                    // The bubbling pair: the box stays hovered while the
                    // pointer is over either segment, not only its padding.
                    .on_pointer_over(move |_| control_hover.set(true))
                    .on_pointer_out(move |_| control_hover.set(false))
            })
            .child(primary)
            .maybe_child(has_menu.then(|| {
                // The hairline sits in the gap between the segments, so the
                // layout is the same filled and unfilled.
                rect()
                    .tw(
                        &tokens,
                        &[
                            w(Dim::Hairline),
                            h(Dim::Box5),
                            mx(S0_5),
                            bg(Paint::Separator),
                        ],
                    )
                    .opacity(divider)
            }))
            .maybe_child(menu)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(key: NamedKey) -> Key {
        Key::Named(key)
    }

    #[test]
    fn enter_and_space_press_the_primary_segment() {
        for key in [named(NamedKey::Enter), Key::Character(" ".into())] {
            assert_eq!(
                route_key(Segment::Primary, &key, true),
                Some(KeyAction::Press)
            );
            assert_eq!(
                route_key(Segment::Primary, &key, false),
                Some(KeyAction::Press)
            );
        }
    }

    #[test]
    fn enter_and_space_open_the_menu_from_the_chevron() {
        for key in [named(NamedKey::Enter), Key::Character(" ".into())] {
            assert_eq!(
                route_key(Segment::Menu, &key, true),
                Some(KeyAction::OpenMenu)
            );
        }
    }

    #[test]
    fn down_opens_the_menu_from_either_segment() {
        let down = named(NamedKey::ArrowDown);
        assert_eq!(
            route_key(Segment::Primary, &down, true),
            Some(KeyAction::OpenMenu)
        );
        assert_eq!(
            route_key(Segment::Menu, &down, true),
            Some(KeyAction::OpenMenu)
        );
    }

    #[test]
    fn without_a_menu_down_does_nothing() {
        let down = named(NamedKey::ArrowDown);
        assert_eq!(route_key(Segment::Primary, &down, false), None);
        assert_eq!(route_key(Segment::Menu, &down, false), None);
    }

    #[test]
    fn other_keys_keep_travelling() {
        for key in [
            named(NamedKey::Escape),
            named(NamedKey::Tab),
            named(NamedKey::ArrowUp),
            Key::Character("a".into()),
        ] {
            assert_eq!(route_key(Segment::Primary, &key, true), None);
            assert_eq!(route_key(Segment::Menu, &key, true), None);
        }
    }
}
