//! The time grid of a week or a day: columns of days, hours down the side,
//! timed events as blocks, whole-day events as bars above the hours.
//!
//! Events that overlap in time sit side by side (`layout::pack`). Press on
//! an empty slot and drag to make a range; press on a block and drag to move
//! it, or drag its top or bottom edge to change when it starts or ends. A
//! press that does not move opens the block; two quick presses on a slot ask
//! for a new event there. Times snap to `step` minutes. A line marks the
//! time now in today's column. The grid scrolls, and starts at the hour the
//! caller names; asking for another hour scrolls there.
//!
//! The grid draws what it is given and reports what a person did; the caller
//! owns the events and decides what each report changes. Up and Down on the
//! focused grid move through its events, Enter opens the one chosen.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use freya::prelude::*;

use super::date::{Date, format_clock, format_hour};
use super::layout::{self, DRAG_SLOP, Run, Span};
use super::palette::event_colours;
use crate::component::ComponentName;
use crate::controls::capture::{CaptureEvent, Phase, use_capture};
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, EVENT_TITLE, Ink, NUMERIC};

/// How close to the top or bottom edge of a block a press has to be to take
/// that edge, in pixels.
const EDGE: f32 = 7.0;

/// Two presses closer in time than this, and in place, ask for an event.
const DOUBLE_PRESS: Duration = Duration::from_millis(400);

/// A timed event.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub id: String,
    /// The column (day) it is in.
    pub column: usize,
    /// When it is, in minutes after midnight.
    pub span: Span,
    pub title: String,
    /// A second line, such as the time or the place.
    pub detail: String,
    pub colour: Color,
    /// Drawn paler, as an event that is not certain.
    pub tentative: bool,
    /// Whether it can be moved and resized.
    pub editable: bool,
    /// It began on an earlier day / goes on to the next.
    pub clipped_start: bool,
    pub clipped_end: bool,
}

/// An event that covers whole days.
#[derive(Clone, Debug, PartialEq)]
pub struct Banner {
    pub id: String,
    pub first: usize,
    pub last: usize,
    pub title: String,
    pub colour: Color,
    pub before: bool,
    pub after: bool,
}

/// A range of one day's minutes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotRange {
    pub column: usize,
    pub span: Span,
}

/// What a person did to a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Move,
    /// An edge was dragged: the start, the end, or both moved.
    Resize,
}

/// A block moved or resized to a new place.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockEdit {
    pub id: String,
    pub column: usize,
    pub span: Span,
    pub kind: EditKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    Body,
    Top,
    Bottom,
}

/// What a press landed on.
#[derive(Clone, Debug, PartialEq)]
enum Target {
    Slot {
        column: usize,
        minutes: f32,
    },
    Block {
        id: String,
        column: usize,
        span: Span,
        /// Minutes from the block's top edge to the pointer.
        grab: f32,
        edge: Edge,
        editable: bool,
    },
}

/// The drag in progress.
#[derive(Clone, Debug, PartialEq)]
enum Drag {
    Idle,
    /// Pressed, not yet moved far enough to be a drag.
    Pressed {
        origin: (f32, f32),
        target: Target,
    },
    Creating {
        column: usize,
        anchor: f32,
        span: Span,
    },
    Moving {
        id: String,
        grab: f32,
        column: usize,
        span: Span,
    },
    Resizing {
        id: String,
        column: usize,
        span: Span,
        edge: Edge,
    },
}

