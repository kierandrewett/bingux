//! A dropdown (libadwaita `GtkDropDown`, SwiftUI `Picker(.menu)`): a 32 px
//! pill on `elevated` showing the current choice; pressing it floats a
//! card of 32 px options with the selected one ticked.

use freya::animation::*;
use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::scale::*;
use crate::surface::{field_border, is_activation_key, surface_color, use_tokens};

/// Width of the floating option list.
const MENU_WIDTH: f32 = 240.0;

/// `options` are `(key, label)` pairs; `on_select` receives the chosen key.
#[derive(PartialEq)]
pub struct Dropdown {
    options: Vec<(String, String)>,
    selected: String,
    enabled: bool,
    width: Option<f32>,
    on_select: EventHandler<String>,
}

pub fn dropdown(
    options: Vec<(String, String)>,
    selected: impl Into<String>,
    on_select: impl Into<EventHandler<String>>,
) -> Dropdown {
    Dropdown {
        options,
        selected: selected.into(),
        enabled: true,
        width: None,
        on_select: on_select.into(),
    }
}

impl Dropdown {
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Fix the button width; by default it fits the current label.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }
}

impl Component for Dropdown {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let mut open = use_state(|| false);
        let mut highlighted = use_state(|| None::<usize>);
        let mut button_width = use_state(|| 0.0_f32);
        let mut hovered_item = use_state(|| None::<usize>);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let enabled = self.enabled;
        let on_select = self.on_select.clone();
        let options = self.options.clone();
        let selected_index = options.iter().position(|(key, _)| *key == self.selected);
        let current = selected_index
            .map(|index| options[index].1.clone())
            .unwrap_or_else(|| self.selected.clone());

        let motion = tokens.motion(90);
        let reveal = use_animation_transition(if open() { 1.0 } else { 0.0 }, move |from, to| {
            AnimNum::new(from, to)
                .time(motion)
                .function(Function::Quad)
                .ease(Ease::Out)
        });
        let progress = reveal.get().value();
        let visible = open() || progress > 0.0;

        let choose = {
            let on_select = on_select.clone();
            let options = options.clone();
            EventHandler::new(move |index: usize| {
                if let Some((key, _)) = options.get(index) {
                    on_select.call(key.clone());
                }
                open.set(false);
                highlighted.set(None);
            })
        };
        let count = options.len();
        let on_key_down = {
            let choose = choose.clone();
            move |event: Event<KeyboardEventData>| {
                match &event.key {
                    key if is_activation_key(key) => {
                        if open() {
                            if let Some(index) = highlighted() {
                                choose.call(index);
                            } else {
                                open.set(false);
                            }
                        } else {
                            highlighted.set(selected_index);
                            open.set(true);
                        }
                    }
                    Key::Named(NamedKey::Escape) => {
                        open.set(false);
                        highlighted.set(None);
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        let next = highlighted()
                            .or(selected_index)
                            .map_or(0, |index| (index + 1).min(count.saturating_sub(1)));
                        highlighted.set(Some(next));
                        open.set(true);
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        let next = highlighted()
                            .or(selected_index)
                            .map_or(0, |index| index.saturating_sub(1));
                        highlighted.set(Some(next));
                        open.set(true);
                    }
                    _ => return,
                }
                event.stop_propagation();
            }
        };

