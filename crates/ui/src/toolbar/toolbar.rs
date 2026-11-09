//! The toolbar: groups of items that fit the bar they are in.

use std::cell::RefCell;
use std::rc::Rc;

use freya::prelude::*;
use serde::{Deserialize, Serialize};

use super::group::linked_bar;
use super::item::{Action, Placement, ToolbarGroup, ToolbarItem, toolbar_group};
use super::layout::{Slot, fit};
use crate::component::ComponentName;
use crate::controls::button::{action_button, icon_button};
use crate::controls::direction::use_direction;
use crate::controls::path_label::text_width;
use crate::focus::focus_node;
use crate::icons::Icon;
use crate::saved;
use crate::shell::panel::{PanelRect, PanelTrigger};
use crate::shell::{
    HeaderBar, MenuItem, Overlays, WindowControls, menu_item, menu_separator, submenu,
};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, CAPTION, EMPHASIS};

/// The room a header's title keeps when there are items to place.
const TITLE_MIN: f32 = 200.0;

/// What the person chose to hide, saved by the toolbar's id. A new item of an
/// update is not in it, so it shows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Customised {
    #[serde(default)]
    hidden: Vec<String>,
}

/// A bar of items. Build it with [`toolbar`].
#[derive(Clone, PartialEq)]
pub struct Toolbar {
    id: Option<&'static str>,
    groups: Vec<ToolbarGroup>,
    title: Option<String>,
    subtitle: Option<String>,
    header: bool,
    window_controls: bool,
    customizable: bool,
    label: String,
}

/// An empty toolbar. Add [`toolbar_group`]s with [`Toolbar::group`].
///
/// ```ignore
/// toolbar()
///     .id("main")
///     .customizable()
///     .group(toolbar_group(vec![
///         toolbar_item("back", "Back", Icon::GoPrevious, back).leading(),
///         toolbar_item("forward", "Forward", Icon::GoNext, forward).leading(),
///     ]).linked())
///     .group(toolbar_group(vec![
///         toolbar_item("add", "Add", Icon::ListAdd, add),
///         toolbar_item("share", "Share", Icon::DocumentOpen, share).secondary(),
///     ]))
/// ```
pub fn toolbar() -> Toolbar {
    Toolbar {
        id: None,
        groups: Vec::new(),
        title: None,
        subtitle: None,
        header: false,
        window_controls: false,
        customizable: false,
        label: "Toolbar".to_string(),
    }
}

impl Toolbar {
    pub fn group(mut self, group: ToolbarGroup) -> Self {
        self.groups.push(group);
        self
    }

    /// One item in a group of its own.
    pub fn item(self, item: ToolbarItem) -> Self {
        self.group(toolbar_group(vec![item]))
    }

    /// Names the toolbar in the app's saved state, where its customisation is
    /// kept. Needed by [`Toolbar::customizable`].
    pub fn id(mut self, id: &'static str) -> Self {
        self.id = Some(id);
        self
    }

    /// Lets the person choose which items show: a right click on the bar, or
    /// the overflow menu, lists them with ticks and a Reset. The choice is saved
    /// under the [`id`](Toolbar::id).
    pub fn customizable(mut self) -> Self {
        self.customizable = true;
        self
    }

    /// The window's title: short (about fifteen characters) and never the
    /// app's name. In a bar it sits between the clusters; in a
    /// [`header`](Toolbar::header) it is the header's title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// A muted line under the title, such as how many items there are.
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Draws the toolbar as the window's header bar: its leading items, the
    /// centred title and the trailing items, in one bar that drags the window.
    /// The items give way to the overflow menu before the title is cut.
    pub fn header(mut self) -> Self {
        self.header = true;
        self
    }

    /// In a header, ends the bar with the window's minimise, maximise and close.
    pub fn window_controls(mut self) -> Self {
        self.window_controls = true;
        self
    }

    /// The toolbar's name for assistive technology.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Every item as a menu row, grouped: a toolbar item is also a command, so
    /// an app's menu can list the same ones.
    pub fn menu_items(&self) -> Vec<MenuItem> {
        let mut items = Vec::new();
        for group in &self.groups {
            let rows: Vec<MenuItem> = group
                .items
                .iter()
                .filter_map(ToolbarItem::menu_item)
                .collect();
            if rows.is_empty() {
                continue;
            }
            if !items.is_empty() {
                items.push(menu_separator());
            }
            items.extend(rows);
        }
        items
    }
}

/// An item with where it sits.
struct Placed<'a> {
    item: &'a ToolbarItem,
    group: usize,
    cluster: u8,
    linked: bool,
}

