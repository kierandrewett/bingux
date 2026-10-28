//! The Properties pane: the selected node as bingux-ui code. A component that
//! published its call shows it with each argument as an editable field; the
//! box shows the utilities that build it (`gap(S3)`, `p(S4)`, `w_full()`,
//! `bg(Paint::Card)`), each picked from its token set with the value the token
//! stands for beside it. Edits apply to the running app through overrides.

use std::rc::Rc;

use freya::prelude::*;
use freya::winit::window::WindowId;

use super::editor::{self, Def, Editor, Group as PropGroup, RAW};
use super::snapshot::{Kind, Node, Snapshot};
use crate::component::{Prop, PropKind, Published};
use crate::controls::colour_picker::{self, colour_chip, colour_picker};
use crate::controls::{action_button, dropdown, icon_button, property_row, switch, text_field};
use crate::group::Group;
use crate::icons::Icon;
use crate::shell::overlays::Overlays;
use crate::shell::panel::{Panel, PanelKind, PanelRect, PanelTrigger};
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{CAPTION, CODE, EMPHASIS, Ink};

/// The pane for the node `id` of the app window `window`, shown at the density
/// the app window renders at.
pub struct PropertiesPane {
    pub snap: Rc<Snapshot>,
    pub id: u64,
    pub window: WindowId,
}

impl PartialEq for PropertiesPane {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.snap, &other.snap) && self.id == other.id && self.window == other.window
    }
}

impl Component for PropertiesPane {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let class = super::app_density(self.window);
        let overlays = try_consume_context::<Overlays>();
        let Some(node) = self.snap.node(self.id) else {
            return rect()
                .tw(&tokens, &[w_full(), p(S4)])
                .child(CAPTION.label(&tokens, "This element is gone"))
                .into_element();
        };
        let (window, id) = (self.window, self.id);
        let defs = editor::defs(node, class, &tokens);

        let mut body = rect().tw(&tokens, &[w_full(), flex_col(), gap(S4), p(S4)]);
        body = body.child(header(&tokens, &self.snap, node, window, class, overlays));
        if let Some(group) = component_group(&tokens, &self.snap, node, window, class) {
            body = body.child(group);
        }
        for group in PropGroup::ALL {
            let rows: Vec<Element> = defs
                .iter()
                .filter(|def| def.group == group)
                .map(|def| def_row(&tokens, window, id, def.clone(), class))
                .collect();
            if rows.is_empty() {
                continue;
            }
            body = body.child(Group::titled(group.title()).rows(rows));
        }
        // The window's navigation stack: Back, the history menu and the
        // title read it, so a wrong one is visible here.
        if let Some(section) = super::route::section(&tokens, window) {
            body = body.child(section);
        }
        ScrollView::new()
            .drag_scrolling(false)
            .width(Size::fill())
            .height(Size::fill())
            .child(body)
            .into_element()
    }
}

/// The node's name and role, and the actions that act on the whole node: copy
/// it as code and reset every edit.
fn header(
    tokens: &Tokens,
    snap: &Rc<Snapshot>,
    node: &Node,
    window: WindowId,
    class: SizeClass,
    overlays: Option<Overlays>,
) -> Element {
    let source = editor::rust_source(node, class, tokens);
    let id = node.id;
    let reset_snap = snap.clone();
    let any_edit = has_any_edit(snap, window, id);
    let copy = action_button("Copy as Rust", move |_| {
        let outcome = freya::clipboard::Clipboard::set(source.clone());
        if let Some(overlays) = overlays {
            match outcome {
                Ok(()) => overlays.toast("Copied as Rust."),
                Err(_) => overlays.toast("The code could not be copied."),
            }
        }
    })
    .icon(Icon::EditCopy)
    .compact();
    let reset = icon_button(Icon::Refresh, move |_| reset_all(&reset_snap, window, id))
        .tooltip("Reset every edit on this element")
        .enabled(any_edit);
    rect()
        .tw(
            tokens,
            &[
                flex_row(),
                grow_children(),
                items(Align::Center),
                w_full(),
                gap(S2),
            ],
        )
        .child(
            rect()
                .tw(tokens, &[flex_col(), w_flex(), gap(S0_5)])
                .child(
                    EMPHASIS
                        .label(tokens, node.title().to_string())
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                )
                .child(
                    CAPTION
                        .in_ink(Ink::Muted)
                        .label(tokens, subtitle(node))
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                ),
        )
        .child(copy)
        .child(reset)
        .into()
}

