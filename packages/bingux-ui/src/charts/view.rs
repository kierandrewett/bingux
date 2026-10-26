//! The chart components: sparkline, line and area, bar, ring gauge and donut.
//!
//! Every chart is a Freya component. It measures its box, draws an SVG at
//! that size, and lays its text, legend and tooltip over it as ordinary
//! elements, because SVG text does not render here. The public builders
//! ([`line_chart`] and friends) only hold data and options; [`Chart`] does
//! the drawing, hover and keyboard work for all of them.

use bytes::Bytes;
use freya::animation::*;
use freya::prelude::*;

use super::data::{self, Point, Segment, Series, SeriesColour};
use super::format::{Unit, format_tick, format_value};
use super::geometry::*;
use super::palette;
use super::path::{self, Pt};
use super::scale::{Domain, Linear, lone, nice, time};
use super::svg::{self, Svg, clip_rect, num, stroke_line};
use crate::focus::{FocusCore, use_focus_core};
use crate::motion::wrap_focusable_with;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{self, CAPTION, Ink, TextStyle};
use std::f32::consts::TAU;

/// Width used before the chart has been measured.
const DEFAULT_WIDTH: f32 = 480.0;
const EMPTY_TEXT: &str = "No data yet";
/// The band behind the hovered bar.
const BAND_OPACITY: f32 = 0.10;
/// Sparklines keep this much room at each side for their min and max marks.
const SPARK_INSET: f32 = ACTIVE_RADIUS + 1.0;
/// Sparklines keep this much room above and below the line for their marks.
const SPARK_EDGE: f32 = SPARK_PAD + ACTIVE_RADIUS;
/// Sparklines pad the data range by this share of its span, or of its value
/// when the data is flat.
const SPARK_MARGIN: f64 = 0.12;
const FLAT_MARGIN: f64 = 0.1;
/// A ring under this sweep is too small to draw.
const MIN_SWEEP: f32 = 0.001;

/// How a line or area joins its points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Curve {
    /// Straight segments between points.
    Linear,
    /// Holds each value until the next x, then steps.
    Step,
    /// A smooth curve through every point that never overshoots.
    #[default]
    Monotone,
}

/// A compact trend line with no axes. Its minimum and maximum get a dot.
#[derive(Clone, Debug, PartialEq)]
pub struct Sparkline {
    values: Vec<f64>,
    colour: SeriesColour,
    unit: Unit,
    area: bool,
    label: Option<String>,
}

/// A chart of one or more series over a shared x axis.
#[derive(Clone, Debug, PartialEq)]
pub struct LineChart {
    series: Vec<Series>,
    y_unit: Unit,
    x_unit: Unit,
    curve: Curve,
    area: bool,
    label: Option<String>,
    height: f32,
}

/// Bars over the distinct x values of its series. Several series are drawn
/// side by side, or stacked.
#[derive(Clone, Debug, PartialEq)]
pub struct BarChart {
    series: Vec<Series>,
    y_unit: Unit,
    categories: Vec<String>,
    stacked: bool,
    label: Option<String>,
    height: f32,
}

/// A gauge for a fraction in `0..=1`, drawn as a 270 degree arc.
#[derive(Clone, Debug, PartialEq)]
pub struct Ring {
    value: f64,
    caption: Option<String>,
    colour: SeriesColour,
    label: Option<String>,
}

/// Shares of a whole as a ring of arcs, with a legend beside it.
#[derive(Clone, Debug, PartialEq)]
pub struct Donut {
    segments: Vec<Segment>,
    unit: Unit,
    caption: Option<String>,
    label: Option<String>,
}

/// A sparkline of `values`, read by position.
pub fn sparkline(values: impl IntoIterator<Item = f64>) -> Sparkline {
    Sparkline {
        values: values.into_iter().collect(),
        colour: SeriesColour::Auto,
        unit: Unit::Number,
        area: false,
        label: None,
    }
}

/// A line chart of `series`.
pub fn line_chart(series: impl IntoIterator<Item = Series>) -> LineChart {
    LineChart {
        series: series.into_iter().collect(),
        y_unit: Unit::Number,
        x_unit: Unit::Number,
        curve: Curve::Monotone,
        area: false,
        label: None,
        height: DEFAULT_HEIGHT,
    }
}

/// A bar chart of `series`.
pub fn bar_chart(series: impl IntoIterator<Item = Series>) -> BarChart {
    BarChart {
        series: series.into_iter().collect(),
        y_unit: Unit::Number,
        categories: Vec::new(),
        stacked: false,
        label: None,
        height: DEFAULT_HEIGHT,
    }
}

/// A gauge showing `value`, a fraction in `0..=1`. Other values are clamped.
pub fn ring(value: f64) -> Ring {
    Ring {
        value,
        caption: None,
        colour: SeriesColour::Auto,
        label: None,
    }
}

/// A donut of `segments`, sized by their values.
pub fn donut(segments: impl IntoIterator<Item = Segment>) -> Donut {
    Donut {
        segments: segments.into_iter().collect(),
        unit: Unit::Number,
        caption: None,
        label: None,
    }
}

impl Sparkline {
    pub fn colour(mut self, colour: SeriesColour) -> Self {
        self.colour = colour;
        self
    }

    pub fn unit(mut self, unit: Unit) -> Self {
        self.unit = unit;
        self
    }

    /// Fills the area under the line with a fade.
    pub fn area(mut self) -> Self {
        self.area = true;
        self
    }

    /// The accessible name of the chart.
    pub fn label(mut self, text: impl Into<String>) -> Self {
        self.label = Some(text.into());
        self
    }
}

impl LineChart {
    /// How the y values are written on the axis and in tooltips.
    pub fn y_unit(mut self, unit: Unit) -> Self {
        self.y_unit = unit;
        self
    }

    /// How the x values are written. Use [`Unit::Clock`] for Unix seconds.
    pub fn x_unit(mut self, unit: Unit) -> Self {
        self.x_unit = unit;
        self
    }

    pub fn curve(mut self, curve: Curve) -> Self {
        self.curve = curve;
        self
    }

    /// Fills the area under each line with a fade to nothing at the base.
    pub fn area(mut self) -> Self {
        self.area = true;
        self
    }

    pub fn label(mut self, text: impl Into<String>) -> Self {
        self.label = Some(text.into());
        self
    }

