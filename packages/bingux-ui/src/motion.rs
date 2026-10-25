//! Motion: small animations that never make the layout around them jump.
//!
//! Every component here keeps layout stable:
//!
//! - [`reveal`] animates its own height, so the content below it moves
//!   smoothly instead of jumping.
//! - [`presence`] fades and scales in place. Its box keeps its space until
//!   the exit finishes, then releases it. Call [`Presence::collapse`] to
//!   animate that space away too.
//! - [`crossfade`] holds the larger of the old and new child's sizes during
//!   the swap, so the container never shrinks mid-swap. When the new child is
//!   smaller, the box settles to it at the end of the swap; wrap the child in
//!   [`reveal`] if that step matters.
//! - [`use_tween`] and [`animated_number`] change values, not the size of the
//!   box around them. Give text a fixed width when its digits must not move
//!   neighbours.
//!
//! Durations go through [`Tokens::motion`], so reduced motion turns every
//! animation into an instant change.

use std::time::Duration;

use freya::animation::*;
use freya::prelude::*;

use crate::focus::{claim_press, use_focus_core};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{NUMERIC, TextStyle, styled_label};

/// The default duration of a motion, in milliseconds, before reduced motion.
pub const MOVE_MS: u64 = 160;

/// A [`presence`] starts this far scaled down and grows to full size.
pub const PRESENCE_SCALE: f32 = 0.96;

/// The value tweening towards `target` over `ms` milliseconds. When `target`
/// changes, the tween starts from the value it has reached, so nothing snaps.
pub fn use_tween(target: f32, ms: u64) -> f32 {
    use_animation_transition(target, move |from, to| {
        AnimNum::new(from, to)
            .time(ms)
            .function(Function::Quad)
            .ease(Ease::Out)
    })
    .get()
    .value()
}

/// Progress from 0 to 1 while `open`, and back to 0 when it closes. `on_exit`
/// runs once the closing motion has finished, unless the element was reopened
/// first.
pub fn use_presence(open: bool, motion: u64, on_exit: impl Fn() + Clone + 'static) -> f32 {
    let generation = use_state(|| 0_u64);
    let appear = use_animation(move |conf| {
        conf.on_creation(OnCreation::Run);
        AnimNum::new(0.0, 1.0)
            .time(motion)
            .function(Function::Quad)
            .ease(Ease::Out)
    });
    use_side_effect_with_deps(&open, move |&open: &bool| {
        let mut appear = appear;
        let mut generation = generation;
        // Every change of `open` starts a new generation, so a stale exit
        // timer from an earlier close never runs `on_exit` for a reopened
        // element.
        let current = generation.peek().wrapping_add(1);
        generation.set(current);
        if open {
            appear.start();
            return;
        }
        appear.reverse();
        let on_exit = on_exit.clone();
        spawn(async move {
            timer(Duration::from_millis(motion)).await;
            if *generation.peek() == current {
                on_exit();
            }
        });
    });
    appear.get().value()
}

/// Shows or hides `child` by animating its height. The box clips the child,
/// so the content below moves with the edge instead of jumping.
#[derive(Clone, PartialEq)]
pub struct Reveal {
    open: bool,
    child: Element,
}

/// A [`Reveal`] of `child`, open or closed.
pub fn reveal(open: bool, child: impl Into<Element>) -> Reveal {
    Reveal {
        open,
        child: child.into(),
    }
}

impl Component for Reveal {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let progress = use_tween(if self.open { 1.0 } else { 0.0 }, tokens.motion(MOVE_MS));
        // The child's natural height, measured while it is laid out inside the
        // clip. Until it is known, an open reveal is auto-sized and a closed
        // one is empty.
        let mut measured = use_state(|| None::<f32>);
        let height = match *measured.read() {
            Some(height) => Size::px(height * progress),
            None if self.open => Size::auto(),
            None => Size::px(0.0),
        };
        rect().tw_layout(&[clip(), w_full()]).height(height).child(
            rect()
                .width(Size::fill())
                .on_sized(move |event: Event<SizedEventData>| {
                    let next = event.area.height();
                    if *measured.peek() != Some(next) {
                        measured.set(Some(next));
                    }
                })
                .child(self.child.clone()),
        )
    }
}

/// Fades and scales `child` in and out. Its box keeps its space until the
/// exit finishes, then releases it.
#[derive(Clone, PartialEq)]
pub struct Presence {
    open: bool,
    child: Element,
    collapse: bool,
}

/// A [`Presence`] of `child`, shown while `open`.
pub fn presence(open: bool, child: impl Into<Element>) -> Presence {
    Presence {
        open,
        child: child.into(),
        collapse: false,
    }
}

