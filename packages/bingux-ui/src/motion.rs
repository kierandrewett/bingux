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

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use freya::animation::*;
use freya::prelude::*;

use crate::component::ComponentName;
use crate::focus::note_press;
use crate::focus::{FocusCore, InsideScope, claim_press, use_focus_core};
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{NUMERIC, TextStyle, styled_label};

/// The default duration of a motion, in milliseconds, before reduced motion.
pub const MOVE_MS: u64 = 160;

/// How long an [`animated_number`] rolls, in milliseconds.
const ROLL_MS: u64 = 320;
/// How far below its place an [`animated_number`] starts, in pixels.
const ROLL_LIFT: f32 = 8.0;

/// A [`presence`] starts this far scaled down and grows to full size.
pub const PRESENCE_SCALE: f32 = 0.96;

/// The value tweening towards `target` over `ms` milliseconds. It starts at
/// `target`, so nothing animates on the first frame. When `target` changes,
/// the tween starts from the value on screen, so reversing a motion part-way
/// continues from where it is and never snaps. With `ms` of 0 (reduced
/// motion) it jumps.
pub fn use_tween(target: f32, ms: u64) -> f32 {
    use_tween_with(target, target, ms, Function::Quad)
}

/// [`use_tween`] for something that mounts already on its way: it starts at
/// `start` and moves to `target` at once, over `ms`. A focus ring that is only
/// mounted when focus first reaches it arrives like this, from 0 to 1.
pub fn use_tween_from(start: f32, target: f32, ms: u64) -> f32 {
    use_tween_with(start, target, ms, Function::Quad)
}

/// [`use_tween`] along another curve than the standard one, such as the
/// cubic of a switch's thumb. A control that moves between two values (a
/// switch) uses this and not [`use_animation_transition`]: that one runs an
/// animation as the control mounts, from the value to itself, and a page of
/// switches starts one task for each of them on the frame that mounts it.
pub fn use_tween_with(start: f32, target: f32, ms: u64, function: Function) -> f32 {
    let animation = use_animation(move |conf| {
        conf.on_creation(OnCreation::Finish);
        ease_along(start, start, ms, function)
    });
    // The last target asked for: a tween turned around on the frame it began
    // is still at its start, which is the new target, yet its run towards the
    // old one must be stopped.
    let asked = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(start)));
    use_side_effect_with_deps(&(target.to_bits(), ms), move |&(bits, ms): &(u32, u64)| {
        let mut animation = animation;
        let mut state = *animation;
        let target = f32::from_bits(bits);
        let from = state.peek().value();
        let before = asked.replace(target);
        if from == target {
            if before != target {
                *state.write() = ease_along(target, target, 0, function);
            }
            return;
        }
        if ms == 0 {
            *state.write() = ease_along(target, target, 0, function);
            return;
        }
        *state.write() = ease_along(from, target, ms, function);
        animation.start();
    });
    animation.get().value()
}

/// `from` to `to` over `ms` along `function`, easing out.
fn ease_along(from: f32, to: f32, ms: u64, function: Function) -> AnimNum {
    AnimNum::new(from, to)
        .time(ms)
        .function(function)
        .ease(Ease::Out)
}

