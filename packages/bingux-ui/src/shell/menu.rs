//! A popover menu of actions, opened at a point on the window: from a button,
//! from a right click, or from the keyboard. Picking an item closes the menu
//! and then runs its action.
//!
//! An item with a submenu opens it on hover (after a short delay), on Right,
//! or on Enter and click. A pointer crossing from an open row toward its
//! submenu keeps the submenu open while it stays inside the safe triangle
//! between the pointer and the submenu's near edge. Up, Down, Home, End and
//! letters move the highlight; Left or Escape steps back one level; Escape at
//! the top level or Tab closes the menu and returns focus to its owner.

use std::time::Duration;

use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::motion::{Focusable, PRESENCE_SCALE, use_presence};
use crate::style::*;
use crate::surface::{is_activation_key, use_tokens};
use crate::typography::{BODY, Ink};

use super::overlays::{Overlays, input_shield, use_overlays};

/// How long a pointer must rest on a row with a submenu before it opens.
const SUBMENU_OPEN_DELAY_MS: u64 = 150;

/// How long a pointer may rest inside the safe triangle before the triangle
/// is dropped, so the pointer can then pick another row of the parent menu.
const CONE_REST_MS: u64 = 300;

/// Space between a menu and its submenu.
const SUBMENU_GAP: f32 = S1.px();

/// Space kept between a menu and the window edges.
const MENU_EDGE: f32 = S2.px();

/// One action in a menu. Build with [`menu_item`] or [`submenu`].
#[derive(Clone, PartialEq)]
pub struct MenuItem {
    pub(crate) label: String,
    pub(crate) icon: Option<Icon>,
    pub(crate) checked: Option<bool>,
    pub(crate) enabled: bool,
    pub(crate) destructive: bool,
    pub(crate) separator: bool,
    pub(crate) submenu: Option<Vec<MenuItem>>,
    pub(crate) on_press: EventHandler<()>,
}

impl MenuItem {
    /// Shows an icon before the label.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Shows a tick before the label when `checked` is true. The space stays
    /// when it is false, so labels line up with checked items.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    /// Greys the item out. A disabled item cannot be picked or highlighted.
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Colours the item as a destructive action.
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }
}

/// An action for a menu. `on_press` runs after the menu closes.
pub fn menu_item(label: impl Into<String>, on_press: impl FnMut(()) + 'static) -> MenuItem {
    MenuItem {
        label: label.into(),
        icon: None,
        checked: None,
        enabled: true,
        destructive: false,
        separator: false,
        submenu: None,
        on_press: EventHandler::new(on_press),
    }
}

/// An item that opens a submenu of `items` beside the menu. It runs no action
/// of its own.
pub fn submenu(label: impl Into<String>, items: Vec<MenuItem>) -> MenuItem {
    MenuItem {
        submenu: Some(items),
        ..menu_item(label, |_| {})
    }
}

/// A thin line between groups of items.
pub fn menu_separator() -> MenuItem {
    MenuItem {
        separator: true,
        ..menu_item(String::new(), |_| {})
    }
}

/// The open menu: where it sits on the window, and its items.
#[derive(Clone, PartialEq)]
pub(crate) struct Menu {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) items: Vec<MenuItem>,
}

/// A menu panel on the window: its top-left corner and its size.
#[derive(Clone, Copy, PartialEq)]
struct Panel {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Panel {
    fn contains(self, point: (f32, f32)) -> bool {
        point.0 >= self.x
            && point.0 <= self.x + self.width
            && point.1 >= self.y
            && point.1 <= self.y + self.height
    }
}

/// The safe triangle of an open row. A pointer that leaves the row toward its
/// submenu stays inside the triangle from `origin` to the submenu's near edge,
/// so the submenu stays open while the pointer travels to it.
#[derive(Clone, Copy, PartialEq)]
struct Cone {
    /// The panel holding the open row.
    level: usize,
    origin: (f32, f32),
    sub: Panel,
    /// Counts the pointer moves inside the triangle, so a rest check can tell
    /// whether the pointer has stopped since it was started.
    moves: u64,
}

impl Cone {
    fn contains(self, point: (f32, f32)) -> bool {
        let near = if self.sub.x >= self.origin.0 {
            self.sub.x
        } else {
            self.sub.x + self.sub.width
        };
        let a = self.origin;
        let b = (near, self.sub.y);
        let c = (near, self.sub.y + self.sub.height);
        let d1 = side(point, a, b);
        let d2 = side(point, b, c);
        let d3 = side(point, c, a);
        let negative = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
        let positive = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
        !(negative && positive)
    }
}

/// The sign of the cross product of `point` against the edge `a` to `b`.
fn side(point: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    (point.0 - b.0) * (a.1 - b.1) - (a.0 - b.0) * (point.1 - b.1)
}

/// The keyboard and pointer state of an open menu.
///
/// `open[level]` is the row of panel `level` whose submenu is open, so panel
/// `level + 1` exists for each entry. `highlight[level]` is the highlighted row
/// of panel `level`, and `active` is the panel that keys act on.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct MenuNav {
    /// The control that opened the menu, focused again when it closes from the
    /// keyboard.
    pub(crate) owner: Option<AccessibilityId>,
    open: Vec<usize>,
    highlight: Vec<Option<usize>>,
    active: usize,
    cone: Option<Cone>,
}

