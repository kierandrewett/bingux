//! The file grid: tiles with a picture of each image and an icon for the rest.
//!
//! Tiles are built row by row as they scroll into view, and each picture is
//! decoded only then, at twice the tile's size so it stays sharp on a HiDPI
//! screen. A picture that cannot be decoded shows its icon. The keys and
//! the pointer work as in the [`FileList`](super::FileList), with Left and
//! Right moving one tile and Up and Down one row of tiles.

use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use bingux_files::selection::TypeAhead;
use bingux_files::{Category, Entry, Listing, Selection};
use freya::prelude::*;
use freya::torin::prelude::Size2D;

use super::file_list::{Behaviour, Rows, offset_for};
use super::glyphs::entry_icon;
use crate::component::ComponentName;
use crate::icons::{self, Icon};
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::BODY;

/// The share of a tile's width its picture is tall.
const THUMB_RATIO: f32 = 0.68;
/// A picture is decoded at this many times the size it is drawn.
const DECODE_SCALE: f32 = 2.0;

/// A grid of tiles; see the module documentation.
#[derive(PartialEq)]
pub struct FileGrid {
    listing: Arc<Listing>,
    order: Arc<Vec<usize>>,
    selection: State<Selection>,
    multiple: bool,
    folders_only: bool,
    autofocus: bool,
    on_pick: EventHandler<usize>,
    on_activate: EventHandler<usize>,
}

impl FileGrid {
    pub fn new(listing: Arc<Listing>, order: Arc<Vec<usize>>, selection: State<Selection>) -> Self {
        FileGrid {
            listing,
            order,
            selection,
            multiple: false,
            folders_only: false,
            autofocus: false,
            on_pick: EventHandler::new(|_: usize| {}),
            on_activate: EventHandler::new(|_: usize| {}),
        }
    }

    pub fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }

    /// Dim the files and make only folders selectable.
    pub fn folders_only(mut self, folders_only: bool) -> Self {
        self.folders_only = folders_only;
        self
    }

    /// Take keyboard focus when the grid appears; see `FileList::autofocus`.
    pub fn autofocus(mut self, autofocus: bool) -> Self {
        self.autofocus = autofocus;
        self
    }

    /// A tile was picked. Gets the entry's index in the listing.
    pub fn on_pick(mut self, handler: impl Into<EventHandler<usize>>) -> Self {
        self.on_pick = handler.into();
        self
    }

    /// A tile was opened. Gets the entry's index in the listing.
    pub fn on_activate(mut self, handler: impl Into<EventHandler<usize>>) -> Self {
        self.on_activate = handler.into();
        self
    }
}

/// The measurements of one layout of the grid.
#[derive(Clone, Copy, PartialEq)]
struct Metrics {
    columns: usize,
    tile: f32,
    thumb: f32,
    gap: f32,
    /// A full row of tiles with its caption and the gap under it.
    row: f32,
}

impl Metrics {
    fn for_width(width: f32) -> Metrics {
        let gap = S3.px();
        let pad = S3.px();
        let minimum = Dim::GridTile.px();
        let inner = (width - 2.0 * pad).max(minimum);
        let columns = (((inner + gap) / (minimum + gap)) as usize).max(1);
        let tile = (inner - gap * (columns as f32 - 1.0)) / columns as f32;
        let thumb = (tile * THUMB_RATIO).floor();
        Metrics {
            columns,
            tile: tile.floor(),
            thumb,
            gap,
            row: thumb + Dim::Box7.px() + gap,
        }
    }
}

#[derive(Clone, PartialEq)]
struct GridData {
    behaviour: Behaviour,
    selection: Selection,
    metrics: Metrics,
}