/// Progress from 0 to 1 while `open`, and back to 0 when it closes. `on_exit`
/// runs once the closing motion has finished, unless the element was reopened
/// first.
pub fn use_presence(open: bool, motion: u64, on_exit: impl Fn() + Clone + 'static) -> f32 {
    let generation = use_state(|| 0_u64);
    let appear = use_animation(move |conf| {
        if open {
            conf.on_creation(OnCreation::Run);
        }
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
        // A run restarts from the animation's origin, so the motion is rebuilt
        // from the value on screen. Reversing part-way through then eases back
        // from there instead of jumping to full progress.
        let mut retarget = move |target: f32| {
            let mut state = *appear;
            let from = state.peek().value();
            *state.write() = AnimNum::new(from, target)
                .time(motion)
                .function(Function::Quad)
                .ease(Ease::Out);
            appear.start();
        };
        if open {
            retarget(1.0);
            return;
        }
        // An element that mounted closed has never opened, so it has nothing to
        // exit. Reversing it would start from full progress and flash it.
        if !*appear.has_run_yet().peek() {
            return;
        }
        retarget(0.0);
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

/// How far a [`reveal`]'s content sits above its place while closed, in
/// pixels. It settles down as the box opens, under the fade.
pub const REVEAL_LIFT: f32 = 6.0;

/// The box height of a reveal at `progress` (0 closed, 1 open) when its
/// content is `natural` pixels tall. Linear in `progress`: the easing is in
/// the progress itself.
pub fn reveal_height(natural: f32, progress: f32) -> f32 {
    natural.max(0.0) * progress.clamp(0.0, 1.0)
}

/// The opacity and vertical offset of a reveal's content at `progress`. The
/// content fades in over the second half of the opening, so the box is
/// already moving when the first rows appear, and the lift settles with it.
pub fn reveal_layers(progress: f32) -> (f32, f32) {
    let progress = progress.clamp(0.0, 1.0);
    let fade = ((progress - 0.25) / 0.75).clamp(0.0, 1.0);
    (fade, -(1.0 - progress) * REVEAL_LIFT)
}

/// The height a reveal lays out with. A settled open reveal is auto-sized, so
/// content that changes (async rows, wrapped text) never waits on a
/// measurement; a moving one is the measured height times its progress; a
/// settled closed one is empty. Until the content is measured, an open
/// reveal is auto-sized and a closed one is empty.
fn reveal_size(open: bool, progress: f32, measured: Option<f32>, lazy: bool) -> Size {
    if open && progress >= 1.0 {
        return Size::auto();
    }
    match measured {
        Some(natural) => Size::px(reveal_height(natural, progress)),
        None if open && !lazy => Size::auto(),
        None => Size::px(0.0),
    }
}

/// The shared state of [`Reveal`] and [`RevealBlock`]: the eased progress,
/// and the content's natural height as measured while laid out. The tween
/// starts from the value on screen when `open` flips mid-motion, so a
/// reversal continues from the current height. It starts at its target, so a
/// reveal that mounts closed (or open) never animates on the first frame.
fn use_reveal(open: bool) -> (f32, State<Option<f32>>) {
    let tokens = use_tokens();
    let progress = use_tween(if open { 1.0 } else { 0.0 }, tokens.motion(MOVE_MS));
    (progress, use_state(|| None::<f32>))
}

/// The measuring wrapper around a reveal's content: tracks the natural
/// height, and lays the content out with the fade and lift of `progress`.
fn reveal_content(mut measured: State<Option<f32>>, progress: f32, child: Element) -> Rect {
    let (fade, lift) = reveal_layers(progress);
    rect()
        .width(Size::fill())
        .on_sized(move |event: Event<SizedEventData>| {
            let next = event.area.height();
            if *measured.peek() != Some(next) {
                measured.set(Some(next));
            }
        })
        .offset_y(lift)
        .opacity(fade)
        .child(child)
}

/// Shows or hides `child` by animating its height. The box clips the child,
/// so the content below moves with the edge instead of jumping. This is the
/// one primitive for every conditional row or section.
#[derive(Clone, PartialEq)]
pub struct Reveal {
    open: bool,
    child: Element,
    lazy: bool,
}

/// A [`Reveal`] of `child`, open or closed.
pub fn reveal(open: bool, child: impl Into<Element>) -> Reveal {
    Reveal {
        open,
        child: child.into(),
        lazy: false,
    }
}

impl Reveal {
    /// Unmount the child once the reveal has settled closed, so nothing in a
    /// hidden section can take focus or work. The child mounts again as the
    /// reveal opens.
    pub fn lazy(mut self) -> Self {
        self.lazy = true;
        self
    }
}

impl Component for Reveal {
    fn render(&self) -> impl IntoElement {
        let (progress, measured) = use_reveal(self.open);
        if self.lazy && !self.open && progress <= 0.001 {
            return rect().tw_layout(&[clip(), w_full()]).height(Size::px(0.0));
        }
        let natural = *measured.read();
        let height = reveal_size(self.open, progress, natural, self.lazy);
        rect()
            .tw_layout(&[clip(), w_full()])
            .height(height)
            .child(reveal_content(measured, progress, self.child.clone()))
    }
}

/// A [`reveal`] for a whole block, such as a group, in a column that spaces
/// its children `gap` pixels apart. The block fades and grows to its height
/// together with the gap it brings, so the content below moves smoothly and the
/// content above never moves. Closed, it is out of the flow: it takes no gap.
#[derive(Clone, PartialEq)]
pub struct RevealBlock {
    open: bool,
    gap: Space,
    child: Element,
    lazy: bool,
}

/// A [`RevealBlock`] of `child`, open or closed, in a column with the space
/// token `gap` between its children.
pub fn reveal_block(open: bool, gap: Space, child: impl Into<Element>) -> RevealBlock {
    RevealBlock {
        open,
        gap,
        child: child.into(),
        lazy: false,
    }
}

impl RevealBlock {
    /// As [`Reveal::lazy`]: unmount the child while settled closed.
    pub fn lazy(mut self) -> Self {
        self.lazy = true;
        self
    }
}

impl Component for RevealBlock {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let (progress, measured) = use_reveal(self.open);
        // Unless lazy, the child stays mounted while closed, so its natural
        // height is known when it opens.
        let settled_closed = !self.open && progress <= 0.001;
        if self.lazy && settled_closed {
            return rect()
                .tw_layout(&[clip(), w_full()])
                .position(Position::new_absolute().top(0.0).left(0.0))
                .height(Size::px(0.0));
        }
        let moving = !settled_closed && progress < 1.0;
        let natural = *measured.read();
        let height = reveal_size(self.open, progress, natural, self.lazy);
        rect()
            .tw_layout(&[clip(), w_full()])
            .maybe(settled_closed, |element| {
                element.position(Position::new_absolute().top(0.0).left(0.0))
            })
            .maybe(moving, |element| {
                element.tw(&tokens, &[pull_up(self.gap, 1.0 - progress)])
            })
            .height(height)
            .child(reveal_content(measured, progress, self.child.clone()))
    }
}

/// A conditional child: shown, with its height animating in, while it is
/// `Some`; kept for the closing motion after it becomes `None`, then
/// unmounted. This is what replaces `.maybe_child(..)` for a row, banner or
/// section whose presence follows state. With `gap` it is a [`RevealBlock`]
/// for a column that spaces its children; without, a [`Reveal`].
#[derive(Clone, PartialEq)]
pub struct RevealIf {
    child: Option<Element>,
    gap: Option<Space>,
}

/// A [`RevealIf`] of `child` in a column without a gap.
pub fn reveal_if(child: Option<Element>) -> RevealIf {
    RevealIf { child, gap: None }
}

/// A [`RevealIf`] of `child` in a column that spaces its children by `gap`.
pub fn reveal_block_if(gap: Space, child: Option<Element>) -> RevealIf {
    RevealIf {
        child,
        gap: Some(gap),
    }
}

impl Component for RevealIf {
    fn render(&self) -> impl IntoElement {
        // The last child shown, so the closing motion still has content.
        let last = use_hook(|| Rc::new(RefCell::new(None::<Element>)));
        let open = self.child.is_some();
        if open {
            *last.borrow_mut() = self.child.clone();
        }
        let shown = last.borrow().clone().unwrap_or_else(|| rect().into());
        match self.gap {
            Some(gap) => Element::from(reveal_block(open, gap, shown).lazy()),
            None => Element::from(reveal(open, shown).lazy()),
        }
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
            .tw(
                &tokens,
                &[flex_col(), justify(Align::Center), items(Align::Center)],
            )
            .min_width(Size::px(hold_width))
            .min_height(Size::px(hold_height))
            .child(incoming);
        if let Some(previous) = outgoing.read().clone() {
            container = container.child(
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .expanded()
                    .tw(
                        &tokens,
                        &[flex_col(), justify(Align::Center), items(Align::Center)],
                    )
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
        // The digits count through the values between, and each change also
        // lifts the number in from below while it fades up, so a step of one
        // (which counts through no value at all) still reads as a change.
        let shown = use_tween(self.value, tokens.motion(ROLL_MS));
        let bump_ms = tokens.motion(ROLL_MS);
        let bump = use_animation(move |_| {
            AnimNum::new(0.0, 1.0)
                .time(bump_ms)
                .function(Function::Quad)
                .ease(Ease::Out)
        });
        use_side_effect_with_deps(&self.value.to_bits(), move |_| {
            let mut bump = bump;
            bump.start();
        });
        let progress = if *bump.has_run_yet().read() {
            bump.get().value()
        } else {
            1.0
        };
        rect()
            .offset_y((1.0 - progress) * ROLL_LIFT)
            .opacity(0.35 + 0.65 * progress)
            .child(styled_label(
                self.style,
                format!("{:.*}", self.decimals, shown),
            ))
    }
}

/// The gap, in pixels, between a control's edge and the inner edge of its
/// focus ring once the ring has settled.
pub const FOCUS_GAP: f32 = 2.0;
/// The ring's own width: the shared `Ring` dimension.
pub const FOCUS_WIDTH: f32 = 2.0;
/// How far outside its settled place the ring starts, in pixels. The ring
/// travels this far inward as focus arrives and outward as it leaves.
pub const FOCUS_TRAVEL: f32 = 4.0;
/// The blur of the ring's halo, in pixels, as focus arrives and as it leaves:
/// the ring sharpens from this to 0 and softens from 0 to this.
pub const FOCUS_BLUR: f32 = 4.0;
/// Focus arrives in this long, ease out.
pub const FOCUS_IN_MS: u64 = 160;
/// Focus leaves in this long, ease out. Leaving is quicker so the ring never
/// lingers after the control has lost focus.
pub const FOCUS_OUT_MS: u64 = 120;

/// Wraps a control so that its keyboard focus ring is drawn around the
/// control's own painted box, its **face**.
///
/// The control passed to `focusable` is the face: the node that carries the
/// fill and the corner radius. The wrapper hugs it (it has no padding, no
/// border and no margin of its own) and measures it after layout, and the ring
/// is laid out from that measured box, never from the face's padding or
/// content. A padded face whose ring was its child ended up inset by the
/// padding (the dropdown's ring ran inside its fill, 12 px short on the left);
/// measuring the wrapper makes the padding irrelevant. A face must not carry a
/// margin: the margin would sit inside the wrapper's box and outside the face.
///
/// Only keyboard focus draws the ring. On arrival it grows in from outside and
/// fades in; on departure it grows back out and fades away. The ring ignores
/// pointer events, so the control keeps every click. Reduced motion makes both
/// changes instant.
pub trait Focusable: Sized {
    /// Wraps this control while `focus` is keyboard focus. Return the wrapper
    /// from `render`.
    ///
    /// The ring's corners are the face's own: each is the radius the face is
    /// painted with, plus the ring's outset, or square where the face is
    /// square. Nothing is declared twice; a face that should have a rounded
    /// ring paints a radius.
    ///
    /// The wrapper is a row that takes its content's size. A control that
    /// grows to fill its row also grows the wrapper, with `w_flex()` on the
    /// returned box.
    fn focusable(self, focus: Focus) -> Rect;

    /// Wraps a pane whose edges sit on a clipped container edge, such as the
    /// window's. The ring is drawn just inside the face's box, with the face's
    /// own radii, so the clip cannot cut it off. Return the wrapper from
    /// `render`.
    fn focusable_inside(self, focus: Focus) -> Rect;

    /// [`focusable`](Self::focusable) for a control that can be invalid: while
    /// `invalid` the ring is drawn in the error colour, always, and not only
    /// on focus; a focused invalid control shows both (see the focus section
    /// of docs/bingux-ui.md).
    fn focusable_invalid(self, focus: Focus, invalid: bool) -> Rect;
}

impl Focusable for Rect {
    fn focusable(self, focus: Focus) -> Rect {
        wrap_focusable(self, focus, false, false)
    }

    fn focusable_inside(self, focus: Focus) -> Rect {
        wrap_focusable(self, focus, true, false)
    }

    fn focusable_invalid(self, focus: Focus, invalid: bool) -> Rect {
        wrap_focusable(self, focus, false, invalid)
    }
}

/// What a focus wrapper keeps for its ring between renders: the measured box
/// of the face, in logical pixels (the wrapper writes it after layout and the
/// ring reads it), and whether keyboard focus has ever reached the control.
///
/// Until it has, the control has no ring at all: no component, no nodes and no
/// animations. A page of forty rows is forty faces and no ring, and the ring
/// of the one that gets focus is made when it does, growing in as it always
/// has. Once made it stays, so it can fade out when focus leaves.
#[derive(Clone)]
pub(crate) struct RingFace {
    size: State<(f32, f32)>,
    reached: Rc<Cell<bool>>,
}

impl PartialEq for RingFace {
    fn eq(&self, other: &Self) -> bool {
        self.size == other.size && Rc::ptr_eq(&self.reached, &other.reached)
    }
}

impl RingFace {
    /// Notes whether focus is on the control now, and returns whether the ring
    /// has been wanted yet.
    fn reached(&self, now: bool) -> bool {
        if now {
            self.reached.set(true);
        }
        self.reached.get()
    }
}

/// The ring state of the calling component's focus wrapper: see [`RingFace`].
pub(crate) fn use_ring_size() -> RingFace {
    RingFace {
        size: use_state(|| (0.0, 0.0)),
        reached: use_hook(|| Rc::new(Cell::new(false))),
    }
}

/// Where a ring gets its face's size.
#[derive(Clone, PartialEq)]
pub(crate) enum RingSize {
    /// Measured from the wrapper after layout; see [`use_ring_size`].
    Measured(RingFace),
    /// The caller sets the face's size itself, in logical pixels. Its ring is
    /// always made.
    Known(f32, f32),
}

impl RingSize {
    fn get(&self) -> (f32, f32) {
        match self {
            Self::Measured(face) => *face.size.read(),
            Self::Known(width, height) => (*width, *height),
        }
    }
}

/// Builds the wrapper for [`Focusable`]: the press claim and the ring.
fn wrap_focusable(control: Rect, focus: Focus, inside: bool, invalid: bool) -> Rect {
    let tokens = use_tokens();
    let core = use_focus_core();
    let size = use_ring_size();
    wrap_ring(&tokens, core, control, focus, inside, invalid, RingSize::Measured(size))
}

/// [`wrap_focusable`] with its hooks already taken and the face's size given.
/// A caller whose focus wrapper is conditional takes the hooks every render and
/// calls this, so the hook count never changes.
pub(crate) fn wrap_focusable_with(
    tokens: &Tokens,
    core: FocusCore,
    control: Rect,
    focus: Focus,
    inside: bool,
    size: RingSize,
) -> Rect {
    wrap_ring(tokens, core, control, focus, inside, false, size)
}

/// Wraps `control` (the face) with the press claim and `ring`.
fn wrap_ring(
    tokens: &Tokens,
    core: FocusCore,
    mut control: Rect,
    focus: Focus,
    inside: bool,
    invalid: bool,
    size: RingSize,
) -> Rect {
    // The ring's corners are the face's, as painted.
    let radii = Radii::of(&control.get_style().corner_radius);
    let ring = FocusRing { focus, radii, inside, size, invalid };
    // The wrapper claims presses, and records the pressed control, so whatever
    // modal scope the press opens returns focus to it. It measures itself,
    // which is the face's box, for the ring.
    let face = control.get_accessibility_data().a11y_id;
    let (measured, wanted) = match &ring.size {
        RingSize::Measured(face) => (
            Some(face.size),
            face.reached(ring.focus == Focus::Keyboard || ring.invalid),
        ),
        RingSize::Known(..) => (None, true),
    };
    rect()
        .tw(tokens, &[flex_row()])
        .maybe(measured.is_some(), |wrapper| {
            let mut size = measured.expect("checked by maybe");
            wrapper.on_sized(move |event: Event<SizedEventData>| {
                size.set_if_modified((event.area.width(), event.area.height()));
            })
        })
        .on_press(move |_: Event<PressEventData>| {
            claim_press(&core);
            if let Some(id) = face {
                note_press(&core, id);
            }
        })
        .child(control)
        .maybe_child(wanted.then_some(ring))
}

/// The ring itself, drawn as the last child of a [`Focusable::focusable`]
/// wrapper, after the face.
#[derive(Clone, PartialEq)]
struct FocusRing {
    focus: Focus,
    /// The face's painted corner radii; the ring's corners follow them.
    radii: Radii,
    inside: bool,
    size: RingSize,
    /// The control is invalid: the ring shows in the error colour, always.
    invalid: bool,
}

/// What the ring looks like at animation `progress` (0 gone, 1 settled): it
/// fades in from a blurred, expanded state and fades and blurs out again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RingLook {
    pub opacity: f32,
    /// How far the ring is outside its settled place, in pixels.
    pub travel: f32,
    /// The gaussian blur of the ring's halo, in pixels.
    pub blur: f32,
    /// How far the halo spreads, in pixels.
    pub spread: f32,
}

impl RingLook {
    /// Whether the halo is drawn: while the ring is on its way, not once it is
    /// settled or has gone.
    pub fn glows(self) -> bool {
        self.blur > 0.0 && self.opacity > 0.01
    }

    pub fn at(progress: f32) -> Self {
        let gone = 1.0 - progress.clamp(0.0, 1.0);
        Self {
            opacity: progress.clamp(0.0, 1.0),
            travel: FOCUS_TRAVEL * gone,
            blur: FOCUS_BLUR * gone,
            spread: FOCUS_WIDTH * gone,
        }
    }
}

/// One concentric band of a ring: `gap` pixels off the face, `FOCUS_WIDTH`
/// thick, in `paint`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Band {
    gap: f32,
    paint: Paint,
}

impl Band {
    /// How far the band's outer edge is from the face, at `travel`.
    fn outset(self, travel: f32) -> f32 {
        self.gap + FOCUS_WIDTH + travel
    }
}

/// The bands of a ring, the main one first. This is the one place the ring's
/// look is decided: a 2 px ring `FOCUS_GAP` off the face, in the focus colour,
/// or in the error colour for an invalid control. An invalid control adds a
/// second, fainter band just outside the first, which arrives with focus, so a
/// focused invalid control reads differently from an invalid one at rest.
fn bands(invalid: bool) -> Vec<Band> {
    if invalid {
        vec![
            Band { gap: FOCUS_GAP, paint: Paint::ErrorRing },
            Band { gap: FOCUS_GAP + FOCUS_WIDTH, paint: Paint::ErrorHalo },
        ]
    } else {
        vec![Band { gap: FOCUS_GAP, paint: Paint::FocusRing }]
    }
}

impl FocusRing {
    /// A band's node. `step` is how far its outer edge is outside its parent's
    /// (the wrapper, which is the face's box, or the band inside it): the node
    /// is the parent's box grown by that on every side, so every band is
    /// concentric with the face and follows its shape. The first band is sized
    /// from the face's measured box, the others from the band inside them.
    fn band(
        &self,
        tokens: &Tokens,
        band: Band,
        step: f32,
        look: RingLook,
        face: Option<(f32, f32)>,
        main: bool,
    ) -> Rect {
        let outset = band.outset(look.travel);
        let (width, height) = match face {
            Some((width, height)) => (
                Size::px(width + step + step),
                Size::px(height + step + step),
            ),
            None => (
                Size::func(move |ctx| Some(ctx.parent + step + step)),
                Size::func(move |ctx| Some(ctx.parent + step + step)),
            ),
        };
        let node = rect()
            .interactive(false)
            .position(Position::new_absolute().top(-step).left(-step))
            .width(width)
            .height(height)
            .tw(
                tokens,
                &[
                    border(Line::Ring(band.paint)),
                    rounded_radii(self.radii, outset),
                ],
            )
            .maybe(main && look.glows(), |node| {
                node.tw(tokens, &[glow(band.paint, look.blur, look.spread, false)])
            });
        if main { node.component("FocusRing") } else { node.component("FocusBand") }
    }
}

impl Component for FocusRing {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        // A ring on the page behind a modal scope is not drawn: the page is
        // inert, whatever still holds the window's focus.
        let inert = use_focus_core().page_inert(try_consume_context::<InsideScope>().is_some());
        // A window that has lost focus keeps no ring: it blurs out with the rest
        // and comes back when the window does.
        let active = *crate::runtime::window_focus().read();
        let focused = self.focus == Focus::Keyboard && !inert && active;
        let in_ms = tokens.motion(FOCUS_IN_MS);
        let out_ms = tokens.motion(FOCUS_OUT_MS);
        // A ring is made when focus first reaches its control (see
        // [`RingFace`]), so it starts on its way in: from 0 to 1 at once. One
        // made for an invalid control starts shown. One progress value drives
        // the offset, the blur and the opacity, so the ring grows, sharpens
        // and fades in together, and softens, shrinks out and fades together
        // on the way out. An invalid control keeps its ring whether or not it
        // is focused.
        let wanted = if focused || self.invalid { 1.0 } else { 0.0 };
        let reaching = if focused { 1.0 } else { 0.0 };
        let time = |target: f32| if target > 0.5 { in_ms } else { out_ms };
        let shown = use_tween_from(if self.invalid { 1.0 } else { 0.0 }, wanted, time(wanted));
        let arrived = use_tween_from(0.0, reaching, time(reaching));
        let (width, height) = self.size.get();
        // Until the face has been measured there is nothing to ring.
        let measured = width > 0.0 && height > 0.0;
        let look = RingLook::at(shown);
        if self.inside {
            // The ring is drawn within the face's own box, so a clipped
            // container edge cannot cut it off.
            let paint = if self.invalid { Paint::ErrorRing } else { Paint::FocusRing };
            return rect()
                .component("FocusRingInside")
                .interactive(false)
                .position(Position::new_absolute().top(0.0).left(0.0))
                .width(Size::px(width))
                .height(Size::px(height))
                .opacity(if measured { look.opacity } else { 0.0 })
                .tw(
                    &tokens,
                    &[border(Line::Ring(paint)), rounded_radii(self.radii, 0.0)],
                )
                .maybe(look.glows(), |node| {
                    node.tw(&tokens, &[glow(paint, look.blur, 0.0, true)])
                });
        }
        let bands = bands(self.invalid);
        let outsets: Vec<f32> = bands.iter().map(|band| band.outset(look.travel)).collect();
        // Build from the outermost band in, so each is its parent's child.
        let mut nested: Option<Rect> = None;
        for (index, band) in bands.iter().enumerate().rev() {
            let step = outsets[index] - if index == 0 { 0.0 } else { outsets[index - 1] };
            let face = (index == 0).then_some((width, height));
            let mut node = self
                .band(&tokens, *band, step, look, face, index == 0)
                .maybe_child(nested.take());
            if index > 0 {
                // The outer band arrives with focus; the ring itself stays for
                // as long as the control is invalid.
                node = node.opacity(arrived);
            }
            nested = Some(node);
        }
        nested
            .unwrap_or_else(rect)
            .opacity(if measured { look.opacity } else { 0.0 })
    }
}

#[cfg(all(test, feature = "testing"))]
#[path = "motion_tests.rs"]
mod reveal_frames;
