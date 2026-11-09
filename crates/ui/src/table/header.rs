//! The heading row of a table: a heading for each column, over the cells the
//! rows draw. It stays above the rows while they scroll.
//!
//! A press sorts. A column's trailing edge drags to resize it, and a double
//! click there fits the column to its widest cell. Dragging a heading moves the
//! column. A right click asks for the columns menu. From the keyboard see
//! `Ctx::key_down`: a heading that holds the cursor sorts with Enter, moves
//! with Shift and the arrows and resizes with Ctrl and the arrows.

use std::hash::Hash;
use std::time::{Duration, Instant};

use accesskit::SortDirection as AccessSort;
use freya::prelude::*;

use super::ctx::{Columns, Latest};
use super::model::{Plan, SortDirection, SortOrder, drop_slot, line_position};
use super::row::{CHECK_CELL, side};
use crate::component::ComponentName;
use crate::controls::checkbox::CheckState;
use crate::controls::press::point_of;
use crate::icons::{self, Icon};
use crate::shell::panel::PanelTrigger;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink};

/// How far a pointer travels from a heading before it is a drag and not a press.
const DRAG_SLOP: f32 = 6.0;
/// The width of the grab area on a column's edge.
const HANDLE_PX: f32 = 8.0;
/// Above the heading's own text, so the handle takes the press.
const HANDLE_LAYER: i16 = 8;
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// What the pointer is doing to the header.
#[derive(Clone, PartialEq)]
enum Drag {
    /// Pressed on a heading; a move past the slop turns it into a `Move`.
    Pending { column: usize, start_x: f32 },
    /// Dragging a heading to another place.
    Move {
        column: usize,
        start_x: f32,
        dx: f32,
    },
    /// Dragging the trailing edge of a column. The plan is the one the drag
    /// began with, so each step is measured from where it started.
    Resize {
        edge: usize,
        start_x: f32,
        plan: Plan,
    },
}

pub(super) struct TableHeader<T, K> {
    pub latest: Latest<T, K>,
    pub columns: Columns<T>,
    pub plan: Plan,
    pub sort: Option<SortOrder>,
    /// The width of the rows' area; the header is as wide.
    pub width: f32,
    /// The state of the select-all mark, when rows have marks.
    pub marks: Option<CheckState>,
    /// The plan column the keyboard cursor is on in the header.
    pub cursor: Option<usize>,
}

impl<T, K> PartialEq for TableHeader<T, K> {
    fn eq(&self, other: &Self) -> bool {
        self.columns == other.columns
            && self.plan == other.plan
            && self.sort == other.sort
            && self.width == other.width
            && self.marks == other.marks
            && self.cursor == other.cursor
    }
}

