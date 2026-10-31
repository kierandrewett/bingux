//! A popover menu of actions, opened at a point on the window: from a button,
//! from a right click, or from the keyboard. Picking an item closes the menu
//! and then runs its action.
//!
//! Every row answers the pointer and the keyboard the same way: hovering or
//! focusing it highlights it, a disabled row included in the hover (it is lit
//! by nothing, but it never leaves a dead zone). A panel with a tick or an
//! icon on any row reserves one leading column for all of them, so labels
//! line up. Radio groups use the same tick.
//!
//! An item with a submenu opens it on hover (after 150 ms, at once when a
//! sibling's is already open), on Right, Enter or Space, and on a click or a
//! tap. A pointer crossing from an open row toward its submenu keeps the
//! submenu open while it stays inside the safe triangle between the pointer
//! and the submenu's near edge. A submenu with no room on the right opens to
//! the left. Up, Down, Home and End move the highlight; Left or Escape steps
//! back one level; Escape at the top level or Tab closes the menu and returns
//! focus to its owner.
//!
//! Typing does not close the menu: the letters build a query, the rows that
//! match show their matched letters marked, the others dim, and a small pill
//! shows the query. Down, Up, Tab and Shift+Tab walk through the matches,
//! Enter picks one, and Escape clears the query before it closes the menu. The
//! rules are in [`super::menu_query`], the pointer and key state machine in
//! [`super::menu_nav`]; this file draws them.

use std::ops::{Deref, DerefMut};

use freya::prelude::*;

use crate::app_icon::{AppChoice, app_icon};
use crate::component::ComponentName;
use crate::controls::direction::{Direction, use_direction};
use crate::controls::path_label::path_label;
use crate::controls::press::{is_touch, use_press};
use crate::focus::step_focus;
use crate::icons::{self, Icon};
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::{is_activation_key, use_tokens};
use crate::typography::{BODY, CAPTION, Ink};

use super::menu_nav::{NavKey, NavState, Node, Outcome, RowKind, first_selectable, panel_items};
use super::menu_query::Mark;
use super::overlays::{Overlays, use_overlays};
use super::panel::{Panel, PanelKind, PanelRect, PanelTrigger, Side, menu_rect};
use super::type_ahead::{dimmed, marked_label, nav_key, now_ms, wake_after};

/// One action in a menu. Build with [`menu_item`] or [`submenu`].
#[derive(Clone, PartialEq)]
pub struct MenuItem {
    pub(crate) label: String,
    pub(crate) icon: Option<Icon>,
    /// The icon name of the application this row stands for, drawn full
    /// colour at [`IconSize::AppSmall`]. Set only by [`app_menu_item`].
    pub(crate) app: Option<String>,
    pub(crate) checked: Option<bool>,
    pub(crate) enabled: bool,
    pub(crate) destructive: bool,
    pub(crate) separator: bool,
    pub(crate) shortcut: Option<String>,
    pub(crate) submenu: Option<Vec<MenuItem>>,
    pub(crate) on_press: EventHandler<()>,
    pub(crate) on_middle: Option<EventHandler<()>>,
    /// The label is a name: cut in the middle when it does not fit.
    pub(crate) name: bool,
}

impl MenuItem {
    /// Shows an icon before the label. Menus are text: state is a tick
    /// ([`MenuItem::checked`]), and the file views' menus never carry icons.
    /// Use an icon only in a menu whose rows are things the icon names, such
    /// as the actions of a device or connection row in Settings, or the tabs
    /// of a tab list. An item with an icon and a tick shows the icon first
    /// and the tick at the end of the row.
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

    /// Shows a keyboard shortcut hint at the trailing edge, such as
    /// `Ctrl+C`. It is a hint only: the app handles the keys. A menu with any
    /// hint is `Dim::MenuWide`, so labels and hints share a row.
    pub fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    /// What a middle click on the row does instead of its action, such as
    /// opening the choice in a new tab. The menu closes first, as for a press.
    pub fn on_middle(mut self, on_middle: impl FnMut(()) + 'static) -> Self {
        self.on_middle = Some(EventHandler::new(on_middle));
        self
    }

    /// The label is a name (a tab's, a file's): too long for the row, it is cut
    /// in the middle, keeping its end, which is what tells two names apart, and
    /// the whole name is the tooltip. Without it a long label ends in an ellipsis.
    pub fn name(mut self) -> Self {
        self.name = true;
        self
    }
}

