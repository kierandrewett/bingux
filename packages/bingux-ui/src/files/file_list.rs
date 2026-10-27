//! The list: rows of icon, name and columns.
//!
//! Only the rows in view are built. A row is one component with its own
//! key, and it holds no closure over the listing: its handlers read the
//! latest [`Ctx`](super::view::Ctx) through [`Shared`], so a row that did
//! not change is not drawn again.
//!
//! Zooming changes the icon and nothing else. The icon slot is `icon_px`
//! wide and tall; the row is as tall as the sidebar's rows (`Dim::NavItem`)
//! and grows only as far as the icon needs. The text, its size and the column
//! widths are the same at every level (see `columns.rs`). From
//! [`LIST_PICTURE_PX`] up, the slot of an image, video or document shows its
//! picture (see `thumbnails.rs`). The header is mounted by
//! [`FileView`](super::view::FileView), which makes the one [`Plan`] header
//! and rows share.
//!
//! A row is the sidebar's row: `LIST_ITEM`, the same radius, inset, fills and
//! minimum height, held `LIST_EDGE` clear of the list's sides so its rounded
//! fill shows. The press, though, is taken across the full width.
//!
//! On a phone the list has no header and two lines to a row, and a finger
//! scrolls it.

use std::cell::Cell;
use std::rc::Rc;
use std::time::SystemTime;

use bingux_files::Entry;
use freya::prelude::*;

use super::actions::Ui;
use super::columns::{
    CELL_PADDING, Column, ColumnLayout, DateStyle, Plan, cell, cell_text_styled, gutter, name_cell, plan_in,
    row_padding,
};
use super::interaction::Lattice;
use super::rename_field::RenameField;
use super::thumbnails::{LIST_PICTURE_PX, face};
use super::view::{Ctx, Rows, Shared};
use super::zoom::bucket_px;
use crate::component::ComponentName;
use crate::controls::path_label::{measure_text, path_label};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, CAPTION, TextStyle};

/// Numbers in a column share one width per digit, so a column of sizes lines
/// up.
const FIGURES: TextStyle = TextStyle {
    tabular: true,
    ..CAPTION
};

/// How many rows around the viewport a double click on a column edge
/// measures.
const FIT_SAMPLE: usize = 300;

/// The height of a list row whose icon is `icon_px`: the sidebar row's height
/// `nav_item`, or the icon and `pad` above and below it when that is more.
pub(super) fn row_height_for(icon_px: u16, nav_item: f32, pad: f32) -> f32 {
    nav_item.max(f32::from(icon_px) + 2.0 * pad)
}

/// [`row_height_for`] at the current density.
pub(super) fn row_height(icon_px: u16) -> f32 {
    row_height_for(icon_px, Dim::NavItem.px(), S1.px())
}

/// The rows of a list. Mounted by the file view.
#[derive(PartialEq)]
pub(super) struct ListView {
    pub shared: Shared,
    pub rows: Rows,
    pub icon_px: u16,
    pub layout: ColumnLayout,
    pub compact: bool,
}

/// What every row needs to draw.
#[derive(Clone, PartialEq)]
struct RowsData {
    shared: Shared,
    rows: Rows,
    icon_px: u16,
    /// The decode size of a row's picture, or 0 when rows show icons only.
    bucket: u32,
    plan: Plan,
    compact: bool,
    row_height: f32,
    /// The room the Name cell has, for deciding whether a name needs cutting.
    name_room: f32,
}

