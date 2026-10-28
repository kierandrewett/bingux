//! A dropdown: a `control` button with no idle outline, showing the current
//! choice. Pressing it floats a `menu` of `menu-item` rows; the selected row
//! carries the accent selection layer and a tick. A long list scrolls inside
//! a capped menu with a scrollbar and opens the current choice in view; the
//! menu opens upward when there is no room below the button.

use freya::prelude::*;

use super::press::use_press;
use crate::app_icon::app_icon;
use crate::component::ComponentName;
use crate::icons::{self, Icon};
use crate::motion::Focusable;
use crate::rows::RowTitle;
use crate::shell::panel::{Panel, PanelKind, PanelRect, PanelTrigger};
use crate::shell::menu_query::{Candidate, Mark, Reaction};
use crate::shell::type_ahead::{dimmed, marked_label, use_list_query};
use crate::style::*;
use crate::surface::{is_activation_key, use_enabled, use_tokens};
use crate::theme::Tokens;
use crate::typography::{BODY, Ink};

/// A generous average glyph width at body size. Freya cannot size a button
/// to an ellipsised label, so the button is sized from its longest option
/// and does not change width when the choice changes.
const GLYPH_WIDTH: f32 = 8.2;

/// Left padding, gap and chevron with its right padding. Used for the
/// measured width only.
fn button_chrome() -> f32 {
    S3.px() + S2.px() + IconSize::Row.px() + S2.px()
}

/// The button: a filled control, the icon and label from the start inset, the
/// chevron against the end inset.
const VALUE: Recipe = &[pl(S3), pr(S2)];

/// What an option shows before its label.
#[derive(Clone, PartialEq)]
pub enum OptionIcon {
    /// A desktop application's icon, by its name.
    App(String),
    /// A bundled symbolic icon, such as a headphone or a microphone.
    Symbol(Icon),
    /// A colour, drawn as a chip over a checkerboard so translucent colours
    /// show their alpha. For a choice between colours or colour tokens.
    Colour(Color),
}

/// `options` are `(key, label)` pairs; `on_select` receives the chosen key.
#[derive(PartialEq)]
pub struct Dropdown {
    options: Vec<(String, String)>,
    selected: String,
    /// The icon before each option's label, one per option, in order.
    icons: Vec<OptionIcon>,
    enabled: bool,
    /// The button is `Dim::DenseControl` tall rather than `Dim::Control`.
    dense: bool,
    min_width: Option<Dim>,
    fixed_width: Option<Dim>,
    /// The sheet's header on a phone: the control's label.
    title: Option<String>,
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
        dense: false,
        enabled: true,
        min_width: None,
        title: None,
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

