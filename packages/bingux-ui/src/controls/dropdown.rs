//! A dropdown: a `control` button with no idle outline, showing the current
//! choice. Pressing it floats a `menu` of `menu-item` rows; the selected row
//! carries the accent selection layer and a tick. A long list scrolls inside
//! a capped menu with a scrollbar and opens the current choice in view; the
//! menu opens upward when there is no room below the button.

use freya::animation::*;
use freya::prelude::*;

use crate::app_icon::app_icon;
use crate::icons::{self, Icon};
use crate::shell::overlays::input_shield;
use crate::style::*;
use crate::surface::{is_activation_key, use_enabled, use_tokens};
use crate::typography::{BODY, Ink};

/// Space between the button and the menu, and how far the menu slides in.
const MENU_GAP: f32 = 4.0;
const MENU_SLIDE: f32 = 3.0;
/// A generous average glyph width at body size. Freya cannot size a button
/// to an ellipsised label, so the button is sized from its longest option
/// and does not change width when the choice changes.
const GLYPH_WIDTH: f32 = 8.2;

/// Left padding, gap and chevron with its right padding. Used for the
/// measured width only.
fn button_chrome() -> f32 {
    S3.px() + S2.px() + IconSize::Control.px() + S2.px()
}

/// The button on a phone: the value at the row's end, no box.
const PHONE_VALUE: Recipe = &[pl(S0), pr(S0_5), justify(Align::End)];
/// The button on a desktop: a filled control, the chevron at its end.
const DESKTOP_VALUE: Recipe = &[pl(S3), pr(S2), justify(Align::Between)];

/// What an option shows before its label.
#[derive(Clone, PartialEq)]
pub enum OptionIcon {
    /// A desktop application's icon, by its name.
    App(String),
    /// A bundled symbolic icon, such as a headphone or a microphone.
    Symbol(Icon),
}

/// `options` are `(key, label)` pairs; `on_select` receives the chosen key.
#[derive(PartialEq)]
pub struct Dropdown {
    options: Vec<(String, String)>,
    selected: String,
    /// The icon before each option's label, one per option, in order.
    icons: Vec<OptionIcon>,
    enabled: bool,
    min_width: Option<Dim>,
    fixed_width: Option<Dim>,
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
        icons: Vec::new(),
        enabled: true,
        min_width: None,
        fixed_width: None,
        on_select: on_select.into(),
    }
}

impl Dropdown {
    /// Draw an application icon before each option; `icons` has one icon
    /// name per option, in the same order.
    pub fn app_icons(mut self, icons: Vec<String>) -> Self {
        self.icons = icons.into_iter().map(OptionIcon::App).collect();
        self
    }