impl Component for FileGrid {
    fn render_key(&self) -> DiffKey {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.listing.path.hash(&mut hasher);
        self.listing.recent.hash(&mut hasher);
        DiffKey::U64(hasher.finish())
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let autofocus = self.autofocus;
        use_side_effect(move || {
            if autofocus {
                a11y_id.request_focus();
            }
        });
        let scroll = use_scroll_controller(ScrollConfig::default);
        let mut viewport = use_state(Area::default);
        let modifiers = use_state(Modifiers::default);
        let last_click = use_state(|| None::<(Instant, usize)>);
        let typed = use_hook(|| Rc::new(RefCell::new(TypeAhead::new())));
        let selection = self.selection;
        let width = viewport.read().width();
        let metrics = Metrics::for_width(width);

        let rows = Rows {
            listing: self.listing.clone(),
            order: self.order.clone(),
        };
        let behaviour = Behaviour {
            rows: rows.clone(),
            selection,
            multiple: self.multiple,
            folders_only: self.folders_only,
            modifiers,
            last_click,
            on_pick: self.on_pick.clone(),
            on_activate: self.on_activate.clone(),
        };

        let on_key_down = {
            let behaviour = behaviour.clone();
            let typed = typed.clone();
            let mut scroll = scroll;
            move |event: Event<KeyboardEventData>| {
                let count = behaviour.rows.len();
                let columns = Metrics::for_width(viewport.peek().width()).columns;
                let extend = event.modifiers.contains(Modifiers::SHIFT);
                let ctrl = event.modifiers.contains(Modifiers::CONTROL);
                let last = count.saturating_sub(1);
                let from = behaviour.cursor_row();
                let visible_rows = ((viewport.peek().height()
                    / Metrics::for_width(viewport.peek().width()).row)
                    as usize)
                    .max(1);
                let target = match &event.key {
                    Key::Named(NamedKey::ArrowRight) => Some(from.map_or(0, |at| at + 1)),
                    Key::Named(NamedKey::ArrowLeft) => {
                        Some(from.map_or(last, |at| at.saturating_sub(1)))
                    }
                    Key::Named(NamedKey::ArrowDown) => Some(from.map_or(0, |at| at + columns)),
                    Key::Named(NamedKey::ArrowUp) => {
                        Some(from.map_or(last, |at| at.saturating_sub(columns)))
                    }
                    Key::Named(NamedKey::PageDown) => {
                        Some(from.map_or(0, |at| at + columns * visible_rows))
                    }
                    Key::Named(NamedKey::PageUp) => {
                        Some(from.map_or(last, |at| at.saturating_sub(columns * visible_rows)))
                    }
                    Key::Named(NamedKey::Home) => Some(0),
                    Key::Named(NamedKey::End) => Some(last),
                    Key::Named(NamedKey::Enter) => {
                        if let Some(row) = from {
                            event.stop_propagation();
                            behaviour.on_activate.call(behaviour.rows.order[row]);
                        }
                        return;
                    }
                    Key::Character(text) if ctrl && text.eq_ignore_ascii_case("a") => {
                        event.stop_propagation();
                        behaviour.select_all();
                        return;
                    }
                    Key::Character(text)
                        if !ctrl
                            && !event.modifiers.contains(Modifiers::ALT)
                            && text.chars().count() == 1
                            && text != " "
                            && text != "/"
                            && text != "~" =>
                    {
                        let prefix = typed
                            .borrow_mut()
                            .push(text.chars().next().unwrap_or(' '), Instant::now())
                            .to_string();
                        event.stop_propagation();
                        behaviour.type_ahead(&prefix)
                    }
                    _ => None,
                };
                let Some(target) = target.filter(|_| count > 0) else {
                    return;
                };
                event.stop_propagation();
                let target = target.min(last);
                if !behaviour.selectable(behaviour.rows.entry(target)) {
                    return;
                }
                behaviour.go_to(target, extend);
                let metrics = Metrics::for_width(viewport.peek().width());
                let offset = offset_for(
                    target / metrics.columns,
                    metrics.row,
                    <(i32, i32)>::from(scroll).1,
                    count.div_ceil(metrics.columns),
                    viewport.peek().height(),
                );
                scroll.scroll_to_y(offset);
            }
        };

        let data = GridData {
            behaviour,
            selection: selection.read().clone(),
            metrics,
        };
        let lines = self.order.len().div_ceil(metrics.columns);
        let body = VirtualScrollView::new_with_data_controlled(
            data,
            |item, data: &GridData| {
                let metrics = data.metrics;
                let first = item.index * metrics.columns;
                let last = (first + metrics.columns).min(data.behaviour.rows.len());
                let tiles: Vec<Element> = (first..last)
                    .map(|row| {
                        let entry = data.behaviour.rows.entry(row).clone();
                        let behaviour = data.behaviour.clone();
                        Element::from(Tile {
                            selected: data.selection.contains(&entry.path),
                            enabled: data.behaviour.selectable(&entry),
                            entry,
                            row,
                            metrics,
                            on_press: EventHandler::new(move |touch: bool| {
                                behaviour.pressed(row, touch)
                            }),
                        })
                    })
                    .collect();
                rect()
                    .key(item.index)
                    .tw_layout(&[flex_row(), w_full(), px(S3), gap(S3)])
                    .height(Size::px(item.size))
                    .children(tiles)
                    .into()
            },
            scroll,
        )
        .length(lines)
        .item_size(metrics.row)
        .drag_scrolling(is_compact())
        .on_sized(move |event: Event<SizedEventData>| viewport.set(event.area));

        rect()
            .component("FileGrid")
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::List)
            .a11y_alt("Files")
            .tw(&tokens, &[flex_col(), w_full(), h_full()])
            .on_key_down(on_key_down)
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                let mut held = modifiers;
                held.set(event.modifiers);
            })
            .on_global_key_up(move |event: Event<KeyboardEventData>| {
                let mut held = modifiers;
                held.set(event.modifiers);
            })
            .child(
                rect()
                    .tw(&tokens, &[w_full(), h_flex(), pt(S3)])
                    .child(body),
            )
            .focusable(focus(), Radius::Control)
            .tw(&tokens, &[w_full(), h_full()])
    }
}

