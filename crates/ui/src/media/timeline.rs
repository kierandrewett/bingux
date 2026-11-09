//! The timeline of a song or a film: [`timeline`].
//!
//! It is the shell's `SeekSlider.qml` made for time. A thin track holds, from
//! the bottom up: what is read ahead (buffered stretches, faint), how far the
//! pointer is (a lighter fill to the pointer while it hovers), what has played
//! (the text colour at rest, the accent while it is touched), the chapter
//! breaks (gaps cut into the track) and an A-B loop's points. A knob appears
//! under the pointer and the keyboard focus.
//!
//! Hovering shows a bubble over the pointer with the time there, the chapter
//! it falls in, and a picture of that moment when the caller gives one
//! (`preview`, fed by `on_hover`). Dragging moves the knob at once and asks
//! for fast seeks while the pointer moves ([`Seek::done`] false: the nearest
//! keyframe), then one exact seek where it is let go.
//!
//! With `times()`, the time played is on its left and the time left on its
//! right; a press on the time left shows the length instead, and back.
//!
//! Keys, once it has the focus: Left and Right 5 s (Shift: 30 s), Page Up and
//! Page Down a minute, Home the start, End the end, and `,`/`.` the chapter
//! before and after.

use freya::prelude::*;

use super::time::{format_time, spoken_time};
use crate::component::ComponentName;
use crate::controls::capture::{CaptureEvent, Phase, use_capture};
use crate::controls::direction::use_direction;
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::{use_enabled, use_tokens};
use crate::typography::{CAPTION, Ink, NUMERIC};

/// The width of a chapter gap and of a loop point, in pixels.
const MARK: f32 = 2.0;

/// A seek the timeline asks for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seek {
    /// Seconds from the start.
    pub to: f64,
    /// The pointer was let go (or a key pressed): seek exactly. While a drag
    /// moves it is false, and a fast seek is enough.
    pub done: bool,
}

/// A song's or a film's timeline. Build with [`timeline`].
#[derive(PartialEq)]
pub struct Timeline {
    position: f64,
    duration: Option<f64>,
    buffered: Vec<(f64, f64)>,
    chapters: Vec<(f64, String)>,
    loop_points: (Option<f64>, Option<f64>),
    on_seek: EventHandler<Seek>,
    on_hover: Option<EventHandler<Option<f64>>>,
    preview: Option<Element>,
    times: bool,
    floating: bool,
    surface: Paint,
    label: String,
}

/// A timeline at `position` seconds of `duration` (unknown: a stream, or not
/// loaded; the track is then disabled). `on_seek` gets every seek asked for.
pub fn timeline(
    position: f64,
    duration: Option<f64>,
    on_seek: impl Into<EventHandler<Seek>>,
) -> Timeline {
    Timeline {
        position,
        duration,
        buffered: Vec::new(),
        chapters: Vec::new(),
        loop_points: (None, None),
        on_seek: on_seek.into(),
        on_hover: None,
        preview: None,
        times: false,
        floating: false,
        surface: Paint::Bg,
        label: "Seek".to_owned(),
    }
}

impl Timeline {
    /// Stretches that are read ahead, in seconds.
    pub fn buffered(mut self, ranges: Vec<(f64, f64)>) -> Self {
        self.buffered = ranges;
        self
    }

    /// Chapter starts and titles, in order. The track has a gap at each
    /// start after the first, and the hover bubble names the chapter.
    pub fn chapters(mut self, chapters: Vec<(f64, String)>) -> Self {
        self.chapters = chapters;
        self
    }

    /// The A and B points of a loop being played.
    pub fn loop_points(mut self, a: Option<f64>, b: Option<f64>) -> Self {
        self.loop_points = (a, b);
        self
    }

