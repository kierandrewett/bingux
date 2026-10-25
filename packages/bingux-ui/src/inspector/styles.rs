//! The styles pane: the selected node's properties under four tabs. The
//! Styles tab edits declared values in place and lists the overrides in force.
//! Computed, Layout and Accessibility show what the node resolved to.

use std::rc::Rc;

use freya::prelude::*;
use freya::winit::window::WindowId;

use super::box_model;
use super::editor::{self, Declared, Input};
use super::snapshot::{Node, Snapshot};
use crate::controls::colour_picker::{self, colour_picker};
use crate::controls::scrub::scrub;
use crate::controls::{CheckState, checkbox, search_field, tab_strip};
use crate::layout::vstack;
use crate::shell::panel::{Panel, PanelKind, PanelRect, PanelTrigger};
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{BODY, CAPTION, CODE, styled_label};

#[derive(Clone, Copy, PartialEq)]
enum StylesTab {
    Styles,
    Computed,
    Layout,
    Accessibility,
}

impl StylesTab {
    const ALL: [StylesTab; 4] = [
        StylesTab::Styles,
        StylesTab::Computed,
        StylesTab::Layout,
        StylesTab::Accessibility,
    ];

    fn title(self) -> &'static str {
        match self {
            StylesTab::Styles => "Styles",
            StylesTab::Computed => "Computed",
            StylesTab::Layout => "Layout",
            StylesTab::Accessibility => "Accessibility",
        }
    }
}

/// The pane for one selected node of the application window.
pub struct StylesPane {
    pub snap: Rc<Snapshot>,
    pub id: u64,
    pub window: WindowId,
}

impl PartialEq for StylesPane {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.snap, &other.snap) && self.id == other.id && self.window == other.window
    }
}

impl Component for StylesPane {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let tab = use_state(|| StylesTab::Styles);
        // The property whose colour picker is open. Held here, not in the row,
        // because committing a colour renews the row's key.
        let picker = use_state::<Option<&'static str>>(|| None);
        // The Computed tab's filter; held here so it survives tab switches.
        let filter = use_state(String::new);
        let selected = *tab.read();
        let body: Element = match self.snap.node(self.id) {
            Some(node) => match selected {
                StylesTab::Styles => styles_tab(&tokens, self.window, self.id, node, picker),
                StylesTab::Computed => computed_tab(&tokens, node, filter),
                StylesTab::Layout => box_model::view(&tokens, self.window, self.id, node, picker),
                StylesTab::Accessibility => accessibility_tab(&tokens, node),
            },
            None => rect()
                .tw(&tokens, &[w_full(), h_full(), p(S2)])
                .child(styled_label(CAPTION, "This element is gone"))
                .into(),
        };
        // Scrolls sideways rather than clipping when the pane is narrow.
        let bar = tab_strip(
            StylesTab::ALL
                .iter()
                .map(|tab| (*tab, tab.title().to_string()))
                .collect(),
            selected,
            move |next: StylesTab| {
                let mut tab = tab;
                tab.set(next);
            },
        )
        .scroll();
        rect()
            .tw(&tokens, &[w_full(), h_full(), flex_col(), p(S2), gap(S2)])
            .child(bar)
            .child(body)
    }
}

/// The height of one code line in the Styles tab.
const CODE_LINE: f32 = 20.0;
/// The advance of one character of the code face, which sizes the inline field.
const CODE_CHAR: f32 = 7.8;
/// The edge of a colour swatch in a code line.
const SWATCH_SIZE: f32 = 10.0;
/// The space between a code line and the picker popover that opens below it.
const MENU_GAP: f32 = 4.0;

/// The Styles tab as code: the element's own overrides as an `element.style`
/// rule block, then the node's declared properties as its rule block. The
/// declared block leaves out the default values and anything overridden.
fn styles_tab(
    tokens: &Tokens,
    window: WindowId,
    id: u64,
    node: &Node,
    picker: State<Option<&'static str>>,
) -> Element {
    let overrides = super::overrides_of(window, id);
    let declared = editor::declared(node);
    let overridden: Vec<&'static str> = overrides.iter().map(|(name, _)| *name).collect();

    let mut own: Vec<Element> = Vec::new();
    for (name, entry) in overrides {
        let Some(input) = declared
            .iter()
            .find(|decl| decl.name == name)
            .map(|decl| decl.input)
        else {
            continue;
        };
        // An enabled override shows its value, with the value it replaced as a
        // comment; a disabled one shows the replaced value, its own held back.
        let (live, other, note) = if entry.enabled {
            (entry.value, entry.original, "was")
        } else {
            (entry.original, entry.value, "off")
        };
        own.push(
            rect()
                .key(format!("override:{id}:{name}"))
                .child(OverrideRow {
                    window,
                    node: id,
                    enabled: entry.enabled,
                    decl: Declared {
                        name,
                        value: live,
                        input,
                    },
                    comment: format!("{note}: {other}"),
                    picker,
                })
                .into(),
        );
    }

    let mut rule: Vec<Element> = Vec::new();
    for decl in declared {
        if overridden.contains(&decl.name) || is_default(&decl.value) {
            continue;
        }
        rule.push(prop_row(tokens, window, id, decl, picker));
    }

    let mut body = rect().tw(tokens, &[w_full(), flex_col(), gap(S2)]);
    if !own.is_empty() {
        body = body.child(code_block(tokens, "element.style", own));
    }
    if !rule.is_empty() {
        body = body.child(code_block(tokens, node.title(), rule));
    }
    ScrollView::new()
        .drag_scrolling(false)
        .width(Size::fill())
        .height(Size::fill())
        .child(rect().tw(tokens, &[w_full(), flex_col()]).child(body))
        .into()
}