/// A block with its place on screen.
#[derive(Clone, Debug, PartialEq)]
struct Placed {
    index: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// The block (or empty slot) at a point of the grid's body.
fn hit(
    placed: &[Placed],
    blocks: &[Block],
    gutter: f32,
    colw: f32,
    hour_h: f32,
    columns: usize,
    point: (f32, f32),
) -> Option<Target> {
    let (x, y) = point;
    if x < gutter || colw <= 0.0 || y < 0.0 {
        return None;
    }
    // The last block drawn is on top.
    for p in placed.iter().rev() {
        if x >= p.x && x <= p.x + p.w && y >= p.y && y <= p.y + p.h {
            let block = &blocks[p.index];
            let edge = if block.editable && p.h > EDGE * 3.0 {
                if y - p.y <= EDGE && !block.clipped_start {
                    Edge::Top
                } else if p.y + p.h - y <= EDGE && !block.clipped_end {
                    Edge::Bottom
                } else {
                    Edge::Body
                }
            } else {
                Edge::Body
            };
            return Some(Target::Block {
                id: block.id.clone(),
                column: block.column,
                span: block.span,
                grab: (y - p.y) / hour_h * 60.0,
                edge,
                editable: block.editable,
            });
        }
    }
    let column = (((x - gutter) / colw).floor() as usize).min(columns.saturating_sub(1));
    Some(Target::Slot {
        column,
        minutes: y / hour_h * 60.0,
    })
}

/// A grid of days with hours down it.
#[derive(PartialEq)]
pub struct TimeGrid {
    days: Vec<Date>,
    today: Date,
    now: Option<u32>,
    blocks: Vec<Block>,
    banners: Vec<Banner>,
    selected: Option<String>,
    ghost: Option<SlotRange>,
    twelve_hour: bool,
    step: u32,
    default_length: u32,
    scroll_to: Option<(u32, u64)>,
    on_open: EventHandler<String>,
    on_select: Option<EventHandler<String>>,
    on_create: EventHandler<SlotRange>,
    on_edit: EventHandler<BlockEdit>,
    on_day: Option<EventHandler<Date>>,
}

/// A grid of `days`, one column each. `today` marks that column's header.
pub fn time_grid(days: Vec<Date>, today: Date) -> TimeGrid {
    TimeGrid {
        days,
        today,
        now: None,
        blocks: Vec::new(),
        banners: Vec::new(),
        selected: None,
        ghost: None,
        twelve_hour: false,
        step: 15,
        default_length: 60,
        scroll_to: None,
        on_open: EventHandler::new(|_| {}),
        on_select: None,
        on_create: EventHandler::new(|_| {}),
        on_edit: EventHandler::new(|_| {}),
        on_day: None,
    }
}

impl TimeGrid {
    /// The time now, in minutes, to draw the line at in today's column.
    pub fn now(mut self, minutes: u32) -> Self {
        self.now = Some(minutes);
        self
    }

    pub fn blocks(mut self, blocks: Vec<Block>) -> Self {
        self.blocks = blocks;
        self
    }

    pub fn banners(mut self, banners: Vec<Banner>) -> Self {
        self.banners = banners;
        self
    }

    /// The block with the keyboard's choice.
    pub fn selected(mut self, id: Option<String>) -> Self {
        self.selected = id;
        self
    }

    /// A range the caller is making an event for, drawn until it is gone.
    pub fn ghost(mut self, ghost: Option<SlotRange>) -> Self {
        self.ghost = ghost;
        self
    }

    pub fn twelve_hour(mut self, twelve_hour: bool) -> Self {
        self.twelve_hour = twelve_hour;
        self
    }

    /// The minutes a drag snaps to. 15 by default.
    pub fn step(mut self, step: u32) -> Self {
        self.step = step.max(1);
        self
    }

    /// The length of the event a double press asks for, in minutes.
    pub fn default_length(mut self, minutes: u32) -> Self {
        self.default_length = minutes.max(self.step);
        self
    }

    /// Scrolls so `minutes` after midnight is at the top. Each distinct
    /// `nonce` scrolls once, so asking again for the same hour needs a new
    /// one; the first value is where the grid starts.
    pub fn scroll_to(mut self, minutes: u32, nonce: u64) -> Self {
        self.scroll_to = Some((minutes, nonce));
        self
    }

    /// A block was pressed, or chosen with Enter.
    pub fn on_open(mut self, handler: impl Into<EventHandler<String>>) -> Self {
        self.on_open = handler.into();
        self
    }

    /// Up and Down moved the choice to this block.
    pub fn on_select(mut self, handler: impl Into<EventHandler<String>>) -> Self {
        self.on_select = Some(handler.into());
        self
    }

    /// A range was dragged out, or a slot pressed twice.
    pub fn on_create(mut self, handler: impl Into<EventHandler<SlotRange>>) -> Self {
        self.on_create = handler.into();
        self
    }

    /// A block was moved or resized.
    pub fn on_edit(mut self, handler: impl Into<EventHandler<BlockEdit>>) -> Self {
        self.on_edit = handler.into();
        self
    }

