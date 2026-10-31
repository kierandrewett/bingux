//! The grid: tiles with a picture of each file that has one and an icon for
//! the rest.
//!
//! Tiles are built line by line as they scroll into view. A picture loads
//! only after its tile has stayed in view for a moment, and is decoded at a
//! bucketed size (see `thumbnails.rs`), so neither flicking through a folder
//! nor zooming decodes more than it must. A folder on a network mount shows
//! icons only.
//!
//! A tile is the rounded square of GNOME Files: the icon or the picture
//! (kept in its own proportions, never cropped) centred in the upper part,
//! and the name centred under it. The icon is `icon_px` square and the tile
//! grows with it (see `zoom::tile_width`); the name is the same text size as
//! the list's at every level. The fill is the sidebar row's: transparent at
//! rest, lighter when hovered, pressed or selected. The keys and the pointer
//! work as in the list, with Left and Right moving one tile and Up and Down
//! one line of tiles.
//!
//! Radii nest: a tile is [`Radius::Card`] (16 px) and its picture sits
//! `TILE_INSET` (12 px) inside, so its own corners are `Radius::Mark` (4 px).

use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use freya::prelude::*;

use super::file_list::{Settle, anchor_zoom, name_label, settle_zoom};
use super::interaction::Lattice;
use super::rename_field::RenameField;
use super::thumbnails::{Preview, face};
use super::view::{Ctx, Rows, Shared};
use super::zoom::{TILE_GAP, TILE_INSET, bucket_px, tile_width};
use crate::component::ComponentName;
use crate::style::*;
use crate::surface::use_tokens;

/// The measurements of one layout of the grid, in whole pixels.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Metrics {
    pub(super) columns: usize,
    pub(super) tile: f32,
    /// The icon or picture, a square of this edge.
    pub(super) icon: f32,
    pub(super) gap: f32,
    /// The height of the name's line.
    pub(super) caption: f32,
    /// A tile: the inset, the icon, a small gap, the name and the inset.
    pub(super) tile_height: f32,
    /// A line of tiles with the gap above it.
    pub(super) line: f32,
}

impl Metrics {
    /// The layout of tiles with icons of `icon_px` in `width` pixels. Every
    /// tile has the one width the icon asks for, and the line holds as many
    /// as fit between margins of one gap.
    pub(super) fn new(width: f32, icon_px: u16, caption: f32, name_gap: f32) -> Metrics {
        let gap = TILE_GAP;
        let tile = tile_width(icon_px);
        let inner = (width - 2.0 * gap).max(tile);
        let columns = (((inner + gap) / (tile + gap)) as usize).max(1);
        let icon = f32::from(icon_px);
        let tile_height = 2.0 * TILE_INSET + icon + name_gap + caption;
        Metrics {
            columns,
            tile,
            icon,
            gap,
            caption,
            tile_height,
            line: gap + tile_height,
        }
    }

    fn lattice(&self, count: usize) -> Lattice {
        Lattice {
            columns: self.columns,
            count,
            pad_x: self.gap,
            pad_y: self.gap,
            item_w: self.tile,
            item_h: self.tile_height,
            gap_x: self.gap,
            gap_y: self.gap,
        }
    }
}

/// The tiles of a grid. Mounted by the file view.
#[derive(PartialEq)]
pub(super) struct GridView {
    pub shared: Shared,
    pub rows: Rows,
    pub icon_px: u16,
}

#[derive(Clone, PartialEq)]
struct GridData {
    shared: Shared,
    rows: Rows,
    metrics: Metrics,
    icon_px: u16,
    bucket: u32,
}

impl Component for GridView {
    fn render_key(&self) -> DiffKey {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.rows.listing.path.hash(&mut hasher);
        self.rows.listing.recent.hash(&mut hasher);
        DiffKey::U64(hasher.finish())
    }

