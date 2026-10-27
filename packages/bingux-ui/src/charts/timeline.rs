//! A timeline chart: bars placed along a time axis, with a limit line, the
//! bars over it picked out in the danger colour, and a range the user selects
//! by dragging.
//!
//! Use it for events that have a start and a size, such as the frame times of
//! an application (the inspector's Performance tab) or the round trips of a
//! network check against its timeout. Bars may be sparse: an idle stretch is
//! an empty stretch of the axis, not a squeezed one.
//!
//! ```ignore
//! use bingux_ui::charts::{Bar, Unit, timeline_chart};
//!
//! timeline_chart(frames.iter().map(|f| Bar::new(f.start, f.len, f.ms)))
//!     .x_range(0.0, 12.0)
//!     .x_unit(Unit::Suffix("s"))
//!     .y_unit(Unit::Suffix("ms"))
//!     .limit(16.7, "Budget")
//!     .selection(selected)
//!     .on_select(move |range| selected_state.set(range))
//! ```
//!
//! The selection belongs to the caller: the chart draws it, and reports a new
//! one with `on_select` when a drag ends (and `None` for a click). Keyboard: a
//! focused chart moves between bars with Left and Right, Home and End jump to
//! the ends, Shift with an arrow selects from where the move began, and
//! Escape clears the selection.

use std::rc::Rc;

use bytes::Bytes;
use freya::prelude::*;

use super::format::{Formatter, Unit, ValueText, format_tick, format_value};
use super::geometry::*;
use super::scale::{Linear, nice};
use super::svg::{Svg, num, paint, stroke_line};
use super::view::{
    Align as LabelAlign, Text, Tip, anchored, baseline, focus_outline, gutter_for, text_element,
    tooltip, x_labels, y_grid,
};
use crate::focus::use_focus_core;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink};

/// Width used before the chart has been measured.
const DEFAULT_WIDTH: f32 = 480.0;
/// The narrowest a bar is drawn, so a short event stays visible.
const MIN_BAR_WIDTH: f32 = 1.5;
/// A drag shorter than this many pixels is a click.
const CLICK_SLOP: f32 = 3.0;
/// How near, in pixels, the pointer must be to a bar to pick it.
const PICK_SLOP: f32 = 3.0;
/// The fewest seconds an axis spans, so a single event does not divide by zero.
const MIN_SPAN: f64 = 1.0e-3;
/// Bars are drawn at this opacity; the picked bar is drawn solid.
const BAR_OPACITY: f32 = 0.9;
const LIMIT_DASH: &str = "5 4";
/// The selection's edges and the picked bar's crosshair.
const EDGE_WIDTH: f32 = 1.5;

/// One bar: starts at `at` on the x axis, is `width` long there, and is
/// `value` tall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bar {
    pub at: f64,
    pub width: f64,
    pub value: f64,
}

impl Bar {
    pub fn new(at: f64, width: f64, value: f64) -> Self {
        Self { at, width, value }
    }
}

/// A timeline chart. Build it with [`timeline_chart`].
#[derive(Clone, PartialEq)]
pub struct TimelineChart {
    bars: Rc<Vec<Bar>>,
    x_range: Option<(f64, f64)>,
    y_max: Option<f64>,
    x_unit: Unit,
    y: ValueText,
    limit: Option<(f64, String)>,
    value_name: String,
    selection: Option<(f64, f64)>,
    on_select: Option<EventHandler<Option<(f64, f64)>>>,
    height: f32,
    label: Option<String>,
}

/// A timeline of `bars`, in ascending order of `at`.
pub fn timeline_chart(bars: impl IntoIterator<Item = Bar>) -> TimelineChart {
    TimelineChart {
        bars: Rc::new(bars.into_iter().collect()),
        x_range: None,
        y_max: None,
        x_unit: Unit::Number,
        y: ValueText::new(Unit::Number),
        limit: None,
        value_name: "Value".to_string(),
        selection: None,
        on_select: None,
        height: 150.0,
        label: None,
    }
}

impl TimelineChart {
    /// The span of the x axis. Without it the axis runs from zero to the end
    /// of the last bar.
    pub fn x_range(mut self, from: f64, to: f64) -> Self {
        self.x_range = Some((from, to));
        self
    }

