//! The player's pure rules: which keys do what, when the control bar shows,
//! and what a position on the seek bar means. No window, no host: these are
//! tested directly.

use freya::prelude::*;

/// How long the control bar stays after the pointer last moved, while playing.
pub(super) const HIDE_AFTER_MS: u64 = 2500;
/// Milliseconds Shift and Left or Right move, and J and L. (The arrows alone
/// choose the file.)
pub(super) const SEEK_SHORT_MS: i64 = 5_000;
pub(super) const SEEK_JUMP_MS: i64 = 10_000;

/// What the control bar's visibility depends on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct BarInputs {
    pub playing: bool,
    /// The pointer is over the bar.
    pub hovered: bool,
    /// A control of the bar has the keyboard, or its menu is open.
    pub engaged: bool,
    /// The player has failed or has not opened: the bar is never in the way.
    pub failed: bool,
    /// Milliseconds since the pointer last moved over the player.
    pub idle_ms: u64,
}

/// Whether the bar shows. While paused, hovered, in use from the keyboard or
/// failed it always does; while playing it fades [`HIDE_AFTER_MS`] after the
/// pointer last moved.
pub(super) fn bar_visible(inputs: BarInputs) -> bool {
    !inputs.playing || inputs.hovered || inputs.engaged || inputs.failed || inputs.idle_ms < HIDE_AFTER_MS
}

/// How long until the bar should be checked again, if it is waiting to hide.
pub(super) fn hide_in_ms(inputs: BarInputs) -> Option<u64> {
    (bar_visible(inputs) && inputs.playing && !inputs.hovered && !inputs.engaged && !inputs.failed).then(|| HIDE_AFTER_MS.saturating_sub(inputs.idle_ms))
}

/// What a key does in the player.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum MediaKey {
    TogglePlay,
    /// Move by this many milliseconds (negative is back).
    SeekBy(i64),
    /// Jump to this fraction of the length (the digit keys: 0 to 9 tenths).
    SeekTo(f32),
    /// Steps of the volume.
    Volume(i32),
    Mute,
    /// Slower (-1) or faster (1).
    Rate(i32),
    /// The previous or next subtitle track; `None` when past either end.
    Subtitles,
}

/// The meaning of `key` in the player. Alt and Ctrl belong to the preview
/// and the file view. A bare arrow chooses the file (the host steps it), so
/// the player seeks with Shift and an arrow, J and L, and sets the volume
/// with Shift and Up or Down. Space plays and pauses only in a docked pane
/// that has the keyboard: in a popup it closes, like every Quick Look, and K
/// plays and pauses.
pub(super) fn route(key: &Key, modifiers: Modifiers, docked: bool) -> Option<MediaKey> {
    if modifiers.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META) {
        return None;
    }
    let shift = modifiers.contains(Modifiers::SHIFT);
    match key {
        Key::Named(NamedKey::ArrowLeft) if shift => Some(MediaKey::SeekBy(-SEEK_SHORT_MS)),
        Key::Named(NamedKey::ArrowRight) if shift => Some(MediaKey::SeekBy(SEEK_SHORT_MS)),
        Key::Named(NamedKey::ArrowUp) if shift => Some(MediaKey::Volume(1)),
        Key::Named(NamedKey::ArrowDown) if shift => Some(MediaKey::Volume(-1)),
        Key::Named(NamedKey::Home) => Some(MediaKey::SeekTo(0.0)),
        Key::Character(text) => match text.as_str() {
            " " if docked => Some(MediaKey::TogglePlay),
            "k" | "K" => Some(MediaKey::TogglePlay),
            "j" | "J" => Some(MediaKey::SeekBy(-SEEK_JUMP_MS)),
            "l" | "L" => Some(MediaKey::SeekBy(SEEK_JUMP_MS)),
            "m" | "M" => Some(MediaKey::Mute),
            "[" => Some(MediaKey::Rate(-1)),
            "]" => Some(MediaKey::Rate(1)),
            "c" | "C" => Some(MediaKey::Subtitles),
            digit if digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit() => Some(MediaKey::SeekTo(f32::from(digit.as_bytes()[0] - b'0') / 10.0)),
            _ => None,
        },
        _ => None,
    }
}

/// The next subtitle track after `current` (-1 is off), cycling through off.
pub(super) fn next_subtitle(current: i32, count: usize) -> i32 {
    if count == 0 {
        return -1;
    }
    if current + 1 >= count as i32 { -1 } else { current + 1 }
}

/// How far along the track, 0 to 1, a pointer at `x` is on a track of `width`
/// starting at 0. A pointer past an end is that end.
pub(super) fn fraction_at(x: f32, width: f32) -> f32 {
    if width <= 0.0 || x.is_nan() { 0.0 } else { (x / width).clamp(0.0, 1.0) }
}

/// The volume (thousandths) a pointer at `x` on a track of `width` means.
pub(super) fn volume_at(x: f32, width: f32) -> u32 {
    (fraction_at(x, width) * 1000.0).round() as u32
}

/// The icon-worthy level of a volume: 0 muted, 1 low, 2 medium, 3 high.
pub(super) fn volume_level(permille: u32, muted: bool) -> u8 {
    if muted || permille == 0 {
        0
    } else if permille < 340 {
        1
    } else if permille < 670 {
        2
    } else {
        3
    }
}