    /// Draw a symbolic icon before each option, in the same order as the
    /// options: a headphone for a headset, a speaker for the built-in output.
    pub fn icons(mut self, icons: Vec<Icon>) -> Self {
        self.icons = icons.into_iter().map(OptionIcon::Symbol).collect();
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The button is at least this wide, so a column of dropdowns shares
    /// one left edge. It fits the longest option otherwise, up to
    /// [`Dim::DropdownMax`].
    pub fn min_width(mut self, width: Dim) -> Self {
        self.min_width = Some(width);
        self
    }

    /// The button is exactly this wide, so every dropdown in a column shares
    /// both edges. A longer option is cut with an ellipsis.
    pub fn width(mut self, width: Dim) -> Self {
        self.fixed_width = Some(width);
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
        let enabled = use_enabled(self.enabled);
        let on_select = self.on_select.clone();
        let options = self.options.clone();
        let option_icons = self.icons.clone();
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
        let longest = options
            .iter()
            .map(|(_, text)| text.chars().count())
            .max()
            .unwrap_or(0) as f32;
        // An application icon and its gap come before the label.
        let icon_space = if self.icons.is_empty() {
            0.0
        } else {
            IconSize::AppSmall.px() + S2.px()
        };
        // The measured width is only maths; the bounds come from tokens.
        let width = match self.fixed_width {
            Some(fixed) => fixed.px(),
            None => {
                let min = self.min_width.map_or(0.0, Dim::px);
                (longest * GLYPH_WIDTH + button_chrome() + icon_space)
                    .min(Dim::DropdownMax.px())
                    .max(min)
            }
        };
        // On a phone the value never takes more than the field column, so the
        // row's title keeps its room; a long value is cut with an ellipsis.
        let width = if touch() {
            width.min(Dim::Field.px())
        } else {
            width
        };
        let current_icon = selected_index.and_then(|index| self.icons.get(index).cloned());
        // The rows' height, and the part of it the menu shows.
        let item_height = Dim::MenuItem.px();
        let list_height = count as f32 * item_height;
        let viewport_height = list_height.min(Dim::MenuList.px());
        // Rows that one Page Up or Page Down press moves through.
        let page_rows = ((viewport_height / item_height) as usize).max(1);
        let menu_height = viewport_height + 2.0 * S2.px();
        // Scroll offset that puts row `index` in the middle of the menu, or
        // keeps it in view when `centre` is false.
        let offset_for = move |index: usize, current: i32, centre: bool| -> i32 {
            let top = index as f32 * item_height;
            let furthest = (list_height - viewport_height).max(0.0);
            let want = if centre {
                top - (viewport_height - item_height) / 2.0
            } else {
                let shown_top = -(current as f32);
                if top < shown_top {
                    top
                } else if top + item_height > shown_top + viewport_height {
                    top + item_height - viewport_height
                } else {
                    shown_top
                }
            };
            -(want.clamp(0.0, furthest) as i32)
        };
        let on_key_down = {
            let choose = choose.clone();
            let type_options = options.clone();
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
                    Key::Named(named)
                        if matches!(
                            named,
                            NamedKey::ArrowDown
                                | NamedKey::ArrowUp
                                | NamedKey::PageDown
                                | NamedKey::PageUp
                                | NamedKey::Home
                                | NamedKey::End
                        ) =>
                    {
                        let last = count.saturating_sub(1);
                        let from = highlighted().or(selected_index);
                        let next = match named {
                            NamedKey::ArrowDown => from.map_or(0, |index| (index + 1).min(last)),
                            NamedKey::ArrowUp => from.map_or(0, |index| index.saturating_sub(1)),
                            NamedKey::PageDown => {
                                from.map_or(0, |index| (index + page_rows).min(last))
                            }
                            NamedKey::PageUp => {
                                from.map_or(0, |index| index.saturating_sub(page_rows))
                            }
                            NamedKey::Home => 0,
                            _ => last,
                        };
                        highlighted.set(Some(next));
                        scroll.scroll_to_y(offset_for(next, <(i32, i32)>::from(scroll).1, !open()));
                        open.set(true);
                    }
                    // Type-ahead: the next option, after the current one, whose
                    // label starts with the typed letter. Closed, it chooses.
                    Key::Character(typed) if event.modifiers.is_empty() => {
                        let Some(letter) = typed.chars().next() else {
                            return;
                        };
                        let start = highlighted()
                            .or(selected_index)
                            .map_or(0, |index| index + 1);
                        let found =
                            (0..count)
                                .map(|offset| (start + offset) % count)
                                .find(|&index| {
                                    type_options[index].1.chars().next().is_some_and(|first| {
                                        first.to_lowercase().eq(letter.to_lowercase())
                                    })
                                });
                        let Some(index) = found else {
                            return;
                        };
                        if open() {
                            highlighted.set(Some(index));
                            scroll.scroll_to_y(offset_for(
                                index,
                                <(i32, i32)>::from(scroll).1,
                                false,
                            ));
                        } else {
                            choose.call(index);
                        }
                    }
                    _ => return,
                }
                event.stop_propagation();
            }
        };