    /// The top of the y axis. A taller bar is cut off at the top, and the
    /// tooltip still reads its real value. Without it the axis fits the data.
    pub fn y_max(mut self, top: f64) -> Self {
        self.y_max = Some(top);
        self
    }

    pub fn x_unit(mut self, unit: Unit) -> Self {
        self.x_unit = unit;
        self
    }

    pub fn y_unit(mut self, unit: Unit) -> Self {
        self.y.unit = unit;
        self
    }

    /// Writes the y values with `format` instead of a unit, on the axis and
    /// in the tooltip.
    pub fn y_format(mut self, format: impl Fn(f64) -> String + 'static) -> Self {
        self.y.custom = Some(Formatter::new(format));
        self
    }

    /// A dashed line at `value`, called `name` in the tooltip. Bars over it
    /// are drawn in the danger colour.
    pub fn limit(mut self, value: f64, name: impl Into<String>) -> Self {
        self.limit = Some((value, name.into()));
        self
    }

    /// What a bar's height is called in the tooltip.
    pub fn value_name(mut self, name: impl Into<String>) -> Self {
        self.value_name = name.into();
        self
    }

    /// The selected range of the x axis, as `(from, to)`.
    pub fn selection(mut self, range: Option<(f64, f64)>) -> Self {
        self.selection = range;
        self
    }

    /// Called when the user ends a drag with the new range, or clicks with
    /// `None`.
    pub fn on_select(mut self, handler: impl Into<EventHandler<Option<(f64, f64)>>>) -> Self {
        self.on_select = Some(handler.into());
        self
    }

    /// The chart's height. The plot is the part of it between the axes.
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// The accessible name.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// The geometry of one drawing: the plot box and the two scales.
#[derive(Clone, Copy)]
struct Geometry {
    plot: Plot,
    x: Linear,
    y: Linear,
}

impl Geometry {
    /// The x value under pixel `px`.
    fn at(&self, px: f32) -> f64 {
        let span = (self.x.to - self.x.from) as f64;
        if span <= 0.0 {
            return self.x.lo;
        }
        self.x.lo + f64::from(px - self.x.from) / span * (self.x.hi - self.x.lo)
    }
}

/// The bar the pointer is on or next to at `px`, preferring the tallest.
fn bar_at(bars: &[Bar], frame: &Geometry, px: f32) -> Option<usize> {
    let per_pixel = (frame.x.hi - frame.x.lo) / f64::from(frame.plot.width().max(1.0));
    let t = frame.at(px);
    let slop = f64::from(PICK_SLOP) * per_pixel;
    let first = bars.partition_point(|bar| bar.at + bar.width < t - slop);
    bars[first..]
        .iter()
        .take_while(|bar| bar.at <= t + slop)
        .enumerate()
        .max_by(|a, b| a.1.value.total_cmp(&b.1.value))
        .map(|(offset, _)| first + offset)
}

impl Component for TimelineChart {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut size = use_state(Area::default);
        let mut active = use_state(|| None::<usize>);
        let mut drag = use_state(|| None::<f32>);
        let mut preview = use_state(|| None::<(f64, f64)>);
        let mut anchor = use_state(|| None::<usize>);
        let tip_content = use_state(|| 0.0f32);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let focus_core = use_focus_core();

        let measured = size.read().width();
        let width = if measured > 0.0 {
            measured
        } else {
            DEFAULT_WIDTH
        };
        let height = self.height;
        let bars = self.bars.clone();