/// What is under the node's name: its size and id.
fn subtitle(node: &Node) -> String {
    // The border box the layout measured: the area it keeps for the node also
    // holds the node's margin.
    let size = node
        .used
        .border
        .or(node.area)
        .map(|area| format!("{:.0} × {:.0}", area.width(), area.height()))
        .unwrap_or_else(|| "not laid out".to_string());
    format!("{size} · #{}", node.id)
}

/// Whether the node, or the text a published prop of it shows, is edited.
fn has_any_edit(snap: &Snapshot, window: WindowId, id: u64) -> bool {
    if !super::overrides_of(window, id).is_empty() {
        return true;
    }
    let Some(node) = snap.node(id) else {
        return false;
    };
    node.a11y.published.as_ref().is_some_and(|call| {
        call.props().any(|prop| {
            let prop = effective(snap, window, node, prop);
            prop.changed
        })
    })
}

/// Drops every edit of the node, and of the labels and component props its
/// published call is shown through.
fn reset_all(snap: &Snapshot, window: WindowId, id: u64) {
    super::reset(window, id, None);
    let Some(node) = snap.node(id) else {
        return;
    };
    if let Some(call) = &node.a11y.published {
        for prop in call.props() {
            let found = effective(snap, window, node, prop);
            if let Some(target) = found.target {
                super::reset(window, target, Some("text"));
            }
            if let Some(a11y) = node.a11y.id {
                crate::component::clear_prop_override(a11y, Some(&prop.name));
            }
        }
    }
}

/// A published prop as it stands now.
struct Effective {
    /// The value now: the original, or what an edit made it.
    value: String,
    changed: bool,
    /// The label that shows a text prop, when it can be found.
    target: Option<u64>,
}

/// Works out a published prop's current value, whether it is edited, and for a
/// text prop which label of the component shows it.
fn effective(snap: &Snapshot, window: WindowId, node: &Node, prop: &Prop) -> Effective {
    if prop.kind == PropKind::Text {
        let target = text_target(snap, window, node, &prop.value);
        let (value, changed) = match target.and_then(|id| snap.node(id)) {
            Some(label) => {
                let now = label.text.clone().unwrap_or_default();
                (
                    now.clone(),
                    super::has_override(window, label.id, "text") && now != prop.value,
                )
            }
            None => (prop.value.clone(), false),
        };
        return Effective {
            value,
            changed,
            target,
        };
    }
    let overridden = node
        .a11y
        .id
        .and_then(|id| crate::component::peek_prop_override(id, &prop.name));
    match overridden {
        Some(value) => Effective {
            value,
            changed: true,
            target: None,
        },
        None => Effective {
            value: prop.value.clone(),
            changed: false,
            target: None,
        },
    }
}

/// The label or paragraph under `node` that shows `original`: the one an edit
/// of it already changed, else the first whose text is `original`.
fn text_target(snap: &Snapshot, window: WindowId, node: &Node, original: &str) -> Option<u64> {
    let mut stack = vec![node.id];
    let mut preorder = Vec::new();
    while let Some(next) = stack.pop() {
        let Some(found) = snap.node(next) else {
            continue;
        };
        preorder.push(found);
        stack.extend(found.children.iter().rev().copied());
    }
    let texts: Vec<&Node> = preorder
        .into_iter()
        .filter(|found| matches!(found.kind, Kind::Label | Kind::Paragraph))
        .collect();
    texts
        .iter()
        .find(|found| {
            super::overrides_of(window, found.id)
                .iter()
                .any(|(key, entry)| *key == "text" && entry.original == original)
        })
        .or_else(|| {
            texts
                .iter()
                .find(|found| found.text.as_deref() == Some(original))
        })
        .map(|found| found.id)
}