    /// Called with the time under the pointer as it moves over the track,
    /// and `None` when it leaves: a player renders a picture of that moment
    /// and hands it back with [`Timeline::preview`].
    pub fn on_hover(mut self, on_hover: impl Into<EventHandler<Option<f64>>>) -> Self {
        self.on_hover = Some(on_hover.into());
        self
    }

    /// A picture shown in the hover bubble, above the time.
    pub fn preview(mut self, preview: Option<Element>) -> Self {
        self.preview = preview;
        self
    }

    /// The time played on the left and the time left on the right.
    pub fn times(mut self) -> Self {
        self.times = true;
        self
    }

    /// Drawn on a floating bar over a picture: light ink on the dark bar.
    pub fn floating(mut self) -> Self {
        self.floating = true;
        self.surface = Paint::Floating;
        self
    }

    /// The colour behind the timeline, which the chapter gaps are cut in.
    /// The window's background by default.
    pub fn on_surface(mut self, surface: Paint) -> Self {
        self.surface = surface;
        self
    }

    /// What a screen reader calls it.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }
}

/// The chapter at `seconds`, as an index into `chapters`.
pub(crate) fn chapter_at(chapters: &[(f64, String)], seconds: f64) -> Option<usize> {
    chapters.iter().rposition(|(start, _)| *start <= seconds)
}

/// Where a key moves the timeline to, from `position`.
pub(crate) fn key_target(
    key: &Key,
    shift: bool,
    position: f64,
    duration: f64,
    chapters: &[(f64, String)],
    along: i32,
) -> Option<f64> {
    let step = if shift { 30.0 } else { 5.0 };
    let target = match key {
        Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight) => {
            position + step * f64::from(along)
        }
        Key::Named(NamedKey::ArrowUp) => position + step,
        Key::Named(NamedKey::ArrowDown) => position - step,
        Key::Named(NamedKey::PageUp) => position + 60.0,
        Key::Named(NamedKey::PageDown) => position - 60.0,
        Key::Named(NamedKey::Home) => 0.0,
        Key::Named(NamedKey::End) => duration,
        Key::Character(text) if text == "." => {
            let next = chapters.iter().find(|(start, _)| *start > position + 0.5)?;
            next.0
        }
        Key::Character(text) if text == "," => {
            let current = chapter_at(chapters, position)?;
            // Within a few seconds of a chapter's start, go to the one before.
            let index = if position - chapters[current].0 < 3.0 {
                current.saturating_sub(1)
            } else {
                current
            };
            chapters[index].0
        }
        _ => return None,
    };
    Some(target.clamp(0.0, duration))
}

impl Component for Timeline {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let direction = use_direction();
        let enabled = use_enabled(self.duration.is_some_and(|duration| duration > 0.0));
        let duration = self.duration.unwrap_or(0.0).max(0.0);
        let mut width = use_state(|| 0.0f32);
        let mut row_width = use_state(|| 0.0f32);
        let mut hover = use_state(|| None::<f32>);
        let mut drag = use_state(|| None::<f64>);
        let mut show_length = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let focused = focus().is_focused();

        let measured = *width.read();
        let thumb = Dim::SliderThumb.px();
        let hit = Dim::Control.px();
        let centre = hit / 2.0;
        let rtl = direction.is_rtl();
        // Seconds to a distance from the reading start, and back.
        let at = move |x: f32| -> f64 {
            if measured <= 0.0 || duration <= 0.0 {
                return 0.0;
            }
            let x = if rtl { measured - x } else { x };
            f64::from((x / measured).clamp(0.0, 1.0)) * duration
        };
        let x_of = move |seconds: f64| -> f32 {
            if duration <= 0.0 {
                return 0.0;
            }
            let along = (seconds / duration).clamp(0.0, 1.0) as f32 * measured;
            if rtl { measured - along } else { along }
        };