        // Axes. The x axis spans the caller's range or the bars; the y axis
        // reaches the limit's double or the tallest bar, whichever is higher.
        let (x_lo, mut x_hi) = self.x_range.unwrap_or_else(|| {
            let end = bars
                .iter()
                .map(|bar| bar.at + bar.width)
                .fold(0.0, f64::max);
            (0.0, end)
        });
        if !(x_hi - x_lo >= MIN_SPAN) {
            x_hi = x_lo + MIN_SPAN;
        }
        let tallest = bars.iter().map(|bar| bar.value).fold(0.0, f64::max);
        let limit = self.limit.as_ref().map(|(value, _)| *value);
        let wanted = self
            .y_max
            .unwrap_or_else(|| tallest.max(limit.map_or(0.0, |limit| limit * 2.0)));
        let y_domain = nice(0.0, wanted.max(MIN_SPAN), 4, true);
        // The limit is named in the gutter, in the danger colour; the tick
        // labels that would sit on it, and the zero the x axis already
        // starts from, are left out.
        let limit_text = self
            .limit
            .as_ref()
            .map(|(value, _)| self.y.value(*value));
        let gutter = limit_text
            .as_ref()
            .map_or(0.0, |text| text_width(text, TextSize::Sm.px()) + GUTTER_GAP);
        let gutter = gutter.max(gutter_for(&y_domain, &self.y));
        let plot = Plot::new(width, height, gutter);
        let frame = Geometry {
            plot,
            x: Linear::new((x_lo, x_hi), (plot.left, plot.right)),
            y: Linear::new((y_domain.lo, y_domain.hi), (plot.bottom, plot.top)),
        };

        let mut svg = Svg::new(width, height);
        let mut labelled = y_domain.clone();
        labelled.ticks.retain(|&tick| {
            tick != 0.0
                && limit.is_none_or(|limit| {
                    (frame.y.map(tick) - frame.y.map(limit)).abs() >= LABEL_HEIGHT
                })
        });
        let mut texts: Vec<Text> = y_grid(
            &mut svg,
            Paint::Muted.color(&tokens),
            plot,
            &frame.y,
            &labelled,
            &self.y,
            width,
        );
        let x_domain = nice(x_lo, x_hi, ticks_for(plot.width(), X_TICK_SPACING), false);
        let mut labels = Vec::new();
        for &tick in x_domain.ticks.iter().filter(|t| (x_lo..=x_hi).contains(t)) {
            let px = frame.x.map(tick);
            svg.push(stroke_line(
                crisp(px),
                plot.top,
                crisp(px),
                plot.bottom,
                Paint::Muted.color(&tokens).with_af32(GRID_OPACITY * 0.5),
                HAIRLINE,
            ));
            labels.push((px, format_tick(tick, self.x_unit, x_domain.step)));
        }
        x_labels(&mut texts, plot, width, labels);
        let zero = baseline(&mut svg, &tokens, plot, &frame.y, &y_domain);

        // Bars: the ones within the limit first, then the ones over it, so a
        // dense run of frames never hides a slow one.
        let mut within = String::new();
        let mut over = String::new();
        for bar in bars.iter() {
            let left = frame.x.map(bar.at);
            if left > plot.right || frame.x.map(bar.at + bar.width) < plot.left {
                continue;
            }
            let left = left.max(plot.left);
            let wide = (frame.x.map(bar.at + bar.width).min(plot.right) - left).max(MIN_BAR_WIDTH);
            let top = frame.y.map(bar.value.min(y_domain.hi)).min(zero - MIN_BAR);
            let path = format!(
                "M{} {}h{}v{}h-{}z",
                num(left),
                num(top),
                num(wide),
                num(zero - top),
                num(wide)
            );
            if limit.is_some_and(|limit| bar.value > limit) {
                over.push_str(&path);
            } else {
                within.push_str(&path);
            }
        }
        for (path, colour) in [
            (within, Paint::Accent.color(&tokens)),
            (over, Paint::Danger.color(&tokens)),
        ] {
            if !path.is_empty() {
                svg.push(format!(
                    r#"<path d="{path}" {}/>"#,
                    paint("fill", colour.with_af32(BAR_OPACITY))
                ));
            }
        }
        if let Some((value, _)) = &self.limit {
            let line = crisp(frame.y.map(*value));
            svg.push(format!(
                r#"<path d="M{} {}L{} {}" fill="none" {} stroke-width="1" stroke-dasharray="{LIMIT_DASH}"/>"#,
                num(plot.left),
                num(line),
                num(plot.right),
                num(line),
                paint("stroke", Paint::Danger.color(&tokens)),
            ));
            if let Some(text) = limit_text.clone() {
                texts.push(anchored(
                    CAPTION.in_ink(Ink::Danger),
                    text,
                    plot.left - GUTTER_GAP,
                    line - LABEL_HEIGHT / 2.0,
                    plot.left - GUTTER_GAP,
                    LabelAlign::Right,
                    width,
                ));
            }
        }
        let svg = svg.finish();