    fn render(&self) -> impl IntoElement {
        let ctx = self.shared.get();
        let mut viewport = ctx.viewport;
        let width = viewport.read().width();
        let metrics = Metrics::new(width, self.icon_px, Dim::Box7.px(), S1.px());
        let device_scale = *Platform::get().current_window().scale_factor.read() as f32;
        let bucket = bucket_px(metrics.icon, device_scale);
        ctx.lattice.set(Some(metrics.lattice(self.rows.len())));
        let settle: Settle = use_hook(|| Rc::new(Cell::new(None)));
        anchor_zoom(
            &ctx,
            f32::from(self.icon_px),
            metrics.line,
            metrics.columns,
            &settle,
        );

        let lines = self.rows.len().div_ceil(metrics.columns);
        let data = GridData {
            shared: self.shared.clone(),
            rows: self.rows.clone(),
            metrics,
            icon_px: self.icon_px,
            bucket,
        };
        VirtualScrollView::new_with_data_controlled(
            data,
            |item, data: &GridData| {
                let metrics = data.metrics;
                let first = item.index * metrics.columns;
                let last = (first + metrics.columns).min(data.rows.len());
                let mut tiles: Vec<Element> = Vec::with_capacity(2 * (last - first) + 1);
                tiles.push(rect().width(Size::px(metrics.gap)).into());
                for row in first..last {
                    tiles.push(
                        Tile {
                            shared: data.shared.clone(),
                            rows: data.rows.clone(),
                            row,
                            metrics,
                            icon_px: data.icon_px,
                            bucket: data.bucket,
                        }
                        .into(),
                    );
                    tiles.push(rect().width(Size::px(metrics.gap)).into());
                }
                rect()
                    .key(item.index)
                    .tw_layout(&[flex_col(), w_full()])
                    .height(Size::px(item.size))
                    .child(rect().height(Size::px(metrics.gap)))
                    .child(rect().tw_layout(&[flex_row(), w_full()]).children(tiles))
                    .into()
            },
            ctx.scroll,
        )
        .length(lines)
        .item_size(metrics.line)
        // The first frame has the lines and the scrollbar the grid had last.
        .initial_size(viewport.peek().size)
        .drag_scrolling(is_compact())
        .pass_ctrl_wheel(true)
        .on_sized({
            let scroll = ctx.scroll;
            move |event: Event<SizedEventData>| {
                viewport.set_if_modified(event.area);
                settle_zoom(scroll, &settle);
            }
        })
    }
}

/// The keys of the grid that move the cursor. Returns true when used.
pub(super) fn navigate(
    ctx: &Ctx,
    event: &Event<KeyboardEventData>,
    direction: crate::controls::direction::Direction,
) -> bool {
    let count = ctx.rows.len();
    if count == 0 {
        return false;
    }
    let lattice = ctx.lattice.get();
    let columns = lattice.map_or(1, |lattice| lattice.columns.max(1));
    let pitch = lattice.map_or(1.0, |lattice| (lattice.item_h + lattice.gap_y).max(1.0));
    let extend = event.modifiers.contains(Modifiers::SHIFT);
    let last = count - 1;
    let from = ctx.cursor_row();
    let visible_lines = ((ctx.viewport.peek().height() / pitch) as usize).max(1);
    // The grid reads in the interface's direction: the next cell is on the left in a window that reads
    // from the right.
    let key = direction.reading_key(&event.key);
    let target = match &key {
        Key::Named(NamedKey::ArrowRight) => from.map_or(0, |at| at + 1),
        Key::Named(NamedKey::ArrowLeft) => from.map_or(last, |at| at.saturating_sub(1)),
        Key::Named(NamedKey::ArrowDown) => from.map_or(0, |at| at + columns),
        Key::Named(NamedKey::ArrowUp) => from.map_or(last, |at| at.saturating_sub(columns)),
        Key::Named(NamedKey::PageDown) => from.map_or(0, |at| at + columns * visible_lines),
        Key::Named(NamedKey::PageUp) => {
            from.map_or(last, |at| at.saturating_sub(columns * visible_lines))
        }
        Key::Named(NamedKey::Home) => 0,
        Key::Named(NamedKey::End) => last,
        _ => return false,
    };
    let target = target.min(last);
    if ctx.selectable(ctx.rows.entry(target)) {
        ctx.go_to(target, extend);
        ctx.reveal(target);
    }
    true
}

#[derive(PartialEq)]
struct Tile {
    shared: Shared,
    rows: Rows,
    row: usize,
    metrics: Metrics,
    icon_px: u16,
    bucket: u32,
}