        let on_seek = self.on_seek.clone();
        let on_hover = self.on_hover.clone();
        let capture = use_capture({
            let on_seek = on_seek.clone();
            move |event: CaptureEvent| {
                let seconds = at(event.local.0);
                match event.phase {
                    Phase::Start => {
                        a11y_id.request_focus();
                        drag.set(Some(seconds));
                        on_seek.call(Seek {
                            to: seconds,
                            done: false,
                        });
                    }
                    Phase::Move => {
                        drag.set(Some(seconds));
                        on_seek.call(Seek {
                            to: seconds,
                            done: false,
                        });
                    }
                    Phase::End => {
                        drag.set(None);
                        on_seek.call(Seek {
                            to: seconds,
                            done: true,
                        });
                    }
                    Phase::Cancel => drag.set(None),
                }
            }
        });
        let tracker = capture.clone();

        let shown = drag
            .read()
            .unwrap_or(self.position)
            .clamp(0.0, duration.max(self.position));
        let interacting = drag.read().is_some() || hover.read().is_some() || focused;
        let track_height = if interacting {
            Dim::SliderTrackActive.px()
        } else {
            Dim::SliderTrack.px()
        };
        let track_top = centre - track_height / 2.0;
        let (base, faint, played_paint) = if self.floating {
            (
                Paint::FloatingOutline,
                Paint::OnFloating,
                if interacting {
                    Paint::Accent
                } else {
                    Paint::OnFloating
                },
            )
        } else {
            (
                Paint::Outline,
                Paint::Muted,
                if interacting {
                    Paint::Accent
                } else {
                    Paint::Text
                },
            )
        };
        // A stretch of the track from `start` to `end` seconds, whichever way it reads.
        let span = |start: f64, end: f64| -> (f32, f32) {
            let (a, b) = (x_of(start), x_of(end));
            (a.min(b), (a - b).abs())
        };
        let bar = |start: f64, end: f64, paint: Paint, opacity: f32, layer: i16| -> Element {
            let (left, length) = span(start, end);
            rect()
                .layer(Layer::Relative(layer))
                .position(Position::new_absolute().top(track_top).left(left))
                .width(Size::px(length))
                .height(Size::px(track_height))
                .opacity(opacity)
                .tw(&tokens, &[rounded(Radius::Pill), bg(paint)])
                .into()
        };