    /// The height of the plot, legend excluded.
    pub fn height(mut self, height: Dim) -> Self {
        self.height = height.px();
        self
    }
}

impl BarChart {
    pub fn y_unit(mut self, unit: Unit) -> Self {
        self.y_unit = unit;
        self
    }

    /// Names for the bands, in the order of the distinct x values. Bands
    /// without a name show their x value.
    pub fn categories<S: Into<String>>(mut self, names: impl IntoIterator<Item = S>) -> Self {
        self.categories = names.into_iter().map(Into::into).collect();
        self
    }

    /// Stacks the series on each other instead of placing them side by side.
    pub fn stacked(mut self) -> Self {
        self.stacked = true;
        self
    }

    pub fn label(mut self, text: impl Into<String>) -> Self {
        self.label = Some(text.into());
        self
    }

    /// The height of the plot, legend excluded.
    pub fn height(mut self, height: Dim) -> Self {
        self.height = height.px();
        self
    }
}

impl Ring {
    /// A line of text under the percentage in the centre.
    pub fn caption(mut self, text: impl Into<String>) -> Self {
        self.caption = Some(text.into());
        self
    }

    pub fn colour(mut self, colour: SeriesColour) -> Self {
        self.colour = colour;
        self
    }

    pub fn label(mut self, text: impl Into<String>) -> Self {
        self.label = Some(text.into());
        self
    }
}

impl Donut {
    /// How the segment values are written in the legend and the centre.
    pub fn unit(mut self, unit: Unit) -> Self {
        self.unit = unit;
        self
    }

    /// A line of text under the total in the centre, such as "Total".
    pub fn caption(mut self, text: impl Into<String>) -> Self {
        self.caption = Some(text.into());
        self
    }

    pub fn label(mut self, text: impl Into<String>) -> Self {
        self.label = Some(text.into());
        self
    }
}

impl Component for Sparkline {
    fn render(&self) -> impl IntoElement {
        let chart: Element = Chart {
            spec: Spec::Sparkline(self.clone()),
        }
        .into();
        chart
    }
}

impl Component for LineChart {
    fn render(&self) -> impl IntoElement {
        let chart: Element = Chart {
            spec: Spec::Line(self.clone()),
        }
        .into();
        chart
    }
}

impl Component for BarChart {
    fn render(&self) -> impl IntoElement {
        let chart: Element = Chart {
            spec: Spec::Bars(self.clone()),
        }
        .into();
        chart
    }
}

impl Component for Ring {
    fn render(&self) -> impl IntoElement {
        let chart: Element = Chart {
            spec: Spec::Ring(self.clone()),
        }
        .into();
        chart
    }
}

impl Component for Donut {
    fn render(&self) -> impl IntoElement {
        let chart: Element = Chart {
            spec: Spec::Donut(self.clone()),
        }
        .into();
        chart
    }
}

/// The data behind one chart, as the public builders hold it.
#[derive(Clone, Debug, PartialEq)]
enum Spec {
    Sparkline(Sparkline),
    Line(LineChart),
    Bars(BarChart),
    Ring(Ring),
    Donut(Donut),
}

impl Spec {
    /// The height the chart takes, fixed from the first frame.
    fn height(&self) -> f32 {
        match self {
            Spec::Sparkline(_) => SPARK_HEIGHT,
            Spec::Line(chart) => chart.height,
            Spec::Bars(chart) => chart.height,
            Spec::Ring(_) => RING_SIZE,
            Spec::Donut(donut) => DONUT_SIZE.max(donut.segments.len() as f32 * DONUT_ROW),
        }
    }

    /// The accessible name, the caller's label or a default.
    fn alt(&self) -> String {
        let (label, default) = match self {
            Spec::Sparkline(chart) => (&chart.label, "Sparkline"),
            Spec::Line(chart) => (&chart.label, "Line chart"),
            Spec::Bars(chart) => (&chart.label, "Bar chart"),
            Spec::Ring(chart) => (&chart.label, "Gauge"),
            Spec::Donut(chart) => (&chart.label, "Donut chart"),
        };
        label.clone().unwrap_or_else(|| default.to_string())
    }
}

/// A chart element. Each public chart renders one of these.
#[derive(PartialEq)]
struct Chart {
    spec: Spec,
}

impl Component for Chart {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut size = use_state(Area::default);
        let mut active = use_state(|| None::<usize>);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let focus_core = use_focus_core();
        let draw = use_animation(|conf| {
            conf.on_creation(OnCreation::Run);
            AnimNum::new(0.0, 1.0)
                .time(DRAW_MS)
                .function(Function::Cubic)
                .ease(Ease::Out)
        });
        let progress = if tokens.reduced_motion {
            1.0
        } else {
            draw.get().value()
        };

        let spec = &self.spec;
        match spec {
            Spec::Ring(ring) => return ring_element(&tokens, ring, progress),
            Spec::Donut(donut) => return donut_element(&tokens, donut, progress),
            Spec::Sparkline(_) | Spec::Line(_) | Spec::Bars(_) => {}
        }

        let measured = size.read().width();
        let width = if measured > 0.0 {
            measured
        } else {
            DEFAULT_WIDTH
        };
        let height = spec.height();
        let scene = build(spec, &tokens, width, progress, active());
        let interactive = !scene.anchors.is_empty();
        let count = scene.anchors.len();
        let anchors = scene.anchors.clone();

        let on_move = move |event: Event<PointerEventData>| {
            let (min_x, min_y) = {
                let area = size.read();
                (area.min_x(), area.min_y())
            };
            let x = event.global_location().x as f32 - min_x;
            let y = event.global_location().y as f32 - min_y;
            let inside = (0.0..=width).contains(&x) && (0.0..=height).contains(&y);
            active.set_if_modified(if inside {
                nearest_anchor(&anchors, x)
            } else {
                None
            });
        };
        let on_key = move |event: Event<KeyboardEventData>| {
            if count == 0 {
                return;
            }
            let current = *active.peek();
            let next = match event.key {
                Key::Named(NamedKey::ArrowRight) => {
                    Some(current.map_or(0, |index| (index + 1).min(count - 1)))
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    Some(current.map_or(count - 1, |index| index.saturating_sub(1)))
                }
                Key::Named(NamedKey::Home) => Some(0),
                Key::Named(NamedKey::End) => Some(count - 1),
                Key::Named(NamedKey::Escape) => None,
                _ => return,
            };
            active.set_if_modified(next);
        };