impl MenuNav {
    /// The state of a new menu. From the keyboard, the first item is highlighted.
    pub(crate) fn new(owner: Option<AccessibilityId>, items: &[MenuItem], keyboard: bool) -> Self {
        let mut nav = Self {
            owner,
            ..Self::default()
        };
        nav.sync();
        if keyboard {
            nav.highlight[0] = first_selectable(items);
        }
        nav
    }

    /// Keeps one highlight slot per panel and the active panel in range.
    fn sync(&mut self) {
        self.highlight.resize(self.open.len() + 1, None);
        self.active = self.active.min(self.open.len());
    }

    /// Highlights row `index` of panel `level`. Moving off the row whose
    /// submenu is open closes that submenu.
    fn set_highlight(&mut self, level: usize, index: Option<usize>) {
        if self.open.get(level).copied() != index {
            self.open.truncate(level);
        }
        self.cone = None;
        self.active = level;
        self.sync();
        self.highlight[level] = index;
    }

    /// Opens the submenu of row `index` of panel `level`. `first` is the row
    /// to highlight in the submenu; without one the submenu is only shown, and
    /// the parent panel stays active.
    fn open_sub(&mut self, level: usize, index: usize, first: Option<usize>) {
        self.open.truncate(level);
        self.open.push(index);
        self.cone = None;
        self.active = if first.is_some() { level + 1 } else { level };
        self.sync();
        self.highlight[level] = Some(index);
        self.highlight[level + 1] = first;
    }

    /// Closes the deepest open submenu. Its parent row stays highlighted.
    fn close_deepest(&mut self) {
        if self.open.pop().is_some() {
            self.cone = None;
            self.active = self.open.len();
            self.sync();
        }
    }
}

/// The height of one row of a menu.
fn row_height(item: &MenuItem) -> f32 {
    if item.separator {
        Dim::MenuSeparator.px()
    } else {
        Dim::MenuItem.px()
    }
}

/// The height of a menu of `items`: its rows, separators and padding.
fn menu_height(items: &[MenuItem]) -> f32 {
    items.iter().map(row_height).sum::<f32>() + S2.px() + S2.px()
}

/// Where a menu of `size` opened at `at` sits on a window of `window`. It
/// opens down and to the right of the pointer, flips up or left when there is
/// no room, and is then held inside the window on all four edges. With no
/// window size yet, it stays where it was opened.
fn place_menu(at: (f32, f32), size: (f32, f32), window: (f32, f32)) -> (f32, f32) {
    let (x, y) = at;
    let (width, height) = size;
    let (window_width, window_height) = window;
    if window_width <= 0.0 || window_height <= 0.0 {
        return at;
    }
    let x = if x + width + MENU_EDGE > window_width {
        x - width
    } else {
        x
    };
    let y = if y + height + MENU_EDGE > window_height {
        y - height
    } else {
        y
    };
    let x = x.min(window_width - width - MENU_EDGE).max(MENU_EDGE);
    let y = y.min(window_height - height - MENU_EDGE).max(MENU_EDGE);
    (x, y)
}

/// Holds `value` for a panel of `size` inside a window edge at `limit`. With
/// no window size yet, it is left alone.
fn fit(value: f32, size: f32, limit: f32) -> f32 {
    if limit <= 0.0 {
        value
    } else {
        value.min(limit - size - MENU_EDGE).max(MENU_EDGE)
    }
}

/// The items shown in panel `level`: the root items, then each open submenu.
fn panel_items<'a>(root: &'a [MenuItem], open: &[usize], level: usize) -> &'a [MenuItem] {
    let mut items = root;
    for &index in open.iter().take(level) {
        match items.get(index).and_then(|item| item.submenu.as_deref()) {
            Some(sub) => items = sub,
            None => return &[],
        }
    }
    items
}