/// An action for a menu. `on_press` runs after the menu closes.
pub fn menu_item(label: impl Into<String>, on_press: impl FnMut(()) + 'static) -> MenuItem {
    MenuItem {
        label: label.into(),
        icon: None,
        app: None,
        checked: None,
        enabled: true,
        destructive: false,
        separator: false,
        shortcut: None,
        submenu: None,
        on_press: EventHandler::new(on_press),
        on_middle: None,
        name: false,
    }
}

/// A row for an application: its icon in the leading slot, then its name. A
/// placeholder of the same size holds the slot while the icon loads and an
/// icon the theme lacks shows the generic application glyph, so the row never
/// moves and is never empty. Every list of applications in a menu is built
/// with this or [`app_menu_items`].
pub fn app_menu_item(app: &AppChoice, on_press: impl FnMut(()) + 'static) -> MenuItem {
    MenuItem {
        app: Some(app.icon.clone()),
        ..menu_item(app.label.clone(), on_press)
    }
}

/// The rows of a menu, a submenu or a popover that lists applications, each
/// with its icon. `on_pick` receives the application that was chosen.
pub fn app_menu_items(
    apps: impl IntoIterator<Item = AppChoice>,
    on_pick: impl Fn(&AppChoice) + Clone + 'static,
) -> Vec<MenuItem> {
    apps.into_iter()
        .map(|app| {
            let on_pick = on_pick.clone();
            let chosen = app.clone();
            app_menu_item(&app, move |_| on_pick(&chosen))
        })
        .collect()
}

/// The width of the leading column of a panel of `items`: none when no row
/// has a tick or an icon, the application icon's width when any row is an
/// application, the control icon's otherwise. Every row of the panel takes
/// the same width, so labels line up whatever mix of rows it holds.
pub(super) fn leading_width(items: &[MenuItem]) -> Option<f32> {
    let any = |item: &MenuItem| item.checked.is_some() || item.icon.is_some() || item.app.is_some();
    if !items.iter().any(any) {
        None
    } else if items.iter().any(|item| item.app.is_some()) {
        Some(IconSize::AppSmall.px())
    } else {
        Some(Dim::IconControl.px())
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
    pub(crate) trigger: PanelTrigger,
    pub(crate) items: Vec<MenuItem>,
}

impl Node for MenuItem {
    fn label(&self) -> &str {
        &self.label
    }

    fn selectable(&self) -> bool {
        self.enabled && !self.separator
    }

    fn children(&self) -> Option<&[Self]> {
        self.submenu.as_deref()
    }
}

/// The state of an open menu, and the control that opened it.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct MenuNav {
    /// The control that opened the menu, focused again when it closes from the
    /// keyboard.
    pub(crate) owner: Option<AccessibilityId>,
    state: NavState,
}

impl MenuNav {
    /// The state of a new menu. From the keyboard, the first item is highlighted.
    pub(crate) fn new(owner: Option<AccessibilityId>, items: &[MenuItem], keyboard: bool) -> Self {
        Self {
            owner,
            state: NavState::new(items, keyboard),
        }
    }
}

impl Deref for MenuNav {
    type Target = NavState;

    fn deref(&self) -> &NavState {
        &self.state
    }
}