        let alt = spec.alt();
        let legend = (!scene.legend.is_empty()).then(|| legend_row(&tokens, &scene.legend));
        let tip = scene
            .tip
            .as_ref()
            .map(|tip| tooltip(&tokens, tip, width, height));
        let outline = interactive.then(|| focus_outline(&tokens, focus_core, focus(), width, height));

        let chart = drawing(scene, &tokens, width, height)
            .width(Size::fill())
            .min_width(Size::px(NO_MIN))
            .a11y_id(a11y_id)
            .a11y_focusable(interactive)
            .a11y_role(AccessibilityRole::GenericContainer)
            .a11y_alt(alt)
            .on_sized(move |event: Event<SizedEventData>| size.set(event.area))
            .maybe(interactive, |chart| {
                chart
                    .on_capture_global_pointer_move(on_move)
                    .on_key_down(on_key)
            })
            .maybe_child(tip)
            .maybe_child(outline);

        rect()
            .width(Size::fill())
            .min_width(Size::px(NO_MIN))
            .child(chart)
            .maybe_child(legend)
            .into_element()
    }
}

/// Where a label sits. `Left`, `Centre` and `Right` name the edge at `x`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Align {
    Left,
    Centre,
    Right,
}

fn text_align(align: Align) -> TextAlign {
    match align {
        Align::Left => TextAlign::Left,
        Align::Centre => TextAlign::Center,
        Align::Right => TextAlign::Right,
    }
}

/// A label placed at a pixel box, drawn as a Freya label over the SVG.
pub(super) struct Text {
    left: f32,
    top: f32,
    width: f32,
    text: String,
    style: TextStyle,
    align: Align,
}

impl Text {
    pub(super) fn new(
        style: TextStyle,
        text: impl Into<String>,
        left: f32,
        top: f32,
        width: f32,
        align: Align,
    ) -> Self {
        Self {
            left,
            top,
            width,
            text: text.into(),
            style,
            align,
        }
    }
}

/// A label anchored at `x` by its `align` edge, kept inside `bounds`.
pub(super) fn anchored(
    style: TextStyle,
    text: impl Into<String>,
    x: f32,
    top: f32,
    width: f32,
    align: Align,
    bounds: f32,
) -> Text {
    let left = match align {
        Align::Left => x,
        Align::Centre => x - width / 2.0,
        Align::Right => x - width,
    };
    Text::new(
        style,
        text,
        left.clamp(0.0, (bounds - width).max(0.0)),
        top,
        width,
        align,
    )
}

pub(super) fn text_element(tokens: &Tokens, text: Text) -> impl IntoElement {
    rect()
        .position(Position::new_absolute().top(text.top).left(text.left))
        .width(Size::px(text.width))
        .height(Size::px(LABEL_HEIGHT))
        .child(
            text.style
                .label(tokens, text.text)
                .text_align(text_align(text.align)),
        )
}

/// What a chart draws: its SVG, labels over it, hover stops and a tooltip.
#[derive(Default)]
struct Scene {
    svg: String,
    texts: Vec<Text>,
    /// The pixel x of each hover stop, ascending.
    anchors: Vec<f32>,
    tip: Option<Tip>,
    legend: Vec<(Color, String)>,
}

/// The tooltip for the hovered stop.
pub(super) struct Tip {
    pub(super) anchor: f32,
    pub(super) top: f32,
    pub(super) title: String,
    pub(super) rows: Vec<TipRow>,
}

pub(super) struct TipRow {
    pub(super) colour: Color,
    pub(super) name: String,
    pub(super) value: String,
}

fn nearest_anchor(anchors: &[f32], x: f32) -> Option<usize> {
    anchors
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
        .map(|(index, _)| index)
}

fn build(spec: &Spec, tokens: &Tokens, width: f32, progress: f32, active: Option<usize>) -> Scene {
    match spec {
        Spec::Sparkline(chart) => sparkline_scene(chart, tokens, width, progress, active),
        Spec::Line(chart) => line_scene(chart, tokens, width, progress, active),
        Spec::Bars(chart) => bars_scene(chart, tokens, width, progress, active),
        // Rings and donuts have fixed boxes and their own elements.
        Spec::Ring(_) | Spec::Donut(_) => Scene::default(),
    }
}

/// The chart box: the SVG at its size, then its labels.
fn drawing(scene: Scene, tokens: &Tokens, width: f32, height: f32) -> Rect {
    let Scene { svg, texts, .. } = scene;
    let mut area = rect()
        .width(Size::px(width))
        .height(Size::px(height))
        .child(
            SvgViewer::new((svg.clone(), Bytes::from(svg)))
                .width(Size::fill())
                .height(Size::px(height))
                .into_element(),
        );
    for text in texts {
        area = area.child(text_element(tokens, text));
    }
    area
}

pub(super) fn focus_outline(
    tokens: &Tokens,
    core: FocusCore,
    focus: Focus,
    width: f32,
    height: f32,
) -> Rect {
    wrap_focusable_with(
        tokens,
        core,
        rect().width(Size::px(width)).height(Size::px(height)),
        focus,
        Radius::Card,
        false,
        None,
    )
    .position(Position::new_absolute().top(0.0).left(0.0))
    .layer(Layer::Relative(TIP_LAYER))
}