/// Whether the control bar uses its compact form: the docked pane, or a box
/// too narrow for every control.
pub(super) fn compact_bar(width: f32, docked: bool) -> bool {
    docked || width < 520.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn character(text: &str, modifiers: Modifiers) -> Option<MediaKey> {
        route(&Key::Character(text.to_owned().into()), modifiers, false)
    }

    fn named(key: NamedKey, modifiers: Modifiers) -> Option<MediaKey> {
        route(&Key::Named(key), modifiers, false)
    }

    #[test]
    fn the_keys_of_the_player() {
        let none = Modifiers::empty();
        assert_eq!(character("k", none), Some(MediaKey::TogglePlay));
        assert_eq!(named(NamedKey::ArrowLeft, Modifiers::SHIFT), Some(MediaKey::SeekBy(-5_000)));
        assert_eq!(named(NamedKey::ArrowRight, Modifiers::SHIFT), Some(MediaKey::SeekBy(5_000)));
        assert_eq!(character("j", none), Some(MediaKey::SeekBy(-10_000)));
        assert_eq!(character("l", none), Some(MediaKey::SeekBy(10_000)));
        assert_eq!(named(NamedKey::ArrowUp, Modifiers::SHIFT), Some(MediaKey::Volume(1)));
        assert_eq!(named(NamedKey::ArrowDown, Modifiers::SHIFT), Some(MediaKey::Volume(-1)));
        assert_eq!(character("m", none), Some(MediaKey::Mute));
        assert_eq!(character("[", none), Some(MediaKey::Rate(-1)));
        assert_eq!(character("]", none), Some(MediaKey::Rate(1)));
        assert_eq!(character("5", none), Some(MediaKey::SeekTo(0.5)));
        assert_eq!(character("0", none), Some(MediaKey::SeekTo(0.0)));
        assert_eq!(character("x", none), None);
    }

    #[test]
    fn a_bare_arrow_is_never_the_players() {
        // The arrows choose the file in every frame; the host steps it.
        let none = Modifiers::empty();
        for key in [NamedKey::ArrowLeft, NamedKey::ArrowRight, NamedKey::ArrowUp, NamedKey::ArrowDown] {
            assert_eq!(named(key, none), None, "{key:?}");
            assert_eq!(route(&Key::Named(key), none, true), None, "{key:?} in a pane");
        }
    }

    #[test]
    fn space_plays_only_in_a_pane_and_closes_a_popup() {
        let none = Modifiers::empty();
        assert_eq!(character(" ", none), None, "a popup's Space closes it (the host)");
        assert_eq!(route(&Key::Character(" ".into()), none, true), Some(MediaKey::TogglePlay));
    }

    #[test]
    fn alt_and_control_are_not_the_players() {
        // Alt+arrows step the file; Ctrl combinations are the file view's.
        assert_eq!(named(NamedKey::ArrowRight, Modifiers::ALT), None);
        assert_eq!(character("k", Modifiers::CONTROL), None);
        assert_eq!(character("m", Modifiers::CONTROL), None);
    }

    #[test]
    fn the_bar_shows_while_paused_hovered_engaged_failed_or_recently_moved() {
        let playing = BarInputs { playing: true, idle_ms: 10_000, ..BarInputs::default() };
        assert!(!bar_visible(playing), "playing and idle: hidden");
        assert!(bar_visible(BarInputs { playing: false, ..playing }), "paused: shown");
        assert!(bar_visible(BarInputs { hovered: true, ..playing }), "hovered: shown");
        assert!(bar_visible(BarInputs { engaged: true, ..playing }), "a control has the keyboard: shown");
        assert!(bar_visible(BarInputs { failed: true, ..playing }), "failed: shown");
        assert!(bar_visible(BarInputs { idle_ms: HIDE_AFTER_MS - 1, ..playing }), "just moved");
        assert!(!bar_visible(BarInputs { idle_ms: HIDE_AFTER_MS, ..playing }), "after the delay");
    }

    #[test]
    fn the_hide_timer_only_runs_while_the_bar_could_hide() {
        let playing = BarInputs { playing: true, idle_ms: 1_000, ..BarInputs::default() };
        assert_eq!(hide_in_ms(playing), Some(HIDE_AFTER_MS - 1_000));
        assert_eq!(hide_in_ms(BarInputs { playing: false, ..playing }), None);
        assert_eq!(hide_in_ms(BarInputs { hovered: true, ..playing }), None);
        assert_eq!(hide_in_ms(BarInputs { engaged: true, ..playing }), None);
        assert_eq!(hide_in_ms(BarInputs { idle_ms: 10_000, ..playing }), None, "already hidden");
    }

    #[test]
    fn a_position_on_the_track_is_a_fraction_clamped_to_its_ends() {
        assert_eq!(fraction_at(50.0, 200.0), 0.25);
        assert_eq!(fraction_at(-10.0, 200.0), 0.0);
        assert_eq!(fraction_at(500.0, 200.0), 1.0);
        assert_eq!(fraction_at(5.0, 0.0), 0.0);
        assert_eq!(fraction_at(f32::NAN, 100.0), 0.0);
        assert_eq!(volume_at(100.0, 200.0), 500);
    }

    #[test]
    fn the_volume_icon_follows_the_level_and_the_mute() {
        assert_eq!(volume_level(900, true), 0);
        assert_eq!(volume_level(0, false), 0);
        assert_eq!(volume_level(100, false), 1);
        assert_eq!(volume_level(500, false), 2);
        assert_eq!(volume_level(1000, false), 3);
    }

    #[test]
    fn subtitles_cycle_through_off() {
        assert_eq!(next_subtitle(-1, 2), 0);
        assert_eq!(next_subtitle(0, 2), 1);
        assert_eq!(next_subtitle(1, 2), -1);
        assert_eq!(next_subtitle(-1, 0), -1);
    }

    #[test]
    fn the_pane_and_a_narrow_box_get_the_compact_bar() {
        assert!(compact_bar(900.0, true));
        assert!(compact_bar(390.0, false));
        assert!(!compact_bar(900.0, false));
    }
}