    /// Draw a colour chip before each option, in the same order as the options.
    pub fn colours(mut self, colours: Vec<Color>) -> Self {
        self.icons = colours.into_iter().map(OptionIcon::Colour).collect();
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

    /// A compact button, `Dim::DenseControl` tall, for property editors and
    /// inspectors. The menu is unchanged.
    pub fn dense(mut self) -> Self {
        self.dense = true;
        self
    }

    /// The sheet's header on a phone: the control's label.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

impl Component for Dropdown {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let fitted = crate::rows::use_row_fitted() && !self.dense;
        let button_dim = if self.dense {
            Dim::DenseControl
        } else if fitted {
            Dim::RowButton
        } else {
            Dim::Control
        };
        let button_radius = if fitted { Radius::Inner } else { Radius::Control };
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let mut open = use_state(|| false);
        let mut highlighted = use_state(|| None::<usize>);
        let mut button_width = use_state(|| 0.0_f32);
        // A form line that is too narrow for the button asks it to fill the
        // room it has.
        let fills = crate::controls::adaptive::use_line_fill();
        let mut button_area = use_state(Area::default);
        let mut scroll = use_scroll_controller(ScrollConfig::default);
        let mut hovered_item = use_state(|| None::<usize>);
        let ahead = use_list_query();
        let a11y_id = use_a11y();
        let press = use_press();
        let focus = use_focus(a11y_id);
        let enabled = use_enabled(self.enabled);
        let on_select = self.on_select.clone();
        let options = self.options.clone();
        let option_icons = self.icons.clone();
        let selected_index = options.iter().position(|(key, _)| *key == self.selected);
        let current = selected_index
            .map(|index| options[index].1.clone())
            .unwrap_or_else(|| self.selected.clone());

        // Picking a row ends its query, and closes the list.
        let choose = {
            let on_select = on_select.clone();
            let options = options.clone();
            EventHandler::new(move |index: usize| {
                if let Some((key, _)) = options.get(index) {
                    on_select.call(key.clone());
                }
                open.set(false);
                highlighted.set(None);
                ahead.clear();
            })
        };
        let count = options.len();
        let longest = options
            .iter()
            .map(|(_, text)| text.chars().count())
            .max()
            .unwrap_or(0) as f32;
        // An icon and its gap come before the label. A symbol is the height of
        // the text; an application icon is one step larger so it stays legible.
        let icon_space = if self.icons.is_empty() {
            0.0
        } else if self.icons.iter().all(|icon| matches!(icon, OptionIcon::Symbol(_))) {
            IconSize::Row.px() + S2.px()
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
        let width = if is_compact() {
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
            let on_select = on_select.clone();
            let type_options = options.clone();
            move |event: Event<KeyboardEventData>| {
                // A query typed on the shut button has no pill, so Escape
                // takes it back here; the open list's Escape is the panel's.
                if !open()
                    && event.key == Key::Named(NamedKey::Escape)
                    && ahead.escape()
                {
                    event.stop_propagation();
                    return;
                }
                // Type-ahead first: typed letters, and the keys that walk and
                // pick the matches while a query is active.
                let items: Vec<Candidate> = type_options
                    .iter()
                    .map(|(_, text)| Candidate {
                        label: text,
                        selectable: true,
                    })
                    .collect();
                match ahead.key(&event, &items, highlighted().or(selected_index), open()) {
                    Reaction::Ignored => {}
                    Reaction::Handled => {
                        event.stop_propagation();
                        return;
                    }
                    // Open, a match takes the highlight; shut, it is chosen as
                    // it is typed and the query stays for the next letter.
                    Reaction::Focus(index) => {
                        event.stop_propagation();
                        if open() {
                            highlighted.set(Some(index));
                            scroll.scroll_to_y(offset_for(
                                index,
                                <(i32, i32)>::from(scroll).1,
                                false,
                            ));
                        } else if let Some((key, _)) = type_options.get(index) {
                            on_select.call(key.clone());
                        }
                        return;
                    }
                    Reaction::Activate(index) => {
                        event.stop_propagation();
                        choose.call(index);
                        return;
                    }
                }
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
                        if !open() {
                            ahead.clear();
                        }
                        open.set(true);
                    }
                    // Tab leaves the control: the open list closes with it, so
                    // focus never moves on behind a list that stays up.
                    Key::Named(NamedKey::Tab) => {
                        if open() {
                            open.set(false);
                            highlighted.set(None);
                        }
                        return;
                    }
                    _ => return,
                }
                event.stop_propagation();
            }
        };

        // The button is one row at both densities: the icon, the label and the
        // chevron each take their own room, so the label's ellipsis ends
        // before the chevron. On a phone the box is the taller touch size.
        let button = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::ComboBox)
            .a11y_builder(move |builder| builder.set_expanded(open()))
            .width(if fills { Size::fill() } else { Size::px(width) })
            .tw(
                &tokens,
                &[
                    h(button_dim),
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    gap(S2),
                    rounded(button_radius),
                    surface(
                        Paint::Control,
                        // The button stays lit while its list is open, so it
                        // reads as the owner of the list.
                        enabled && (hovering() || open()),
                        enabled && pressing(),
                        false,
                    ),
                ],
            )
            .tw(&tokens, VALUE)
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
                        press.begin(&event, None);
                    })
                    .on_global_pointer_up(move |_| pressing.set_if_modified(false))
                    .on_global_pointer_move(move |event: Event<PointerEventData>| {
                        press.moved(&event);
                    })
                    .on_press(move |event: Event<PressEventData>| {
                        event.stop_propagation();
                        event.prevent_default();
                        if !press.tap(&event) {
                            return;
                        }
                        a11y_id.request_focus();
                        let next = !open();
                        if next && let Some(index) = selected_index {
                            scroll.scroll_to_y(offset_for(index, 0, true));
                        }
                        if next {
                            // A fresh list starts with a fresh query: what was
                            // typed on the shut button does not carry over.
                            ahead.clear();
                        }
                        open.set(next);
                        highlighted.set(if next { selected_index } else { None });
                    })
                    .on_key_down(on_key_down)
            })
            .maybe_child(current_icon.map(|icon| option_icon(icon, Paint::Text.color(&tokens), tokens)))
            .child(
                rect().tw(&tokens, &[w_flex()]).min_width(Size::px(0.0)).child(
                    BODY.in_ink(Ink::Text)
                        .label(&tokens, current)
                        .width(Size::fill())
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                ),
            )
            .child(
                rect()
                    .tw(&tokens, &[flex_row(), center()])
                    .child(icons::icon(
                        Icon::PanDown,
                        IconSize::Row,
                        Paint::Muted.color(&tokens),
                    )),
            )
            .focusable(
                // The open list owns the keyboard, and the ring would run under it.
                if open() { Focus::Not } else { focus() },
                button_radius,
            );

        let menu_width = button_width().max(Dim::DropdownMenu.px());
        let rows = options
            .iter()
            .enumerate()
            .map(|(index, (_, text))| {
                let choose = choose.clone();
                Element::from(DropdownItem {
                    icon: option_icons.get(index).cloned(),
                    text: text.clone(),
                    selected: Some(index) == selected_index,
                    mark: ahead.mark(index),
                    highlighted: hovered_item().is_none() && Some(index) == highlighted(),
                    on_press: EventHandler::new(move |_: ()| choose.call(index)),
                    on_hover: EventHandler::new(move |inside: bool| {
                        hovered_item.set(if inside { Some(index) } else { None })
                    }),
                })
            })
            .collect::<Vec<_>>();
        let dismiss = EventHandler::new(move |_: ()| {
            open.set(false);
            highlighted.set(None);
            ahead.clear();
        });
        let row_title = try_consume_context::<RowTitle>();
        let mut panel = Panel::new(
            PanelKind::Choice,
            open(),
            PanelTrigger::Control(PanelRect::from_area(&button_area.read())),
            rows,
        )
        .size(menu_width, list_height)
        .scroll(scroll)
        .pill(ahead.pill())
        .on_escape({
            // Escape takes back a typed query first, then closes the list.
            let dismiss = dismiss.clone();
            EventHandler::new(move |_: ()| {
                if !ahead.escape() {
                    dismiss.call(());
                }
            })
        })
        .on_dismiss(dismiss);
        // An explicit title wins; otherwise the sheet header is the row's title.
        if let Some(title) = self
            .title
            .clone()
            .or_else(|| row_title.map(|row| row.0.read().clone()))
        {
            panel = panel.title(title);
        }

        rect()
            .component("Dropdown")
            .maybe(fills, |wrapper| wrapper.tw(&tokens, &[w_full()]))
            .child(button)
            .child(Element::from(panel))
    }
}