impl<T: 'static, K: Clone + Eq + Hash + 'static> Component for TableHeader<T, K> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let drag = use_state(|| None::<Drag>);
        let ignore_press = use_state(|| false);
        let last_edge = use_state(|| None::<(Instant, usize)>);
        let widths = self.plan.widths();

        let on_move = {
            let latest = self.latest.clone();
            move |event: Event<PointerEventData>| {
                let Some(current) = drag.peek().clone() else {
                    return;
                };
                let x = event.global_location().x as f32;
                let mut drag = drag;
                match current {
                    Drag::Pending { column, start_x } => {
                        if (x - start_x).abs() > DRAG_SLOP {
                            let mut ignore = ignore_press;
                            ignore.set(true);
                            drag.set(Some(Drag::Move {
                                column,
                                start_x,
                                dx: x - start_x,
                            }));
                            event.prevent_default();
                        }
                    }
                    Drag::Move {
                        column, start_x, ..
                    } => {
                        drag.set_if_modified(Some(Drag::Move {
                            column,
                            start_x,
                            dx: x - start_x,
                        }));
                        event.prevent_default();
                    }
                    Drag::Resize {
                        edge,
                        start_x,
                        plan,
                    } => {
                        latest.get().resize_edge(&plan, edge, x - start_x);
                        event.prevent_default();
                    }
                }
            }
        };
        let on_up = {
            let latest = self.latest.clone();
            let widths = widths.clone();
            move |_: Event<PointerEventData>| {
                let Some(current) = drag.peek().clone() else {
                    return;
                };
                let mut drag = drag;
                drag.set(None);
                let ctx = latest.get();
                match current {
                    Drag::Pending { .. } => {}
                    Drag::Move { column, dx, .. } => {
                        let slot = drop_slot(&widths, column, dx);
                        let specs = ctx.specs();
                        let ids: Vec<&'static str> = ctx
                            .plan
                            .columns
                            .iter()
                            .map(|placed| specs[placed.index].id)
                            .collect();
                        let moved = ids[column];
                        let others: Vec<&'static str> =
                            ids.iter().copied().filter(|id| *id != moved).collect();
                        let before = others.get(slot).copied();
                        if ctx.edit_layout(|layout| layout.move_before(&specs, moved, before)) {
                            ctx.save_layout();
                        }
                    }
                    Drag::Resize { .. } => ctx.save_layout(),
                }
            }
        };

        let current = drag.read().clone();
        let moving = match &current {
            Some(Drag::Move { column, dx, .. }) => Some((*column, *dx)),
            _ => None,
        };
        let sizing = match &current {
            Some(Drag::Resize { edge, .. }) => Some(*edge),
            _ => None,
        };

        let cells = self.plan.columns.iter().enumerate().map(|(at, placed)| {
            let offset = match moving {
                Some((moved, dx)) if moved == at => {
                    let before: f32 = widths[..at].iter().sum();
                    let total: f32 = widths.iter().sum();
                    dx.clamp(-before, total - before - widths[at])
                }
                _ => 0.0,
            };
            Element::from(HeaderCell {
                latest: self.latest.clone(),
                columns: self.columns.clone(),
                plan: self.plan.clone(),
                at,
                width: placed.width,
                sorted: self
                    .sort
                    .filter(|order| order.column == self.columns.0[placed.index].spec.id)
                    .map(|order| order.direction),
                offset,
                moving: moving.is_some_and(|(moved, _)| moved == at),
                sizing: sizing == Some(at),
                ring: self.cursor == Some(at),
                drag,
                ignore_press,
                last_edge,
            })
        });

        let line = moving.map(|(column, dx)| {
            let slot = drop_slot(&widths, column, dx);
            let at = line_position(&widths, column, slot);
            rect()
                .position(Position::new_absolute().top(0.0).left(at - 1.0))
                .layer(Layer::Relative(2))
                .tw(&tokens, &[w(Dim::Indicator), bg(Paint::Accent)])
                .height(Size::percent(100.0))
        });

        let total: f32 = widths.iter().sum();
        let strip = rect()
            .width(Size::px(total))
            .tw(&tokens, &[flex_row(), h_full()])
            .children(cells)
            .maybe_child(line);

        let mark = self.marks.map(|state| {
            let latest = self.latest.clone();
            let accent = state != CheckState::Off;
            rect()
                .width(Size::px(CHECK_CELL))
                .tw(&tokens, &[flex_row(), center(), h_full()])
                .a11y_role(AccessibilityRole::CheckBox)
                .a11y_alt("Select all rows")
                .on_press(move |event: Event<PressEventData>| {
                    event.stop_propagation();
                    let ctx = latest.get();
                    if ctx.selecting() && state == CheckState::On {
                        ctx.clear_selection();
                    } else {
                        ctx.select_all();
                    }
                })
                .child(
                    rect()
                        .tw(
                            &tokens,
                            &[
                                w(Dim::Box5),
                                h(Dim::Box5),
                                center(),
                                rounded(Radius::Mark),
                                bg(if accent {
                                    Paint::Accent
                                } else {
                                    Paint::Transparent
                                }),
                                if accent {
                                    border(Line::Stroke(Dim::Hairline, Paint::Accent))
                                } else {
                                    border(Line::Outline)
                                },
                            ],
                        )
                        .maybe_child(accent.then(|| {
                            icons::icon(
                                if state == CheckState::Mixed {
                                    Icon::ListRemove
                                } else {
                                    Icon::Check
                                },
                                IconSize::Mark,
                                Ink::OnAccent.color(&tokens),
                            )
                        })),
                )
        });

        rect()
            .component("TableHeader")
            .a11y_role(AccessibilityRole::Row)
            .a11y_builder(|node| node.set_row_index(0))
            .tw(
                &tokens,
                &[
                    flex_row(),
                    h(Dim::Control),
                    px(LIST_EDGE),
                    border(Line::HairlineBottom),
                ],
            )
            .width(Size::px(self.width))
            .on_capture_global_pointer_move(on_move)
            .on_global_pointer_up(on_up)
            .child(
                rect()
                    .tw(
                        &tokens,
                        &[
                            flex_row(),
                            grow_children(),
                            items(Align::Center),
                            w_full(),
                            h_full(),
                            px(FILE_ROW_INSET),
                        ],
                    )
                    .maybe_child(mark)
                    .child(strip),
            )
    }
}