/// Where every open panel sits: the root menu at `at`, then each submenu
/// beside the row that opened it, aligned to that row's top.
fn layout(root: &[MenuItem], open: &[usize], at: (f32, f32), window: (f32, f32)) -> Vec<Panel> {
    let width = Dim::Menu.px();
    let height = menu_height(root);
    let (x, y) = place_menu(at, (width, height), window);
    let mut panels = vec![Panel {
        x,
        y,
        width,
        height,
    }];
    for (level, &index) in open.iter().enumerate() {
        let parent = panels[level];
        let items = panel_items(root, open, level);
        let Some(sub) = items.get(index).and_then(|item| item.submenu.as_deref()) else {
            break;
        };
        let row_top = parent.y + S2.px() + items.iter().take(index).map(row_height).sum::<f32>();
        let height = menu_height(sub);
        let right = parent.x + parent.width + SUBMENU_GAP;
        let x = if window.0 <= 0.0 || right + width + MENU_EDGE <= window.0 {
            right
        } else {
            parent.x - SUBMENU_GAP - width
        };
        panels.push(Panel {
            x: fit(x, width, window.0),
            y: fit(row_top - S2.px(), height, window.1),
            width,
            height,
        });
    }
    panels
}

fn selectable(item: &MenuItem) -> bool {
    item.enabled && !item.separator
}

fn first_selectable(items: &[MenuItem]) -> Option<usize> {
    items.iter().position(selectable)
}

fn last_selectable(items: &[MenuItem]) -> Option<usize> {
    items.iter().rposition(selectable)
}

/// The next selectable row after `from` in `forward` order, wrapping around.
fn step_selectable(items: &[MenuItem], from: Option<usize>, forward: bool) -> Option<usize> {
    let count = items.len();
    let mut current = from;
    for _ in 0..count {
        let next = match (current, forward) {
            (None, true) => 0,
            (None, false) => count - 1,
            (Some(index), true) => (index + 1) % count,
            (Some(index), false) => (index + count - 1) % count,
        };
        if selectable(&items[next]) {
            return Some(next);
        }
        current = Some(next);
    }
    None
}

/// The next selectable row, after `from`, whose label starts with `letter`.
fn typeahead(items: &[MenuItem], from: Option<usize>, letter: char) -> Option<usize> {
    let count = items.len();
    if count == 0 {
        return None;
    }
    let start = from.unwrap_or(count - 1);
    (1..=count)
        .map(|step| (start + step) % count)
        .find(|&index| {
            let item = &items[index];
            selectable(item)
                && item
                    .label
                    .chars()
                    .next()
                    .is_some_and(|first| first.to_lowercase().eq(letter.to_lowercase()))
        })
}

fn global_point(event: &Event<PointerEventData>) -> (f32, f32) {
    let at = event.global_location();
    (at.x as f32, at.y as f32)
}

fn read_nav(overlays: Overlays) -> MenuNav {
    overlays.menu_nav.read().clone()
}

fn update_nav(mut overlays: Overlays, change: impl FnOnce(&mut MenuNav)) {
    let mut nav = overlays.menu_nav.write();
    change(&mut nav);
}

/// A pointer entered row `index` of panel `level`.
fn hover_row(overlays: Overlays, level: usize, index: usize, has_sub: bool, point: (f32, f32)) {
    if let Some(cone) = read_nav(overlays).cone
        && cone.level == level
        && cone.contains(point)
    {
        // Crossing toward the open submenu: keep it open.
        return;
    }
    update_nav(overlays, |nav| nav.set_highlight(level, Some(index)));
    if has_sub {
        spawn(async move {
            timer(Duration::from_millis(SUBMENU_OPEN_DELAY_MS)).await;
            open_if_hovered(overlays, level, index);
        });
    }
}

/// Opens the submenu of row `index` if the pointer is still on that row.
fn open_if_hovered(overlays: Overlays, level: usize, index: usize) {
    let nav = read_nav(overlays);
    if nav.highlight.get(level).copied().flatten() == Some(index)
        && nav.open.get(level) != Some(&index)
    {
        update_nav(overlays, |nav| nav.open_sub(level, index, None));
    }
}