impl Presence {
    /// Also animate the child's height, so the space it held collapses
    /// smoothly on exit and opens on entry, as a [`reveal`] does.
    pub fn collapse(mut self) -> Self {
        self.collapse = true;
        self
    }
}

impl Component for Presence {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let motion = tokens.motion(MOVE_MS);
        // True once the exit has finished, so the child is no longer mounted.
        let exited = use_state(|| !self.open);
        use_side_effect_with_deps(&self.open, move |&open: &bool| {
            let mut exited = exited;
            if open {
                exited.set(false);
            }
        });
        let progress = use_presence(self.open, motion, move || {
            let mut exited = exited;
            exited.set(true);
        });
        if !self.open && *exited.read() {
            return Element::from(rect());
        }
        let scale = PRESENCE_SCALE + (1.0 - PRESENCE_SCALE) * progress;
        let body = rect()
            .opacity(progress)
            .scale(scale)
            .child(self.child.clone());
        if self.collapse {
            Element::from(reveal(self.open, body))
        } else {
            Element::from(body)
        }
    }
}

/// Swaps the shown child with a fade, holding the larger size during the swap.
#[derive(Clone, PartialEq)]
pub struct Crossfade {
    child: Element,
}

/// A [`Crossfade`] that shows `child`. Give it a new child to swap.
pub fn crossfade(child: impl Into<Element>) -> Crossfade {
    Crossfade {
        child: child.into(),
    }
}

impl Component for Crossfade {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let motion = tokens.motion(MOVE_MS);
        // The child shown before the latest swap, fading out.
        let shown = use_state(|| self.child.clone());
        let outgoing = use_state(|| None::<Element>);
        // Measured size of the in-flow child, and the size held for a swap.
        let mut size = use_state(|| (0.0_f32, 0.0_f32));
        let hold = use_state(|| (0.0_f32, 0.0_f32));
        let swap = use_state(|| 0_u64);
        let mut fade = use_animation(move |conf| {
            conf.on_creation(OnCreation::Finish);
            AnimNum::new(0.0, 1.0)
                .time(motion)
                .function(Function::Quad)
                .ease(Ease::Out)
        });
        use_side_effect_with_deps(&self.child, move |child: &Element| {
            let mut shown = shown;
            let mut outgoing = outgoing;
            let mut hold = hold;
            let mut swap = swap;
            if *shown.peek() == *child {
                return;
            }
            // The in-flow child is still the old one when this runs, so `size`
            // is the old child's size: the container keeps it for the swap.
            let previous = shown.peek().clone();
            hold.set(*size.peek());
            outgoing.set(Some(previous));
            shown.set(child.clone());
            let current = swap.peek().wrapping_add(1);
            swap.set(current);
            fade.reset();
            fade.start();
            spawn(async move {
                timer(Duration::from_millis(motion)).await;
                if *swap.peek() == current {
                    outgoing.set(None);
                    hold.set((0.0, 0.0));
                }
            });
        });
        let progress = fade.get().value();
        let (hold_width, hold_height) = *hold.read();
        let incoming = rect()
            .opacity(progress)
            .on_sized(move |event: Event<SizedEventData>| {
                let next = (event.area.width(), event.area.height());
                if *size.peek() != next {
                    size.set(next);
                }
            })
            .child(self.child.clone());
        let mut container = rect()
            .min_width(Size::px(hold_width))
            .min_height(Size::px(hold_height))
            .child(incoming);
        if let Some(previous) = outgoing.read().clone() {
            container = container.child(
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .expanded()
                    .opacity(1.0 - progress)
                    .child(previous),
            );
        }
        container
    }
}

/// A number that counts to `value` instead of jumping.
#[derive(Clone, PartialEq)]
pub struct AnimatedNumber {
    value: f32,
    decimals: usize,
    style: TextStyle,
}

/// An [`AnimatedNumber`] that shows `value`, with the numeric text style.
pub fn animated_number(value: f32) -> AnimatedNumber {
    AnimatedNumber {
        value,
        decimals: 0,
        style: NUMERIC,
    }
}

impl AnimatedNumber {
    /// Digits shown after the decimal point.
    pub fn decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals;
        self
    }

    /// The text style of the number.
    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }
}

impl Component for AnimatedNumber {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let shown = use_tween(self.value, tokens.motion(MOVE_MS));
        styled_label(self.style, format!("{:.*}", self.decimals, shown))
    }
}