impl DerefMut for MenuNav {
    fn deref_mut(&mut self) -> &mut NavState {
        &mut self.state
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

/// The height of a menu's rows, separators included, before its padding.
fn rows_height(items: &[MenuItem]) -> f32 {
    items.iter().map(row_height).sum()
}

/// The height of a menu of `items`: its rows and its top and bottom padding.
fn menu_height(items: &[MenuItem]) -> f32 {
    rows_height(items) + 2.0 * S2.px()
}

/// The width of a panel of `items`: wide when any item shows a shortcut.
fn panel_width(items: &[MenuItem]) -> f32 {
    if items.iter().any(|item| item.shortcut.is_some()) {
        Dim::MenuWide.px()
    } else {
        Dim::Menu.px()
    }
}

/// Where every open panel sits, each with its trigger: the root menu against
/// its own trigger, then each submenu beside the row that opened it, aligned
/// to that row's top. A [`Panel`] places itself from the same trigger.
fn layout(
    root: &[MenuItem],
    open: &[usize],
    trigger: PanelTrigger,
    window: (f32, f32),
    direction: Direction,
) -> Vec<(PanelTrigger, PanelRect)> {
    let width = panel_width(root);
    let height = menu_height(root);
    let mut panels = vec![(trigger, menu_rect(trigger, (width, height), window, direction))];
    for (level, &index) in open.iter().enumerate() {
        let parent = panels[level].1;
        let items = panel_items(root, open, level);
        let Some(item) = items.get(index) else {
            break;
        };
        let Some(sub) = item.submenu.as_deref() else {
            break;
        };
        let row_top = parent.y + S2.px() + items.iter().take(index).map(row_height).sum::<f32>();
        // The row's top is lifted by the menu's top padding, so the submenu's
        // padding sits over the row's top edge.
        // A submenu goes the way its parent went: when the parent opened to
        // the left of its own parent, this one does too, so it never lies
        // over the panels it came from. The first one opens towards the end of the
        // reading direction: the left in a window that reads from the right.
        let went = (level > 0).then(|| parent.x.total_cmp(&panels[level - 1].1.x));
        let side = match went {
            Some(std::cmp::Ordering::Less) => Side::Left,
            Some(std::cmp::Ordering::Greater) => Side::Right,
            _ if direction.is_rtl() => Side::Left,
            _ => Side::Right,
        };
        let row = PanelTrigger::Row(
            PanelRect {
                x: parent.x,
                y: row_top - S2.px(),
                width: parent.width,
                height: row_height(item),
            },
            side,
        );
        let rect = menu_rect(row, (panel_width(sub), menu_height(sub)), window, direction);
        panels.push((row, rect));
    }
    panels
}

/// The pointer of an event, in the coordinates the menu was opened in: a row in a popup window
/// reports its position in that window, and the menu's geometry is the opener's.
fn global_point(event: &Event<PointerEventData>) -> (f32, f32) {
    let at = event.global_location();
    crate::popup_windows::to_opener_space((at.x as f32, at.y as f32))
}

fn read_nav(overlays: Overlays) -> MenuNav {
    overlays.menu_nav.read().clone()
}

fn update_nav(mut overlays: Overlays, change: impl FnOnce(&mut MenuNav)) {
    let mut nav = overlays.menu_nav.write();
    change(&mut nav);
}

/// Wakes the menu when its next timed change is due: a submenu waiting for
/// its delay, a rest in the safe triangle, a typed query that ran out. Every
/// event that can start one calls this; a wake-up that finds nothing due
/// does nothing, so extra ones are harmless.
fn schedule(overlays: Overlays) {
    let Some(at) = overlays.menu_nav.peek().wake_at() else {
        return;
    };
    wake_after(at.saturating_sub(now_ms()), move |now| {
        let due = overlays
            .menu_nav
            .peek()
            .wake_at()
            .is_some_and(|at| at <= now);
        if due {
            update_nav(overlays, |nav| {
                nav.poll(now);
            });
            schedule(overlays);
        }
    });
}

/// A pointer entered row `index` of panel `level`.
fn pointer_enter(overlays: Overlays, level: usize, index: usize, kind: RowKind, point: (f32, f32)) {
    update_nav(overlays, |nav| nav.enter(level, index, kind, point, now_ms()));
    schedule(overlays);
}

/// A pointer left row `index` of panel `level`. `sub` is the row's open
/// submenu, if it has one.
fn pointer_leave(
    overlays: Overlays,
    level: usize,
    index: usize,
    point: (f32, f32),
    sub: Option<PanelRect>,
) {
    update_nav(overlays, |nav| nav.leave(level, index, point, sub, now_ms()));
    schedule(overlays);
}

/// The pointer moved anywhere over the menu. Only a pointer crossing toward
/// a submenu cares, so a move with no crossing changes nothing, and does not
/// draw the menu again.
fn pointer_moved(overlays: Overlays, point: (f32, f32)) {
    if !overlays.menu_nav.peek().crossing() {
        return;
    }
    update_nav(overlays, |nav| nav.moved(point, now_ms()));
    schedule(overlays);
}

/// Escape, heard anywhere in the window. It takes back a typed query, then
/// closes one submenu, then closes the menu and returns focus to its owner.
fn escape_menu(overlays: Overlays, root: &[MenuItem]) {
    let mut state = overlays.menu_nav;
    let outcome = state.write().press(root, NavKey::Escape);
    schedule(overlays);
    if outcome == Outcome::Dismiss {
        overlays.dismiss_menu_by_key();
    }
}

/// A key was pressed while the menu was open. Keys act on the active panel.
fn handle_key(overlays: Overlays, root: &[MenuItem], event: Event<KeyboardEventData>, direction: Direction) {
    // A key press is keyboard navigation, so the owner's focus ring shows
    // once the menu hands focus back.
    Platform::get()
        .current_window()
        .navigation_mode
        .set(NavigationMode::Keyboard);
    let Some(key) = nav_key(&event, direction) else {
        return;
    };
    // Escape is the panel's: it hears it anywhere in the window, so a menu
    // with the keyboard elsewhere still closes (see `escape_menu`).
    if key == NavKey::Escape {
        return;
    }
    let open = read_nav(overlays).open().to_vec();
    let mut state = overlays.menu_nav;
    let outcome = state.write().press(root, key);
    schedule(overlays);
    match outcome {
        Outcome::Ignored => {}
        Outcome::Handled => event.stop_propagation(),
        Outcome::Dismiss => {
            event.stop_propagation();
            overlays.dismiss_menu_by_key();
        }
        Outcome::Leave => {
            // Tab leaves the menu: focus returns to the owner (see
            // `close_menu`), and the same key then moves on from the owner
            // to the next stop, as Tab does from any control.
            event.stop_propagation();
            overlays.dismiss_menu_by_key();
            step_focus(event.modifiers.contains(Modifiers::SHIFT));
        }
        Outcome::Activate { level, index } => {
            event.stop_propagation();
            let Some(item) = panel_items(root, &open, level).get(index) else {
                return;
            };
            // Close first, so the action can open its own popup.
            overlays.dismiss_menu_by_key();
            item.on_press.call(());
        }
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
        let overlays = self.overlays;
        let a11y_id = overlays.menu_panel;
        // A closing menu has already handed focus back to its owner, so it
        // must not take it again when it renders its exit.
        let closing = self.closing;
        use_side_effect(move || {
            if !closing {
                a11y_id.request_focus();
            }
        });

        let nav = read_nav(overlays);
        // A menu drawn as popup windows is placed by the compositor, which knows the outputs:
        // its layout assumes no window to hold it in.
        let window = if crate::popup_windows::backend().is_some() {
            torin::prelude::Size2D::new(0.0, 0.0)
        } else {
            crate::surface::window_size()
        };
        let direction = use_direction();
        let panels = layout(
            &self.menu.items,
            nav.open(),
            self.menu.trigger,
            (window.width, window.height),
            direction,
        );
        let open = !self.closing;
        let active = nav.active();
        let items_at = |level: usize| panel_items(&self.menu.items, nav.open(), level);
        let rows = |level: usize| -> Vec<Element> {
            let items = items_at(level);
            // One leading column for every row of a panel that has a tick or
            // an icon anywhere, so the labels line up.
            let leading = leading_width(&items);
            items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let sub = (nav.open_row(level) == Some(index))
                        .then(|| panels.get(level + 1).map(|(_, rect)| *rect))
                        .flatten();
                    Element::from(MenuRow {
                        item: item.clone(),
                        overlays,
                        level,
                        index,
                        leading,
                        highlighted: nav.highlighted(level) == Some(index),
                        sub,
                        // The typed query belongs to the panel the keys act on.
                        mark: if level == active {
                            nav.ahead().mark(index)
                        } else {
                            Mark::Plain
                        },
                    })
                })
                .collect()
        };
        let pill = |level: usize| {
            if level == active {
                nav.ahead().pill()
            } else {
                None
            }
        };

        // Each open submenu is a nested panel, drawn over the root's shield.
        let submenus: Vec<Element> = panels
            .iter()
            .enumerate()
            .skip(1)
            .map(|(level, (trigger, _))| {
                Element::from(
                    Panel::new(PanelKind::Menu, open, *trigger, rows(level))
                        .size(panel_width(items_at(level)), rows_height(items_at(level)))
                        .pill(pill(level))
                        .nested(level),
                )
            })
            .collect();

        // The root panel takes the keyboard and the pointer's moves. Keys
        // reach the active panel through the menu state, so submenus need no
        // focus. Its shield closes the menu on a press outside it.
        let keys = self.menu.items.clone();
        let escape_keys = self.menu.items.clone();
        let root = Panel::new(PanelKind::Menu, open, self.menu.trigger, rows(0))
            .size(panel_width(items_at(0)), rows_height(items_at(0)))
            .pill(pill(0))
            .layer(self.layer)
            .on_dismiss(EventHandler::new(move |_: ()| overlays.close_menu()))
            .on_closed(EventHandler::new(move |_: ()| overlays.finish_menu()))
            .navigation(
                a11y_id,
                EventHandler::new(move |event: Event<KeyboardEventData>| {
                    handle_key(overlays, &keys, event, direction);
                }),
                EventHandler::new(move |point: (f32, f32)| pointer_moved(overlays, point)),
            )
            .on_escape(EventHandler::new(move |_: ()| {
                escape_menu(overlays, &escape_keys)
            }))
            .overlaid(submenus);
        Element::from(root)
    }
}