/// A pointer left row `index` of panel `level`. When that row has an open
/// submenu, the pointer is now crossing toward it, so the safe triangle starts.
fn leave_row(overlays: Overlays, level: usize, point: (f32, f32), sub: Option<Panel>) {
    if let Some(sub) = sub {
        update_nav(overlays, |nav| {
            nav.cone = Some(Cone {
                level,
                origin: point,
                sub,
                moves: 0,
            });
        });
    }
}

/// The pointer moved anywhere over the menu. Inside the submenu the safe
/// triangle ends; inside the triangle it waits for a rest; anywhere else the
/// open submenu closes.
fn pointer_moved(overlays: Overlays, point: (f32, f32)) {
    let Some(cone) = read_nav(overlays).cone else {
        return;
    };
    if cone.sub.contains(point) {
        update_nav(overlays, |nav| nav.cone = None);
    } else if cone.contains(point) {
        let moves = cone.moves + 1;
        update_nav(overlays, |nav| nav.cone = Some(Cone { moves, ..cone }));
        spawn(async move {
            timer(Duration::from_millis(CONE_REST_MS)).await;
            rest_cone(overlays, moves);
        });
    } else {
        update_nav(overlays, |nav| {
            nav.open.truncate(cone.level);
            nav.cone = None;
            nav.sync();
            nav.highlight[cone.level] = None;
        });
    }
}

/// Drops the safe triangle if the pointer has not moved since `moves`.
fn rest_cone(overlays: Overlays, moves: u64) {
    update_nav(overlays, |nav| {
        if nav.cone.is_some_and(|cone| cone.moves == moves) {
            nav.cone = None;
        }
    });
}

/// A key was pressed while the menu was open. Keys act on the active panel.
fn handle_key(overlays: Overlays, root: &[MenuItem], event: Event<KeyboardEventData>) {
    // A key press is keyboard navigation, so the owner's focus ring shows
    // once the menu hands focus back.
    Platform::get()
        .current_window()
        .navigation_mode
        .set(NavigationMode::Keyboard);
    let nav = read_nav(overlays);
    let level = nav.active;
    let items = panel_items(root, &nav.open, level);
    let highlight = nav.highlight.get(level).copied().flatten();
    let key = event.key.clone();
    match &key {
        Key::Named(NamedKey::Escape) => {
            event.stop_propagation();
            if level > 0 {
                update_nav(overlays, |nav| nav.close_deepest());
            } else {
                overlays.dismiss_menu_by_key();
            }
        }
        Key::Named(NamedKey::ArrowLeft) if level > 0 => {
            event.stop_propagation();
            update_nav(overlays, |nav| nav.close_deepest());
        }
        Key::Named(NamedKey::ArrowRight) => {
            let Some(index) = highlight else {
                return;
            };
            let Some(item) = items.get(index) else {
                return;
            };
            let Some(sub) = item.submenu.as_deref() else {
                return;
            };
            if !item.enabled {
                return;
            }
            event.stop_propagation();
            let first = first_selectable(sub);
            update_nav(overlays, |nav| nav.open_sub(level, index, first));
        }
        Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowUp) => {
            event.stop_propagation();
            let forward = matches!(key, Key::Named(NamedKey::ArrowDown));
            if let Some(next) = step_selectable(items, highlight, forward) {
                update_nav(overlays, |nav| nav.set_highlight(level, Some(next)));
            }
        }
        Key::Named(NamedKey::Home) => {
            event.stop_propagation();
            if let Some(first) = first_selectable(items) {
                update_nav(overlays, |nav| nav.set_highlight(level, Some(first)));
            }
        }
        Key::Named(NamedKey::End) => {
            event.stop_propagation();
            if let Some(last) = last_selectable(items) {
                update_nav(overlays, |nav| nav.set_highlight(level, Some(last)));
            }
        }
        Key::Named(NamedKey::Tab) => overlays.dismiss_menu_by_key(),
        _ if is_activation_key(&key) => {
            event.stop_propagation();
            let Some(index) = highlight else {
                return;
            };
            let Some(item) = items.get(index) else {
                return;
            };
            if !selectable(item) {
                return;
            }
            if let Some(sub) = item.submenu.as_deref() {
                let first = first_selectable(sub);
                update_nav(overlays, |nav| nav.open_sub(level, index, first));
                return;
            }
            // Close first, so the action can open its own popup.
            overlays.dismiss_menu_by_key();
            item.on_press.call(());
        }
        Key::Character(text) if event.modifiers.is_empty() => {
            let Some(letter) = text.chars().next() else {
                return;
            };
            event.stop_propagation();
            if let Some(next) = typeahead(items, highlight, letter) {
                update_nav(overlays, |nav| nav.set_highlight(level, Some(next)));
            }
        }
        _ => {}
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct MenuLayer {
    pub(crate) menu: Menu,
    pub(crate) closing: bool,
    pub(crate) overlays: Overlays,
    pub(crate) layer: Layer,
}

impl Component for MenuLayer {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = self.overlays;
        let a11y_id = overlays.menu_panel;
        let progress = use_presence(!self.closing, tokens.motion(90), move || {
            overlays.finish_menu()
        });
        use_side_effect(move || a11y_id.request_focus());

        let nav = read_nav(overlays);
        let window = *Platform::get().current_window().root_size.read();
        let panels = layout(
            &self.menu.items,
            &nav.open,
            (self.menu.x, self.menu.y),
            (window.width, window.height),
        );
        let root_items = self.menu.items.clone();

        let mut elements = Vec::with_capacity(panels.len());
        for (level, panel) in panels.iter().copied().enumerate() {
            let items = panel_items(&self.menu.items, &nav.open, level);
            let rows: Vec<Element> = items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let sub = (nav.open.get(level) == Some(&index))
                        .then(|| panels.get(level + 1).copied())
                        .flatten();
                    Element::from(MenuRow {
                        item: item.clone(),
                        overlays,
                        level,
                        index,
                        highlighted: nav.highlight.get(level).copied().flatten() == Some(index),
                        sub,
                    })
                })
                .collect();
            let mut body = rect()
                .position(Position::new_absolute().top(panel.y).left(panel.x))
                .layer(Layer::Overlay)
                .opacity(progress)
                .scale(PRESENCE_SCALE + (1.0 - PRESENCE_SCALE) * progress)
                .transform_origin(TransformOrigin::top_left())
                .tw(&tokens, MENU)
                .tw(&tokens, &[w(Dim::Menu)])
                // Clicks inside a panel never reach the shield behind it.
                .on_press(|event: Event<PressEventData>| event.stop_propagation())
                .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
                .children(rows);
            if level == 0 {
                // The root panel takes the keyboard. Keys reach the active
                // panel through the menu state, so submenus need no focus.
                let keys = root_items.clone();
                body = body
                    .a11y_id(a11y_id)
                    .a11y_focusable(true)
                    .on_global_pointer_move(move |event: Event<PointerEventData>| {
                        pointer_moved(overlays, global_point(&event));
                    })
                    .on_key_down(move |event: Event<KeyboardEventData>| {
                        handle_key(overlays, &keys, event);
                    });
            }
            elements.push(body.into_element());
        }

        // The shield closes the menu on a press outside it and keeps the
        // window behind from reacting while the menu is open.
        input_shield(&tokens, move || overlays.close_menu())
            .layer(self.layer)
            .children(elements)
    }
}

