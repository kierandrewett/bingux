//! A dropdown: a 36 px `elevated` button with no idle outline, showing the
//! current choice. Pressing it floats a popover of 36 px rows at radius 20
//! with a 1 px outline; the selected row carries the accent selection layer
//! and a tick. A long list scrolls inside a capped menu with a scrollbar and
//! opens the current choice in view; the menu opens upward when there is no
//! room below the button.

use freya::animation::*;
use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::scale::*;
use crate::surface::{focus_border, is_activation_key, surface_color, use_tokens};

/// Width of the floating option list.
const MENU_WIDTH: f32 = 240.0;
/// The tallest the option list gets before it scrolls, rows only.
const MENU_MAX_ROWS_HEIGHT: f32 = 320.0;
/// Space between the button and the menu, and how far the menu slides in.
const MENU_GAP: f32 = 4.0;
const MENU_SLIDE: f32 = 3.0;

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
        let mut button_area = use_state(Area::default);
        let mut scroll = use_scroll_controller(ScrollConfig::default);
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
        // The rows' height, and the part of it the menu shows.
        let list_height = count as f32 * CONTROL_HEIGHT;
        let viewport_height = list_height.min(MENU_MAX_ROWS_HEIGHT);
        let menu_height = viewport_height + 2.0 * S2;
        // Scroll offset that puts row `index` in the middle of the menu, or
        // keeps it in view when `centre` is false.
        let offset_for = move |index: usize, current: i32, centre: bool| -> i32 {
            let top = index as f32 * CONTROL_HEIGHT;
            let furthest = (list_height - viewport_height).max(0.0);
            let want = if centre {
                top - (viewport_height - CONTROL_HEIGHT) / 2.0
            } else {
                let shown_top = -(current as f32);
                if top < shown_top {
                    top
                } else if top + CONTROL_HEIGHT > shown_top + viewport_height {
                    top + CONTROL_HEIGHT - viewport_height
                } else {
                    shown_top
                }
            };
            -(want.clamp(0.0, furthest) as i32)
        };
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
                            if let Some(index) = selected_index {
                                scroll.scroll_to_y(offset_for(index, 0, true));
                            }
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
                        scroll.scroll_to_y(offset_for(next, <(i32, i32)>::from(scroll).1, !open()));
                        open.set(true);
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        let next = highlighted()
                            .or(selected_index)
                            .map_or(0, |index| index.saturating_sub(1));
                        highlighted.set(Some(next));
                        scroll.scroll_to_y(offset_for(next, <(i32, i32)>::from(scroll).1, !open()));
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
            .border(focus_border(&tokens, focus()))
            .on_sized(move |event: Event<SizedEventData>| {
                button_width.set_if_modified(event.area.width());
                button_area.set_if_modified(event.area);
            })
            .maybe(!enabled, |element| element.opacity(0.45))
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
                        if next && let Some(index) = selected_index {
                            scroll.scroll_to_y(offset_for(index, 0, true));
                        }
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
                    .maybe(self.width.is_some(), |element| {
                        // Button width less padding, spacing and the chevron.
                        let label_width = self.width.unwrap_or_default() - S3 - S2 - S2 - ICON_MD;
                        element.width(Size::px(label_width.max(0.0)))
                    })
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .child(
                rect()
                    .width(Size::px(ICON_MD))
                    .height(Size::px(ICON_MD))
                    .child(icons::icon(Icon::PanDown, ICON_MD, tokens.muted)),
            );

        // The list is at least MENU_WIDTH and never narrower than the button.
        let menu_width = button_width().max(MENU_WIDTH);
        // Open upward when the window has no room below the button and more
        // room above it.
        let window_height = Platform::get().current_window().root_size.read().height;
        let below = window_height - button_area.read().max_y() - MENU_GAP;
        let above = button_area.read().min_y() - MENU_GAP;
        let open_up = menu_height > below && above > below;
        let slide = MENU_SLIDE * (1.0 - progress);
        let menu_offset = if open_up {
            -(CONTROL_HEIGHT + menu_height + MENU_GAP) + slide
        } else {
            MENU_GAP - slide
        };
        // `offset_x`/`offset_y` move an element's children, so the card sits
        // inside a zero-sized anchor that carries the offsets.
        let menu = visible.then(|| {
            rect()
                .width(Size::px(0.0))
                .height(Size::px(0.0))
                .offset_x((button_width() - menu_width).min(0.0))
                .offset_y(menu_offset)
                .child(
                    rect()
                        .layer(Layer::Overlay)
                        .opacity(progress)
                        .width(Size::px(menu_width))
                        .padding(S2)
                        .corner_radius(RADIUS_XL)
                        .background(tokens.popover)
                        .border(
                            Border::new()
                                .width(1.0)
                                .alignment(BorderAlignment::Inner)
                                .fill(tokens.outline),
                        )
                        .shadow(
                            Shadow::new()
                                .position(ShadowPosition::Normal)
                                .y(S2)
                                .blur(S5)
                                .color(Color::BLACK.with_af32(0.35)),
                        )
                        .child(
                            rect()
                                .width(Size::fill())
                                .height(Size::px(viewport_height))
                                .child(
                                    ScrollView::new_controlled(scroll)
                                        .drag_scrolling(false)
                                        .auto_hide_scrollbars(false)
                                        .children(options.iter().enumerate().map(
                                            |(index, (_, text))| {
                                                let choose = choose.clone();
                                                Element::from(DropdownItem {
                                                    text: text.clone(),
                                                    selected: Some(index) == selected_index,
                                                    highlighted: hovered_item().is_none()
                                                        && Some(index) == highlighted(),
                                                    on_press: EventHandler::new(move |_: ()| {
                                                        choose.call(index)
                                                    }),
                                                    on_hover: EventHandler::new(
                                                        move |inside: bool| {
                                                            hovered_item.set(if inside {
                                                                Some(index)
                                                            } else {
                                                                None
                                                            })
                                                        },
                                                    ),
                                                })
                                            },
                                        )),
                                ),
                        ),
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
                self.selected,
            ))
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