    /// A day's header was pressed.
    pub fn on_day(mut self, handler: impl Into<EventHandler<Date>>) -> Self {
        self.on_day = Some(handler.into());
        self
    }
}

impl Component for TimeGrid {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let columns = self.days.len().max(1);
        let hour_h = Dim::CalendarHour.px();
        let gutter = Dim::CalendarGutter.px();
        let chip = Dim::CalendarChip.px();
        let gap = S0_5.px();
        let mut width = use_state(|| 0.0f32);
        let mut drag = use_state(|| Drag::Idle);
        let last_press = use_hook(|| Rc::new(Cell::new(None::<(Instant, f32, f32)>)));
        let start_minutes = self.scroll_to.map_or(0, |(minutes, _)| minutes);
        let scroll =
            use_hook(|| ScrollController::new(0, -((start_minutes as f32 / 60.0 * hour_h) as i32)));
        let applied = use_hook(|| Rc::new(Cell::new(self.scroll_to.map(|(_, nonce)| nonce))));
        if let Some((minutes, nonce)) = self.scroll_to {
            if applied.get() != Some(nonce) {
                applied.set(Some(nonce));
                let mut scroll = scroll;
                scroll.scroll_to_y(-((minutes as f32 / 60.0 * hour_h) as i32));
            }
        }
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let body_width = *width.read();
        let colw = ((body_width - gutter) / columns as f32).max(0.0);
        let total_h = hour_h * 24.0;

        // Where each block goes: packed per column among those it overlaps.
        let mut placed: Vec<Placed> = Vec::with_capacity(self.blocks.len());
        for column in 0..columns {
            let members: Vec<usize> = (0..self.blocks.len())
                .filter(|&i| self.blocks[i].column == column)
                .collect();
            let spans: Vec<Span> = members.iter().map(|&i| self.blocks[i].span).collect();
            let slots = layout::pack(&spans);
            for (member, &index) in members.iter().enumerate() {
                let slot = slots[member];
                let block = &self.blocks[index];
                let lane = ((colw - gap * 2.0) / slot.columns as f32).max(0.0);
                placed.push(Placed {
                    index,
                    x: gutter + column as f32 * colw + gap + slot.column as f32 * lane,
                    y: block.span.start as f32 / 60.0 * hour_h,
                    w: (lane * slot.span as f32 - gap).max(0.0),
                    h: ((block.span.end - block.span.start) as f32 / 60.0 * hour_h - gap).max(chip),
                });
            }
        }

        let step = self.step;
        let columns_total = columns;
        let on_create = self.on_create.clone();
        let on_edit = self.on_edit.clone();
        let on_open = self.on_open.clone();
        let default_length = self.default_length;
        let blocks_for_hit = self.blocks.clone();
        let placed_for_hit = placed.clone();
        let touch = matches!(pointer_kind(), PointerKind::Touch);
        let last = last_press.clone();