#[derive(Clone, PartialEq)]
struct MenuRow {
    item: MenuItem,
    overlays: Overlays,
    level: usize,
    index: usize,
    /// The width of the panel's leading column for ticks and icons, when it
    /// reserves one.
    leading: Option<f32>,
    highlighted: bool,
    /// The panel of this row's submenu, when that submenu is open.
    sub: Option<PanelRect>,
    /// How the row shows while a typed query is active.
    mark: Mark,
}

impl Component for MenuRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut pressing = use_state(|| false);
        let press = use_press();
        if self.item.separator {
            return rect()
                .tw(&tokens, &[w_full(), h(Dim::MenuSeparator), center()])
                .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
                .into_element();
        }
        let enabled = self.item.enabled;
        let has_sub = self.item.submenu.is_some();
        let kind = match (enabled, has_sub) {
            (false, _) => RowKind::Disabled,
            (true, true) => RowKind::Submenu,
            (true, false) => RowKind::Plain,
        };
        let overlays = self.overlays;
        let (level, index) = (self.level, self.index);
        let highlighted = self.highlighted;
        let sub = self.sub;
        let on_press = self.item.on_press.clone();
        let on_middle = self.item.on_middle.clone();
        let submenu_items = self.item.submenu.clone();
        // A disabled item keeps its colour and fades like any disabled
        // control, so it reads as unavailable at a glance. A row a typed
        // query does not match fades the same way, less.
        let ink = if enabled && self.item.destructive {
            Ink::Danger
        } else {
            Ink::Text
        };
        let text_color = ink.color(&tokens);
        let opacity = if enabled {
            dimmed(&self.mark)
        } else {
            Some(DISABLED_OPACITY)
        };
        // A tick for checked items; the icon otherwise. A panel with either
        // anywhere keeps the column on every row. An item with both shows its
        // icon here and its tick at the trailing edge.
        let ticked = self.item.checked == Some(true);
        let glyph = match (ticked, self.item.icon) {
            (true, None) => Some(Icon::Check),
            (_, icon) => icon,
        };
        let app = self.item.app.clone();
        let leading = self.leading.map(|width| {
            // A tick or an icon is centred in the column; an application icon
            // fills it.
            rect()
                .width(Size::px(width))
                .height(Size::px(width))
                .tw(&tokens, &[center()])
                .maybe_child(match app {
                    Some(name) => Some(app_icon(name, IconSize::AppSmall).into_element()),
                    None => glyph.map(|icon| icons::icon(icon, IconSize::Control, text_color).into_element()),
                })
        });
        // A submenu's arrow, or the tick of an item that also has an icon (the tab
        // list: each tab's icon, the selected one ticked at the end of its row).
        let trailing: Option<Element> = if has_sub {
            Some(icons::icon(Icon::PanEnd, IconSize::Control, text_color).into_element())
        } else if ticked && self.item.icon.is_some() {
            Some(icons::icon(Icon::Check, IconSize::Control, text_color).into_element())
        } else {
            None
        };
        let shortcut = self.item.shortcut.clone().map(|text| {
            CAPTION
                .label(&tokens, text)
                .max_lines(1)
        });
        // A name that is too long is cut in the middle, keeping its end, with the
        // whole name as the tooltip, as file names are; a row a typed query has
        // marked keeps its marks, so it is cut at the end.
        let direction = use_direction();
        let label: Element = match (&self.mark, self.item.name) {
            (Mark::Hit(_), _) | (_, false) => marked_label(&tokens, ink, &self.item.label, &self.mark),
            (_, true) => {
                let name = path_label(self.item.label.clone()).name().style(BODY).ink(ink);
                rect()
                    .width(Size::flex(1.0))
                    .child(if direction.is_rtl() { name.end() } else { name })
                    .into_element()
            }
        };
        // The row's pieces in reading order: the engine puts them on the right in an
        // interface that reads from the right.
        let mut parts: Vec<Element> = Vec::with_capacity(4);
        parts.extend(leading.map(Element::from));
        parts.push(label);
        parts.extend(shortcut.map(Element::from));
        parts.extend(trailing);
        rect()
            .component("MenuRow")
            .a11y_role(AccessibilityRole::MenuItem)
            .a11y_enabled(enabled)
            .tw(&tokens, MENU_ITEM)
            .maybe(opacity.is_some(), |element| {
                element.opacity(opacity.unwrap_or(1.0))
            })
            .tw(
                &tokens,
                &[surface(
                    Paint::Transparent,
                    highlighted,
                    enabled && pressing(),
                    false,
                )],
            )
            // Every row hears the pointer, a disabled one too: it takes no
            // highlight, but moving onto it still leaves the row before it.
            .on_pointer_enter(move |event: Event<PointerEventData>| {
                pointer_enter(overlays, level, index, kind, global_point(&event));
            })
            .on_pointer_leave(move |event: Event<PointerEventData>| {
                pressing.set_if_modified(false);
                pointer_leave(overlays, level, index, global_point(&event), sub);
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                event.stop_propagation();
                // A middle click runs the row's own alternative, when it has one.
                if let PointerEventData::Mouse(mouse) = event.data()
                    && mouse.button == Some(MouseButton::Middle)
                {
                    if enabled && let Some(middle) = &on_middle {
                        overlays.close_menu();
                        middle.call(());
                    }
                    return;
                }
                if enabled {
                    pressing.set(true);
                    press.begin(&event, None);
                }
            })
            .on_global_pointer_move(move |event: Event<PointerEventData>| {
                press.moved(&event);
            })
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                pressing.set(false);
                if !press.tap(&event) {
                    return;
                }
                if !enabled {
                    return;
                }
                // A click or a tap opens the submenu at once, with no delay:
                // it is how a touch screen reaches it.
                if let Some(items) = &submenu_items {
                    let first = first_selectable(items);
                    update_nav(overlays, |nav| nav.open_sub(level, index, first));
                    return;
                }
                // Close first, so the action can open its own popup.
                overlays.close_menu();
                on_press.call(());
            })
            .children(parts)
            .into_element()
    }
}