#[derive(PartialEq)]
struct DropdownItem {
    icon: Option<OptionIcon>,
    text: String,
    selected: bool,
    highlighted: bool,
    /// How the option shows while a typed query is active.
    mark: Mark,
    on_press: EventHandler<()>,
    on_hover: EventHandler<bool>,
}

impl Component for DropdownItem {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let press = use_press();
        let on_press = self.on_press.clone();
        let on_hover = self.on_hover.clone();
        let on_leave = on_hover.clone();
        rect()
            .component("DropdownItem")
            .a11y_role(AccessibilityRole::MenuItem)
            .tw(&tokens, MENU_ITEM)
            .maybe(dimmed(&self.mark).is_some(), |row| {
                row.opacity(dimmed(&self.mark).unwrap_or(1.0))
            })
            // The selected option keeps the fill it has when highlighted, so
            // it does not dim when the pointer moves to another option.
            .tw(
                &tokens,
                &[surface(
                    Paint::Transparent,
                    hovering() || self.highlighted || self.selected,
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
                // A touch passes up to the sheet's list, so the list can
                // scroll under the finger and a pull can start: Freya also
                // drops a stopped touch's global release.
                if !super::press::is_touch(&event) {
                    event.stop_propagation();
                }
                pressing.set(true);
                press.begin(&event, None);
            })
            .on_global_pointer_move(move |event: Event<PointerEventData>| {
                press.moved(&event);
                // A finger that strays into a drag is no longer pressing.
                if super::press::is_touch(&event) && *pressing.peek() && !press.pending() {
                    pressing.set(false);
                }
            })
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                if !press.tap(&event) {
                    return;
                }
                pressing.set(false);
                on_press.call(());
            })
            .maybe_child(
                self.icon
                    .clone()
                    .map(|icon| option_icon(icon, Paint::Text.color(&tokens), tokens)),
            )
            .child(marked_label(&tokens, Ink::Text, &self.text, &self.mark))
            .child(icons::icon(
                Icon::Check,
                IconSize::Control,
                if self.selected {
                    Paint::Accent.color(&tokens)
                } else {
                    Paint::Transparent.color(&tokens)
                },
            ))
    }
}