        // Overlays that change with the pointer are plain elements, so a
        // hover or a drag does not redraw the bars.
        let shown = preview().or(self.selection);
        let picked = if drag().is_some() { None } else { active() };
        let on_select = self.on_select.clone();
        let selection_overlay = shown.map(|(from, to)| {
            let (a, b) = (frame.x.map(from.min(to)), frame.x.map(from.max(to)));
            let (a, b) = (
                a.clamp(plot.left, plot.right),
                b.clamp(plot.left, plot.right),
            );
            region(
                &tokens,
                a,
                plot.top,
                (b - a).max(EDGE_WIDTH),
                plot.height(),
                Paint::AccentSoft,
            )
            .child(edge(&tokens, 0.0, plot.height()))
            .child(edge(&tokens, (b - a - EDGE_WIDTH).max(0.0), plot.height()))
        });
        let pick = picked.and_then(|index| bars.get(index)).map(|bar| {
            let centre = frame.x.map(bar.at + bar.width / 2.0);
            let left = frame.x.map(bar.at);
            let wide = (frame.x.map(bar.at + bar.width) - left).max(MIN_BAR_WIDTH + 1.0);
            let top = frame.y.map(bar.value.min(y_domain.hi)).min(zero - MIN_BAR);
            (
                region(
                    &tokens,
                    centre - 0.5,
                    plot.top,
                    1.0,
                    plot.height(),
                    Paint::Muted,
                ),
                region(&tokens, left, top, wide, zero - top, Paint::Text),
            )
        });
        let (pick_line, pick_bar) = match pick {
            Some((line, bar)) => (Some(line), Some(bar)),
            None => (None, None),
        };
        let tip = picked.and_then(|index| bars.get(index)).map(|bar| {
            let over = limit.is_some_and(|limit| bar.value > limit);
            let mut rows = vec![super::view::TipRow {
                colour: if over {
                    Paint::Danger.color(&tokens)
                } else {
                    Paint::Accent.color(&tokens)
                },
                name: self.value_name.clone(),
                value: self.y.value(bar.value),
            }];
            if let Some((value, name)) = &self.limit {
                rows.push(super::view::TipRow {
                    colour: Paint::Muted.color(&tokens),
                    name: name.clone(),
                    value: self.y.value(*value),
                });
            }
            let tip = Tip {
                anchor: frame.x.map(bar.at + bar.width / 2.0),
                reach: (frame.x.map(bar.at + bar.width) - frame.x.map(bar.at)).max(MIN_BAR_WIDTH)
                    / 2.0,
                top: plot.top,
                title: format_value(bar.at, self.x_unit),
                rows,
                total: None,
            };
            tooltip(&tokens, &tip, width, height, tip_content)
        });
        let outline = focus_outline(&tokens, focus_core, focus(), width, height);

        // Pointer. Positions are relative to the chart's box.
        let local = move |event: &Event<PointerEventData>| {
            let area = size.read();
            (
                event.global_location().x as f32 - area.min_x(),
                event.global_location().y as f32 - area.min_y(),
            )
        };
        let move_bars = bars.clone();
        let on_move = move |event: Event<PointerEventData>| {
            let (x, y) = local(&event);
            if let Some(from) = drag() {
                if (x - from).abs() >= CLICK_SLOP {
                    let (a, b) = (frame.at(from), frame.at(x.clamp(plot.left, plot.right)));
                    preview.set(Some((a.min(b), a.max(b))));
                }
                return;
            }
            let inside = (plot.left..=plot.right).contains(&x) && (0.0..=height).contains(&y);
            active.set_if_modified(if inside {
                bar_at(&move_bars, &frame, x)
            } else {
                None
            });
        };
        let on_down = move |event: Event<PointerEventData>| {
            let (x, _) = local(&event);
            if (plot.left..=plot.right).contains(&x) {
                drag.set(Some(x));
                a11y_id.request_focus();
            }
        };
        let up_select = on_select.clone();
        let on_up = move |event: Event<PointerEventData>| {
            let Some(from) = drag.take() else {
                return;
            };
            let (x, _) = local(&event);
            let range = preview.take();
            if let Some(handler) = &up_select {
                if (x - from).abs() < CLICK_SLOP {
                    handler.call(None);
                } else {
                    handler.call(range);
                }
            }
            anchor.set(None);
        };