/// One heading with its resize handle.
struct HeaderCell<T, K> {
    latest: Latest<T, K>,
    columns: Columns<T>,
    plan: Plan,
    at: usize,
    width: f32,
    sorted: Option<SortDirection>,
    /// How far a dragged heading is shifted from its place.
    offset: f32,
    moving: bool,
    sizing: bool,
    ring: bool,
    drag: State<Option<Drag>>,
    ignore_press: State<bool>,
    last_edge: State<Option<(Instant, usize)>>,
}

impl<T, K> PartialEq for HeaderCell<T, K> {
    fn eq(&self, other: &Self) -> bool {
        self.columns == other.columns
            && self.plan == other.plan
            && self.at == other.at
            && self.width == other.width
            && self.sorted == other.sorted
            && self.offset == other.offset
            && self.moving == other.moving
            && self.sizing == other.sizing
            && self.ring == other.ring
    }
}

impl<T: 'static, K: Clone + Eq + Hash + 'static> Component for HeaderCell<T, K> {
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(self.plan.columns[self.at].index as u64)
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let mut edge_hot = use_state(|| false);
        let at = self.at;
        let spec = self.columns.0[self.plan.columns[at].index].spec.clone();
        let latest = self.latest.clone();
        let drag = self.drag;
        let ignore_press = self.ignore_press;
        let last_edge = self.last_edge;
        let sorted = self.sorted;
        let sortable = spec.sortable;
        let right = spec.right;
        let resizable = spec.resizable && self.plan.columns.len() > at;
        let many = self.plan.columns.len() > 1;