impl Toolbar {
    /// The items that are not hidden, leading cluster first.
    fn placed<'a>(&'a self, hidden: &[String]) -> Vec<Placed<'a>> {
        let mut leading = Vec::new();
        let mut trailing = Vec::new();
        for (index, group) in self.groups.iter().enumerate() {
            for item in &group.items {
                if self.customizable && hidden.iter().any(|id| id == item.id) {
                    continue;
                }
                let cluster = if item.placement == Placement::Leading {
                    0
                } else {
                    1
                };
                let placed = Placed {
                    item,
                    group: index,
                    cluster,
                    linked: group.linked,
                };
                if cluster == 0 {
                    leading.push(placed)
                } else {
                    trailing.push(placed)
                }
            }
        }
        leading.extend(trailing);
        leading
    }
}

fn item_width(item: &ToolbarItem) -> f32 {
    match &item.action {
        Action::Element { width, .. } => *width,
        _ if item.prominent => {
            let icon = if item.icon.is_some() {
                IconSize::Control.px() + S1.px()
            } else {
                0.0
            };
            text_width(&item.label, BODY) + icon + 2.0 * S3.px()
        }
        _ => Dim::Control.px(),
    }
}

fn rank(item: &ToolbarItem) -> Option<u8> {
    if item.prominent || matches!(item.action, Action::Element { .. }) {
        return None;
    }
    Some(match item.placement {
        Placement::Leading => 0,
        Placement::Primary => 1,
        Placement::Secondary => 2,
    })
}

/// A handle that changes the saved customisation from a menu row.
#[derive(Clone, Copy)]
struct Customise {
    state: State<Customised>,
    id: Option<&'static str>,
}

impl Customise {
    fn edit(self, change: impl FnOnce(&mut Customised)) {
        let mut state = self.state;
        let mut next = state.peek().clone();
        change(&mut next);
        if next != *state.peek() {
            state.set(next.clone());
            if let Some(id) = self.id {
                saved::save(&format!("toolbar.{id}"), &next);
            }
        }
    }
}

impl Toolbar {
    /// The menu of what to show: each item with a tick, and Reset.
    fn customise_items(&self, handle: Customise, hidden: &[String]) -> Vec<MenuItem> {
        let mut items: Vec<MenuItem> = self
            .groups
            .iter()
            .flat_map(|group| group.items.iter())
            .map(|item| {
                let shown = !hidden.iter().any(|id| id == item.id);
                let id = item.id;
                menu_item(item.label.clone(), move |()| {
                    handle.edit(|custom| {
                        if shown {
                            custom.hidden.push(id.to_string());
                        } else {
                            custom.hidden.retain(|hidden| hidden != id);
                        }
                    });
                })
                .checked(shown)
            })
            .collect();
        items.push(menu_separator());
        items.push(menu_item("Reset toolbar", move |()| {
            handle.edit(|custom| custom.hidden.clear());
        }));
        items
    }
}