/// The popover card that follows the hovered stop.
pub(super) fn tooltip(tokens: &Tokens, tip: &Tip, width: f32, height: f32) -> Rect {
    let card_height = TIP_PAD * 2.0 + TIP_ROW * (tip.rows.len() as f32 + 1.0);
    let fits_right = tip.anchor + TIP_GAP + TIP_WIDTH <= width;
    let left = if fits_right {
        tip.anchor + TIP_GAP
    } else {
        tip.anchor - TIP_GAP - TIP_WIDTH
    }
    .clamp(0.0, (width - TIP_WIDTH).max(0.0));
    let top = tip.top.clamp(0.0, (height - card_height).max(0.0));
    let inner = TIP_WIDTH - TIP_PAD * 2.0;

    let mut card = rect()
        .position(Position::new_absolute().top(top).left(left))
        .layer(Layer::Relative(TIP_LAYER))
        .width(Size::px(TIP_WIDTH))
        .height(Size::px(card_height))
        .tw(tokens, TOOLTIP)
        .tw(tokens, &[shadow(Elevation::Raised)])
        .child(text_element(
            tokens,
            Text::new(
                CAPTION.in_ink(Ink::Text),
                tip.title.clone(),
                TIP_PAD,
                TIP_PAD,
                inner,
                Align::Left,
            ),
        ));
    for (index, row) in tip.rows.iter().enumerate() {
        let top = TIP_PAD + TIP_ROW * (index as f32 + 1.0);
        card = card
            .child(
                palette::swatch(row.colour).position(
                    Position::new_absolute()
                        .top(top + (TIP_ROW - SWATCH) / 2.0)
                        .left(TIP_PAD),
                ),
            )
            .child(text_element(
                tokens,
                Text::new(
                    CAPTION.in_ink(Ink::Muted),
                    row.name.clone(),
                    TIP_PAD + SWATCH + S2.px(),
                    top,
                    inner - SWATCH - S2.px(),
                    Align::Left,
                ),
            ))
            .child(text_element(
                tokens,
                Text::new(
                    CAPTION.in_ink(Ink::Text),
                    row.value.clone(),
                    TIP_PAD,
                    top,
                    inner,
                    Align::Right,
                ),
            ));
    }
    card
}

/// The legend under a multi-series chart.
fn legend_row(tokens: &Tokens, entries: &[(Color, String)]) -> Rect {
    let mut row = rect()
        .width(Size::fill())
        .height(Size::px(LABEL_HEIGHT))
        .tw(
            tokens,
            &[flex_row(), gap(S4), items(crate::style::Align::Center)],
        );
    for (colour, name) in entries {
        row = row.child(
            rect()
                .tw(
                    tokens,
                    &[flex_row(), gap(S1), items(crate::style::Align::Center)],
                )
                .child(palette::swatch(*colour))
                .child(CAPTION.label(tokens, name.clone())),
        );
    }
    rect()
        .width(Size::fill())
        .child(rect().height(Size::px(LEGEND_GAP)))
        .child(row)
}

/// A scene with a muted baseline and a note, at the chart's height.
fn empty(tokens: &Tokens, width: f32, height: f32) -> Scene {
    let plot = Plot::new(width, height, GUTTER_MIN);
    let mut svg = Svg::new(width, height);
    let baseline = crisp(plot.bottom);
    svg.push(stroke_line(
        plot.left,
        baseline,
        plot.right,
        baseline,
        Paint::Muted.color(tokens).with_af32(BASELINE_OPACITY),
        HAIRLINE,
    ));
    let centre = (plot.left + plot.right) / 2.0;
    let middle = (plot.top + plot.bottom) / 2.0 - LABEL_HEIGHT / 2.0;
    let texts = vec![anchored(
        CAPTION,
        EMPTY_TEXT,
        centre,
        middle,
        text_width(EMPTY_TEXT, TextSize::Sm.px()) + LABEL_PAD,
        Align::Centre,
        width,
    )];
    Scene {
        svg: svg.finish(),
        texts,
        ..Scene::default()
    }
}

fn series_colours(tokens: &Tokens, series: &[Series]) -> Vec<Color> {
    series
        .iter()
        .enumerate()
        .map(|(index, series)| palette::resolve(tokens, series.colour, index))
        .collect()
}

/// The left gutter that fits the widest y label, and at least the minimum.
pub(super) fn gutter_for(domain: &Domain, unit: Unit) -> f32 {
    let widest = domain
        .ticks
        .iter()
        .map(|&tick| text_width(&format_tick(tick, unit, domain.step), TextSize::Sm.px()))
        .fold(0.0, f32::max);
    (widest + GUTTER_GAP).max(GUTTER_MIN)
}

/// Horizontal grid lines, returning their y labels. The caller places the
/// baseline.
pub(super) fn y_grid(
    svg: &mut Svg,
    grid: Color,
    plot: Plot,
    y: &Linear,
    domain: &Domain,
    unit: Unit,
    width: f32,
) -> Vec<Text> {
    let grid = grid.with_af32(GRID_OPACITY);
    let mut texts = Vec::new();
    for &tick in &domain.ticks {
        let value = y.map(tick);
        svg.push(stroke_line(
            plot.left,
            crisp(value),
            plot.right,
            crisp(value),
            grid,
            HAIRLINE,
        ));
        texts.push(anchored(
            CAPTION,
            format_tick(tick, unit, domain.step),
            plot.left - GUTTER_GAP,
            value - LABEL_HEIGHT / 2.0,
            plot.left - GUTTER_GAP,
            Align::Right,
            width,
        ));
    }
    texts
}

/// The axis line at zero, or at the bottom of the plot when zero is off the
/// domain. Returns its y pixel, which bars grow from.
pub(super) fn baseline(
    svg: &mut Svg,
    tokens: &Tokens,
    plot: Plot,
    y: &Linear,
    domain: &Domain,
) -> f32 {
    let zero = if domain.lo <= 0.0 && domain.hi >= 0.0 {
        y.map(0.0)
    } else {
        plot.bottom
    };
    svg.push(stroke_line(
        plot.left,
        crisp(zero),
        plot.right,
        crisp(zero),
        Paint::Muted.color(tokens).with_af32(BASELINE_OPACITY),
        HAIRLINE,
    ));
    zero
}

/// X labels at `(pixel, text)`, thinned so none collide.
pub(super) fn x_labels(
    texts: &mut Vec<Text>,
    plot: Plot,
    width: f32,
    labels: Vec<(f32, String)>,
) {
    let mut last: Option<(f32, f32)> = None;
    for (px, name) in labels {
        let box_width = text_width(&name, TextSize::Sm.px()) + LABEL_PAD;
        if let Some((last_px, last_width)) = last
            && px - last_px < (last_width + box_width) / 2.0 + LABEL_GAP
        {
            continue;
        }
        texts.push(anchored(
            CAPTION,
            name,
            px,
            plot.bottom + LABEL_GAP,
            box_width,
            Align::Centre,
            width,
        ));
        last = Some((px, box_width));
    }
}

fn curve_path(curve: Curve, points: &[Pt]) -> String {
    match curve {
        Curve::Linear => path::linear(points),
        Curve::Step => path::step_after(points),
        Curve::Monotone => path::monotone(points),
    }
}