        // On a phone the button has no box: the value is muted and sits at
        // the right, with the chevron.
        let touch = touch();
        let button = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::ComboBox)
            .a11y_builder(move |builder| builder.set_expanded(open()))
            .width(Size::px(width))
            .tw(
                &tokens,
                &[
                    h(Dim::Control),
                    flex_row(),
                    items(Align::Center),
                    gap(S2),
                    rounded(Radius::Control),
                    surface(
                        if touch {
                            Paint::Transparent
                        } else {
                            Paint::Control
                        },
                        enabled && hovering(),
                        enabled && pressing(),
                        false,
                    ),
                    focus_ring(focus()),
                ],
            )
            // On a phone the value sits at the row's end with no box; on a
            // desktop it is a filled control with the chevron at its end.
            .tw(&tokens, if touch { PHONE_VALUE } else { DESKTOP_VALUE })
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
                    .on_global_pointer_up(move |_| pressing.set_if_modified(false))
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
                rect()
                    .tw(&tokens, &[flex_row(), items(Align::Center), gap(S2)])
                    .maybe_child(current_icon.map(|icon| {
                        option_icon(
                            icon,
                            if touch { Paint::Muted } else { Paint::Text }.color(&tokens),
                        )
                    }))
                    .child(
                        BODY.in_ink(if touch { Ink::Muted } else { Ink::Text })
                            .label(&tokens, current)
                            .width(Size::px(width - button_chrome() - icon_space))
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis)
                            .text_align(if touch {
                                TextAlign::Right
                            } else {
                                TextAlign::Left
                            }),
                    ),
            )
            .child(
                rect()
                    .tw(&tokens, &[flex_row(), center()])
                    .child(icons::icon(
                        Icon::PanDown,
                        IconSize::Control,
                        Paint::Muted.color(&tokens),
                    )),
            );

        // The list is at least Dim::DropdownMenu and never narrower than the button.
        let menu_width = button_width().max(Dim::DropdownMenu.px());
        // Open upward when the window has no room below the button and more
        // room above it.
        let window_height = Platform::get().current_window().root_size.read().height;
        let below = window_height - button_area.read().max_y() - MENU_GAP;
        let above = button_area.read().min_y() - MENU_GAP;
        let open_up = menu_height > below && above > below;
        let slide = MENU_SLIDE * (1.0 - progress);
        let menu_offset = if open_up {
            -(Dim::Control.px() + menu_height + MENU_GAP) + slide
        } else {
            MENU_GAP - slide
        };
        // `offset_x`/`offset_y` move an element's children, so the card sits
        // inside a zero-sized anchor that carries the offsets.
        // The list sits two overlay jumps up and the shield one, so the list
        // is above the shield; boxes on one layer have no defined order.
        let shield = open().then(|| {
            input_shield(&tokens, move || {
                open.set(false);
                highlighted.set(None);
            })
            .layer(Layer::Overlay)
        });
        let menu = visible.then(|| {
            rect()
                .layer(Layer::Overlay)
                .width(Size::px(0.0))
                .height(Size::px(0.0))
                .offset_x((button_width() - menu_width).min(0.0))
                .offset_y(menu_offset)
                .child(
                    rect()
                        .layer(Layer::Overlay)
                        .opacity(progress)
                        .width(Size::px(menu_width))
                        .tw(&tokens, MENU)
                        // A wheel turn at the end of the list stops here
                        // instead of scrolling the page behind it.
                        .on_wheel(|event: Event<WheelEventData>| event.stop_propagation())
                        .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
                        .child(
                            rect()
                                .width(Size::fill())
                                .height(Size::px(viewport_height))
                                .child(
                                    ScrollView::new_controlled(scroll)
                                        .drag_scrolling(false)
                                        .children(options.iter().enumerate().map(
                                            |(index, (_, text))| {
                                                let choose = choose.clone();
                                                Element::from(DropdownItem {
                                                    icon: option_icons.get(index).cloned(),
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

        rect().child(button).maybe_child(shield).maybe_child(menu)
    }
}

#[derive(PartialEq)]
struct DropdownItem {
    icon: Option<OptionIcon>,
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
            .tw(&tokens, MENU_ITEM)
            .tw(
                &tokens,
                &[surface(
                    Paint::Transparent,
                    hovering() || self.highlighted,
                    pressing(),
                    self.selected,
                )],
            )
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
            .maybe_child(
                self.icon
                    .clone()
                    .map(|icon| option_icon(icon, Paint::Text.color(&tokens))),
            )
            .child(
                BODY.in_ink(Ink::Text)
                    .label(&tokens, self.text.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .width(Size::flex(1.0)),
            )
            .child(icons::icon(
                Icon::Check,
                IconSize::Mark,
                if self.selected {
                    Paint::Accent.color(&tokens)
                } else {
                    Paint::Transparent.color(&tokens)
                },
            ))
    }
}

/// The icon before an option's label: an application icon or a symbol.
fn option_icon(icon: OptionIcon, colour: Color) -> Element {
    match icon {
        OptionIcon::App(name) => app_icon(name, IconSize::AppSmall).into(),
        OptionIcon::Symbol(symbol) => icons::icon(symbol, IconSize::AppSmall, colour).into(),
    }
}
