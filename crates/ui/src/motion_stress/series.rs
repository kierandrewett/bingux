//! Invariants over a series: one number (or colour) per frame. Each returns the failing series in
//! its message, so a failed test shows the frames it saw.

use freya::prelude::Color;

/// The series as a short list for a message.
pub(crate) fn show(values: &[f32]) -> String {
    let mut out = String::from("[");
    for (at, value) in values.iter().enumerate() {
        if at > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{value:.2}"));
    }
    out.push(']');
    out
}

/// Never goes down by more than `eps` from one frame to the next.
pub(crate) fn monotone_up(what: &str, values: &[f32], eps: f32) -> Result<(), String> {
    for at in 1..values.len() {
        if values[at] < values[at - 1] - eps {
            return Err(format!("{what} went back at frame {at}: {}", show(values)));
        }
    }
    Ok(())
}

/// Never goes up by more than `eps` from one frame to the next.
pub(crate) fn monotone_down(what: &str, values: &[f32], eps: f32) -> Result<(), String> {
    for at in 1..values.len() {
        if values[at] > values[at - 1] + eps {
            return Err(format!("{what} went back at frame {at}: {}", show(values)));
        }
    }
    Ok(())
}

/// Monotone in the direction from the first value to the last: no flicker, no overshoot back.
pub(crate) fn monotone_between_ends(what: &str, values: &[f32], eps: f32) -> Result<(), String> {
    match (values.first(), values.last()) {
        (Some(first), Some(last)) if last >= first => monotone_up(what, values, eps),
        (Some(_), Some(_)) => monotone_down(what, values, eps),
        _ => Ok(()),
    }
}

/// Every value is inside `lo..=hi`, give or take `eps`: nothing overshoots.
pub(crate) fn within(what: &str, values: &[f32], lo: f32, hi: f32, eps: f32) -> Result<(), String> {
    for (at, value) in values.iter().enumerate() {
        if *value < lo - eps || *value > hi + eps {
            return Err(format!(
                "{what} left {lo:.2}..{hi:.2} at frame {at}: {}",
                show(values)
            ));
        }
    }
    Ok(())
}

/// No frame differs from the one before by more than `max`: nothing jumps.
pub(crate) fn no_jump(what: &str, values: &[f32], max: f32) -> Result<(), String> {
    for at in 1..values.len() {
        if (values[at] - values[at - 1]).abs() > max {
            return Err(format!(
                "{what} jumped by {:.2} at frame {at} (more than {max:.2}): {}",
                values[at] - values[at - 1],
                show(values)
            ));
        }
    }
    Ok(())
}

/// Every value equals the first, give or take `eps`: the box does not move.
pub(crate) fn constant(what: &str, values: &[f32], eps: f32) -> Result<(), String> {
    let Some(first) = values.first() else {
        return Ok(());
    };
    for (at, value) in values.iter().enumerate() {
        if (value - first).abs() > eps {
            return Err(format!(
                "{what} moved at frame {at} ({first:.2} -> {value:.2}): {}",
                show(values)
            ));
        }
    }
    Ok(())
}

/// The last value is `target`, give or take `eps`.
pub(crate) fn ends_at(what: &str, values: &[f32], target: f32, eps: f32) -> Result<(), String> {
    match values.last() {
        Some(last) if (last - target).abs() <= eps => Ok(()),
        Some(last) => Err(format!(
            "{what} ended at {last:.3}, not {target:.3}: {}",
            show(values)
        )),
        None => Err(format!("{what} has no frames")),
    }
}

/// The index of the first frame from which the series stays at its last value (give or take `eps`).
pub(crate) fn rest_index(values: &[f32], eps: f32) -> usize {
    let Some(last) = values.last() else {
        return 0;
    };
    let mut at = values.len();
    while at > 0 && (values[at - 1] - last).abs() <= eps {
        at -= 1;
    }
    at
}

/// The index of the first frame at which the series left its first value (give or take `eps`).
pub(crate) fn start_index(values: &[f32], eps: f32) -> usize {
    let Some(first) = values.first() else {
        return 0;
    };
    values
        .iter()
        .position(|value| (value - first).abs() > eps)
        .unwrap_or(values.len())
}

/// A colour as four channels.
pub(crate) fn channels(colour: Color) -> [f32; 4] {
    [
        colour.r() as f32,
        colour.g() as f32,
        colour.b() as f32,
        colour.a() as f32,
    ]
}

/// Colours go from the first to the last along one path: every channel stays between the two ends
/// and never turns back, so no frame draws a colour that is neither the start, the end nor an in
/// between (a flicker, a flash of a third colour). `tolerance` is in levels of 0..=255.
pub(crate) fn colour_path(what: &str, colours: &[Color], tolerance: f32) -> Result<(), String> {
    let (Some(first), Some(last)) = (colours.first(), colours.last()) else {
        return Ok(());
    };
    let (start, end) = (channels(*first), channels(*last));
    for channel in 0..4 {
        let track: Vec<f32> = colours
            .iter()
            .map(|colour| channels(*colour)[channel])
            .collect();
        let (lo, hi) = (
            start[channel].min(end[channel]),
            start[channel].max(end[channel]),
        );
        for (at, value) in track.iter().enumerate() {
            if *value < lo - tolerance || *value > hi + tolerance {
                return Err(format!(
                    "{what}: channel {channel} left {lo:.0}..{hi:.0} at frame {at}: {}",
                    show(&track)
                ));
            }
        }
        monotone_between_ends(&format!("{what}: channel {channel}"), &track, tolerance)?;
    }
    Ok(())
}

/// The colour as `#rrggbbaa`.
pub(crate) fn hex(colour: Color) -> String {
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        colour.r(),
        colour.g(),
        colour.b(),
        colour.a()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_series_checks_find_what_they_are_for() {
        assert!(monotone_up("x", &[0.0, 1.0, 1.0, 2.0], 0.01).is_ok());
        assert!(monotone_up("x", &[0.0, 2.0, 1.0], 0.01).is_err());
        assert!(monotone_down("x", &[2.0, 1.0, 1.0], 0.01).is_ok());
        assert!(monotone_between_ends("x", &[5.0, 3.0, 4.0, 1.0], 0.01).is_err());
        assert!(within("x", &[0.0, 1.0, 1.2], 0.0, 1.0, 0.1).is_err());
        assert!(within("x", &[0.0, 1.0, 1.05], 0.0, 1.0, 0.1).is_ok());
        assert!(no_jump("x", &[0.0, 1.0, 5.0], 2.0).is_err());
        assert!(constant("x", &[1.0, 1.0, 1.4], 0.1).is_err());
        assert!(ends_at("x", &[0.0, 9.0], 10.0, 0.5).is_err());
        assert_eq!(rest_index(&[0.0, 1.0, 2.0, 2.0, 2.0], 0.01), 2);
        assert_eq!(start_index(&[0.0, 0.0, 0.0, 1.0], 0.01), 3);
    }

    #[test]
    fn a_colour_that_flashes_a_third_colour_is_caught() {
        let a = Color::from_rgb(10, 10, 10);
        let b = Color::from_rgb(200, 200, 200);
        let flash = Color::from_rgb(255, 0, 0);
        assert!(colour_path("c", &[a, a, b, b], 1.0).is_ok());
        assert!(colour_path("c", &[a, flash, b], 1.0).is_err());
        assert!(
            colour_path("c", &[a, b, a, b], 1.0).is_err(),
            "a flicker goes back"
        );
    }
}