        let capture = use_capture(move |event: CaptureEvent| {
            let point = event.local;
            let minutes = point.1 / hour_h * 60.0;
            let column_at = |x: f32| -> usize {
                (((x - gutter) / colw.max(1.0)).floor().max(0.0) as usize)
                    .min(columns_total.saturating_sub(1))
            };
            match event.phase {
                Phase::Start => {
                    let target = hit(
                        &placed_for_hit,
                        &blocks_for_hit,
                        gutter,
                        colw,
                        hour_h,
                        columns_total,
                        point,
                    );
                    drag.set(match target {
                        Some(target) => Drag::Pressed {
                            origin: point,
                            target,
                        },
                        None => Drag::Idle,
                    });
                }
                Phase::Move => {
                    let current = drag.peek().clone();
                    match current {
                        Drag::Pressed { origin, target } => {
                            let moved = ((point.0 - origin.0).powi(2)
                                + (point.1 - origin.1).powi(2))
                            .sqrt();
                            if moved < DRAG_SLOP || touch {
                                return;
                            }
                            match target {
                                Target::Slot {
                                    column,
                                    minutes: anchor,
                                } => {
                                    let span = layout::drag_range(anchor, minutes, step);
                                    drag.set(Drag::Creating {
                                        column,
                                        anchor,
                                        span,
                                    });
                                }
                                Target::Block {
                                    id,
                                    column,
                                    span,
                                    grab,
                                    edge,
                                    editable,
                                } => {
                                    if !editable {
                                        drag.set(Drag::Idle);
                                    } else if edge == Edge::Body {
                                        drag.set(Drag::Moving {
                                            id,
                                            grab,
                                            column,
                                            span,
                                        });
                                    } else {
                                        drag.set(Drag::Resizing {
                                            id,
                                            column,
                                            span,
                                            edge,
                                        });
                                    }
                                }
                            }
                        }
                        Drag::Creating { column, anchor, .. } => {
                            let span = layout::drag_range(anchor, minutes, step);
                            drag.set(Drag::Creating {
                                column,
                                anchor,
                                span,
                            });
                        }
                        Drag::Moving { id, grab, span, .. } => {
                            let length = span.end - span.start;
                            let start = layout::carried_start(minutes, grab, length, step);
                            drag.set(Drag::Moving {
                                id,
                                grab,
                                column: column_at(point.0),
                                span: Span {
                                    start,
                                    end: start + length,
                                },
                            });
                        }
                        Drag::Resizing {
                            id,
                            column,
                            span,
                            edge,
                        } => {
                            let at = layout::snap(minutes, step);
                            let span = if edge == Edge::Top {
                                Span {
                                    start: at.min(span.end - step),
                                    end: span.end,
                                }
                            } else {
                                Span {
                                    start: span.start,
                                    end: at.clamp(span.start + step, 24 * 60),
                                }
                            };
                            drag.set(Drag::Resizing {
                                id,
                                column,
                                span,
                                edge,
                            });
                        }
                        Drag::Idle => {}
                    }
                }
                Phase::End => {
                    let current = drag.peek().clone();
                    drag.set(Drag::Idle);
                    match current {
                        Drag::Pressed {
                            target: Target::Block { id, .. },
                            ..
                        } => {
                            on_open.call(id);
                        }
                        Drag::Pressed {
                            target: Target::Slot { column, minutes },
                            ..
                        } => {
                            let now = Instant::now();
                            let again = last.get().is_some_and(|(at, x, y)| {
                                now.duration_since(at) < DOUBLE_PRESS
                                    && (x - point.0).abs() < DRAG_SLOP * 2.0
                                    && (y - point.1).abs() < DRAG_SLOP * 2.0
                            });
                            if again {
                                last.set(None);
                                let start =
                                    layout::snap_down(minutes, 30).min(24 * 60 - default_length);
                                on_create.call(SlotRange {
                                    column,
                                    span: Span {
                                        start,
                                        end: start + default_length,
                                    },
                                });
                            } else {
                                last.set(Some((now, point.0, point.1)));
                            }
                        }
                        Drag::Creating { column, span, .. } => {
                            on_create.call(SlotRange { column, span });
                        }
                        Drag::Moving {
                            id, column, span, ..
                        } => {
                            on_edit.call(BlockEdit {
                                id,
                                column,
                                span,
                                kind: EditKind::Move,
                            });
                        }
                        Drag::Resizing {
                            id, column, span, ..
                        } => {
                            on_edit.call(BlockEdit {
                                id,
                                column,
                                span,
                                kind: EditKind::Resize,
                            });
                        }
                        Drag::Idle => {}
                    }
                }
                Phase::Cancel => drag.set(Drag::Idle),
            }
        });
        let tracker = capture.clone();

        // The ghost: what a drag in progress, or the caller, is making.
        let (ghost, moving_id) = match drag.read().clone() {
            Drag::Creating { column, span, .. } => (Some(SlotRange { column, span }), None),
            Drag::Moving {
                id, column, span, ..
            } => (Some(SlotRange { column, span }), Some(id)),
            Drag::Resizing {
                id, column, span, ..
            } => (Some(SlotRange { column, span }), Some(id)),
            _ => (self.ghost, None),
        };