/// The component group: the call that built the node, and each value of it as
/// a field. Nodes that name a component without publishing a call show the
/// name alone.
fn component_group(
    tokens: &Tokens,
    snap: &Rc<Snapshot>,
    node: &Node,
    window: WindowId,
    class: SizeClass,
) -> Option<Element> {
    let call = node.a11y.published.as_ref();
    let name = node.a11y.component.as_deref();
    if call.is_none() && name.is_none() {
        return None;
    }
    let mut rows: Vec<Element> = Vec::new();
    if let Some(call) = call {
        let current = effective_call(snap, window, node, call);
        rows.push(
            rect()
                .tw(tokens, &[w_full(), px(GUTTER), py(S3), flex_col(), gap(S1)])
                .child(CODE.paragraph(tokens, current.source()).width(Size::fill()))
                .into(),
        );
        for (index, prop) in call.args.iter().chain(&call.methods).enumerate() {
            let found = effective(snap, window, node, prop);
            let role = if index < call.args.len() {
                "argument"
            } else {
                "method"
            };
            rows.push(prop_row(
                tokens,
                window,
                node,
                prop.clone(),
                found,
                role,
                class,
            ));
        }
    } else if let Some(name) = name {
        rows.push(
            property_row("component")
                .hint(format!("a bingux-ui or app component: {name}"))
                .control(CODE.label(tokens, name.to_string()))
                .into(),
        );
    }
    Some(Group::titled("Component").rows(rows).into())
}

/// The call with the values the inspector has edited in it.
fn effective_call(snap: &Snapshot, window: WindowId, node: &Node, call: &Published) -> Published {
    let mut out = call.clone();
    for prop in out.args.iter_mut().chain(out.methods.iter_mut()) {
        prop.value = effective(snap, window, node, prop).value;
    }
    out
}

/// One published prop: its name, what kind of argument it is, and the editor.
fn prop_row(
    tokens: &Tokens,
    window: WindowId,
    node: &Node,
    prop: Prop,
    found: Effective,
    role: &str,
    class: SizeClass,
) -> Element {
    let live = found.target.is_some() || (prop.live && node.a11y.id.is_some());
    let hint = if live {
        format!("{role} · {}", kind_hint(&prop.kind))
    } else {
        format!("{role} · {} · set in code", kind_hint(&prop.kind))
    };
    let target = found.target;
    let a11y = node.a11y.id;
    let name = prop.name.clone();
    let reset_name = prop.name.clone();
    let mut row = property_row(name)
        .hint(hint)
        .key(format!("{}:prop:{}:{}", node.id, prop.name, found.value));
    if found.changed {
        row = row.on_reset(move |_| {
            if let Some(target) = target {
                super::reset(window, target, Some("text"));
            }
            if let Some(a11y) = a11y {
                crate::component::clear_prop_override(a11y, Some(&reset_name));
            }
        });
    }
    row.control(PropField {
        window,
        node: node.id,
        a11y,
        target,
        prop,
        value: found.value,
        live,
        tokens: *tokens,
        class,
    })
    .into()
}

fn kind_hint(kind: &PropKind) -> String {
    match kind {
        PropKind::Text => "text".to_string(),
        PropKind::Flag => "on or off".to_string(),
        PropKind::Number { min, max, .. } => format!("number {min} to {max}"),
        PropKind::Choice(options) => format!("{} choices", options.len()),
    }
}

/// The editor of one published prop.
#[derive(PartialEq)]
struct PropField {
    window: WindowId,
    node: u64,
    a11y: Option<freya_core::accessibility::id::AccessibilityId>,
    target: Option<u64>,
    prop: Prop,
    value: String,
    live: bool,
    tokens: Tokens,
    class: SizeClass,
}