impl Component for ListView {
    fn render_key(&self) -> DiffKey {
        // A different folder is a new list, scrolled to where it was left.
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&self.rows.listing.path, &mut hasher);
        std::hash::Hash::hash(&self.rows.listing.recent, &mut hasher);
        DiffKey::U64(std::hash::Hasher::finish(&hasher))
    }

    fn render(&self) -> impl IntoElement {
        let ctx = self.shared.get();
        let icon_px = self.icon_px;
        let compact = self.compact;
        let row_height = row_height(icon_px);
        let mut viewport = ctx.viewport;
        let width = viewport.read().width();
        let row_width = width - 2.0 * row_padding();
        let plan = plan_in(&self.layout, f32::from(icon_px), row_width, ctx.location_column());
        ctx.lattice.set(Some(Lattice {
            columns: 1,
            count: self.rows.len(),
            pad_x: 0.0,
            pad_y: 0.0,
            item_w: width,
            item_h: row_height,
            gap_x: 0.0,
            gap_y: 0.0,
        }));
        anchor_zoom(&ctx, f32::from(icon_px), row_height, 1);

        let name_room = plan_name_room(&plan, row_width, icon_px);
        let bucket = if ctx.thumbnails && icon_px >= LIST_PICTURE_PX {
            let device_scale = *Platform::get().current_window().scale_factor.read() as f32;
            bucket_px(f32::from(icon_px), device_scale)
        } else {
            0
        };
        let data = RowsData {
            shared: self.shared.clone(),
            rows: self.rows.clone(),
            icon_px,
            bucket,
            plan,
            compact,
            row_height,
            name_room,
        };
        VirtualScrollView::new_with_data_controlled(
            data,
            |item, data: &RowsData| {
                Element::from(FileRow {
                    shared: data.shared.clone(),
                    rows: data.rows.clone(),
                    row: item.index,
                    icon_px: data.icon_px,
                    bucket: data.bucket,
                    plan: data.plan.clone(),
                    compact: data.compact,
                    height: data.row_height,
                    name_room: data.name_room,
                })
            },
            ctx.scroll,
        )
        .length(self.rows.len())
        .item_size(row_height)
        .drag_scrolling(compact)
        .pass_ctrl_wheel(true)
        .on_sized(move |event: Event<SizedEventData>| viewport.set_if_modified(event.area))
    }
}

/// Keeps the content under the pointer fixed while the zoom changes, and the
/// first row at the top when only the width does. `zoom_px` is the icon size
/// of this frame.
pub(super) fn anchor_zoom(ctx: &Ctx, zoom_px: f32, pitch: f32, columns: usize) {
    let seen = ctx.zoom_seen.clone();
    let before = seen.get();
    seen.set(Some((zoom_px, pitch, columns)));
    let Some((old_zoom, old_pitch, old_columns)) = before else {
        return;
    };
    if (old_pitch - pitch).abs() < f32::EPSILON && old_columns == columns {
        return;
    }
    let mut scroll = ctx.scroll;
    let offset = -(<(i32, i32)>::from(scroll).1 as f32);
    let viewport = *ctx.viewport.peek();
    let zoomed = (old_zoom - zoom_px).abs() > f32::EPSILON;
    let pointer = if zoomed {
        ctx.zoom
            .anchor()
            .map_or(viewport.height() / 2.0, |anchor| anchor.1)
    } else {
        0.0
    };
    let next = super::interaction::anchored_offset(
        offset, pointer, old_pitch, old_columns, pitch, columns,
    );
    scroll.scroll_to_y(-(next.max(0.0).round() as i32));
}

/// The room the Name cell leaves for text: the row less the icon slot, the
/// other columns and the cell's own padding.
fn plan_name_room(plan: &Plan, row_width: f32, icon_px: u16) -> f32 {
    let others: f32 = plan.columns.iter().map(|(_, width)| width).sum();
    (row_width - gutter(f32::from(icon_px)) - others - 2.0 * CELL_PADDING.px()).max(0.0)
}