        // Hour lines, hour labels and day dividers.
        let hours = (0..24u32).map(|hour| {
            let y = hour as f32 * hour_h;
            rect()
                .key(hour)
                .position(Position::new_absolute().left(gutter).top(y))
                .width(Size::px((body_width - gutter).max(0.0)))
                .tw(&tokens, &[h(Dim::Hairline), bg(Paint::Separator)])
                .into_element()
        });
        let labels = (1..24u32).map(|hour| {
            let y = hour as f32 * hour_h - Dim::CaptionLine.px() / 2.0;
            rect()
                .key(hour)
                .position(Position::new_absolute().left(0.0).top(y))
                .width(Size::px(gutter))
                .tw(
                    &tokens,
                    &[h(Dim::CaptionLine), flex_row(), justify(Align::End), pr(S2)],
                )
                .child(
                    CAPTION
                        .label(&tokens, format_hour(hour, self.twelve_hour))
                        .max_lines(1),
                )
                .into_element()
        });
        let dividers = (0..columns).map(|column| {
            rect()
                .key(column)
                .position(
                    Position::new_absolute()
                        .left(gutter + column as f32 * colw)
                        .top(0.0),
                )
                .height(Size::px(total_h))
                .tw(&tokens, &[w(Dim::Hairline), bg(Paint::Separator)])
                .into_element()
        });

        let blocks = placed.iter().map(|p| {
            let block = &self.blocks[p.index];
            BlockView {
                id: block.id.clone(),
                title: block.title.clone(),
                detail: block.detail.clone(),
                colour: block.colour,
                x: p.x,
                y: p.y,
                w: p.w,
                h: p.h,
                selected: self.selected.as_deref() == Some(block.id.as_str()),
                tentative: block.tentative,
                faded: moving_id.as_deref() == Some(block.id.as_str()),
                clipped_start: block.clipped_start,
                clipped_end: block.clipped_end,
            }
            .into_element()
        });

        let ghost_view = ghost.map(|ghost| {
            let top = ghost.span.start as f32 / 60.0 * hour_h;
            let height = (ghost.span.end - ghost.span.start) as f32 / 60.0 * hour_h;
            let label = format!(
                "{} – {}",
                format_clock(ghost.span.start, self.twelve_hour),
                format_clock(ghost.span.end, self.twelve_hour)
            );
            rect()
                .position(
                    Position::new_absolute()
                        .left(gutter + ghost.column as f32 * colw + gap)
                        .top(top),
                )
                // ui-lint: allow(a column's width, measured at run time)
                .width(Size::px((colw - gap * 2.0).max(0.0)))
                .height(Size::px(height.max(chip)))
                .tw(
                    &tokens,
                    &[
                        rounded(Radius::Mark),
                        bg(Paint::AccentSoft),
                        border(Line::Edge(Paint::Accent)),
                        p(S1),
                    ],
                )
                .child(
                    CAPTION
                        .in_ink(Ink::Accent)
                        .label(&tokens, label)
                        .max_lines(1),
                )
        });

        let now_line = self.now.and_then(|minutes| {
            let column = self.days.iter().position(|day| *day == self.today)?;
            let y = minutes as f32 / 60.0 * hour_h;
            Some(
                rect()
                    .position(
                        Position::new_absolute()
                            .left(gutter + column as f32 * colw)
                            .top(y - Dim::Hairline.px()),
                    )
                    .width(Size::px(colw))
                    .tw(&tokens, &[h(Dim::Ring), bg(Paint::Danger)]),
            )
        });

        let body = capture
            .handlers(
                rect()
                    .tw(&tokens, &[w_full()])
                    .height(Size::px(total_h))
                    .cursor(match &*drag.read() {
                        Drag::Moving { .. } => CursorIcon::Grabbing,
                        Drag::Resizing { .. } => CursorIcon::RowResize,
                        _ => CursorIcon::default(),
                    }),
            )
            .on_sized(move |event: Event<SizedEventData>| {
                width.set_if_modified(event.area.width());
                tracker.sized(&event);
            })
            .children(hours)
            .children(dividers)
            .children(labels)
            .children(blocks)
            .maybe_child(ghost_view)
            .maybe_child(now_line);

        // Day headers and the bars of whole-day events.
        let narrow = colw < 64.0;
        let on_day = self.on_day.clone();
        let headers = rect()
            .tw(
                &tokens,
                &[w_full(), flex_row(), grow_children(), items(Align::Center)],
            )
            .child(rect().tw(&tokens, &[w(Dim::CalendarGutter)]))
            .children(self.days.iter().map(|day| {
                DayHead {
                    date: *day,
                    today: *day == self.today,
                    narrow,
                    on_day: on_day.clone(),
                }
                .into_element()
            }));