#[derive(Clone, PartialEq)]
struct MenuRow {
    item: MenuItem,
    overlays: Overlays,
    level: usize,
    index: usize,
    highlighted: bool,
    /// The panel of this row's submenu, when that submenu is open.
    sub: Option<Panel>,
}

impl Component for MenuRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut pressing = use_state(|| false);
        if self.item.separator {
            return rect()
                .tw(&tokens, &[w_full(), h(Dim::MenuSeparator), center()])
                .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
                .into_element();
        }
        let enabled = self.item.enabled;
        let has_sub = self.item.submenu.is_some();
        let overlays = self.overlays;
        let (level, index) = (self.level, self.index);
        let highlighted = self.highlighted;
        let sub = self.sub;
        let on_press = self.item.on_press.clone();
        let submenu_items = self.item.submenu.clone();
        // A disabled item keeps its colour and fades like any disabled
        // control, so it reads as unavailable at a glance.
        let ink = if enabled && self.item.destructive {
            Ink::Danger
        } else {
            Ink::Text
        };
        let text_color = ink.color(&tokens);
        // A tick for checked items; the icon otherwise. The space is kept
        // whenever either is set, so labels line up.
        let glyph = match (self.item.checked, self.item.icon) {
            (Some(true), _) => Some(Icon::Check),
            (_, icon) => icon,
        };
        let leading = (self.item.checked.is_some() || self.item.icon.is_some()).then(|| {
            rect()
                .tw(&tokens, &[w(Dim::IconControl), h(Dim::IconControl)])
                .maybe_child(glyph.map(|icon| icons::icon(icon, IconSize::Control, text_color)))
        });
        let trailing = has_sub.then(|| icons::icon(Icon::PanEnd, IconSize::Control, text_color));
        rect()
            .a11y_role(AccessibilityRole::MenuItem)
            .a11y_enabled(enabled)
            .tw(&tokens, MENU_ITEM)
            .maybe(!enabled, |element| element.opacity(DISABLED_OPACITY))
            .tw(
                &tokens,
                &[surface(
                    Paint::Transparent,
                    enabled && highlighted,
                    enabled && pressing(),
                    false,
                )],
            )
            .on_pointer_enter(move |event: Event<PointerEventData>| {
                if enabled {
                    hover_row(overlays, level, index, has_sub, global_point(&event));
                }
            })
            .on_pointer_leave(move |event: Event<PointerEventData>| {
                pressing.set_if_modified(false);
                leave_row(overlays, level, global_point(&event), sub);
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                event.stop_propagation();
                if enabled {
                    pressing.set(true);
                }
            })
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                pressing.set(false);
                if !enabled {
                    return;
                }
                if let Some(items) = &submenu_items {
                    let first = first_selectable(items);
                    update_nav(overlays, |nav| nav.open_sub(level, index, first));
                    return;
                }
                // Close first, so the action can open its own popup.
                overlays.close_menu();
                on_press.call(());
            })
            .maybe_child(leading)
            .child(
                BODY.in_ink(ink)
                    .label(&tokens, self.item.label.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .width(Size::flex(1.0)),
            )
            .maybe_child(trailing)
            .into_element()
    }
}