/// Values the Styles tab leaves out unless overridden: the initial values a
/// fresh node carries.
fn is_default(value: &str) -> bool {
    value
        .split_whitespace()
        .all(|token| matches!(token, "auto" | "0" | "0px" | "none" | "start" | "vertical"))
}

/// A rule block: `selector {`, its lines, then `}`.
fn code_block(tokens: &Tokens, selector: &str, lines: Vec<Element>) -> Element {
    let head = code_line(tokens)
        .child(code_text(tokens, selector, Paint::SyntaxTag))
        .child(code_text(tokens, " {", Paint::Text));
    let mut block = rect().tw(tokens, &[w_full(), flex_col()]).child(head);
    for line in lines {
        block = block.child(line);
    }
    block
        .child(code_line(tokens).child(code_text(tokens, "}", Paint::Text)))
        .into()
}

/// One line of code at the code line height.
fn code_line(tokens: &Tokens) -> Rect {
    rect()
        .tw(tokens, &[w_full(), flex_row(), items(Align::Center)])
        .height(Size::px(CODE_LINE))
}

/// Code text in one paint.
fn code_text(tokens: &Tokens, text: &str, paint: Paint) -> Element {
    CODE.label_in_paint(tokens, text.to_string(), paint)
        .into_element()
}

/// One declared property, keyed by node and name so a commit keeps the row's
/// state and a selection change does not reuse another node's row state.
pub(super) fn prop_row(
    tokens: &Tokens,
    window: WindowId,
    node: u64,
    decl: Declared,
    picker: State<Option<&'static str>>,
) -> Element {
    let tokens = *tokens;
    // The gutter matches an override's checkbox slot, so both columns line up.
    rect()
        .key(format!("{node}:{}", decl.name))
        .tw(&tokens, &[pl(GUTTER)])
        .child(PropRow {
            window,
            node,
            decl,
            picker,
            comment: None,
        })
        .into()
}

/// One property as a code line, `  name: value;`. A numeric name scrubs. A
/// click on the value opens an inline field in its place. A colour gets a
/// swatch that opens the picker below the line. A comment, when given, trails.
struct PropRow {
    window: WindowId,
    node: u64,
    decl: Declared,
    picker: State<Option<&'static str>>,
    comment: Option<String>,
}

impl PartialEq for PropRow {
    fn eq(&self, other: &Self) -> bool {
        self.window == other.window
            && self.node == other.node
            && self.decl == other.decl
            && self.comment == other.comment
    }
}