impl Component for PropField {
    fn render(&self) -> impl IntoElement {
        let draft = use_state(|| self.value.clone());
        let (window, class, tokens) = (self.window, self.class, self.tokens);
        let (target, a11y, live) = (self.target, self.a11y, self.live);
        let name = self.prop.name.clone();
        let original = self.prop.value.clone();
        // A text prop is edited on the label that shows it; any other prop is
        // handed to the component, which reads it back.
        let apply = {
            let name = name.clone();
            move |text: String| {
                if let Some(target) = target {
                    editor::commit(window, target, "text", &text, &original, class, &tokens);
                } else if let Some(a11y) = a11y {
                    crate::component::set_prop_override(a11y, &name, text);
                }
            }
        };
        match &self.prop.kind {
            PropKind::Flag => {
                let on = self.value == "true";
                switch(on, move |next: bool| apply(next.to_string()))
                    .enabled(live)
                    .into_element()
            }
            PropKind::Choice(options) => dropdown(
                options
                    .iter()
                    .map(|option| (option.clone(), option.clone()))
                    .collect(),
                self.value.clone(),
                move |picked: String| apply(picked),
            )
            .dense()
            .width(Dim::DenseField)
            .enabled(live)
            .into_element(),
            PropKind::Number { min, max, step } => {
                let (min, max, step) = (*min, *max, *step);
                let apply_arrow = apply.clone();
                let mut draft_arrow = draft;
                text_field(draft, move |text: String| apply(text))
                    .dense()
                    .width(Dim::DenseField)
                    .enabled(live)
                    .on_arrow(move |delta: f64| {
                        let now: f64 = draft_arrow.peek().trim().parse().unwrap_or(min);
                        let next = (now + delta * step).clamp(min, max);
                        let text = editor::number_text(next as f32);
                        draft_arrow.set(text.clone());
                        apply_arrow(text);
                    })
                    .into_element()
            }
            PropKind::Text => text_field(draft, move |text: String| apply(text))
                .dense()
                .width(Dim::DenseField)
                .enabled(live)
                .into_element(),
        }
    }
}

/// One property of the box as a row: its name, the call it stands for and its
/// editor, with a reset when the property is edited.
pub(super) fn def_row(
    tokens: &Tokens,
    window: WindowId,
    node: u64,
    def: Def,
    class: SizeClass,
) -> Element {
    let key = def.key;
    let changed = super::has_override(window, node, key);
    let mut row = property_row(key)
        .hint(def.hint.clone())
        .key(format!("{node}:{key}:{}", def.current()));
    if changed {
        row = row.on_reset(move |_| super::reset(window, node, Some(key)));
    }
    row.control(DefField {
        window,
        node,
        def,
        tokens: *tokens,
        class,
    })
    .into()
}

/// The editor of one property of the box.
#[derive(PartialEq)]
struct DefField {
    window: WindowId,
    node: u64,
    def: Def,
    tokens: Tokens,
    class: SizeClass,
}