/// A button that opens a menu of its items where the pointer is pressed, or
/// from the keyboard with Enter, Space or Down. Build with [`menu_button`].
#[derive(Clone, PartialEq)]
pub struct MenuButton {
    label: String,
    icon: Option<Icon>,
    items: Vec<MenuItem>,
}

/// A button that opens a menu of `items` where the pointer is pressed.
pub fn menu_button(label: impl Into<String>, items: Vec<MenuItem>) -> MenuButton {
    MenuButton {
        label: label.into(),
        icon: None,
        items,
    }
}

impl MenuButton {
    /// Show `icon` alone in a square button, as an icon button. The label
    /// stays the accessible name.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
}

impl Component for MenuButton {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = use_overlays();
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let mut area = use_state(Area::default);
        let items = self.items.clone();
        let open = overlays.menu_owned_by(a11y_id);
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .maybe(self.icon.is_none(), |button| {
                button.tw(
                    &tokens,
                    &[
                        h(Dim::Control),
                        min_w(Dim::Box9),
                        px(S3),
                        gap(S2),
                        flex_row(),
                    ],
                )
            })
            .maybe(self.icon.is_some(), |button| {
                button
                    .tw(&tokens, &[w(Dim::Button), h(Dim::Button)])
                    .a11y_alt(self.label.clone())
            })
            .tw(
                &tokens,
                &[
                    center(),
                    rounded(if self.icon.is_some() {
                        Radius::Pill
                    } else {
                        Radius::Control
                    }),
                    surface(Paint::Control, hovering() || open, pressing(), false),
                ],
            )
            .on_sized(move |event: Event<SizedEventData>| {
                area.set_if_modified(event.area);
            })
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            .on_pointer_down({
                let items = items.clone();
                move |event: Event<PointerEventData>| {
                    event.stop_propagation();
                    pressing.set(true);
                    let at = global_point(&event);
                    overlays.show_menu_from(Some(a11y_id), at, items.clone(), false);
                }
            })
            .on_global_pointer_up(move |_| pressing.set_if_modified(false))
            .on_key_down(move |event: Event<KeyboardEventData>| {
                let opens =
                    is_activation_key(&event.key) || event.key == Key::Named(NamedKey::ArrowDown);
                if !opens {
                    return;
                }
                event.stop_propagation();
                let below = area.read().clone();
                overlays.show_menu_from(
                    Some(a11y_id),
                    (below.min_x(), below.max_y()),
                    items.clone(),
                    true,
                );
            })
            .maybe_child(self.icon.is_none().then(|| {
                BODY.in_ink(Ink::Text)
                    .label(&tokens, self.label.clone())
                    .max_lines(1)
                    .into_element()
            }))
            .maybe_child(
                self.icon
                    .map(|icon| icons::icon(icon, IconSize::Control, Ink::Text.color(&tokens))),
            )
            .maybe_child(
                self.icon.is_none().then(|| {
                    icons::icon(Icon::PanDown, IconSize::Mark, Paint::Muted.color(&tokens))
                }),
            )
            .focusable(focus(), Radius::Control)
            .into_element()
    }
}