/// The icon before an option's label: an application icon or a symbol.
fn option_icon(icon: OptionIcon, colour: Color, tokens: Tokens) -> Element {
    match icon {
        OptionIcon::App(name) => app_icon(name, IconSize::AppSmall).into(),
        OptionIcon::Symbol(symbol) => icons::icon(symbol, IconSize::Row, colour).into(),
        OptionIcon::Colour(chip) => {
            super::colour_picker::colour_chip(tokens, chip, IconSize::AppSmall.px()).into_element()
        }
    }
}

/// The button is one row: the icon, the label and the chevron lie inside its
/// box, in that order, each clear of the next, at both densities.
#[cfg(all(test, feature = "testing"))]
mod geometry_tests {
    use std::time::Duration;

    use freya::prelude::*;
    use freya_core::elements::image::Image;
    use torin::prelude::Area;

    use super::*;
    use crate::snapshot::Snapshot;
    use crate::theme::provide_tokens;
    use crate::window::WindowHost;

    /// The window root: it provides the tokens the window host reads.
    #[derive(PartialEq)]
    struct Root(Host);

    impl Component for Root {
        fn render(&self) -> impl IntoElement {
            let _ = provide_tokens();
            WindowHost::new(Host {
                icon: self.0.icon,
                long: self.0.long,
            })
        }
    }

    const LONG: &str = "A name far too long for the room it is given, which must be cut";

    #[derive(PartialEq)]
    struct Host {
        icon: bool,
        long: bool,
    }

    impl Component for Host {
        fn render(&self) -> impl IntoElement {
            let tokens = use_tokens();
            let text = if self.long { LONG } else { "Bluetooth" };
            let options = vec![
                ("a".to_string(), text.to_string()),
                ("b".to_string(), "Other".to_string()),
            ];
            let mut dropdown = dropdown(options, "a", |_: String| {}).width(Dim::DropdownMenu);
            if self.icon {
                dropdown = dropdown.icons(vec![Icon::Bluetooth, Icon::Bluetooth]);
            }
            rect()
                .expanded()
                .tw(&tokens, &[p(S4)])
                .child(dropdown)
        }
    }