/// A gradient fill under `line`, down to `bottom`.
fn area_fill(svg: &mut Svg, line: &str, points: &[Pt], bottom: f32, colour: Color) {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return;
    };
    let gradient = svg.id("fill");
    svg.def(svg::linear_gradient(
        &gradient,
        &[
            (0.0, colour.with_af32(FILL_TOP)),
            (1.0, colour.with_af32(0.0)),
        ],
        true,
    ));
    svg.push(format!(
        r#"<path d="{line}L{} {}L{} {}Z" fill="url(#{gradient})" stroke="none"/>"#,
        num(last.0),
        num(bottom),
        num(first.0),
        num(bottom),
    ));
}

fn stroke_curve(svg: &mut Svg, d: &str, colour: Color) {
    svg.push(format!(
        r#"<path d="{d}" fill="none" {} stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"/>"#,
        svg::paint("stroke", colour),
        num(LINE_WIDTH),
    ));
}

fn sparkline_scene(
    spark: &Sparkline,
    tokens: &Tokens,
    width: f32,
    progress: f32,
    active: Option<usize>,
) -> Scene {
    let height = SPARK_HEIGHT;
    let points = Series::values("", spark.values.iter().copied()).cleaned();
    let Some(bounds) = data::bounds(std::slice::from_ref(&points)) else {
        return empty(tokens, width, height);
    };
    let colour = palette::resolve(tokens, spark.colour, 0);
    let span = bounds.y_max - bounds.y_min;
    let pad = if span > 0.0 {
        span * SPARK_MARGIN
    } else {
        bounds.y_max.abs().max(1.0) * FLAT_MARGIN
    };
    let x = Linear::new(
        (bounds.x_min, bounds.x_max),
        (SPARK_INSET, width - SPARK_INSET),
    );
    let y = Linear::new(
        (bounds.y_min - pad, bounds.y_max + pad),
        (height - SPARK_EDGE, SPARK_EDGE),
    );
    let px: Vec<Pt> = points
        .iter()
        .map(|point| (x.map(point.x), y.map(point.y)))
        .collect();

    let mut svg = Svg::new(width, height);
    let reveal = svg.id("reveal");
    svg.def(clip_rect(&reveal, width * progress, height));
    svg.push(format!(r#"<g clip-path="url(#{reveal})">"#));
    let line = curve_path(Curve::Monotone, &px);
    if spark.area {
        area_fill(&mut svg, &line, &px, height - SPARK_EDGE, colour);
    }
    if px.len() >= 2 {
        stroke_curve(&mut svg, &line, colour);
    }

    // The minimum and maximum get a dot. A single value gets one dot.
    let mut marks = Vec::new();
    if points.len() == 1 {
        marks.push(0);
    } else if let (Some(low), Some(high)) = (
        index_of(&points, |a, b| a.y < b.y),
        index_of(&points, |a, b| a.y > b.y),
    ) {
        marks.push(low);
        if high != low {
            marks.push(high);
        }
    }
    for index in marks {
        let (cx, cy) = px[index];
        svg.push(svg::dot(
            cx,
            cy,
            DOT_RADIUS,
            colour,
            Some((Paint::Card.color(tokens), LINE_WIDTH)),
        ));
    }
    svg.push("</g>");

    let anchors: Vec<f32> = px.iter().map(|point| point.0).collect();
    let tip = active.and_then(|index| {
        let point = *points.get(index)?;
        let (cx, cy) = *px.get(index)?;
        svg.push(svg::dot(
            cx,
            cy,
            ACTIVE_RADIUS,
            colour,
            Some((Paint::Card.color(tokens), LINE_WIDTH)),
        ));
        Some(Tip {
            anchor: cx,
            top: 0.0,
            title: format!("Point {} of {}", index + 1, points.len()),
            rows: vec![TipRow {
                colour,
                name: spark.label.clone().unwrap_or_else(|| "Value".to_string()),
                value: format_value(point.y, spark.unit),
            }],
        })
    });

    Scene {
        svg: svg.finish(),
        texts: Vec::new(),
        anchors,
        tip,
        legend: Vec::new(),
    }
}

/// The index of the first point that no other point beats under `better`.
fn index_of(points: &[Point], better: impl Fn(&Point, &Point) -> bool) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (index, point) in points.iter().enumerate() {
        match best {
            Some(current) if !better(point, &points[current]) => {}
            _ => best = Some(index),
        }
    }
    best
}

fn line_scene(
    chart: &LineChart,
    tokens: &Tokens,
    width: f32,
    progress: f32,
    active: Option<usize>,
) -> Scene {
    let height = chart.height;
    let cleaned: Vec<Vec<Point>> = chart.series.iter().map(Series::cleaned).collect();
    let Some(bounds) = data::bounds(&cleaned) else {
        return empty(tokens, width, height);
    };
    let colours = series_colours(tokens, &chart.series);

    let y_domain = nice(
        bounds.y_min,
        bounds.y_max,
        ticks_for(height - PLOT_TOP - PLOT_BOTTOM, Y_TICK_SPACING),
        chart.area,
    );
    let x_target = ticks_for(width - GUTTER_MIN - PLOT_RIGHT, X_TICK_SPACING);
    let x_domain = match chart.x_unit {
        Unit::Clock { offset } => time(bounds.x_min, bounds.x_max, x_target, offset as f64),
        _ if bounds.x_min == bounds.x_max => lone(bounds.x_min),
        _ => nice(bounds.x_min, bounds.x_max, x_target, false),
    };
    let plot = Plot::new(width, height, gutter_for(&y_domain, chart.y_unit));
    let x = Linear::new((x_domain.lo, x_domain.hi), (plot.left, plot.right));
    let y = Linear::new((y_domain.lo, y_domain.hi), (plot.bottom, plot.top));

    let mut svg = Svg::new(width, height);
    let mut texts = y_grid(
        &mut svg,
        Paint::Outline.color(tokens),
        plot,
        &y,
        &y_domain,
        chart.y_unit,
        width,
    );
    baseline(&mut svg, tokens, plot, &y, &y_domain);
    x_labels(
        &mut texts,
        plot,
        width,
        x_domain
            .ticks
            .iter()
            .map(|&tick| (x.map(tick), format_tick(tick, chart.x_unit, x_domain.step)))
            .collect(),
    );

    let reveal = svg.id("reveal");
    svg.def(clip_rect(&reveal, width * progress, height));
    svg.push(format!(r#"<g clip-path="url(#{reveal})">"#));
    let runs: Vec<(Vec<Point>, Color)> = chart
        .series
        .iter()
        .zip(&colours)
        .flat_map(|(series, &colour)| series.runs().into_iter().map(move |run| (run, colour)))
        .collect();
    for (points, colour) in &runs {
        let colour = *colour;
        if points.is_empty() {
            continue;
        }
        let px: Vec<Pt> = points
            .iter()
            .map(|point| (x.map(point.x), y.map(point.y)))
            .collect();
        let line = curve_path(chart.curve, &px);
        if chart.area && px.len() >= 2 {
            area_fill(&mut svg, &line, &px, plot.bottom, colour);
        }
        if px.len() >= 2 {
            stroke_curve(&mut svg, &line, colour);
        }
        if px.len() == 1 || px.len() <= DOT_LIMIT {
            let radius = if px.len() == 1 {
                ACTIVE_RADIUS
            } else {
                DOT_RADIUS
            };
            for &(cx, cy) in &px {
                svg.push(svg::dot(
                    cx,
                    cy,
                    radius,
                    colour,
                    Some((Paint::Card.color(tokens), LINE_WIDTH)),
                ));
            }
        }
    }
    svg.push("</g>");
    // A series with one value (flat, or a single point) has no shape to read,
    // so its value is written above its last point.
    for points in &cleaned {
        let Some(first) = points.first() else {
            continue;
        };
        if points.iter().any(|point| point.y != first.y) {
            continue;
        }
        let last = points.last().unwrap_or(first);
        texts.push(anchored(
            CAPTION,
            format_value(first.y, chart.y_unit),
            x.map(last.x),
            y.map(first.y) - LABEL_HEIGHT - S1.px(),
            POINT_LABEL_WIDTH,
            Align::Right,
            width,
        ));
    }

    let xs = data::xs(&cleaned);
    let anchors: Vec<f32> = xs.iter().map(|&value| x.map(value)).collect();
    let tip = active.and_then(|index| {
        let value = *xs.get(index)?;
        let ax = crisp(x.map(value));
        svg.push(svg::stroke_line(
            ax,
            plot.top,
            ax,
            plot.bottom,
            Paint::Muted.color(tokens).with_af32(CROSSHAIR_OPACITY),
            HAIRLINE,
        ));
        let mut rows = Vec::new();
        for ((series, points), &colour) in chart.series.iter().zip(&cleaned).zip(&colours) {
            let Some(reading) = data::value_at(points, value) else {
                continue;
            };
            svg.push(svg::dot(
                ax,
                y.map(reading),
                ACTIVE_RADIUS,
                colour,
                Some((Paint::Card.color(tokens), LINE_WIDTH)),
            ));
            rows.push(TipRow {
                colour,
                name: series.name.clone(),
                value: format_value(reading, chart.y_unit),
            });
        }
        Some(Tip {
            anchor: ax,
            top: plot.top,
            title: format_value(value, chart.x_unit),
            rows,
        })
    });

    let legend = if chart.series.len() > 1 {
        chart
            .series
            .iter()
            .zip(&colours)
            .map(|(series, &colour)| (colour, series.name.clone()))
            .collect()
    } else {
        Vec::new()
    };

    let svg_text = svg.finish();
    Scene {
        svg: svg_text,
        texts,
        anchors,
        tip,
        legend,
    }
}

fn bars_scene(
    chart: &BarChart,
    tokens: &Tokens,
    width: f32,
    progress: f32,
    active: Option<usize>,
) -> Scene {
    let height = chart.height;
    let cleaned: Vec<Vec<Point>> = chart.series.iter().map(Series::cleaned).collect();
    let Some(bounds) = data::bounds(&cleaned) else {
        return empty(tokens, width, height);
    };
    let colours = series_colours(tokens, &chart.series);
    let xs = data::xs(&cleaned);
    let count = xs.len();

    // Each band's stack of positive values, and the tallest one.
    let stacks: Vec<f64> = xs
        .iter()
        .map(|&value| {
            cleaned
                .iter()
                .filter_map(|points| data::value_at(points, value))
                .map(|reading| reading.max(0.0))
                .sum()
        })
        .collect();
    let tallest = if chart.stacked {
        stacks.iter().copied().fold(0.0, f64::max)
    } else {
        bounds.y_max
    };
    let y_domain = nice(
        bounds.y_min.min(0.0),
        tallest.max(0.0),
        ticks_for(height - PLOT_TOP - PLOT_BOTTOM, Y_TICK_SPACING),
        true,
    );
    let plot = Plot::new(width, height, gutter_for(&y_domain, chart.y_unit));
    let y = Linear::new((y_domain.lo, y_domain.hi), (plot.bottom, plot.top));
    let band = plot.width() / count as f32;
    let x_centre = |index: usize| plot.left + band * (index as f32 + 0.5);

    let mut svg = Svg::new(width, height);
    let mut texts = y_grid(
        &mut svg,
        Paint::Outline.color(tokens),
        plot,
        &y,
        &y_domain,
        chart.y_unit,
        width,
    );
    let base = baseline(&mut svg, tokens, plot, &y, &y_domain);

    // Hover shades the band first, so the bars draw over it.
    let anchors: Vec<f32> = (0..count).map(x_centre).collect();
    if let Some(index) = active.filter(|&index| index < count) {
        svg.push(format!(
            r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" {}/>"#,
            num(plot.left + band * index as f32),
            num(plot.top),
            num(band),
            num(plot.height()),
            num(Radius::Control.px()),
            svg::paint("fill", Paint::Outline.color(tokens).with_af32(BAND_OPACITY)),
        ));
    }

    let series_count = chart.series.len() as f32;
    let bar_width = if chart.stacked {
        (band * BAR_FILL).min(BAR_MAX)
    } else {
        ((band * BAR_FILL - (series_count - 1.0) * BAR_GAP) / series_count).min(BAR_MAX)
    }
    .max(0.0);
    let group = if chart.stacked {
        bar_width
    } else {
        series_count * bar_width + (series_count - 1.0) * BAR_GAP
    };

    let reveal = |value: f32| base + (value - base) * progress;
    for (index, &value) in xs.iter().enumerate() {
        let cx = x_centre(index);
        if chart.stacked {
            // The topmost positive segment carries the rounded corners.
            let top_series = (0..chart.series.len()).rev().find(|&series| {
                data::value_at(&cleaned[series], value).is_some_and(|reading| reading > 0.0)
            });
            let mut low = 0.0;
            for (series, points) in cleaned.iter().enumerate() {
                let reading = data::value_at(points, value).unwrap_or(0.0).max(0.0);
                if reading <= 0.0 {
                    continue;
                }
                let from = y.map(low);
                low += reading;
                let to = y.map(low);
                let segment = bar_segment(
                    cx - bar_width / 2.0,
                    reveal(from),
                    reveal(to),
                    bar_width,
                    top_series == Some(series),
                );
                if let Some(d) = segment {
                    svg.push(format!(
                        r#"<path d="{d}" {}/>"#,
                        svg::paint("fill", colours[series])
                    ));
                }
            }
        } else {
            let group_left = cx - group / 2.0;
            for (series, points) in cleaned.iter().enumerate() {
                let Some(reading) = data::value_at(points, value) else {
                    continue;
                };
                let bar_x = group_left + series as f32 * (bar_width + BAR_GAP);
                let segment = bar_segment(
                    bar_x,
                    base,
                    reveal(y.map(reading)),
                    bar_width,
                    reading > 0.0,
                );
                if let Some(d) = segment {
                    svg.push(format!(
                        r#"<path d="{d}" {}/>"#,
                        svg::paint("fill", colours[series])
                    ));
                }
            }
        }
    }

    // Value labels appear once the bars have grown in.
    if chart.series.len() == 1 && count <= VALUE_LABEL_MAX && progress >= 1.0 {
        for (index, &value) in xs.iter().enumerate() {
            let Some(reading) = data::value_at(&cleaned[0], value) else {
                continue;
            };
            let top = y.map(reading).min(base);
            let label = format_value(reading, chart.y_unit);
            texts.push(anchored(
                CAPTION.in_ink(Ink::Text),
                label.clone(),
                x_centre(index),
                (top - LABEL_GAP - LABEL_HEIGHT).max(0.0),
                text_width(&label, TextSize::Sm.px()) + LABEL_PAD,
                Align::Centre,
                width,
            ));
        }
    }

    let names: Vec<(f32, String)> = xs
        .iter()
        .enumerate()
        .map(|(index, &value)| {
            let name = chart
                .categories
                .get(index)
                .cloned()
                .unwrap_or_else(|| format_tick(value, Unit::Number, 1.0));
            (x_centre(index), name)
        })
        .collect();
    x_labels(&mut texts, plot, width, names.clone());

    let tip = active.filter(|&index| index < count).map(|index| {
        let value = xs[index];
        let rows = chart
            .series
            .iter()
            .zip(&cleaned)
            .zip(&colours)
            .filter_map(|((series, points), &colour)| {
                data::value_at(points, value).map(|reading| TipRow {
                    colour,
                    name: series.name.clone(),
                    value: format_value(reading, chart.y_unit),
                })
            })
            .collect();
        Tip {
            anchor: x_centre(index),
            top: plot.top,
            title: names[index].1.clone(),
            rows,
        }
    });

    let legend = if chart.series.len() > 1 {
        chart
            .series
            .iter()
            .zip(&colours)
            .map(|(series, &colour)| (colour, series.name.clone()))
            .collect()
    } else {
        Vec::new()
    };

    Scene {
        svg: svg.finish(),
        texts,
        anchors,
        tip,
        legend,
    }
}

/// One bar from `from` to `to` pixels on the y axis. Rounds the end that
/// points away from the baseline. Returns `None` when the bar is too small.
fn bar_segment(x: f32, from: f32, to: f32, width: f32, round: bool) -> Option<String> {
    let top = from.min(to);
    let height = (to - from).abs();
    if height < MIN_BAR || width <= 0.0 {
        return None;
    }
    let radius = if round { Radius::Mark.px() } else { 0.0 };
    let upward = to < from;
    Some(if upward {
        path::bar(x, top, width, height, radius, 0.0)
    } else {
        path::bar(x, top, width, height, 0.0, radius)
    })
}

/// The fraction, clamped to `0..=1`. Non-finite values read as zero.
fn fraction(value: f64) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0) as f32
    } else {
        0.0
    }
}