        let arrow = sorted.map(|direction| {
            icons::icon(
                if direction.is_descending() {
                    Icon::PanDown
                } else {
                    Icon::PanUp
                },
                IconSize::Mark,
                Paint::Muted.color(&tokens),
            )
        });
        let label = CAPTION
            .in_ink(if sorted.is_some() {
                Ink::Text
            } else {
                Ink::Muted
            })
            .label(&tokens, spec.title.clone())
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis);

        let body = rect()
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    justify(if right { Align::End } else { Align::Start }),
                    w_full(),
                    h_full(),
                    px(S2),
                    gap(S1),
                ],
            )
            .child(label)
            .maybe_child(arrow);

        let face = rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    h_full(),
                    rounded(Radius::Mark),
                    surface(
                        Paint::Transparent,
                        hovering() && sortable,
                        self.moving || pressing(),
                        false,
                    ),
                ],
            )
            .maybe(self.ring, |face| {
                face.tw(
                    &tokens,
                    &[border(Line::Stroke(Dim::Ring, Paint::FocusRing))],
                )
            })
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            .on_pointer_down({
                let latest = latest.clone();
                move |event: Event<PointerEventData>| {
                    let PointerEventData::Mouse(mouse) = event.data() else {
                        return;
                    };
                    let mut ignore = ignore_press;
                    let mut drag = drag;
                    match mouse.button {
                        Some(MouseButton::Right) => {
                            ignore.set(true);
                            latest
                                .get()
                                .open_columns_menu(PanelTrigger::Pointer(point_of(&event)), false);
                        }
                        Some(MouseButton::Left) => {
                            ignore.set(false);
                            pressing.set(true);
                            let ctx = latest.get();
                            ctx.a11y_id.request_focus();
                            ctx.set_header_cursor(at);
                            if many {
                                drag.set(Some(Drag::Pending {
                                    column: at,
                                    start_x: event.global_location().x as f32,
                                }));
                            }
                        }
                        _ => {}
                    }
                }
            })
            .on_global_pointer_up(move |_| pressing.set_if_modified(false))
            .on_press({
                let latest = latest.clone();
                move |event: Event<PressEventData>| {
                    event.stop_propagation();
                    let mut ignore = ignore_press;
                    if *ignore.peek() {
                        ignore.set(false);
                        return;
                    }
                    if matches!(*drag.peek(), Some(Drag::Move { .. })) {
                        return;
                    }
                    latest.get().sort_press(at);
                }
            })
            .child(body);

        let edge = resizable.then(|| {
            let latest = latest.clone();
            let plan = self.plan.clone();
            rect()
                // Centred on the cell's trailing edge.
                .position(Position::new_absolute().top(0.0).right(-HANDLE_PX / 2.0))
                .width(Size::px(HANDLE_PX))
                .layer(Layer::Relative(HANDLE_LAYER))
                .tw(&tokens, &[items(Align::Center), justify(Align::Center)])
                // `h_full` on an absolute box measures zero; a percent does not.
                .height(Size::percent(100.0))
                .cursor(CursorIcon::ColResize)
                .on_pointer_enter(move |_| edge_hot.set(true))
                .on_pointer_leave(move |_| edge_hot.set(false))
                .on_pointer_down(move |event: Event<PointerEventData>| {
                    if !matches!(
                        event.data(),
                        PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Left)
                    ) {
                        return;
                    }
                    event.stop_propagation();
                    // The release that ends the drag is not a press on a heading.
                    let mut ignore = ignore_press;
                    ignore.set(true);
                    let now = Instant::now();
                    let mut last = last_edge;
                    let mut drag = drag;
                    let double = matches!(
                        *last.peek(),
                        Some((then, pressed)) if pressed == at && now.duration_since(then) < DOUBLE_CLICK
                    );
                    if double {
                        last.set(None);
                        drag.set(None);
                        latest.get().fit_edge(&plan, at);
                        return;
                    }
                    last.set(Some((now, at)));
                    drag.set(Some(Drag::Resize {
                        edge: at,
                        start_x: event.global_location().x as f32,
                        plan: plan.clone(),
                    }));
                })
                .child(rect().tw(
                    &tokens,
                    &[
                        w(Dim::Hairline),
                        h_full(),
                        bg(if self.sizing {
                            Paint::Accent
                        } else if edge_hot() {
                            Paint::Separator
                        } else {
                            Paint::Transparent
                        }),
                    ],
                ))
        });

        let cell = rect()
            .a11y_id(latest.get().cells.get(usize::MAX, at))
            .a11y_role(AccessibilityRole::ColumnHeader)
            .a11y_alt(spec.title.clone())
            .a11y_builder(move |node| {
                node.set_column_index(at);
                if sortable {
                    node.set_sort_direction(match sorted {
                        Some(SortDirection::Ascending) => AccessSort::Ascending,
                        Some(SortDirection::Descending) => AccessSort::Descending,
                        None => AccessSort::Other,
                    });
                }
            })
            .offset_x(self.offset)
            .width(Size::px(self.width))
            .tw(&tokens, &[h_full()])
            .maybe(self.moving, |cell| cell.layer(Layer::Relative(1)));
        cell.child(face).maybe_child(edge)
    }
}