impl Component for Toolbar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let direction = use_direction();
        let overlays = use_try_consume::<Overlays>();
        let mut width = use_state(|| 0.0_f32);
        let id = self.id;
        let custom = use_state(move || {
            id.and_then(|id| saved::load::<Customised>(&format!("toolbar.{id}")))
                .unwrap_or_default()
        });
        let current = use_state(|| 0_usize);
        let want = use_state(|| None::<AccessibilityId>);
        let ids = use_hook(|| Rc::new(RefCell::new(Vec::<(&'static str, AccessibilityId)>::new())));
        let handle = Customise { state: custom, id };
        let hidden = custom.read().hidden.clone();

        let id_for = |item: &'static str| {
            let mut ids = ids.borrow_mut();
            match ids.iter().find(|(known, _)| *known == item) {
                Some((_, id)) => *id,
                None => {
                    let created = AccessibilityId::new_unique();
                    ids.push((item, created));
                    created
                }
            }
        };

        // ---- what fits ------------------------------------------------------
        let placed = self.placed(&hidden);
        let slots: Vec<Slot> = placed
            .iter()
            .map(|at| Slot {
                width: item_width(at.item),
                rank: rank(at.item),
                group: at.group,
                cluster: at.cluster,
            })
            .collect();
        let controls = if self.header && self.window_controls {
            3.0 * Dim::WindowControl.px() + 2.0 * S1.px() + S2.px()
        } else {
            0.0
        };
        let title_room = if self.header && self.title.is_some() {
            TITLE_MIN
        } else {
            0.0
        };
        // The bar's own padding, the gaps between its parts and some slack for
        // the widths that are measured by estimate.
        let available = (width() - 6.0 * S2.px() - 12.0 - title_room - controls).max(0.0);
        let shown = if width() <= 0.0 {
            vec![true; slots.len()]
        } else {
            fit(&slots, available, S1.px(), S3.px(), Dim::Control.px())
        };
        let overflowing = shown.iter().any(|shown| !shown);

        // ---- the keyboard stop: one button holds the Tab stop ---------------
        let stops: Vec<&'static str> = placed
            .iter()
            .zip(&shown)
            .filter(|(at, shown)| {
                **shown
                    && at.item.enabled
                    && !at.item.prominent
                    && !matches!(at.item.action, Action::Element { .. })
            })
            .map(|(at, _)| at.item.id)
            .collect();
        let held = (*current.read()).min(stops.len().saturating_sub(1));
        let stop_of = |item: &'static str| stops.iter().position(|known| *known == item);

        // ---- the items --------------------------------------------------------
        let build = |at: &Placed| -> Element {
            let item = at.item;
            let a11y_id = id_for(item.id);
            let stop = stop_of(item.id);
            let roving_here = stop == Some(held);
            let pointer_only = stop.is_some() && !roving_here;
            let index = stop.unwrap_or(0);
            let take = EventHandler::new(move |()| {
                let mut current = current;
                current.set(index);
            });
            match &item.action {
                Action::Element { element, .. } => element.clone(),
                Action::Press { on_press, .. } if item.prominent => {
                    let on_press = on_press.clone();
                    let mut button = action_button(item.label.clone(), move |()| on_press.call(()))
                        .suggested()
                        .compact()
                        .enabled(item.enabled);
                    if let Some(icon) = item.icon {
                        button = button.icon(icon);
                    }
                    button.into()
                }
                Action::Press { on_press, toggled } => {
                    let on_press = on_press.clone();
                    let mut button = icon_button(
                        item.icon.unwrap_or(Icon::ViewMore),
                        EventHandler::new(move |()| {
                            take.call(());
                            on_press.call(());
                        }),
                    )
                    .tooltip(&item.label)
                    .size(Dim::Control)
                    .active(toggled.unwrap_or(false))
                    .enabled(item.enabled)
                    .a11y_id(a11y_id);
                    if pointer_only {
                        button = button.pointer_only();
                    }
                    button.into()
                }
                Action::Menu(items) => MenuStop {
                    label: item.label.clone(),
                    icon: item.icon.unwrap_or(Icon::ViewMore),
                    items: items.clone(),
                    enabled: item.enabled,
                    a11y_id,
                    pointer_only,
                    take,
                }
                .into(),
            }
        };

        let cluster = |which: u8| -> Option<Element> {
            let mut runs: Vec<(usize, bool, Vec<Element>)> = Vec::new();
            for (at, _) in placed
                .iter()
                .zip(&shown)
                .filter(|(at, shown)| **shown && at.cluster == which)
            {
                let element = build(at);
                match runs.last_mut() {
                    Some((group, _, elements)) if *group == at.group => elements.push(element),
                    _ => runs.push((at.group, at.linked, vec![element])),
                }
            }
            if runs.is_empty() {
                return None;
            }
            let groups: Vec<Element> = runs
                .into_iter()
                .map(|(_, linked, elements)| {
                    if linked && elements.len() > 1 {
                        linked_bar(&tokens, elements)
                    } else {
                        rect()
                            .tw(&tokens, &[flex_row(), items(Align::Center), gap(S1)])
                            .children(elements)
                            .into()
                    }
                })
                .collect();
            Some(
                rect()
                    .tw(&tokens, &[flex_row(), items(Align::Center), gap(S3)])
                    .children(groups)
                    .into(),
            )
        };

        // ---- the overflow menu -----------------------------------------------
        let overflow = overflowing.then(|| {
            let mut rows: Vec<MenuItem> = Vec::new();
            let mut last_group = None;
            for (at, _) in placed.iter().zip(&shown).filter(|(_, shown)| !**shown) {
                let Some(row) = at.item.menu_item() else {
                    continue;
                };
                if last_group.is_some_and(|group| group != at.group) {
                    rows.push(menu_separator());
                }
                last_group = Some(at.group);
                rows.push(row);
            }
            if self.customizable {
                rows.push(menu_separator());
                rows.push(submenu(
                    "Customise toolbar",
                    self.customise_items(handle, &hidden),
                ));
            }
            Element::from(MenuStop {
                label: "More".to_string(),
                icon: Icon::ViewMore,
                items: rows,
                enabled: true,
                a11y_id: id_for("\u{0}overflow"),
                pointer_only: false,
                take: EventHandler::new(|()| {}),
            })
        });

        // ---- the roving stop ---------------------------------------------------
        let focus_ids: Vec<AccessibilityId> = stops.iter().map(|item| id_for(item)).collect();
        use_side_effect_with_deps(&held, move |_: &usize| {
            let mut want = want;
            if let Some(id) = want.write().take() {
                focus_node(id);
            }
        });
        let on_key = move |event: Event<KeyboardEventData>| {
            let towards = match direction.reading_key(&event.key) {
                Key::Named(NamedKey::ArrowRight) => 1_i32,
                Key::Named(NamedKey::ArrowLeft) => -1,
                Key::Named(NamedKey::Home) => i32::MIN,
                Key::Named(NamedKey::End) => i32::MAX,
                _ => return,
            };
            if focus_ids.is_empty() {
                return;
            }
            let last = focus_ids.len() as i32 - 1;
            let next = match towards {
                i32::MIN => 0,
                i32::MAX => last,
                step => (held as i32 + step).clamp(0, last),
            } as usize;
            event.stop_propagation();
            if next != held {
                let mut current = current;
                let mut want = want;
                current.set(next);
                want.set(Some(focus_ids[next]));
            }
        };

        let customise_menu = self
            .customizable
            .then(|| self.customise_items(handle, &hidden));
        let on_context = move |event: Event<PointerEventData>| {
            let PointerEventData::Mouse(mouse) = event.data() else {
                return;
            };
            if mouse.button != Some(MouseButton::Right) {
                return;
            }
            if let (Some(items), Some(overlays)) = (&customise_menu, overlays) {
                event.stop_propagation();
                let at = event.global_location();
                overlays.show_menu((at.x as f32, at.y as f32), items.clone());
            }
        };

        // ---- the bar ------------------------------------------------------------
        let leading = cluster(0);
        let trailing = cluster(1);
        let title_block: Option<Element> = self.title.as_ref().map(|title| {
            rect()
                .tw(&tokens, &[flex_col(), items(Align::Center), w_full()])
                .child(
                    EMPHASIS
                        .label(&tokens, title.clone())
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis)
                        .text_align(TextAlign::Center),
                )
                .maybe_child(self.subtitle.as_ref().map(|subtitle| {
                    CAPTION
                        .label(&tokens, subtitle.clone())
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis)
                        .text_align(TextAlign::Center)
                }))
                .into()
        });

        let label = self.label.clone();
        if self.header {
            let mut bar = HeaderBar::new(self.title.clone().unwrap_or_default());
            if self.subtitle.is_some()
                && let Some(block) = title_block
            {
                bar = bar.title_slot(block);
            }
            if let Some(leading) = leading {
                bar = bar.leading(leading);
            }
            if let Some(trailing) = trailing {
                bar = bar.trailing(trailing);
            }
            if let Some(overflow) = overflow {
                bar = bar.trailing(overflow);
            }
            if self.window_controls {
                bar = bar.trailing(WindowControls);
            }
            return rect()
                .component("Toolbar")
                .a11y_role(AccessibilityRole::Toolbar)
                .a11y_alt(label)
                .tw(&tokens, &[w_full()])
                .on_sized(move |event: Event<SizedEventData>| {
                    width.set_if_modified(event.area.width())
                })
                .on_key_down(on_key)
                .on_pointer_down(on_context)
                .child(bar)
                .into_element();
        }

        rect()
            .component("Toolbar")
            .a11y_role(AccessibilityRole::Toolbar)
            .a11y_alt(label)
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    gap(S2),
                    w_full(),
                    h(Dim::Header),
                    px(S2),
                ],
            )
            .on_sized(move |event: Event<SizedEventData>| width.set_if_modified(event.area.width()))
            .on_key_down(on_key)
            .on_pointer_down(on_context)
            .maybe_child(leading)
            .child(
                rect()
                    .tw(&tokens, &[w_flex(), flex_row(), center()])
                    .maybe_child(title_block),
            )
            .maybe_child(trailing)
            .maybe_child(overflow)
            .into_element()
    }
}