/// The gap, in pixels, between a control's edge and the inner edge of its
/// focus ring once the ring has settled.
pub const FOCUS_GAP: f32 = 2.0;
/// How far outside its settled place the ring starts, in pixels. The ring
/// travels this far inward as focus arrives and outward as it leaves.
pub const FOCUS_TRAVEL: f32 = 4.0;
/// Focus arrives in this long, ease out.
pub const FOCUS_IN_MS: u64 = 160;
/// Focus leaves in this long, ease out. Leaving is quicker so the ring never
/// lingers after the control has lost focus.
pub const FOCUS_OUT_MS: u64 = 120;

/// Wraps a control so that its keyboard focus ring hugs the control's own
/// outer box. The ring is measured from the wrapper, which has no padding or
/// border, so its absolute offsets start at exactly the control's top-left
/// corner and its size is exactly the control's size.
///
/// Only keyboard focus draws the ring. On arrival it grows in from outside and
/// fades in; on departure it grows back out and fades away. The ring ignores
/// pointer events, so the control keeps every click. Reduced motion makes both
/// changes instant.
pub trait Focusable: Sized {
    /// Wraps this control, drawing its focus ring for corner radius `radius`
    /// while `focus` is keyboard focus. Return the wrapper from `render`.
    ///
    /// The wrapper is a row that takes its content's size. A control that
    /// grows to fill its row also grows the wrapper, with `w_flex()` on the
    /// returned box.
    fn focusable(self, focus: Focus, radius: Radius) -> Rect;

    /// Wraps a row whose edges may sit on a clipped container edge, such as a
    /// card's. The ring is drawn just inside the wrapper, so the clip cannot
    /// cut it off. Return the wrapper from `render`.
    fn focusable_inside(self, focus: Focus, radius: Radius) -> Rect;
}

impl Focusable for Rect {
    fn focusable(self, focus: Focus, radius: Radius) -> Rect {
        wrap_focusable(self, focus, radius, false)
    }

    fn focusable_inside(self, focus: Focus, radius: Radius) -> Rect {
        wrap_focusable(self, focus, radius, true)
    }
}

/// Builds the wrapper for [`Focusable`]: the press claim and the ring.
fn wrap_focusable(control: Rect, focus: Focus, radius: Radius, inside: bool) -> Rect {
    let tokens = use_tokens();
    let core = use_focus_core();
    // A press on a control claims it, so its region does not take the
    // press as empty space and move focus.
    rect()
        .tw(&tokens, &[flex_row()])
        .on_press(move |_: Event<PressEventData>| claim_press(&core))
        .child(control)
        .child(FocusRing {
            focus,
            radius,
            inside,
        })
}

/// The ring itself, drawn as the last child of a [`Focusable::focusable`]
/// wrapper.
#[derive(Clone, PartialEq)]
struct FocusRing {
    focus: Focus,
    radius: Radius,
    inside: bool,
}

impl Component for FocusRing {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let target = if self.focus == Focus::Keyboard {
            1.0
        } else {
            0.0
        };
        let in_ms = tokens.motion(FOCUS_IN_MS);
        let out_ms = tokens.motion(FOCUS_OUT_MS);
        // One progress value drives both the offset and the opacity, so the
        // ring grows and fades together in each direction.
        let progress = use_animation_transition(target, move |from: f32, to: f32| {
            AnimNum::new(from, to)
                .time(if to > from { in_ms } else { out_ms })
                .function(Function::Quad)
                .ease(Ease::Out)
        })
        .get()
        .value();
        // The ring's outer edge sits this far outside the control, so the
        // ring's corner radius is the control's plus this distance.
        let travel = FOCUS_TRAVEL * (1.0 - progress);
        if self.inside {
            // The ring's outer edge sits this far inside the wrapper's edge.
            let inset = FOCUS_GAP + travel;
            let inner = move |parent: f32| parent - inset - inset;
            return rect()
                .interactive(false)
                .position(Position::new_absolute().top(inset).left(inset))
                .width(Size::func(move |ctx| Some(inner(ctx.parent))))
                .height(Size::func(move |ctx| Some(inner(ctx.parent))))
                .opacity(progress)
                .tw(
                    &tokens,
                    &[
                        rounded_by(self.radius, -inset),
                        border(Line::Ring(Paint::Accent)),
                    ],
                );
        }
        let outset = FOCUS_GAP + travel + Dim::Ring.px();
        // The wrapper has no padding, so the ring's top-left corner is the
        // control's top-left corner and its size is the control's size.
        rect()
            .interactive(false)
            .position(Position::new_absolute().top(-outset).left(-outset))
            .width(Size::func(move |ctx| Some(ctx.parent + outset + outset)))
            .height(Size::func(move |ctx| Some(ctx.parent + outset + outset)))
            .opacity(progress)
            .tw(
                &tokens,
                &[
                    rounded_by(self.radius, outset),
                    border(Line::Ring(Paint::Accent)),
                ],
            )
    }
}