fn ring_scene(ring: &Ring, tokens: &Tokens, progress: f32) -> Scene {
    let value = fraction(ring.value);
    let colour = palette::resolve(tokens, ring.colour, 0);
    let centre = RING_SIZE / 2.0;
    let radius = centre - RING_STROKE / 2.0 - 1.0;
    let mut svg = Svg::new(RING_SIZE, RING_SIZE);
    svg.push(format!(
        r#"<path d="{}" fill="none" {} stroke-width="{}" stroke-linecap="round"/>"#,
        path::arc(
            centre,
            centre,
            radius,
            GAUGE_START,
            GAUGE_START + GAUGE_SWEEP
        ),
        svg::paint(
            "stroke",
            Paint::Muted.color(tokens).with_af32(RING_TRACK_OPACITY)
        ),
        num(RING_STROKE),
    ));
    let sweep = GAUGE_SWEEP * value * progress;
    if sweep > MIN_SWEEP {
        let gradient = svg.id("ring");
        svg.def(svg::linear_gradient(
            &gradient,
            &[(0.0, colour.with_af32(RING_FROM_OPACITY)), (1.0, colour)],
            false,
        ));
        svg.push(format!(
            r#"<path d="{}" fill="none" stroke="url(#{gradient})" stroke-width="{}" stroke-linecap="round"/>"#,
            path::arc(centre, centre, radius, GAUGE_START, GAUGE_START + sweep),
            num(RING_STROKE),
        ));
    }

    let caption_height = match &ring.caption {
        Some(_) => S0_5.px() + LABEL_HEIGHT,
        None => 0.0,
    };
    let top = centre - (CENTRE_LINE + caption_height) / 2.0;
    let inner = RING_SIZE - (RING_STROKE + S1.px()) * 2.0;
    let mut texts = vec![anchored(
        typography::HEADING.in_ink(Ink::Text),
        format_value((f64::from(value * progress) * 100.0).round(), Unit::Percent),
        centre,
        top,
        inner,
        Align::Centre,
        RING_SIZE,
    )];
    if let Some(caption) = &ring.caption {
        texts.push(anchored(
            CAPTION,
            caption.clone(),
            centre,
            top + CENTRE_LINE + S0_5.px(),
            inner,
            Align::Centre,
            RING_SIZE,
        ));
    }
    Scene {
        svg: svg.finish(),
        texts,
        ..Scene::default()
    }
}