    /// The button's area, the label's, and the areas of the icons (the icon
    /// before the label, when there is one, then the chevron), by x.
    fn boxes(icon: bool, long: bool, phone: bool) -> (Area, Area, Vec<Area>, (f32, f32, f32)) {
        let width = if phone { 390.0 } else { 800.0 };
        let mut shot = Snapshot::new(
            move || Root(Host { icon, long }).into_element(),
            (width, 300.0),
        );
        shot.settle(Duration::from_millis(300));
        let (inset_start, inset_end, gap) = shot.runner().run_in(|| (S3.px(), S2.px(), S2.px()));
        let label = shot
            .runner()
            .find_many(|node, element| {
                let label = Label::try_downcast(element)?;
                (label.text.starts_with("A name") || label.text == "Bluetooth")
                    .then(|| node.layout().area)
            })
            .into_iter()
            .next()
            .expect("the label");
        // The button is the smallest one-row box around the label that is
        // narrower than a window (the focus ring's box around it is larger):
        // on a phone it is capped at the field column, so its width is not
        // the menu's.
        let buttons = shot.runner().find_many(|node, _| {
            let area = node.layout().area;
            (area.height() > 30.0
                && area.height() < 70.0
                && area.width() < width - 40.0
                && area.min_x() <= label.min_x() + 0.5
                && area.max_x() >= label.max_x() - 0.5
                && area.min_y() <= label.min_y() + 0.5
                && area.max_y() >= label.max_y() - 0.5)
                .then_some(area)
        });
        let button = buttons
            .into_iter()
            .min_by(|a, b| (a.width() * a.height()).total_cmp(&(b.width() * b.height())))
            .expect("the button");
        let mut glyphs = shot.runner().find_many(|node, element| {
            // The focus ring's band boxes are the ring's own SVG, not the
            // control's glyphs.
            let svg = Image::try_downcast(element).is_some()
                && !element
                    .accessibility()
                    .builder
                    .class_name()
                    .is_some_and(|name| name.contains("FocusRing") || name.contains("FocusBand"));
            let area = node.layout().area;
            (svg && area.min_x() >= button.min_x() - 40.0 && area.max_x() <= button.max_x() + 40.0
                && area.min_y() >= button.min_y() && area.max_y() <= button.max_y())
            .then_some(area)
        });
        glyphs.sort_by(|a, b| a.min_x().total_cmp(&b.min_x()));
        (button, label, glyphs, (inset_start, inset_end, gap))
    }

    #[test]
    fn icon_label_and_chevron_lie_inside_the_button_without_overlap() {
        for phone in [false, true] {
            for (icon, long) in [(false, false), (true, false), (false, true), (true, true)] {
                let (button, label, glyphs, (inset_start, inset_end, gap)) = boxes(icon, long, phone);
                let case = format!("phone {phone}, icon {icon}, long {long}");
                let chevron = glyphs.last().copied().expect("a chevron");
                for glyph in &glyphs {
                    assert!(
                        glyph.min_x() >= button.min_x() - 0.5 && glyph.max_x() <= button.max_x() + 0.5,
                        "{case}: a glyph at {glyph:?} is outside the button {button:?}"
                    );
                }
                assert!(label.min_x() >= button.min_x() + inset_start - 0.5, "{case}: the label starts inside the inset");
                assert!(
                    chevron.max_x() <= button.max_x() - inset_end + 0.5,
                    "{case}: the chevron keeps the end inset"
                );
                assert!(
                    label.max_x() + gap <= chevron.min_x() + 0.5,
                    "{case}: the label (to {}) runs under the chevron (from {})",
                    label.max_x(),
                    chevron.min_x()
                );
                if icon {
                    let symbol = glyphs.first().copied().expect("the icon");
                    assert!(symbol.min_x() < label.min_x(), "{case}: the icon comes first");
                    assert!(
                        symbol.max_x() + gap <= label.min_x() + 0.5,
                        "{case}: the label starts after the icon and its gap"
                    );
                    assert!(
                        (symbol.min_x() - button.min_x() - inset_start).abs() < 0.5,
                        "{case}: the icon sits on the start inset: icon {symbol:?}, button {button:?}, inset {inset_start}",
                    );
                }
            }
        }
    }
}