#[derive(PartialEq)]
struct Tile {
    entry: Entry,
    row: usize,
    selected: bool,
    enabled: bool,
    metrics: Metrics,
    on_press: EventHandler<bool>,
}

/// Whether a file is worth trying to draw as a picture.
fn has_picture(entry: &Entry) -> bool {
    entry.category() == Category::Image
}

impl Component for Tile {
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(self.row as u64)
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let entry = &self.entry;
        let on_press = self.on_press.clone();
        let muted = Paint::Muted.color(&tokens);
        let glyph = |icon: Icon| {
            rect()
                .tw_layout(&[w_full(), h_full(), center()])
                .child(icons::icon(icon, IconSize::Hero, muted))
        };
        let face: Element = if has_picture(entry) {
            let fallback = glyph(entry_icon(entry));
            ImageViewer::new(PathBuf::from(&entry.path))
                .decode_mode(DecodeMode::Custom(Size2D::new(
                    self.metrics.tile * DECODE_SCALE,
                    self.metrics.thumb * DECODE_SCALE,
                )))
                .width(Size::fill())
                .height(Size::fill())
                .aspect_ratio(AspectRatio::Max)
                .loading_placeholder(rect().tw(&tokens, &[w_full(), h_full(), bg(Paint::Control)]))
                .error_renderer(move |_: String| fallback.clone().into())
                .into()
        } else {
            glyph(entry_icon(entry)).into()
        };
        let frame = rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    rounded(Radius::Control),
                    clip(),
                    surface(Paint::Control, hovering() && self.enabled, false, false),
                    border(if self.selected {
                        Line::Stroke(Dim::Ring, Paint::Accent)
                    } else {
                        Line::Outline
                    }),
                ],
            )
            .height(Size::px(self.metrics.thumb))
            .child(face)
            .maybe_child(self.selected.then(|| {
                rect()
                    .position(Position::new_absolute().top(S1.px()).right(S1.px()))
                    .tw(
                        &tokens,
                        &[
                            w(Dim::Box5),
                            h(Dim::Box5),
                            rounded(Radius::Pill),
                            bg(Paint::Accent),
                            center(),
                        ],
                    )
                    .child(icons::icon(
                        Icon::Check,
                        IconSize::Mark,
                        Paint::OnAccent.color(&tokens),
                    ))
            }));
        rect()
            .component("Tile")
            .tw(&tokens, &[flex_col(), gap(S1)])
            .width(Size::px(self.metrics.tile))
            .a11y_role(AccessibilityRole::ListBoxOption)
            .a11y_alt(entry.name.clone())
            .a11y_builder({
                let selected = self.selected;
                move |builder| builder.set_selected(selected)
            })
            .maybe(!self.enabled, |tile| tile.opacity(DISABLED_OPACITY))
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                on_press.call(matches!(event.data(), PressEventData::Touch(_)));
            })
            .child(frame)
            .child(
                rect()
                    .tw(&tokens, &[w_full(), h(Dim::Box7), items(Align::Center)])
                    .child(
                        BODY.label(&tokens, entry.name.clone())
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis)
                            .text_align(TextAlign::Center)
                            .width(Size::fill()),
                    ),
            )
    }
}