fn ring_element(tokens: &Tokens, ring: &Ring, progress: f32) -> Element {
    let scene = ring_scene(ring, tokens, progress);
    let alt = ring.label.clone().unwrap_or_else(|| "Gauge".to_string());
    drawing(scene, tokens, RING_SIZE, RING_SIZE)
        .a11y_alt(alt)
        .into_element()
}

/// The non-negative values of a donut's segments, and their total.
fn donut_values(donut: &Donut) -> (Vec<f64>, f64) {
    let values: Vec<f64> = donut
        .segments
        .iter()
        .map(|segment| {
            if segment.value.is_finite() && segment.value > 0.0 {
                segment.value
            } else {
                0.0
            }
        })
        .collect();
    let total = values.iter().sum();
    (values, total)
}

fn donut_scene(donut: &Donut, tokens: &Tokens, progress: f32) -> Scene {
    let (values, total) = donut_values(donut);
    let centre = DONUT_SIZE / 2.0;
    let radius = centre - DONUT_STROKE / 2.0 - 1.0;
    let mut svg = Svg::new(DONUT_SIZE, DONUT_SIZE);
    svg.push(format!(
        r#"<path d="{}" fill="none" {} stroke-width="{}"/>"#,
        path::arc(centre, centre, radius, 0.0, TAU),
        svg::paint(
            "stroke",
            Paint::Muted.color(tokens).with_af32(RING_TRACK_OPACITY)
        ),
        num(DONUT_STROKE),
    ));

    let mut texts = Vec::new();
    let inner = DONUT_SIZE - (DONUT_STROKE + S1.px()) * 2.0;
    if total > 0.0 {
        let segments = values.iter().filter(|&&value| value > 0.0).count();
        let gap = if segments > 1 { DONUT_GAP } else { 0.0 };
        let mut cumulative = 0.0;
        for (index, &value) in values.iter().enumerate() {
            if value <= 0.0 {
                continue;
            }
            let start = cumulative / total * TAU as f64;
            cumulative += value;
            let end = cumulative / total * TAU as f64;
            let from = start as f32 * progress + gap / 2.0;
            let to = end as f32 * progress - gap / 2.0;
            if to <= from {
                continue;
            }
            svg.push(format!(
                r#"<path d="{}" fill="none" {} stroke-width="{}"/>"#,
                path::arc(centre, centre, radius, from, to),
                svg::paint(
                    "stroke",
                    palette::resolve(tokens, donut.segments[index].colour, index)
                ),
                num(DONUT_STROKE),
            ));
        }
        let caption_height = match donut.caption {
            Some(_) => S0_5.px() + LABEL_HEIGHT,
            None => 0.0,
        };
        let top = centre - (CENTRE_LINE + caption_height) / 2.0;
        texts.push(anchored(
            typography::HEADING.in_ink(Ink::Text),
            format_value(total, donut.unit),
            centre,
            top,
            inner,
            Align::Centre,
            DONUT_SIZE,
        ));
        if let Some(caption) = &donut.caption {
            texts.push(anchored(
                CAPTION,
                caption.clone(),
                centre,
                top + CENTRE_LINE + S0_5.px(),
                inner,
                Align::Centre,
                DONUT_SIZE,
            ));
        }
    } else {
        texts.push(anchored(
            CAPTION,
            EMPTY_TEXT,
            centre,
            centre - LABEL_HEIGHT / 2.0,
            inner,
            Align::Centre,
            DONUT_SIZE,
        ));
    }
    Scene {
        svg: svg.finish(),
        texts,
        ..Scene::default()
    }
}