        let mut track = rect()
            .component("TimelineTrack")
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_role(AccessibilityRole::Slider)
            .a11y_alt(self.label.clone())
            .a11y_builder({
                let text = format!("{} of {}", spoken_time(shown), spoken_time(duration));
                move |builder| {
                    builder.set_numeric_value(shown);
                    builder.set_min_numeric_value(0.0);
                    builder.set_max_numeric_value(duration);
                    builder.set_value(text.clone());
                }
            })
            .width(if self.times {
                Size::px((*row_width.read() - 2.0 * Dim::Box24.px()).max(0.0))
            } else {
                Size::flex(1.0)
            })
            .min_width(Size::px(0.0))
            .tw(&tokens, &[h(Dim::Control)])
            .maybe(!enabled, |element| element.opacity(DISABLED_OPACITY))
            .on_sized(move |event: Event<SizedEventData>| {
                if (*width.peek() - event.area.width()).abs() > 0.01 {
                    width.set(event.area.width());
                }
                tracker.sized(&event);
            })
            .child(bar(0.0, duration, base, 1.0, 0));
        for (start, end) in &self.buffered {
            track = track.child(bar(*start, *end, faint, 0.35, 1));
        }
        if let Some(x) = *hover.read()
            && drag.read().is_none()
        {
            track = track.child(bar(0.0, at(x), faint, 0.32, 2));
        }
        track = track.child(bar(0.0, shown, played_paint, 1.0, 3));
        // Chapter breaks: a gap in the surface's colour, wider than the track is tall.
        for (start, _) in self.chapters.iter().skip(1) {
            let x = x_of(*start);
            track = track.child(
                rect()
                    .layer(Layer::Relative(4))
                    .position(Position::new_absolute().top(track_top - 1.0).left(x - 1.0))
                    .width(Size::px(MARK))
                    .height(Size::px(track_height + MARK))
                    .tw(&tokens, &[bg(self.surface)]),
            );
        }
        for point in [self.loop_points.0, self.loop_points.1]
            .into_iter()
            .flatten()
        {
            let tick = thumb * 0.8;
            track = track.child(
                rect()
                    .layer(Layer::Relative(5))
                    .position(
                        Position::new_absolute()
                            .top(centre - tick / 2.0)
                            .left(x_of(point) - 1.0),
                    )
                    .width(Size::px(MARK))
                    .height(Size::px(tick))
                    .tw(&tokens, &[rounded(Radius::Pill), bg(Paint::Accent)]),
            );
        }
        // The knob: under the pointer, while dragging and with the keyboard.
        if enabled && interacting {
            let x = x_of(shown);
            track = track.child(
                rect()
                    .layer(Layer::Relative(6))
                    .position(
                        Position::new_absolute()
                            .top(centre - thumb / 2.0)
                            .left((x - thumb / 2.0).round()),
                    )
                    .width(Size::px(thumb))
                    .height(Size::px(thumb))
                    .tw(
                        &tokens,
                        &[
                            rounded(Radius::Pill),
                            bg(Paint::Knob),
                            shadow(Elevation::Knob),
                        ],
                    ),
            );
        }
        // The bubble over the pointer: the picture, the time, the chapter.
        let bubble_at = (*drag.read()).map(x_of).or(*hover.read());
        if let Some(x) = bubble_at.filter(|_| enabled) {
            let seconds = at(x);
            let chapter =
                chapter_at(&self.chapters, seconds).map(|index| self.chapters[index].1.clone());
            let ink = if self.floating {
                Ink::OnFloating
            } else {
                Ink::Text
            };
            let preview = self
                .preview
                .clone()
                .filter(|_| hover.read().is_some() || drag.read().is_some());
            let card = rect()
                .component("TimelineBubble")
                .tw(
                    &tokens,
                    &[
                        flex_col(),
                        items(Align::Center),
                        gap(S1),
                        p(S1),
                        rounded(Radius::Control),
                        bg(if self.floating {
                            Paint::Floating
                        } else {
                            Paint::Popover
                        }),
                        shadow(Elevation::Floating),
                    ],
                )
                .maybe_child(preview)
                .child(NUMERIC.in_ink(ink).label(&tokens, format_time(seconds)))
                .maybe_child(
                    chapter.map(|title| CAPTION.in_ink(ink).label(&tokens, title).max_lines(1)),
                );
            // The bubble's bottom sits a little above the track, its middle at the pointer.
            track = track.child(CentredOn {
                content: card.into_element(),
                left: x,
                bottom: hit - S1.px(),
            });
        }
        let track = if enabled {
            capture
                .handlers(track)
                .on_pointer_move(move |event: Event<PointerEventData>| {
                    let x = event.element_location().x as f32;
                    hover.set_if_modified(Some(x));
                    if let Some(on_hover) = &on_hover {
                        on_hover.call(Some(at(x)));
                    }
                })
                .on_pointer_leave({
                    let on_hover = self.on_hover.clone();
                    move |_| {
                        hover.set_if_modified(None);
                        if let Some(on_hover) = &on_hover {
                            on_hover.call(None);
                        }
                    }
                })
                .on_key_down({
                    let chapters = self.chapters.clone();
                    let position = self.position;
                    let on_seek = on_seek.clone();
                    move |event: Event<KeyboardEventData>| {
                        let along = direction.along(&event.key).unwrap_or(0);
                        let shift = event.modifiers.contains(Modifiers::SHIFT);
                        if let Some(to) =
                            key_target(&event.key, shift, position, duration, &chapters, along)
                        {
                            event.stop_propagation();
                            on_seek.call(Seek { to, done: true });
                        }
                    }
                })
        } else {
            track
        };
        let track = track.focusable(focus());