impl Component for PropRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let text = use_state(|| self.decl.value.clone());
        let draft = use_state(String::new);
        let editing = use_state(|| false);
        let swatch_area = use_state(Area::default);
        let picker = self.picker;
        let (window, node) = (self.window, self.node);
        let name = self.decl.name;
        let input = self.decl.input;
        let original = self.decl.value.clone();
        let current = text.read().clone();
        let numeric = matches!(input, Input::Size | Input::Pixels | Input::Gaps);
        let open = *picker.read() == Some(name);

        let mut text_step = text;
        let mut draft_step = draft;
        let step_original = original.clone();
        let nudge = move |delta: f64| {
            let before = text_step.peek().clone();
            if let Some(next) = crate::controls::scrub::step(&before, delta)
                && editor::commit(window, node, name, &next, &step_original)
            {
                text_step.set(next.clone());
                draft_step.set(next);
            }
        };

        let mut line = code_line(&tokens)
            .child(code_text(&tokens, "  ", Paint::Text))
            .child(
                scrub(name, nudge.clone())
                    .style(CODE)
                    .paint(Paint::SyntaxAttr)
                    .enabled(numeric)
                    .fit(),
            )
            .child(code_text(&tokens, ":", Paint::Text))
            .child(
                rect()
                    .width(Size::px(CODE_CHAR))
                    .height(Size::px(CODE_LINE)),
            );

        if input == Input::Colour {
            let colour =
                colour_from_hex(&current).unwrap_or_else(|| Paint::Transparent.color(&tokens));
            let mut picker_set = picker;
            let mut close = picker;
            let on_original = original.clone();
            let picked =
                colour_from_hex(&current).unwrap_or_else(|| Paint::Backdrop.color(&tokens));
            // A menu anchored to the swatch: its right edge meets the swatch's
            // and it opens just below the line. Its root has no size, so the
            // line does not reflow when it opens.
            let picker_panel = Panel::new(
                PanelKind::Menu,
                open,
                PanelTrigger::Control(PanelRect::from_area(&swatch_area.read())),
                vec![Element::from(colour_picker(picked, move |next: Color| {
                    editor::commit(window, node, name, &colour_to_hex(next), &on_original);
                }))],
            )
            .size(colour_picker::WIDTH + 2.0 * S2.px(), colour_picker::height())
            .on_dismiss(EventHandler::new(move |_: ()| close.set(None)));
            let mut area_set = swatch_area;
            line = line
                .child(
                    rect()
                        .tw(&tokens, &[fill(colour)])
                        .width(Size::px(SWATCH_SIZE))
                        .height(Size::px(SWATCH_SIZE))
                        .on_sized(move |event: Event<SizedEventData>| {
                            area_set.set_if_modified(event.area);
                        })
                        .on_pointer_down(move |_: Event<PointerEventData>| {
                            picker_set.set(if open { None } else { Some(name) });
                        }),
                )
                .child(picker_panel)
                .child(code_text(&tokens, " ", Paint::Text));
        }

        let value: Element = if input == Input::ReadOnly {
            code_text(&tokens, &current, Paint::SyntaxValue)
        } else if editing() {
            let mut text_set = text;
            let mut editing_set = editing;
            let submit_original = original.clone();
            let mut nudge_keys = nudge.clone();
            let clear = || Some(Preference::Specific(Paint::Transparent.color(&tokens)));
            let colors = InputColorsThemePartial {
                background: clear(),
                focus_background: clear(),
                border_fill: clear(),
                focus_border_fill: clear(),
                color: Some(Preference::Specific(Paint::SyntaxValue.color(&tokens))),
                placeholder_color: Some(Preference::Specific(Paint::Muted.color(&tokens))),
            };
            let layout = InputLayoutThemePartial {
                corner_radius: Some(Preference::Specific(CornerRadius::new_all(0.0))),
                padding: Some(Preference::Specific(Gaps::new_symmetric(0.0, 0.0))),
            };
            let width =
                (draft.read().chars().count() as f32 * CODE_CHAR + 6.0).max(CODE_CHAR * 3.0);
            // Freya's own `Input`: the editor's `Input` is the property kind.
            let field = freya::prelude::Input::new(draft)
                .width(Size::px(width))
                .theme_colors(colors)
                .theme_layout(layout)
                .auto_focus(true)
                .on_submit(move |next: String| {
                    if editor::commit(window, node, name, &next, &submit_original) {
                        text_set.set(next);
                        editing_set.set(false);
                    }
                })
                .on_pre_key_down(move |event: Event<KeyboardEventData>| match &event.key {
                    Key::Named(NamedKey::Escape) => {
                        editing_set.set(false);
                        event.stop_propagation();
                        false
                    }
                    Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowDown) => {
                        let sign = if matches!(event.key, Key::Named(NamedKey::ArrowUp)) {
                            1.0
                        } else {
                            -1.0
                        };
                        let scale = if event.modifiers.contains(Modifiers::SHIFT) {
                            10.0
                        } else if event.modifiers.contains(Modifiers::ALT) {
                            0.1
                        } else {
                            1.0
                        };
                        nudge_keys(sign * scale);
                        event.stop_propagation();
                        false
                    }
                    // Enter bubbles up as the field's submit; the rest is typing.
                    _ => true,
                });
            rect()
                .tw(
                    &tokens,
                    &[
                        flex_row(),
                        items(Align::Center),
                        border(Line::Edge(Paint::Accent)),
                    ],
                )
                .child(field)
                .into()
        } else {
            let mut draft_open = draft;
            let mut editing_open = editing;
            let start = current.clone();
            rect()
                .on_pointer_down(move |_: Event<PointerEventData>| {
                    draft_open.set(start.clone());
                    editing_open.set(true);
                })
                .child(code_text(&tokens, &current, Paint::SyntaxValue))
                .into()
        };
        line = line
            .child(value)
            .child(code_text(&tokens, ";", Paint::Text));
        if let Some(comment) = &self.comment {
            line = line.child(
                CODE.label_in_paint(&tokens, format!("  /* {comment} */"), Paint::SyntaxComment)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .into_element(),
            );
        }

        rect()
            .tw(&tokens, &[w_full(), flex_col()])
            .child(line)
    }
}