/// A file name. A name that fits is a plain label, which costs one node and
/// no second layout; one that does not is cut in the middle, keeping its
/// extension (see `path_label`). The text is the same size in a row and under
/// a tile. `marks` are the bytes of `name` a search matched, drawn on an
/// accent tint; a name that has to be cut is drawn without them.
pub(super) fn name_label(
    tokens: &crate::theme::Tokens,
    name: &str,
    room: f32,
    centre: bool,
    marks: &[std::ops::Range<usize>],
) -> Element {
    if room <= 0.0 || measure_text(name, BODY) <= room {
        if !marks.is_empty() {
            let label = crate::search_highlight::highlighted_label(BODY, tokens, name, marks);
            return if centre {
                label
                    .text_align(TextAlign::Center)
                    .width(Size::fill())
                    .into()
            } else {
                label.into()
            };
        }
        let label = BODY.label(tokens, name.to_string()).max_lines(1);
        return if centre {
            label
                .text_align(TextAlign::Center)
                .width(Size::fill())
                .into()
        } else {
            label.into()
        };
    }
    let cut = path_label(name.to_string()).name().style(BODY).tooltip(false);
    if centre { cut.centre().into() } else { cut.into() }
}

/// The keys of the list that move the cursor. Returns true when used.
pub(super) fn navigate(ctx: &Ctx, event: &Event<KeyboardEventData>) -> bool {
    let count = ctx.rows.len();
    if count == 0 {
        return false;
    }
    let extend = event.modifiers.contains(Modifiers::SHIFT);
    let row_height = ctx
        .lattice
        .get()
        .map_or(1.0, |lattice| lattice.item_h.max(1.0));
    let page = ((ctx.viewport.peek().height() / row_height) as usize).max(1);
    let from = ctx.cursor_row();
    let last = count - 1;
    let target = match &event.key {
        Key::Named(NamedKey::ArrowDown) => from.map_or(0, |at| at + 1),
        Key::Named(NamedKey::ArrowUp) => from.map_or(last, |at| at.saturating_sub(1)),
        Key::Named(NamedKey::PageDown) => from.map_or(0, |at| at + page),
        Key::Named(NamedKey::PageUp) => from.map_or(last, |at| at.saturating_sub(page)),
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

/// The widest text in `column` among the rows near the top of the view, in
/// drawn pixels, for a double click on a column's edge.
pub(super) fn widest(ctx: &Ctx, column: Column) -> f32 {
    let now = SystemTime::now();
    let dates = if ctx.recent {
        DateStyle::Relative
    } else {
        DateStyle::Absolute
    };
    let pitch = ctx
        .lattice
        .get()
        .map_or(1.0, |lattice| (lattice.item_h + lattice.gap_y).max(1.0));
    let top = (-(<(i32, i32)>::from(ctx.scroll).1 as f32) / pitch) as usize;
    let first = top.saturating_sub(FIT_SAMPLE / 4);
    let last = (first + FIT_SAMPLE).min(ctx.rows.len());
    (first..last)
        .map(|row| {
            let entry = ctx.rows.entry(row);
            let text = ctx
                .result_folder(entry)
                .filter(|_| column == Column::Location)
                .unwrap_or_else(|| cell_text_styled(column, entry, now, &ctx.home, dates));
            measure_text(&text, FIGURES)
        })
        .fold(0.0, f32::max)
}

#[derive(PartialEq)]
struct FileRow {
    shared: Shared,
    rows: Rows,
    row: usize,
    icon_px: u16,
    bucket: u32,
    plan: Plan,
    compact: bool,
    height: f32,
    name_room: f32,
}

impl Component for FileRow {
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(self.row as u64)
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
            ctx.wants
                .want(&self.rows.listing, &[self.rows.order[row]]);
        }
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

        let icon = face(&tokens, entry, self.icon_px, self.bucket, self.bucket > 0);
        let now = SystemTime::now();
        let dates = if ctx.recent {
            DateStyle::Relative
        } else {
            DateStyle::Absolute
        };
        let name: Element = if renaming {
            RenameField::new(&ctx.env, entry).list(ctx.focus).into()
        } else {
            name_label(&tokens, &entry.name, self.name_room, false, &ctx.marks(&entry.name))
        };

        // The fill is the sidebar's row, inset from the list's sides. The
        // events are on the outer box, so the press is taken across the
        // whole width, and the rounded fill is only what shows.
        let mut line = rect()
            .tw(&tokens, LIST_ITEM)
            .tw(
                &tokens,
                &[
                    grow_children(),
                    h_full(),
                    // The header has no gap between its cells, and the row's
                    // cells must line up with it.
                    gap(S0),
                    surface(
                        Paint::Transparent,
                        (hovering() && enabled) || drop_target,
                        pressing() && enabled,
                        selected,
                    ),
                ],
            )
            .maybe(drop_target, |line| {
                line.tw(&tokens, &[border(Line::Stroke(Dim::Ring, Paint::Accent))])
            })
            .child(
                rect()
                    .width(Size::px(gutter(f32::from(self.icon_px))))
                    .tw_layout(&[flex_row(), items(Align::Center), h_full()])
                    .child(icon),
            );

        if self.compact {
            let size = if entry.is_dir() {
                String::new()
            } else {
                bingux_files::display::size(entry.size)
            };
            let modified = cell_text_styled(Column::Modified, entry, now, &ctx.home, dates);
            let detail = [size.as_str(), modified.as_str()]
                .iter()
                .filter(|part| !part.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" · ");
            // A search result says where it is, not how big.
            let detail = ctx.result_folder(entry).unwrap_or(detail);
            line = line.child(
                rect()
                    .tw(&tokens, &[flex_col(), w_flex()])
                    .child(name)
                    .maybe_child((!detail.is_empty()).then(|| {
                        CAPTION
                            .label(&tokens, detail)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis)
                    })),
            );
        } else {
            line = line.child(name_cell(&self.plan, name));
            for (column, width) in &self.plan.columns {
                let text = ctx
                    .result_folder(entry)
                    .filter(|_| *column == Column::Location)
                    .unwrap_or_else(|| cell_text_styled(*column, entry, now, &ctx.home, dates));
                let content: Element = match column {
                    Column::Location => path_label(text)
                        .style(CAPTION)
                        .ink(crate::typography::Ink::Muted)
                        .tooltip(false)
                        .into(),
                    _ => FIGURES
                        .in_ink(crate::typography::Ink::Muted)
                        .label(&tokens, text)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis)
                        .into(),
                };
                line = line.child(cell(*width, column.right_aligned(), content));
            }
        }

        rect()
            .component("FileRow")
            .height(Size::px(self.height))
            .tw(&tokens, &[flex_col(), w_full(), px(LIST_EDGE)])
            .a11y_role(AccessibilityRole::ListBoxOption)
            .a11y_alt(entry.name.clone())
            .a11y_builder(move |builder| builder.set_selected(selected))
            .maybe(dim, |outer| outer.opacity(DISABLED_OPACITY))
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
                        // A touch in the rename field is the field's: its own
                        // long press and taps.
                        if renaming {
                            return;
                        }
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
                    if touch && (renaming || !ctx.press.tap(&event)) {
                        return;
                    }
                    ctx.pressed(row, touch);
                }
            })
            .child(line)
            .into_element()
    }
}

#[allow(dead_code)]
fn unused(_: &Ui, _: &Cell<u8>, _: &Rc<u8>, _: &Entry) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::zoom::LADDER;

    #[test]
    fn a_row_is_a_sidebar_row_until_the_icon_needs_more() {
        // At mouse density a sidebar row is 40 px and the padding is 4 px.
        for px in [16, 20, 24, 32] {
            assert_eq!(row_height_for(px, 40.0, 4.0), 40.0, "{px}");
        }
        // Touch density: 56 px rows hold the whole list range.
        for px in [16, 20, 24, 32] {
            assert_eq!(row_height_for(px, 56.0, 4.0), 56.0, "{px}");
        }
        // An icon that does not fit with its padding makes the row taller.
        assert_eq!(row_height_for(48, 40.0, 4.0), 56.0);
    }

    #[test]
    fn the_row_never_shrinks_as_the_icon_grows() {
        let heights: Vec<f32> = LADDER.iter().map(|px| row_height_for(*px, 40.0, 4.0)).collect();
        assert!(heights.windows(2).all(|pair| pair[0] <= pair[1]));
    }
}