        if !self.times {
            return track.into_element();
        }
        let ink = if self.floating {
            Ink::OnFloating
        } else {
            Ink::Muted
        };
        let right = if *show_length.read() {
            format_time(duration)
        } else {
            format_time(shown - duration)
        };
        // Wide enough for the longest signed reading ("-1:02:03"), so the track
        // does not move as digits change and the remaining time is never clipped.
        let label_width = Dim::Box24;
        rect()
            .component("Timeline")
            .text_direction(TextDirection::Ltr)
            .width(Size::fill())
            .on_sized(move |event: Event<SizedEventData>| {
                if (*row_width.peek() - event.area.width()).abs() > 0.01 {
                    row_width.set(event.area.width());
                }
            })
            .tw(
                &tokens,
                &[flex_row(), grow_children(), items(Align::Center)],
            )
            .child(
                rect()
                    .tw(
                        &tokens,
                        &[w(label_width), flex_row(), justify(Align::End), pl(S2)],
                    )
                    .child(NUMERIC.in_ink(ink).label(&tokens, format_time(shown))),
            )
            .child(track)
            .child(
                rect()
                    .tw(
                        &tokens,
                        &[w(label_width), flex_row(), justify(Align::Start), pr(S2)],
                    )
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt(if *show_length.read() {
                        "Show time left"
                    } else {
                        "Show length"
                    })
                    .on_press(move |_| {
                        let now = *show_length.peek();
                        show_length.set(!now);
                    })
                    .child(NUMERIC.in_ink(ink).label(&tokens, right)),
            )
            .into_element()
    }
}

/// Moves its content back by half its own width, so a bubble placed at the
/// pointer is centred on it.
#[derive(PartialEq)]
struct CentredOn {
    content: Element,
    left: f32,
    bottom: f32,
}

impl Component for CentredOn {
    fn render(&self) -> impl IntoElement {
        let mut width = use_state(|| 0.0f32);
        rect()
            .layer(crate::layers::TOOLTIP)
            .position(Position::new_absolute().bottom(self.bottom).left(self.left))
            .offset_x(-*width.read() / 2.0)
            .on_sized(move |event: Event<SizedEventData>| {
                width.set_if_modified(event.area.width());
            })
            .child(self.content.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapters() -> Vec<(f64, String)> {
        vec![
            (0.0, "One".into()),
            (10.0, "Two".into()),
            (20.0, "Three".into()),
        ]
    }

    #[test]
    fn chapters_are_found_by_time() {
        assert_eq!(chapter_at(&chapters(), 0.0), Some(0));
        assert_eq!(chapter_at(&chapters(), 15.0), Some(1));
        assert_eq!(chapter_at(&chapters(), 99.0), Some(2));
        assert_eq!(chapter_at(&[], 5.0), None);
    }

    #[test]
    fn keys_move_by_steps_and_chapters() {
        let key = |text: &str| Key::Character(text.into());
        let right = Key::Named(NamedKey::ArrowRight);
        assert_eq!(key_target(&right, false, 12.0, 30.0, &[], 1), Some(17.0));
        assert_eq!(key_target(&right, true, 12.0, 30.0, &[], 1), Some(30.0));
        assert_eq!(
            key_target(&Key::Named(NamedKey::Home), false, 12.0, 30.0, &[], 0),
            Some(0.0)
        );
        assert_eq!(
            key_target(&key("."), false, 12.0, 30.0, &chapters(), 0),
            Some(20.0)
        );
        // Well into a chapter, "," goes to its start; near its start, to the one before.
        assert_eq!(
            key_target(&key(","), false, 15.0, 30.0, &chapters(), 0),
            Some(10.0)
        );
        assert_eq!(
            key_target(&key(","), false, 11.0, 30.0, &chapters(), 0),
            Some(0.0)
        );
    }
}