impl Component for DefField {
    fn render(&self) -> impl IntoElement {
        let (window, node, class, tokens) = (self.window, self.node, self.class, self.tokens);
        let def = self.def.clone();
        let key = def.key;
        let original = def.current();
        let draft = use_state(|| def.text.clone());
        // Picking "Raw value" keeps the value and shows its field; the value
        // may still be a token's, so the pick alone changes nothing.
        let raw_mode = use_state(|| false);
        let picker_open = use_state(|| false);
        let swatch_area = use_state(Area::default);

        let commit = {
            let original = original.clone();
            move |text: &str| {
                editor::commit(window, node, key, text, &original, class, &tokens);
            }
        };

        match def.editor {
            Editor::ReadOnly => CODE
                .in_ink(Ink::Muted)
                .label(&tokens, def.text.clone())
                .into_element(),
            Editor::Flag => {
                let on = def.text == "true";
                switch(on, move |next: bool| commit(&next.to_string())).into_element()
            }
            Editor::Text => text_field(draft, move |text: String| commit(&text))
                .dense()
                .width(Dim::DenseField)
                .into_element(),
            Editor::Number { step } => {
                let commit_arrow = commit.clone();
                let mut draft_arrow = draft;
                text_field(draft, move |text: String| commit(&text))
                    .dense()
                    .width(Dim::DenseField)
                    .on_arrow(move |delta: f64| {
                        let now: f64 = draft_arrow.peek().trim().parse().unwrap_or(0.0);
                        let text = editor::number_text((now + delta * step) as f32);
                        draft_arrow.set(text.clone());
                        commit_arrow(&text);
                    })
                    .into_element()
            }
            Editor::Choice | Editor::Colour => {
                let colour = def.editor == Editor::Colour;
                let raw_shown = raw_mode() || def.selected == RAW;
                let shown = if raw_mode() {
                    RAW.to_string()
                } else {
                    def.selected.clone()
                };
                let options: Vec<(String, String)> = def
                    .options
                    .iter()
                    .map(|option| (option.key.clone(), option.label.clone()))
                    .collect();
                let mut raw_set = raw_mode;
                let pick = {
                    let commit = commit.clone();
                    move |picked: String| {
                        if picked == RAW {
                            raw_set.set(true);
                            return;
                        }
                        raw_set.set(false);
                        commit(&picked);
                    }
                };
                let mut menu = dropdown(options, shown, pick)
                    .dense()
                    .width(Dim::DenseField);
                if colour {
                    menu = menu.colours(
                        def.options
                            .iter()
                            .map(|option| {
                                option
                                    .colour
                                    .unwrap_or_else(|| Paint::Transparent.color(&tokens))
                            })
                            .collect(),
                    );
                }
                let mut column = rect()
                    .tw(&tokens, &[flex_col(), gap(S1), items(Align::End)])
                    .child(menu);
                if raw_shown && colour {
                    column = column.child(raw_colour(
                        &tokens,
                        &def,
                        draft,
                        picker_open,
                        swatch_area,
                        commit.clone(),
                    ));
                } else if raw_shown {
                    let commit_arrow = commit.clone();
                    let mut draft_arrow = draft;
                    column = column.child(
                        text_field(draft, move |text: String| commit(&text))
                            .dense()
                            .width(Dim::DenseField)
                            .placeholder("px or 50%")
                            .on_arrow(move |delta: f64| {
                                let text = draft_arrow.peek().clone();
                                let Ok(now) = text.trim().trim_end_matches("px").parse::<f64>()
                                else {
                                    return;
                                };
                                let next = editor::number_text((now + delta).max(0.0) as f32);
                                draft_arrow.set(next.clone());
                                commit_arrow(&next);
                            }),
                    );
                }
                column.into_element()
            }
        }
    }
}

/// The custom colour of a colour property: a swatch that opens the picker, and
/// the hex field beside it.
fn raw_colour(
    tokens: &Tokens,
    def: &Def,
    draft: State<String>,
    picker_open: State<bool>,
    swatch_area: State<Area>,
    commit: impl Fn(&str) + Clone + 'static,
) -> Element {
    let tokens = *tokens;
    let colour = def
        .swatch
        .unwrap_or_else(|| Paint::Transparent.color(&tokens));
    let mut open = picker_open;
    let mut close = picker_open;
    let mut area_set = swatch_area;
    let mut draft_pick = draft;
    let commit_pick = commit.clone();
    let panel = Panel::new(
        PanelKind::Menu,
        picker_open(),
        PanelTrigger::Control(PanelRect::from_area(&swatch_area.read())),
        vec![Element::from(colour_picker(colour, move |next: Color| {
            let hex = colour_to_hex(next);
            draft_pick.set(hex.clone());
            commit_pick(&hex);
        }))],
    )
    .size(
        colour_picker::WIDTH + 2.0 * S2.px(),
        colour_picker::height(),
    )
    .on_dismiss(EventHandler::new(move |_: ()| close.set(false)));
    rect()
        .tw(&tokens, &[flex_row(), items(Align::Center), gap(S2)])
        .child(
            rect()
                .on_sized(move |event: Event<SizedEventData>| {
                    area_set.set_if_modified(event.area);
                })
                .on_pointer_down(move |_: Event<PointerEventData>| {
                    let next = !*open.peek();
                    open.set(next);
                })
                .child(colour_chip(tokens, colour, Dim::Swatch.px())),
        )
        .child(panel)
        .child(
            text_field(draft, move |text: String| commit(&text))
                .dense()
                .width(Dim::FieldColour),
        )
        .into()
}