        let runs: Vec<Run> = self
            .banners
            .iter()
            .enumerate()
            .map(|(index, banner)| Run {
                index,
                first: banner.first.min(columns - 1),
                last: banner.last.min(columns - 1),
                continues_before: banner.before,
                continues_after: banner.after,
            })
            .collect();
        let lanes = layout::lanes(&runs, columns);
        let lane_count = lanes.iter().map(|p| p.lane + 1).max().unwrap_or(1);
        let strip_h = lane_count as f32 * (chip + gap) + gap;
        let strip = rect()
            .tw(&tokens, &[w_full(), flex_row(), grow_children()])
            .height(Size::px(strip_h))
            .child(
                rect()
                    .tw(
                        &tokens,
                        &[
                            w(Dim::CalendarGutter),
                            h_full(),
                            flex_row(),
                            justify(Align::End),
                            pr(S2),
                        ],
                    )
                    .child(CAPTION.label(&tokens, "all-day").max_lines(1)),
            )
            .child(
                rect()
                    .tw(&tokens, &[w_flex(), h_full()])
                    .children(lanes.iter().map(|placed| {
                        let banner = &self.banners[placed.run.index];
                        let x = placed.run.first as f32 * colw + gap;
                        let span_w =
                            (placed.run.last - placed.run.first + 1) as f32 * colw - gap * 2.0;
                        BannerView {
                            title: banner.title.clone(),
                            colour: banner.colour,
                            x,
                            y: gap + placed.lane as f32 * (chip + gap),
                            w: span_w.max(0.0),
                            continues_before: banner.before,
                            continues_after: banner.after,
                            selected: self.selected.as_deref() == Some(banner.id.as_str()),
                            id: banner.id.clone(),
                            on_open: self.on_open.clone(),
                        }
                        .into_element()
                    })),
            );

        // The key reaches the grid from the block's or the body's focus.
        let order: Vec<String> = {
            let mut ids: Vec<(usize, u32, String)> = self
                .blocks
                .iter()
                .map(|block| (block.column, block.span.start, block.id.clone()))
                .collect();
            ids.sort();
            ids.into_iter().map(|(_, _, id)| id).collect()
        };
        let selected = self.selected.clone();
        let on_select = self.on_select.clone();
        let on_open_key = self.on_open.clone();
        let on_key_down = move |event: Event<KeyboardEventData>| {
            let position = selected
                .as_ref()
                .and_then(|id| order.iter().position(|other| other == id));
            let next = match &event.key {
                Key::Named(NamedKey::ArrowDown) => match position {
                    Some(at) => order.get(at + 1).or(order.first()),
                    None => order.first(),
                },
                Key::Named(NamedKey::ArrowUp) => match position {
                    Some(at) if at > 0 => order.get(at - 1),
                    _ => order.last(),
                },
                Key::Named(NamedKey::Home) => order.first(),
                Key::Named(NamedKey::End) => order.last(),
                Key::Named(NamedKey::Enter) => {
                    if let Some(id) = &selected {
                        event.stop_propagation();
                        on_open_key.call(id.clone());
                    }
                    return;
                }
                _ => return,
            };
            let (Some(next), Some(on_select)) = (next, &on_select) else {
                return;
            };
            event.stop_propagation();
            event.prevent_default();
            on_select.call(next.clone());
        };

        rect()
            .component("TimeGrid")
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_enabled(true)
            .a11y_role(AccessibilityRole::Grid)
            .a11y_alt(format!(
                "{}, {} events",
                match (self.days.first(), self.days.last()) {
                    (Some(first), Some(last)) if first != last => {
                        format!("{} to {}", first.long(), last.long())
                    }
                    (Some(first), _) => first.long(),
                    _ => String::new(),
                },
                self.blocks.len() + self.banners.len()
            ))
            .tw(&tokens, &[w_full(), h_full(), flex_col(), grow_children()])
            .on_key_down(on_key_down)
            .child(headers)
            .child(strip)
            .child(
                rect()
                    .tw(&tokens, &[w_full(), h_flex(), border(Line::HairlineTop)])
                    .child(
                        ScrollView::new_controlled(scroll)
                            .width(Size::fill())
                            .height(Size::fill())
                            .child(body),
                    ),
            )
            .focusable_inside(focus())
            .tw(&tokens, &[w_full(), h_full()])
    }
}

