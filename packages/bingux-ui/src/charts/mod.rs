//! Charts: line, area, step, bar, sparkline, ring and donut, drawn in the
//! Bingux style.
//!
//! Each chart is a Freya component. It measures its box, writes an SVG at
//! that exact pixel size (anti-aliased, and crisp at any scale factor), and
//! lays its text, tooltips and focus rings over it as ordinary elements,
//! because SVG text cannot be rendered here.
//!
//! The data model is [`Series`] (named `(x, y)` points, or values read by
//! position) and [`Segment`] (a named share of a whole). Colours come from
//! the theme: the accent first, then categorical colours derived from it and
//! checked for contrast against the surface.
//!
//! A chart keeps its height from the first frame. Before it has points it
//! shows a muted baseline and "No data yet", so nothing shifts when data
//! arrives.
//!
//! ```ignore
//! use bingux_ui::charts::{Curve, Series, Unit, line_chart};
//!
//! line_chart([Series::new("Charge", points)])
//!     .y_unit(Unit::Percent)
//!     .curve(Curve::Monotone)
//!     .area()
//!     .label("Battery charge over the last day")
//! ```
//!
//! Keyboard: a chart with points is focusable. Left and Right move between
//! points, Home and End jump to the ends, and the tooltip follows.
//!
//! Two charts are not about series: [`timeline_chart`] places bars along a time
//! axis with a limit line and a draggable range (frame times, round trips),
//! and [`flame_graph`] draws a tree of weighted parts as stacked bars (where
//! CPU time goes, which folders hold the bytes).

mod data;
mod flame;
mod format;
mod geometry;
mod palette;
mod path;
mod scale;
mod svg;
mod timeline;
mod view;

pub use data::{Point, Segment, Series, SeriesColour};
pub use flame::{FlameGraph, FlameTree, flame_graph};
pub use format::Unit;
pub use timeline::{Bar, TimelineChart, timeline_chart};
pub use view::{
    BarChart, Curve, Donut, LineChart, Ring, Sparkline, bar_chart, donut, line_chart, ring,
    sparkline,
};
