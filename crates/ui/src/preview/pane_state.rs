//! The docked pane's width, its slide and what it remembers: pure, so the
//! state machine is tested without a window.
//!
//! The pane is `Closed` or `Open`. Its width on screen is a tween between 0
//! and the remembered width (the tween is the component's; reduced motion
//! makes it instant). Dragging its left edge sets the remembered width,
//! clamped to what the window allows; the width is saved when the drag ends,
//! and a window too narrow for it shrinks the pane without forgetting.

/// The narrowest the pane gets: wide enough for the header's buttons and a
/// readable column of facts.
pub const MIN_WIDTH: f32 = 340.0;
/// The width of a pane that has never been resized.
pub const DEFAULT_WIDTH: f32 = 380.0;
/// The share of the view the pane may take.
pub const MAX_SHARE: f32 = 0.6;
/// A window narrower than this has no room for a pane beside a list.
pub const MIN_WINDOW: f32 = 600.0;
/// A step of the keyboard on the edge, in logical pixels.
pub const KEY_STEP: f32 = 24.0;

/// The widest the pane may be when its row is `available` wide.
pub fn max_width(available: f32) -> f32 {
    (available * MAX_SHARE).max(MIN_WIDTH)
}

/// `wanted` kept within what `available` allows.
pub fn clamp_width(wanted: f32, available: f32) -> f32 {
    if !wanted.is_finite() {
        return DEFAULT_WIDTH.min(max_width(available));
    }
    wanted.clamp(MIN_WIDTH, max_width(available))
}

/// The width the pane tweens towards: 0 when closed, else the remembered
/// width cut to the room there is.
pub fn target_width(open: bool, remembered: f32, available: f32) -> f32 {
    if open && available >= MIN_WINDOW { clamp_width(remembered, available) } else { 0.0 }
}

/// The width a drag of the left edge makes: the pane runs from the pointer to
/// the right edge of its row at `right`.
pub fn drag_width(pointer_x: f32, right: f32, available: f32) -> f32 {
    clamp_width(right - pointer_x, available)
}

/// What a key on the edge does to the width.
pub fn key_width(width: f32, wider: bool, available: f32) -> f32 {
    clamp_width(if wider { width + KEY_STEP } else { width - KEY_STEP }, available)
}

/// The key the width is saved under.
pub const STATE_KEY: &str = "preview.pane";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_width_stays_between_the_least_and_a_share_of_the_row() {
        assert_eq!(clamp_width(100.0, 1000.0), MIN_WIDTH);
        assert_eq!(clamp_width(900.0, 1000.0), 600.0);
        assert_eq!(clamp_width(400.0, 1000.0), 400.0);
        // A row too small for the share still gives the least.
        assert_eq!(clamp_width(400.0, 300.0), MIN_WIDTH);
        assert_eq!(clamp_width(f32::NAN, 1000.0), DEFAULT_WIDTH);
    }

    #[test]
    fn a_closed_pane_is_nothing_and_an_open_one_is_its_width() {
        assert_eq!(target_width(false, 400.0, 1000.0), 0.0);
        assert_eq!(target_width(true, 400.0, 1000.0), 400.0);
        // A narrow window shrinks the pane to the cap without changing the
        // remembered width.
        assert_eq!(target_width(true, 700.0, 1000.0), 600.0);
        assert_eq!(target_width(true, 700.0, 1500.0), 700.0);
    }

    #[test]
    fn a_phone_sized_window_has_no_pane() {
        assert_eq!(target_width(true, 360.0, 390.0), 0.0);
        assert_eq!(target_width(true, 360.0, MIN_WINDOW), 360.0);
    }

    #[test]
    fn dragging_the_edge_sets_the_width_from_the_pointer() {
        // The row is 1000 wide and ends at x 1100: a pointer at 700 is 400.
        assert_eq!(drag_width(700.0, 1100.0, 1000.0), 400.0);
        // Past the limits it sticks.
        assert_eq!(drag_width(1090.0, 1100.0, 1000.0), MIN_WIDTH);
        assert_eq!(drag_width(0.0, 1100.0, 1000.0), 600.0);
    }

    #[test]
    fn the_keyboard_steps_the_width() {
        assert_eq!(key_width(360.0, true, 1000.0), 384.0);
        assert_eq!(key_width(MIN_WIDTH, false, 1000.0), MIN_WIDTH);
    }
}
