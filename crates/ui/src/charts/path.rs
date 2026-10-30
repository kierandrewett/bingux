//! Path data for chart marks: smooth and stepped lines, rounded bars, and arcs.

use std::f32::consts::PI;
use std::fmt::Write as _;

use super::svg::num;

/// A point in pixels.
pub(crate) type Pt = (f32, f32);

/// Straight segments through `points`.
pub(crate) fn linear(points: &[Pt]) -> String {
    let mut d = String::new();
    for (index, &(x, y)) in points.iter().enumerate() {
        let command = if index == 0 { 'M' } else { 'L' };
        let _ = write!(d, "{command}{} {}", num(x), num(y));
    }
    d
}

/// Holds each value until the next x, then steps to it.
pub(crate) fn step_after(points: &[Pt]) -> String {
    let mut d = String::new();
    for (index, &(x, y)) in points.iter().enumerate() {
        if index == 0 {
            let _ = write!(d, "M{} {}", num(x), num(y));
        } else {
            let _ = write!(d, "H{}V{}", num(x), num(y));
        }
    }
    d
}

/// A monotone cubic curve (Fritsch and Carlson): smooth, passing through
/// every point, and never overshooting between them.
pub(crate) fn monotone(points: &[Pt]) -> String {
    if points.len() < 2 {
        return linear(points);
    }
    let tangents = monotone_tangents(points);
    let mut d = String::new();
    let (x0, y0) = points[0];
    let _ = write!(d, "M{} {}", num(x0), num(y0));
    for (index, pair) in points.windows(2).enumerate() {
        let ((xa, ya), (xb, yb)) = (pair[0], pair[1]);
        let h = (xb - xa) / 3.0;
        let _ = write!(
            d,
            "C{} {} {} {} {} {}",
            num(xa + h),
            num(ya + tangents[index] * h),
            num(xb - h),
            num(yb - tangents[index + 1] * h),
            num(xb),
            num(yb)
        );
    }
    d
}

/// Tangent slopes for [`monotone`], in pixel space.
fn monotone_tangents(points: &[Pt]) -> Vec<f32> {
    const EPS: f32 = 1e-6;
    let count = points.len();
    let slopes: Vec<f32> = points
        .windows(2)
        .map(|pair| {
            let dx = pair[1].0 - pair[0].0;
            if dx.abs() < EPS {
                0.0
            } else {
                (pair[1].1 - pair[0].1) / dx
            }
        })
        .collect();
    let mut m = vec![0.0_f32; count];
    m[0] = slopes[0];
    m[count - 1] = slopes[count - 2];
    for index in 1..count - 1 {
        let (before, after) = (slopes[index - 1], slopes[index]);
        m[index] = if before * after <= 0.0 {
            0.0
        } else {
            (before + after) / 2.0
        };
    }
    for (index, &slope) in slopes.iter().enumerate() {
        if slope == 0.0 {
            m[index] = 0.0;
            m[index + 1] = 0.0;
            continue;
        }
        let a = m[index] / slope;
        let b = m[index + 1] / slope;
        let h = a * a + b * b;
        if h > 9.0 {
            let t = 3.0 / h.sqrt();
            m[index] = t * a * slope;
            m[index + 1] = t * b * slope;
        }
    }
    m
}

/// A bar from `top` down `height`, with corner radii at its top and bottom.
/// Radii are clamped to half the bar's size.
pub(crate) fn bar(x: f32, top: f32, width: f32, height: f32, r_top: f32, r_bottom: f32) -> String {
    let rt = r_top.clamp(0.0, (width / 2.0).min(height / 2.0));
    let rb = r_bottom.clamp(0.0, (width / 2.0).min(height / 2.0));
    let (left, right, t, b) = (x, x + width, top, top + height);
    let mut d = String::new();
    let _ = write!(d, "M{} {}", num(left + rt), num(t));
    let _ = write!(d, "L{} {}", num(right - rt), num(t));
    if rt > 0.0 {
        let _ = write!(
            d,
            "A{r} {r} 0 0 1 {} {}",
            num(right),
            num(t + rt),
            r = num(rt)
        );
    }
    let _ = write!(d, "L{} {}", num(right), num(b - rb));
    if rb > 0.0 {
        let _ = write!(
            d,
            "A{r} {r} 0 0 1 {} {}",
            num(right - rb),
            num(b),
            r = num(rb)
        );
    }
    let _ = write!(d, "L{} {}", num(left + rb), num(b));
    if rb > 0.0 {
        let _ = write!(
            d,
            "A{r} {r} 0 0 1 {} {}",
            num(left),
            num(b - rb),
            r = num(rb)
        );
    }
    let _ = write!(d, "L{} {}", num(left), num(t + rt));
    if rt > 0.0 {
        let _ = write!(
            d,
            "A{r} {r} 0 0 1 {} {}",
            num(left + rt),
            num(t),
            r = num(rt)
        );
    }
    d.push('Z');
    d
}

/// An arc of a circle centred at `(cx, cy)`. Angles run clockwise from twelve
/// o'clock, in radians. Arcs longer than half a circle are split, so a full
/// circle is drawn exactly.
pub(crate) fn arc(cx: f32, cy: f32, r: f32, start: f32, end: f32) -> String {
    let sweep = end - start;
    if !(sweep > 0.0 && r > 0.0) {
        return String::new();
    }
    let parts = (sweep / PI).ceil().max(1.0) as usize;
    let step = sweep / parts as f32;
    let mut d = String::new();
    for part in 0..parts {
        let a0 = start + step * part as f32;
        let a1 = a0 + step;
        let (x0, y0) = point(cx, cy, r, a0);
        let (x1, y1) = point(cx, cy, r, a1);
        if part == 0 {
            let _ = write!(d, "M{} {}", num(x0), num(y0));
        }
        let _ = write!(d, "A{r} {r} 0 0 1 {} {}", num(x1), num(y1), r = num(r));
    }
    d
}

fn point(cx: f32, cy: f32, r: f32, angle: f32) -> Pt {
    (cx + r * angle.sin(), cy - r * angle.cos())
}