/// An override in force, as a code line with its enable checkbox at the left.
/// The checkbox shows on hover, and always while the override is off.
struct OverrideRow {
    window: WindowId,
    node: u64,
    enabled: bool,
    decl: Declared,
    comment: String,
    picker: State<Option<&'static str>>,
}

impl PartialEq for OverrideRow {
    fn eq(&self, other: &Self) -> bool {
        self.window == other.window
            && self.node == other.node
            && self.enabled == other.enabled
            && self.decl == other.decl
            && self.comment == other.comment
    }
}

impl Component for OverrideRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let hovering = use_state(|| false);
        let (window, node, name) = (self.window, self.node, self.decl.name);
        let mut hover_on = hovering;
        let mut hover_off = hovering;
        let shown = hovering() || !self.enabled;
        let state = if self.enabled {
            CheckState::On
        } else {
            CheckState::Off
        };
        let mut slot = rect()
            .tw(&tokens, &[flex_row(), items(Align::Center), w(Dim::Swatch)])
            .height(Size::px(CODE_LINE));
        if shown {
            slot = slot.child(checkbox(state, move |_: bool| {
                super::toggle_override(window, node, name);
            }));
        }
        rect()
            .tw(&tokens, &[w_full(), flex_row(), items(Align::Center)])
            .on_pointer_enter(move |_| hover_on.set(true))
            .on_pointer_leave(move |_| hover_off.set(false))
            .child(slot)
            .child(
                rect().tw(&tokens, &[w_flex()]).child(
                    // Keyed by the enable state, so a toggle re-reads the value.
                    rect()
                        .key(format!("enabled:{}", self.enabled))
                        .child(PropRow {
                            window,
                            node,
                            decl: self.decl.clone(),
                            picker: self.picker,
                            comment: Some(self.comment.clone()),
                        }),
                ),
            )
    }
}

/// The Computed tab: where the node is and how big, its identity, and every
/// declared value as the application resolved it.
fn computed_tab(tokens: &Tokens, node: &Node, filter: State<String>) -> Element {
    let mut rows: Vec<(&'static str, String)> = vec![
        ("Element", node.title().to_string()),
        ("Node id", node.id.to_string()),
        (
            "Parent",
            node.parent.map_or("none".to_string(), |id| id.to_string()),
        ),
        ("Children", node.children.len().to_string()),
    ];
    if let Some(area) = node.area {
        rows.push((
            "Position",
            format!("{:.0}, {:.0}", area.min_x(), area.min_y()),
        ));
        rows.push((
            "Size",
            format!("{:.0} x {:.0}", area.width(), area.height()),
        ));
    }
    if node.kind == super::snapshot::Kind::Label {
        if let Some(text) = &node.text {
            rows.push(("Text", text.clone()));
        }
    }
    for decl in editor::declared(node) {
        rows.push((decl.name, decl.value));
    }
    rows.sort_by(|a, b| a.0.cmp(b.0));

    let query = filter.read().trim().to_lowercase();
    let mut list = vstack(S2);
    for (name, value) in rows {
        let shown = query.is_empty()
            || name.to_lowercase().contains(&query)
            || value.to_lowercase().contains(&query);
        if shown {
            list = list.child(property(tokens, name, value));
        }
    }
    rect()
        .tw(tokens, &[w_full(), h_full(), flex_col(), p(S2), gap(S2)])
        .child(search_field(filter, |_: String| {}))
        .child(list)
        .into()
}

/// The Accessibility tab: the role, name and focus the application reports.
fn accessibility_tab(tokens: &Tokens, node: &Node) -> Element {
    let focus = match node.a11y.focusable {
        Some(true) => "yes",
        Some(false) => "no",
        None => "not set",
    };
    let rows = vstack(S2)
        .child(property(tokens, "Role", node.a11y.role.clone()))
        .child(property(
            tokens,
            "Name",
            node.a11y.name.clone().unwrap_or_else(|| "none".to_string()),
        ))
        .child(property(tokens, "Focusable", focus.to_string()));
    rect()
        .tw(tokens, &[w_full(), h_full(), p(S2)])
        .child(rows)
        .into()
}

/// One property: the name in caption type, the value below it.
pub(super) fn property(tokens: &Tokens, name: &str, value: String) -> Element {
    rect()
        .tw(tokens, &[w_full(), flex_col(), gap(S0_5)])
        .child(styled_label(CAPTION, name))
        .child(styled_label(BODY, value))
        .into()
}
