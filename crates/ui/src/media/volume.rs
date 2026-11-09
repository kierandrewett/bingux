//! A player's volume: [`volume_control`].
//!
//! A mute button whose speaker shows the level, and a slider. The wheel over
//! either changes the volume in steps of 5 (one notch of a mouse wheel), so
//! the level can be set without aiming at the slider; a press on the speaker
//! mutes and unmutes, and moving the slider unmutes. `compact()` leaves the
//! button alone (the wheel still works on it), for a narrow bar.

use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::button::icon_button;
use crate::controls::slider::slider;
use crate::controls::tooltip::TipSide;
use crate::icons::Icon;
use crate::shell::panel::{Panel, PanelKind, PanelRect, PanelTrigger};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::Ink;

/// One wheel notch.
const WHEEL_STEP: f64 = 5.0;

/// A change the control asks for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VolumeChange {
    /// The level, 0 to 100.
    Level(f64),
    /// Mute (true) or unmute.
    Mute(bool),
}

/// The volume control. Build with [`volume_control`].
#[derive(PartialEq)]
pub struct VolumeControl {
    level: f64,
    muted: bool,
    on_change: EventHandler<VolumeChange>,
    compact: bool,
    floating: bool,
    enabled: bool,
}

/// A volume at `level` (0 to 100), `muted` or not.
pub fn volume_control(
    level: f64,
    muted: bool,
    on_change: impl Into<EventHandler<VolumeChange>>,
) -> VolumeControl {
    VolumeControl {
        level,
        muted,
        on_change: on_change.into(),
        compact: false,
        floating: false,
        enabled: true,
    }
}

impl VolumeControl {
    /// The button alone.
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    /// On a floating bar over a picture: light glyphs.
    pub fn floating(mut self) -> Self {
        self.floating = true;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// The speaker for a level: muted, then no waves, one, two.
pub(crate) fn volume_icon(level: f64, muted: bool) -> Icon {
    if muted || level <= 0.0 {
        Icon::AudioVolumeMuted
    } else if level < 34.0 {
        Icon::AudioVolumeLow
    } else if level < 67.0 {
        Icon::AudioVolumeMedium
    } else {
        Icon::AudioVolumeHigh
    }
}

/// The level after a wheel turn of `delta_y` (negative: away from the person, up).
pub(crate) fn wheel_level(level: f64, delta_y: f64) -> f64 {
    if delta_y == 0.0 {
        return level;
    }
    let step = if delta_y < 0.0 {
        WHEEL_STEP
    } else {
        -WHEEL_STEP
    };
    // Land on multiples of the step, so a few notches reach round numbers.
    (((level + step) / WHEEL_STEP).round() * WHEEL_STEP).clamp(0.0, 100.0)
}

impl Component for VolumeControl {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ink = if self.floating {
            Ink::OnFloating
        } else {
            Ink::Text
        };
        let mut open = use_state(|| false);
        let mut area = use_state(Area::default);
        let on_change = self.on_change.clone();
        let (level, muted) = (self.level, self.muted);
        let shown = if muted { 0.0 } else { level };
        // Compact (touch): the button opens a popover holding the mute toggle
        // and the slider. Desktop: the button is a plain mute toggle.
        let mute = if self.compact {
            icon_button(volume_icon(level, muted), move |_| {
                let next = !*open.peek();
                open.set(next);
            })
            .ink(ink)
            .tooltip(&crate::tr!("ui-media-volume"))
        } else {
            let on_change = on_change.clone();
            icon_button(volume_icon(level, muted), move |_| {
                on_change.call(VolumeChange::Mute(!muted))
            })
            .ink(ink)
            .tooltip_side(TipSide::Above)
            .tooltip(&if muted {
                crate::tr!("ui-media-unmute-tooltip")
            } else {
                crate::tr!("ui-media-mute-tooltip")
            })
        }
        .enabled(self.enabled);
        let face = rect()
            .on_sized(move |event: Event<SizedEventData>| {
                area.set_if_modified(event.area);
            })
            .child(mute);
        let popover = self.compact.then(|| {
            let content = rect()
                .component("VolumePopover")
                .tw(
                    &tokens,
                    &[w_full(), flex_row(), items(Align::Center), gap(S2)],
                )
                .child(
                    icon_button(volume_icon(level, muted), {
                        let on_change = on_change.clone();
                        move |_| on_change.call(VolumeChange::Mute(!muted))
                    })
                    .ink(ink)
                    .enabled(self.enabled),
                )
                .child(
                    slider(shown, {
                        let on_change = on_change.clone();
                        move |value: f64| on_change.call(VolumeChange::Level(value))
                    })
                    .label(&crate::tr!("ui-media-volume"))
                    .width(Dim::Box24)
                    .enabled(self.enabled),
                );
            let width = Dim::Box24.px() + Dim::Control.px() + 4.0 * S2.px();
            let height = Dim::Control.px() + 2.0 * S2.px();
            Panel::new(
                PanelKind::Choice,
                open() && self.enabled,
                PanelTrigger::Control(PanelRect::from_area(&area.read())),
                vec![Element::from(content)],
            )
            .size(width, height)
            .on_dismiss(EventHandler::new(move |_: ()| open.set(false)))
        });
        let wheel = {
            let on_change = on_change.clone();
            move |event: Event<WheelEventData>| {
                let next = wheel_level(if muted { 0.0 } else { level }, event.delta_y);
                event.stop_propagation();
                on_change.call(VolumeChange::Level(next));
            }
        };
        rect()
            .component("VolumeControl")
            .tw(&tokens, &[flex_row(), items(Align::Center), gap(S1)])
            .maybe(self.enabled, |row| row.on_wheel(wheel))
            .child(face)
            .maybe_child(popover.map(Element::from))
            .maybe_child((!self.compact).then(|| {
                slider(shown, {
                    let on_change = on_change.clone();
                    move |value: f64| on_change.call(VolumeChange::Level(value))
                })
                .label(&crate::tr!("ui-media-volume"))
                .width(Dim::Box24)
                .enabled(self.enabled)
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wheel_moves_in_round_steps() {
        assert_eq!(wheel_level(50.0, -1.0), 55.0);
        assert_eq!(wheel_level(52.0, -1.0), 55.0);
        assert_eq!(wheel_level(52.0, 1.0), 45.0);
        assert_eq!(wheel_level(98.0, -1.0), 100.0);
        assert_eq!(wheel_level(2.0, 1.0), 0.0);
    }

    #[test]
    fn the_speaker_shows_the_level() {
        assert_eq!(volume_icon(80.0, true), Icon::AudioVolumeMuted);
        assert_eq!(volume_icon(10.0, false), Icon::AudioVolumeLow);
        assert_eq!(volume_icon(80.0, false), Icon::AudioVolumeHigh);
    }
}