        let button = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::ComboBox)
            .a11y_builder(move |builder| builder.set_expanded(open()))
            .height(Size::px(CONTROL_HEIGHT))
            .maybe(self.width.is_some(), |element| {
                element.width(Size::px(self.width.unwrap_or_default()))
            })
            .padding((0.0, S2, 0.0, S3))
            .spacing(S2)
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .main_align(Alignment::SpaceBetween)
            .corner_radius(RADIUS)
            .background(if enabled && pressing() {
                tokens.elevated_pressed
            } else if enabled && hovering() {
                tokens.elevated_hover
            } else {
                tokens.elevated
            })
            .border(field_border(&tokens, focus()))
            .on_sized(move |event: Event<SizedEventData>| {
                button_width.set_if_modified(event.area.width())
            })
            .maybe(!enabled, |element| element.opacity(0.45))
            .cursor(if enabled {
                CursorIcon::Pointer
            } else {
                CursorIcon::NotAllowed
            })
            .maybe(enabled, |element| {
                element
                    .on_pointer_enter(move |_| hovering.set(true))
                    .on_pointer_leave(move |_| {
                        hovering.set(false);
                        pressing.set_if_modified(false);
                    })
                    .on_pointer_down(move |event: Event<PointerEventData>| {
                        event.stop_propagation();
                        pressing.set(true);
                    })
                    .on_global_pointer_up(move |_| {
                        pressing.set_if_modified(false);
                        if open() {
                            open.set(false);
                            highlighted.set(None);
                        }
                    })
                    .on_press(move |event: Event<PressEventData>| {
                        event.stop_propagation();
                        event.prevent_default();
                        a11y_id.request_focus();
                        let next = !open();
                        open.set(next);
                        highlighted.set(if next { selected_index } else { None });
                    })
                    .on_key_down(on_key_down)
            })
            .child(
                label()
                    .text(current)
                    .font_size(TEXT_MD)
                    .color(tokens.text)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .child(icons::icon(Icon::PanDown, ICON_MD, tokens.muted));

        // The list is at least MENU_WIDTH and never narrower than the button.
        let menu_width = button_width().max(MENU_WIDTH);
        // `offset_x`/`offset_y` move an element's children, so the card sits
        // inside a zero-sized anchor that carries the offsets.
        let menu = visible.then(|| {
            rect()
                .width(Size::px(0.0))
                .height(Size::px(0.0))
                .offset_x((button_width() - menu_width).min(0.0))
                .offset_y(4.0 - 3.0 * (1.0 - progress))
                .child(
                    rect()
                        .layer(Layer::Overlay)
                        .opacity(progress)
                        .width(Size::px(menu_width))
                        .padding(S1)
                        .corner_radius(RADIUS_LG)
                        .background(tokens.popover)
                        .border(
                            Border::new()
                                .width(1.0)
                                .alignment(BorderAlignment::Inner)
                                .fill(tokens.tooltip_outline),
                        )
                        .shadow(
                            Shadow::new()
                                .position(ShadowPosition::Normal)
                                .y(8.0)
                                .blur(24.0)
                                .color(Color::BLACK.with_af32(0.35)),
                        )
                        .children(options.iter().enumerate().map(|(index, (_, text))| {
                            let choose = choose.clone();
                            Element::from(DropdownItem {
                                text: text.clone(),
                                selected: Some(index) == selected_index,
                                highlighted: hovered_item().is_none()
                                    && Some(index) == highlighted(),
                                on_press: EventHandler::new(move |_: ()| choose.call(index)),
                                on_hover: EventHandler::new(move |inside: bool| {
                                    hovered_item.set(if inside { Some(index) } else { None })
                                }),
                            })
                        })),
                )
        });

        rect().child(button).maybe_child(menu)
    }
}

#[derive(PartialEq)]
struct DropdownItem {
    text: String,
    selected: bool,
    highlighted: bool,
    on_press: EventHandler<()>,
    on_hover: EventHandler<bool>,
}

impl Component for DropdownItem {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let on_press = self.on_press.clone();
        let on_hover = self.on_hover.clone();
        let on_leave = on_hover.clone();
        rect()
            .a11y_role(AccessibilityRole::MenuItem)
            .width(Size::fill())
            .height(Size::px(CONTROL_HEIGHT))
            .padding((0.0, S2))
            .spacing(S2)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .corner_radius(RADIUS)
            .background(surface_color(
                &tokens,
                Color::TRANSPARENT,
                hovering() || self.highlighted,
                pressing(),
                false,
            ))
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| {
                hovering.set(true);
                on_hover.call(true);
            })
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
                on_leave.call(false);
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                event.stop_propagation();
                pressing.set(true);
            })
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                pressing.set(false);
                on_press.call(());
            })
            .child(
                label()
                    .text(self.text.clone())
                    .font_size(TEXT_MD)
                    .color(tokens.text)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .width(Size::flex(1.0)),
            )
            .child(icons::icon(
                Icon::Check,
                ICON_SM,
                if self.selected {
                    tokens.accent
                } else {
                    Color::TRANSPARENT
                },
            ))
    }
}