impl Component for Tile {
    fn render_key(&self) -> DiffKey {
        // A tile that reflows to another line when the column count changes
        // is a new tile: the old line and the new one are never both holding
        // the same key while the view diffs them.
        DiffKey::U64(((self.metrics.columns as u64) << 40) | self.row as u64)
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let ctx = self.shared.get();
        let Some(entry) = (self.row < self.rows.len()).then(|| self.rows.entry(self.row)) else {
            return rect().into_element();
        };
        let row = self.row;
        if !entry.sized {
            ctx.wants.want(&self.rows.listing, &[self.rows.order[row]]);
        }
        let metrics = self.metrics;
        let selected = ctx.env.selection.read().contains(&entry.path);
        let renaming = ctx
            .env
            .ui
            .rename
            .read()
            .as_ref()
            .is_some_and(|rename| rename.path == entry.path);
        let drop_target = entry.is_dir() && *ctx.gestures.over.read() == Some(row);
        let enabled = ctx.selectable(entry);
        let dim = !enabled || ctx.cut_dim(&entry.path);

        let preview = if ctx.thumbnails {
            Preview::Full
        } else {
            Preview::Off
        };
        let face = face(&tokens, entry, self.icon_px, self.bucket, preview, selected);
        let name_room = metrics.tile - 2.0 * S1.px();
        let caption: Element = if renaming {
            RenameField::new(&ctx.env, entry)
                .grid(true)
                .width(name_room)
                .into()
        } else {
            name_label(&tokens, &entry.name, name_room, true)
        };
        rect()
            .component("Tile")
            .tw(
                &tokens,
                &[
                    flex_col(),
                    items(Align::Center),
                    rounded(Radius::Card),
                    surface(
                        Paint::Transparent,
                        (hovering() && enabled) || drop_target,
                        pressing() && enabled,
                        selected,
                    ),
                ],
            )
            .maybe(drop_target, |tile| {
                tile.tw(&tokens, &[border(Line::Stroke(Dim::Ring, Paint::Accent))])
            })
            .width(Size::px(metrics.tile))
            .height(Size::px(metrics.tile_height))
            .a11y_role(AccessibilityRole::ListBoxOption)
            .a11y_alt(entry.name.clone())
            .a11y_builder(move |builder| builder.set_selected(selected))
            .maybe(dim, |tile| tile.opacity(DISABLED_OPACITY))
            .on_pointer_enter({
                let shared = self.shared.clone();
                move |event: Event<PointerEventData>| {
                    hovering.set(true);
                    shared.get().row_enter(row, &event);
                }
            })
            .on_pointer_leave({
                let shared = self.shared.clone();
                move |_| {
                    hovering.set(false);
                    pressing.set_if_modified(false);
                    shared.get().row_leave(row);
                }
            })
            .on_pointer_down({
                let shared = self.shared.clone();
                move |event: Event<PointerEventData>| {
                    pressing.set(true);
                    let ctx = shared.get();
                    if crate::controls::press::is_touch(&event) {
                        let menu = shared.clone();
                        ctx.press.begin(
                            &event,
                            Some(EventHandler::new(move |at: (f32, f32)| {
                                menu.get().row_menu(
                                    row,
                                    crate::shell::panel::PanelTrigger::Pointer(at),
                                    false,
                                );
                            })),
                        );
                    } else {
                        ctx.row_pointer_down(row, &event);
                    }
                }
            })
            .on_global_pointer_up(move |_| pressing.set_if_modified(false))
            .on_press({
                let shared = self.shared.clone();
                move |event: Event<PressEventData>| {
                    event.stop_propagation();
                    let ctx = shared.get();
                    let touch = matches!(event.data(), PressEventData::Touch(_));
                    if touch && !ctx.press.tap(&event) {
                        return;
                    }
                    ctx.pressed(row, touch);
                }
            })
            .child(rect().height(Size::px(TILE_INSET)))
            .child(face)
            .child(
                rect()
                    .tw(&tokens, &[items(Align::Center), pt(S1)])
                    .width(Size::px(name_room))
                    .height(Size::px(metrics.caption))
                    .child(caption),
            )
            .into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::zoom::{LADDER, ViewKind, kind_of};

    const CAPTION: f32 = 28.0;
    const NAME_GAP: f32 = 4.0;

    fn metrics(width: f32, px: u16) -> Metrics {
        Metrics::new(width, px, CAPTION, NAME_GAP)
    }

    #[test]
    fn the_tile_grows_with_the_icon_and_the_name_line_does_not() {
        let mut last = metrics(2000.0, 48);
        for px in [64, 96, 128, 192] {
            let next = metrics(2000.0, px);
            assert!(
                next.tile >= last.tile && next.tile_height > last.tile_height,
                "{px}"
            );
            assert_eq!(
                next.caption, last.caption,
                "the name's line is {px} px tall too"
            );
            assert_eq!(next.icon, f32::from(px));
            last = next;
        }
    }

    #[test]
    fn the_icon_sits_inside_the_tile_with_room_for_the_name() {
        for px in LADDER
            .into_iter()
            .filter(|px| kind_of(*px) == ViewKind::Grid)
        {
            let m = metrics(2000.0, px);
            assert!(m.icon + 2.0 * TILE_INSET <= m.tile, "{px}");
            assert_eq!(
                m.tile_height,
                2.0 * TILE_INSET + m.icon + NAME_GAP + CAPTION
            );
            assert_eq!(m.line, m.gap + m.tile_height);
        }
    }

    #[test]
    fn a_line_holds_as_many_tiles_as_fit_and_never_fewer_than_one() {
        let wide = metrics(1000.0, 96);
        assert_eq!(wide.tile, 120.0);
        // 1000 - 2 * 8 = 984; (984 + 8) / (120 + 8) = 7.75.
        assert_eq!(wide.columns, 7);
        assert!(wide.columns as f32 * (wide.tile + wide.gap) + wide.gap <= 1000.0);
        assert_eq!(metrics(10.0, 192).columns, 1);
        assert_eq!(metrics(0.0, 48).columns, 1);
    }

    #[test]
    fn every_width_the_cap_allows_holds_two_tiles() {
        for width in (200..1200).step_by(7) {
            let cap = crate::files::zoom::cap_for_width(width as f32);
            if kind_of(cap) == ViewKind::Grid {
                assert!(
                    metrics(width as f32, cap).columns >= 2,
                    "{width} px at {cap}"
                );
            }
        }
    }
}