/// One day's name over its columns.
#[derive(PartialEq)]
struct DayHead {
    date: Date,
    today: bool,
    narrow: bool,
    on_day: Option<EventHandler<Date>>,
}

impl Component for DayHead {
    fn render_key(&self) -> DiffKey {
        super::key_of(self.date.days())
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let date = self.date;
        let on_day = self.on_day.clone();
        let name = if self.narrow {
            date.weekday().narrow()
        } else {
            date.weekday().short()
        };
        let weekend = date.weekday().is_weekend();
        let face = rect()
            .tw(
                &tokens,
                &[
                    w(Dim::Box7),
                    h(Dim::Box7),
                    center(),
                    rounded(Radius::Pill),
                    head_fill(self.today, hovering()),
                ],
            )
            .child(
                NUMERIC
                    .in_ink(head_ink(self.today))
                    .label(&tokens, date.day.to_string())
                    .max_lines(1),
            );
        rect()
            .component("DayHead")
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(date.long())
            .tw(
                &tokens,
                &[w_flex(), flex_col(), items(Align::Center), py(S1)],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set_if_modified(false))
            .on_press(move |_: Event<PressEventData>| {
                if let Some(handler) = &on_day {
                    handler.call(date);
                }
            })
            .child(
                CAPTION
                    .in_ink(if weekend { Ink::Muted } else { Ink::Text })
                    .label(&tokens, name)
                    .max_lines(1),
            )
            .child(face)
    }
}

fn head_ink(today: bool) -> Ink {
    if today { Ink::OnAccent } else { Ink::Text }
}

/// A timed event.
#[derive(PartialEq)]
struct BlockView {
    id: String,
    title: String,
    detail: String,
    colour: Color,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    selected: bool,
    tentative: bool,
    faded: bool,
    clipped_start: bool,
    clipped_end: bool,
}

impl Component for BlockView {
    fn render_key(&self) -> DiffKey {
        super::key_of(&self.id)
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let colours = event_colours(&tokens, self.colour);
        let line = EVENT_TITLE.size.px() * 1.3;
        let inner = self.h - S1.px() * 2.0;
        let lines = (inner / line).floor().max(1.0) as usize;
        let title_lines = if self.detail.is_empty() || lines < 2 {
            lines.min(2)
        } else {
            (lines - 1).min(2)
        };
        let show_detail = !self.detail.is_empty() && lines >= 2;
        let strong = self.selected || hovering();
        rect()
            .component("EventBlock")
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("{}, {}", self.title, self.detail))
            .position(Position::new_absolute().left(self.x).top(self.y))
            .width(Size::px(self.w))
            .height(Size::px(self.h))
            .tw(
                &tokens,
                &[
                    rounded(Radius::Mark),
                    fill(if strong { colours.strong } else { colours.tint }),
                    clip(),
                    pl(S2),
                    py(S0_5),
                    pr(S1),
                ],
            )
            .maybe(self.tentative, |element| element.opacity(0.7))
            .maybe(self.faded, |element| element.opacity(0.35))
            .maybe(self.selected, |element| {
                element.tw(&tokens, &[border(Line::Edge(Paint::Accent))])
            })
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set_if_modified(false))
            .child(
                rect()
                    .position(Position::new_absolute().left(0.0).top(0.0))
                    .tw(
                        &tokens,
                        &[w(Dim::CalendarBar), h_full(), fill(colours.solid)],
                    ),
            )
            .child(
                EVENT_TITLE
                    .label(
                        &tokens,
                        if self.clipped_start {
                            format!("↑ {}", self.title)
                        } else {
                            self.title.clone()
                        },
                    )
                    .max_lines(title_lines)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .maybe_child(show_detail.then(|| {
                CAPTION
                    .label(&tokens, self.detail.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
            }))
    }
}

/// An event over whole days.
#[derive(PartialEq)]
struct BannerView {
    id: String,
    title: String,
    colour: Color,
    x: f32,
    y: f32,
    w: f32,
    continues_before: bool,
    continues_after: bool,
    selected: bool,
    on_open: EventHandler<String>,
}