/// One legend row: a swatch and name on the left, value and share on the right.
struct DonutRow {
    colour: Color,
    name: String,
    value: String,
    share: Option<String>,
}

fn donut_element(tokens: &Tokens, donut: &Donut, progress: f32) -> Element {
    let (values, total) = donut_values(donut);
    let rows: Vec<DonutRow> = donut
        .segments
        .iter()
        .zip(&values)
        .enumerate()
        .map(|(index, (segment, &value))| {
            let share = if total > 0.0 {
                value / total * 100.0
            } else {
                0.0
            };
            DonutRow {
                colour: palette::resolve(tokens, segment.colour, index),
                name: segment.name.clone(),
                value: format_value(value, donut.unit),
                share: (donut.unit != Unit::Percent).then(|| format_value(share, Unit::Percent)),
            }
        })
        .collect();
    let height = DONUT_SIZE.max(rows.len() as f32 * DONUT_ROW);
    let scene = donut_scene(donut, tokens, progress);
    let alt = donut
        .label
        .clone()
        .unwrap_or_else(|| "Donut chart".to_string());
    rect()
        .width(Size::fill())
        .height(Size::px(height))
        .tw(
            tokens,
            &[flex_row(), gap(S4), items(crate::style::Align::Center)],
        )
        .a11y_alt(alt)
        .child(drawing(scene, tokens, DONUT_SIZE, DONUT_SIZE))
        .child(
            rect()
                .width(Size::fill())
                .children(rows.into_iter().map(|row| donut_row(tokens, row))),
        )
        .into_element()
}

fn donut_row(tokens: &Tokens, row: DonutRow) -> Rect {
    rect()
        .width(Size::fill())
        .height(Size::px(DONUT_ROW))
        .tw(
            tokens,
            &[
                flex_row(),
                items(crate::style::Align::Center),
                justify(crate::style::Align::Between),
            ],
        )
        .child(
            rect()
                .tw(
                    tokens,
                    &[flex_row(), gap(S2), items(crate::style::Align::Center)],
                )
                .child(palette::swatch(row.colour))
                .child(CAPTION.in_ink(Ink::Text).label(tokens, row.name)),
        )
        .child(
            rect()
                .tw(
                    tokens,
                    &[flex_row(), gap(S3), items(crate::style::Align::Center)],
                )
                .child(
                    typography::NUMERIC
                        .in_ink(Ink::Text)
                        .label(tokens, row.value),
                )
                .children(row.share.map(|share| CAPTION.label(tokens, share))),
        )
}