/// A button that opens a menu under itself. It takes a part in the toolbar's
/// roving focus.
#[derive(PartialEq)]
struct MenuStop {
    label: String,
    icon: Icon,
    items: Vec<MenuItem>,
    enabled: bool,
    a11y_id: AccessibilityId,
    pointer_only: bool,
    take: EventHandler<()>,
}

impl Component for MenuStop {
    fn render(&self) -> impl IntoElement {
        let overlays = use_try_consume::<Overlays>();
        let mut area = use_state(Area::default);
        let items = self.items.clone();
        let take = self.take.clone();
        let a11y_id = self.a11y_id;
        let mut button = icon_button(
            self.icon,
            EventHandler::new(move |()| {
                take.call(());
                if let Some(overlays) = overlays {
                    overlays.show_menu_from(
                        Some(a11y_id),
                        PanelTrigger::Control(PanelRect::from_area(&area.read())),
                        items.clone(),
                        false,
                    );
                }
            }),
        )
        .tooltip(&self.label)
        .size(Dim::Control)
        .enabled(self.enabled)
        .a11y_id(self.a11y_id);
        if self.pointer_only {
            button = button.pointer_only();
        }
        rect()
            .on_sized(move |event: Event<SizedEventData>| area.set_if_modified(event.area))
            .child(button)
    }
}