impl Component for BannerView {
    fn render_key(&self) -> DiffKey {
        super::key_of(&self.id)
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let colours = event_colours(&tokens, self.colour);
        let id = self.id.clone();
        let on_open = self.on_open.clone();
        let text_colour = colours.on_solid;
        rect()
            .component("EventBanner")
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("{}, all day", self.title))
            .position(Position::new_absolute().left(self.x).top(self.y))
            .width(Size::px(self.w))
            .tw(&tokens, &[h(Dim::CalendarChip)])
            .tw(
                &tokens,
                &[
                    rounded_on(
                        Radius::Mark,
                        banner_corners(self.continues_before, self.continues_after),
                        0.0,
                    ),
                    fill(colours.solid),
                    clip(),
                    px(S2),
                    flex_row(),
                    items(Align::Center),
                ],
            )
            .maybe(hovering() || self.selected, |element| element.opacity(0.85))
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set_if_modified(false))
            .on_press(move |_: Event<PressEventData>| on_open.call(id.clone()))
            .child(
                EVENT_TITLE
                    .label_filled(&tokens, self.title.clone(), text_colour)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
    }
}

/// Which corners of a bar round: not the end where it goes on.
fn banner_corners(before: bool, after: bool) -> Corners {
    match (before, after) {
        (false, false) => Corners::All,
        (true, false) => Corners::Right,
        (false, true) => Corners::Left,
        (true, true) => Corners::None,
    }
}

/// The fill of a day number in a header.
fn head_fill(today: bool, hovering: bool) -> Util {
    if today {
        bg(Paint::Accent)
    } else {
        surface(Paint::Transparent, hovering, false, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(id: &str, column: usize, start: u32, end: u32, editable: bool) -> Block {
        Block {
            id: id.into(),
            column,
            span: Span { start, end },
            title: id.into(),
            detail: String::new(),
            // ui-lint: allow(test fixture colour)
            colour: Color::from_rgb(1, 2, 3),
            tentative: false,
            editable,
            clipped_start: false,
            clipped_end: false,
        }
    }

    #[test]
    fn a_press_on_a_block_takes_its_edge_or_its_body() {
        let blocks = vec![block("a", 1, 540, 600, true)];
        let hour = 56.0;
        // Column 1 of two, 400 wide with a 56 gutter: columns are 172 wide.
        let placed = vec![Placed {
            index: 0,
            x: 56.0 + 172.0 + 2.0,
            y: 9.0 * hour,
            w: 168.0,
            h: hour - 2.0,
        }];
        let at = |x: f32, y: f32| hit(&placed, &blocks, 56.0, 172.0, hour, 2, (x, y));
        let top = at(300.0, 9.0 * hour + 2.0);
        let middle = at(300.0, 9.0 * hour + 28.0);
        let bottom = at(300.0, 10.0 * hour - 4.0);
        let empty = at(100.0, 3.0 * hour);
        assert!(matches!(
            top,
            Some(Target::Block {
                edge: Edge::Top,
                ..
            })
        ));
        assert!(matches!(
            middle,
            Some(Target::Block {
                edge: Edge::Body,
                ..
            })
        ));
        assert!(matches!(
            bottom,
            Some(Target::Block {
                edge: Edge::Bottom,
                ..
            })
        ));
        assert!(matches!(empty, Some(Target::Slot { column: 0, .. })));
        assert_eq!(at(10.0, 100.0), None);
    }

    #[test]
    fn a_block_that_cannot_be_edited_has_no_edges() {
        let blocks = vec![block("a", 0, 540, 600, false)];
        let placed = vec![Placed {
            index: 0,
            x: 58.0,
            y: 504.0,
            w: 100.0,
            h: 54.0,
        }];
        let top = hit(&placed, &blocks, 56.0, 172.0, 56.0, 1, (100.0, 505.0));
        assert!(matches!(
            top,
            Some(Target::Block {
                edge: Edge::Body,
                ..
            })
        ));
    }

    #[test]
    fn a_slot_is_in_the_column_under_the_pointer_and_at_the_minute() {
        let slot = hit(&[], &[], 56.0, 100.0, 60.0, 3, (260.0, 90.0));
        assert_eq!(
            slot,
            Some(Target::Slot {
                column: 2,
                minutes: 90.0
            })
        );
    }
}
