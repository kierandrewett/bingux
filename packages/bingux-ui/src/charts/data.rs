//! Chart data: points, series and segments, and how they are cleaned.
//!
//! A value that is not finite (NaN or an infinity) is dropped rather than
//! drawn, so one bad sample leaves a gap instead of breaking the chart.

use freya::prelude::Color;

/// One data point. `x` is a number: a position, or Unix seconds on a
/// [`Unit::Clock`](super::Unit::Clock) axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// The colour a series or segment is drawn in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SeriesColour {
    /// The palette in order: the accent first, then categorical colours.
    Auto,
    /// The accent colour.
    Accent,
    /// The success colour.
    Success,
    /// The warning colour.
    Warning,
    /// The danger colour.
    Danger,
    /// The muted text colour, for reference data.
    Muted,
    /// Any colour. Check it against the surface yourself.
    Custom(Color),
}

/// A named sequence of points drawn in one colour.
#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub name: String,
    pub points: Vec<Point>,
    pub colour: SeriesColour,
}

impl Series {
    /// A series of `(x, y)` pairs.
    pub fn new(name: impl Into<String>, points: impl IntoIterator<Item = (f64, f64)>) -> Self {
        Self {
            name: name.into(),
            points: points.into_iter().map(|(x, y)| Point { x, y }).collect(),
            colour: SeriesColour::Auto,
        }
    }

    /// A series of values read by position: the first value sits at x = 0.
    pub fn values(name: impl Into<String>, values: impl IntoIterator<Item = f64>) -> Self {
        Self {
            name: name.into(),
            points: values
                .into_iter()
                .enumerate()
                .map(|(index, y)| Point { x: index as f64, y })
                .collect(),
            colour: SeriesColour::Auto,
        }
    }

    /// Sets the colour.
    pub fn colour(mut self, colour: SeriesColour) -> Self {
        self.colour = colour;
        self
    }

    pub(crate) fn cleaned(&self) -> Vec<Point> {
        clean(&self.points)
    }

    /// The finite points split into runs at each non-finite point, so a
    /// line breaks where the data is missing instead of joining across it.
    pub(crate) fn runs(&self) -> Vec<Vec<Point>> {
        let mut runs: Vec<Vec<Point>> = Vec::new();
        let mut current: Vec<Point> = Vec::new();
        for point in &self.points {
            if point.x.is_finite() && point.y.is_finite() {
                current.push(*point);
            } else if !current.is_empty() {
                runs.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            runs.push(current);
        }
        runs.into_iter().map(|run| clean(&run)).collect()
    }
}

/// A named share of a whole, drawn as one arc of a donut.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub name: String,
    pub value: f64,
    pub colour: SeriesColour,
}

impl Segment {
    /// A segment worth `value`.
    pub fn new(name: impl Into<String>, value: f64) -> Self {
        Self {
            name: name.into(),
            value,
            colour: SeriesColour::Auto,
        }
    }

    /// Sets the colour.
    pub fn colour(mut self, colour: SeriesColour) -> Self {
        self.colour = colour;
        self
    }
}

/// The finite points, sorted by x. A repeated x keeps its last value.
pub(crate) fn clean(points: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = points
        .iter()
        .copied()
        .filter(|p| p.x.is_finite() && p.y.is_finite())
        .collect();
    out.sort_by(|a, b| a.x.total_cmp(&b.x));
    out.dedup_by(|later, earlier| {
        if later.x == earlier.x {
            *earlier = *later;
            true
        } else {
            false
        }
    });
    out
}

/// The extremes of several cleaned series.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Bounds {
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
}

/// The extremes over every point, or `None` when there are no points.
pub(crate) fn bounds(all: &[Vec<Point>]) -> Option<Bounds> {
    all.iter().flatten().fold(None, |acc, p| {
        Some(match acc {
            None => Bounds {
                x_min: p.x,
                x_max: p.x,
                y_min: p.y,
                y_max: p.y,
            },
            Some(b) => Bounds {
                x_min: b.x_min.min(p.x),
                x_max: b.x_max.max(p.x),
                y_min: b.y_min.min(p.y),
                y_max: b.y_max.max(p.y),
            },
        })
    })
}

/// Every distinct x across the series, ascending.
pub(crate) fn xs(all: &[Vec<Point>]) -> Vec<f64> {
    let mut xs: Vec<f64> = all.iter().flatten().map(|p| p.x).collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    xs
}

/// The y value at exactly `x`, for series read by position.
pub(crate) fn value_at(points: &[Point], x: f64) -> Option<f64> {
    points
        .binary_search_by(|p| p.x.total_cmp(&x))
        .ok()
        .map(|index| points[index].y)
}