/// A button that opens a menu of its items, anchored to the button: below it
/// (above when there is no room), its edge aligned with the button's and
/// held inside the window. Opens on a press, or from the keyboard with
/// Enter, Space or Down. Build with [`menu_button`].
#[derive(Clone, PartialEq)]
pub struct MenuButton {
    label: String,
    icon: Option<Icon>,
    items: Vec<MenuItem>,
}

/// A button that opens a menu of `items` against itself.
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
        let press = use_press();
        let mut area = use_state(Area::default);
        let items = self.items.clone();
        let open = overlays.menu_owned_by(a11y_id);
        let size = crate::controls::button::header_size(Dim::Button);
        // The same rest state as an icon button: a bare glyph, filled only
        // while hovered, pressed or open. A labelled menu button is a control
        // and keeps its fill.
        let base = if self.icon.is_some() {
            Paint::Transparent
        } else {
            Paint::Control
        };
        rect()
            .component(if self.icon.is_some() { "IconMenuButton" } else { "MenuButton" })
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
                    .tw(&tokens, &[w(size), h(size)])
                    .a11y_alt(self.label.clone())
            })
            .tw(
                &tokens,
                &[
                    center(),
                    rounded(Radius::Control),
                    surface(base, hovering() || open, pressing(), false),
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
                    // A touch opens the menu when it lifts, so a finger that
                    // scrolls past the button does not open it.
                    if is_touch(&event) {
                        press.begin(&event, None);
                        return;
                    }
                    overlays.show_menu_from(
                        Some(a11y_id),
                        PanelTrigger::Control(PanelRect::from_area(&area.read())),
                        items.clone(),
                        false,
                    );
                }
            })
            .on_global_pointer_up(move |_| pressing.set_if_modified(false))
            .on_global_pointer_move(move |event: Event<PointerEventData>| press.moved(&event))
            .on_press({
                let items = items.clone();
                move |event: Event<PressEventData>| {
                    if !matches!(event.data(), PressEventData::Touch(_)) || !press.tap(&event) {
                        return;
                    }
                    overlays.show_menu_from(
                        Some(a11y_id),
                        PanelTrigger::Control(PanelRect::from_area(&area.read())),
                        items.clone(),
                        false,
                    );
                }
            })
            .on_key_down(move |event: Event<KeyboardEventData>| {
                let opens =
                    is_activation_key(&event.key) || event.key == Key::Named(NamedKey::ArrowDown);
                if !opens {
                    return;
                }
                event.stop_propagation();
                overlays.show_menu_from(
                    Some(a11y_id),
                    PanelTrigger::Control(PanelRect::from_area(&area.read())),
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
            ).focusable(focus())
            .into_element()
    }
}