        // Keyboard.
        let key_bars = bars.clone();
        let on_key = move |event: Event<KeyboardEventData>| {
            let count = key_bars.len();
            if let Key::Named(NamedKey::Escape) = event.key {
                event.stop_propagation();
                active.set(None);
                anchor.set(None);
                if let Some(handler) = &on_select {
                    handler.call(None);
                }
                return;
            }
            if count == 0 {
                return;
            }
            let current = *active.peek();
            let next = match event.key {
                Key::Named(NamedKey::ArrowRight) => {
                    current.map_or(0, |index| (index + 1).min(count - 1))
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    current.map_or(count - 1, |index| index.saturating_sub(1))
                }
                Key::Named(NamedKey::Home) => 0,
                Key::Named(NamedKey::End) => count - 1,
                _ => return,
            };
            event.stop_propagation();
            if event.modifiers.contains(Modifiers::SHIFT) {
                let start = (*anchor.peek()).or(current).unwrap_or(next);
                anchor.set(Some(start));
                let (a, b) = (&key_bars[start.min(next)], &key_bars[start.max(next)]);
                if let Some(handler) = &on_select {
                    handler.call(Some((a.at, b.at + b.width)));
                }
            } else {
                anchor.set(None);
            }
            active.set(Some(next));
        };

        let mut chart = rect()
            .width(Size::fill())
            .min_width(Size::px(NO_MIN))
            .height(Size::px(height))
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::GenericContainer)
            .a11y_alt(
                self.label
                    .clone()
                    .unwrap_or_else(|| "Timeline chart".to_string()),
            )
            .on_sized(move |event: Event<SizedEventData>| size.set(event.area))
            .on_pointer_down(on_down)
            .on_global_pointer_up(on_up)
            .on_capture_global_pointer_move(on_move)
            .on_key_down(on_key)
            .child(
                SvgViewer::new((svg.clone(), Bytes::from(svg)))
                    .width(Size::fill())
                    .height(Size::px(height))
                    .into_element(),
            );
        for text in texts {
            chart = chart.child(text_element(&tokens, text));
        }
        chart
            .maybe_child(selection_overlay)
            .maybe_child(pick_line)
            .maybe_child(pick_bar)
            .maybe_child(tip)
            .child(outline)
    }
}

/// An absolutely placed box in a paint. The paint is a token, so it follows
/// the theme.
fn region(
    tokens: &crate::theme::Tokens,
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    paint: Paint,
) -> Rect {
    rect()
        .position(Position::new_absolute().top(top).left(left))
        .width(Size::px(width))
        .height(Size::px(height))
        .tw(tokens, &[bg(paint)])
}

/// One edge of the selection.
fn edge(tokens: &crate::theme::Tokens, left: f32, height: f32) -> Rect {
    region(tokens, left, 0.0, EDGE_WIDTH, height, Paint::Accent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Geometry {
        let plot = Plot::new(200.0, 100.0, 40.0);
        Geometry {
            plot,
            x: Linear::new((0.0, 10.0), (plot.left, plot.right)),
            y: Linear::new((0.0, 40.0), (plot.bottom, plot.top)),
        }
    }

    #[test]
    fn the_pointer_picks_the_tallest_bar_in_reach() {
        let frame = frame();
        let bars = [
            Bar::new(2.0, 0.01, 10.0),
            Bar::new(2.05, 0.01, 30.0),
            Bar::new(8.0, 0.01, 5.0),
        ];
        let px = frame.x.map(2.03);
        assert_eq!(bar_at(&bars, &frame, px), Some(1));
        assert_eq!(bar_at(&bars, &frame, frame.x.map(5.0)), None);
    }

    #[test]
    fn pixels_map_back_to_the_axis() {
        let frame = frame();
        let px = frame.x.map(7.5);
        assert!((frame.at(px) - 7.5).abs() < 1e-4);
    }
}
