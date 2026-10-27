//! Zoom for the file view: one ladder of icon sizes.
//!
//! Zooming the file view changes the size of the icons and nothing else. The
//! text, its font and its line height stay as they are at every level; what
//! grows is the icon (or the picture of an image) and, as far as it needs,
//! the row or tile around it.
//!
//! # The ladder
//!
//! The level is an icon size in logical pixels, one of [`LADDER`]:
//!
//! | Icon | View |
//! | ---- | ---- |
//! | 16 px (the default), 20, 24, 32 | a list: rows with columns. The row grows only as far as the icon needs. |
//! | 48, 64, 96, 128, 192 | a grid: a tile with the icon or a picture of the file, and its name under it in the same text size as the list's |
//!
//! The view follows the level, so zooming in walks from a list through ever
//! larger tiles, and zooming out walks back. The jump from the list to the
//! grid (32 px to 48 px) keeps the icon about the same size. There is no
//! separate list or grid switch to remember: choosing a list or a grid (the
//! toolbar button, the View as menu, Ctrl+1 and Ctrl+2) goes to the level of
//! that view nearest the current icon size ([`switch_kind`]). A folder opens
//! at 16 px in a list unless the person chose another level there; no window
//! size or folder content picks a larger one. The only limit is room: a tile
//! must fit twice across the view, so a narrow view stops at a smaller level
//! ([`cap_for_width`]) and goes back up when it widens.
//!
//! # Keys and gestures
//!
//! | Input | Effect |
//! | ----- | ------ |
//! | Ctrl + wheel notch | one level, up zooms in |
//! | Ctrl + touchpad scroll | one level per 48 px, the remainder is kept |
//! | Ctrl + `+`, `=`, numpad `+` | one level in |
//! | Ctrl + `-`, numpad `-` | one level out |
//! | Ctrl + `0`, numpad `0` | back to 16 px, in a list |
//! | Two-finger pinch | the level nearest the starting size scaled by the finger distance |
//!
//! The view keeps the content under the pointer (or the pinch midpoint) fixed
//! while the level changes, see [`Zoom::anchor`]. A small badge shows the icon
//! size and the view for about 900 ms after each change ([`zoom_hud`]).
//!
//! # What is remembered
//!
//! The level is kept per folder, as an icon size in pixels, in
//! `files/view_state.rs` (`FolderView::icon_px`). The owner of the view keeps
//! it in a `State<u16>` that it gives to [`FileView`](super::view::FileView),
//! and `FileView` saves a change.
//!
//! # How a view wires it
//!
//! 1. Call [`use_zoom`] from `render` with the level state and the largest
//!    level the width allows ([`cap_for_width`]).
//! 2. Size rows, tiles and icons from [`Zoom::px`]. The text never reads the
//!    level. Pick the thumbnail decode size with [`bucket_px`], so a picture
//!    is decoded again only when its bucket changes.
//! 3. Build the scroll view with `VirtualScrollView::pass_ctrl_wheel(true)`.
//! 4. On the element that wraps the scroll view, which must have the same
//!    top-left corner as the scroll viewport, set `.on_wheel(..)` to call
//!    [`Zoom::wheel`] and `.on_touch_start/_move/_end/_cancel(..)` to call the
//!    [`Pinch`] methods.
//! 5. In the view's `on_key_down`, call [`Zoom::key`] first. When it returns
//!    true, call `event.stop_propagation()` and return.
//! 6. After the content, add [`zoom_hud`] as the last child of an element
//!    that has the viewport's size.
//!
//! [`Zoom::wheel`] stops the event itself when Ctrl is held, so the wrapper
//! needs no check of its own. [`Pinch::active`] stays true while two fingers
//! are down and for a short time after the last one lifts, so a press
//! handler can ignore the tap that the lift would otherwise make.

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use freya::prelude::*;

use crate::motion::{MOVE_MS, PRESENCE_SCALE, use_presence};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{Ink, NUMERIC};

/// The icon sizes, in logical pixels, from the smallest to the largest. The
/// first is the default.
pub const LADDER: [u16; 9] = [16, 20, 24, 32, 48, 64, 96, 128, 192];
/// The level a folder opens at: 16 px, in a list.
pub const DEFAULT_PX: u16 = LADDER[0];
/// The largest icon that is shown in a list. Larger ones are shown in a grid.
pub const LAST_LIST_PX: u16 = 32;
/// The level of a grid that an app asked for with `ViewMode::Grid`, and a
/// folder has not been shown another way yet.
pub const REQUESTED_GRID_PX: u16 = 128;

/// The space between a tile's edge and its icon, at the sides and above.
pub const TILE_INSET: f32 = 12.0;
/// The least width of a tile, so a name has room under a small icon.
pub const MIN_TILE_WIDTH: f32 = 96.0;
/// The space between tiles, across and down.
pub const TILE_GAP: f32 = 8.0;
/// What the scrollbar takes from a grid's width.
const SCROLLBAR_WIDTH: f32 = 16.0;
/// A tile must fit this many times across.
const MIN_TILES_ACROSS: f32 = 2.0;

/// How the files show at a level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewKind {
    /// Rows with columns.
    List,
    /// Tiles.
    Grid,
}

impl ViewKind {
    /// The name the zoom badge shows.
    pub const fn name(self) -> &'static str {
        match self {
            ViewKind::List => "List",
            ViewKind::Grid => "Grid",
        }
    }
}

/// How the files show at an icon size of `px`.
pub const fn kind_of(px: u16) -> ViewKind {
    if px <= LAST_LIST_PX {
        ViewKind::List
    } else {
        ViewKind::Grid
    }
}

/// The width of a tile whose icon is `px`: the icon and its insets, and at
/// least [`MIN_TILE_WIDTH`]. Every tile of a view has this width.
pub fn tile_width(px: u16) -> f32 {
    (f32::from(px) + 2.0 * TILE_INSET).max(MIN_TILE_WIDTH)
}

// ---------------------------------------------------------------------------
// Pure helpers

/// The position of the largest level that is at most `px`; 0 when `px` is
/// below the first level.
fn index_at_most(px: u16) -> usize {
    LADDER.iter().rposition(|level| *level <= px).unwrap_or(0)
}

/// The largest level that is at most `px` and at most `max_px`.
pub fn limit_px(px: u16, max_px: u16) -> u16 {
    LADDER[index_at_most(px.min(max_px))]
}

/// The level nearest `px` on a ratio scale, so 24 is closer to 20 than to 32
/// and a pinch treats doubling and halving alike. Anything not above zero is
/// the first level.
pub fn nearest_level(px: f32) -> u16 {
    if !(px > 0.0) {
        return LADDER[0];
    }
    LADDER
        .iter()
        .copied()
        .min_by(|a, b| {
            let distance = |level: &u16| (f32::from(*level) / px).ln().abs();
            distance(a).total_cmp(&distance(b))
        })
        .unwrap_or(LADDER[0])
}

/// `current` moved by `delta` levels, held between the first level and the
/// largest at most `max_px`. A `current` that is not a level counts as the
/// level below it.
pub fn step_level(current: u16, delta: i32, max_px: u16) -> u16 {
    let top = index_at_most(max_px) as i32;
    let at = index_at_most(current.min(max_px)) as i32;
    LADDER[at.saturating_add(delta).clamp(0, top) as usize]
}

/// The level of `kind` nearest `px` in pixels, for choosing a list or a grid:
/// from 16 px a grid starts at 48 px, and from 192 px a list is 32 px.
pub fn switch_kind(px: u16, kind: ViewKind) -> u16 {
    LADDER
        .iter()
        .copied()
        .filter(|level| kind_of(*level) == kind)
        .min_by_key(|level| level.abs_diff(px))
        .unwrap_or(DEFAULT_PX)
}

/// The largest level a view `width` pixels wide shows. Every list level fits;
/// a grid level must leave room for two tiles across, with their gaps and the
/// scrollbar. A width that is not measured yet (zero or less) allows every
/// level.
pub fn cap_for_width(width: f32) -> u16 {
    let last = LADDER[LADDER.len() - 1];
    if !(width > 0.0) {
        return last;
    }
    LADDER
        .iter()
        .copied()
        .rfind(|level| {
            kind_of(*level) == ViewKind::List
                || MIN_TILES_ACROSS * tile_width(*level)
                    + (MIN_TILES_ACROSS + 1.0) * TILE_GAP
                    + SCROLLBAR_WIDTH
                    <= width
        })
        .unwrap_or(LAST_LIST_PX)
}

/// The decode sizes of a thumbnail, in physical pixels.
const BUCKETS: [u32; 9] = [64, 96, 128, 192, 256, 384, 512, 768, 1024];

/// The edge, in physical pixels, to decode a thumbnail at when it shows at
/// `px` logical pixels on a screen of `scale_factor`. It rounds up to the next
/// bucket, and the largest bucket is the limit.
pub fn bucket_px(px: f32, scale_factor: f32) -> u32 {
    let wanted = (px * scale_factor).max(0.0).ceil();
    BUCKETS
        .iter()
        .copied()
        .find(|bucket| *bucket as f32 >= wanted)
        .unwrap_or(BUCKETS[BUCKETS.len() - 1])
}

/// Levels per wheel notch.
const LEVELS_PER_NOTCH: f32 = 1.0;
/// Pixels of touchpad movement per level.
const PIXELS_PER_LEVEL: f32 = 48.0;
/// A wheel pause this long drops the carried remainder, so a stray fraction
/// does not turn into a step later.
const WHEEL_IDLE: Duration = Duration::from_millis(400);
/// Fingers closer than this at the start do not begin a pinch: the ratio of
/// two tiny distances is noise.
const MIN_PINCH_DISTANCE: f32 = 8.0;
/// How long [`Pinch::active`] stays true after the last finger lifts.
const PINCH_GRACE: Duration = Duration::from_millis(300);
/// How long the level badge stays after the last change.
const HUD_HOLD: Duration = Duration::from_millis(900);

/// The change of level for one wheel event, as `(whole levels, remainder)`.
///
/// A line wheel gives one level per notch (`wheel_step` is the pixels one
/// notch is worth in `delta_y`); a touchpad gives one per 48 px. `remainder`
/// is what the earlier events left over, so slow movement still adds up. The
/// new remainder is the part of the sum that is not a whole level. A positive
/// `delta_y` (scroll up) zooms in.
pub fn wheel_to_level_delta(
    source: WheelSource,
    delta_y: f64,
    wheel_step: f64,
    remainder: f32,
) -> (i32, f32) {
    let raw = match source {
        WheelSource::Line => (delta_y / wheel_step.max(1.0)) as f32 * LEVELS_PER_NOTCH,
        WheelSource::Pixel | WheelSource::Custom => delta_y as f32 / PIXELS_PER_LEVEL,
    };
    let total = remainder + raw;
    let whole = total.trunc();
    (whole as i32, total - whole)
}

/// The level of a pinch: the level nearest the size at the start scaled by
/// how far the fingers have moved apart (or together) since, at most
/// `max_px`.
pub fn pinch_level(start_px: u16, start_distance: f32, distance: f32, max_px: u16) -> u16 {
    if start_distance <= 0.0 {
        return limit_px(start_px, max_px);
    }
    limit_px(
        nearest_level(f32::from(start_px) * distance / start_distance),
        max_px,
    )
}

/// What a key press asks of the zoom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyAction {
    In,
    Out,
    Reset,
}

/// The zoom command of a key with Ctrl held, if it is one. Shift is allowed,
/// because `+` is Shift and `=` on most layouts.
fn key_action(key: &Key, code: Code) -> Option<KeyAction> {
    match code {
        Code::NumpadAdd => return Some(KeyAction::In),
        Code::NumpadSubtract => return Some(KeyAction::Out),
        Code::Numpad0 => return Some(KeyAction::Reset),
        _ => {}
    }
    match key {
        Key::Character(text) => match text.as_str() {
            "+" | "=" => Some(KeyAction::In),
            "-" | "_" => Some(KeyAction::Out),
            "0" => Some(KeyAction::Reset),
            _ => None,
        },
        _ => None,
    }
}

/// Pixels one wheel notch is worth in `delta_y`. It mirrors the vendored
/// `freya-winit` step (`BINGUX_WHEEL_STEP`, default 64 px).
fn wheel_step() -> f64 {
    static VALUE: LazyLock<f64> = LazyLock::new(|| {
        std::env::var("BINGUX_WHEEL_STEP")
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite())
            .map_or(64.0, |value| value.clamp(8.0, 400.0))
    });
    *VALUE
}

/// Whether Ctrl is held right now. A wheel handler uses it to tell a zoom
/// from a scroll.
pub fn ctrl_held() -> bool {
    freya_winit::modifiers::live().contains(Modifiers::CONTROL)
}

// ---------------------------------------------------------------------------
// Zoom

/// The zoom of one view. It is `Copy` and made of states, so handlers capture
/// it freely. Make it with [`use_zoom`].
#[derive(Clone, Copy, PartialEq)]
pub struct Zoom {
    /// The chosen icon size, owned by whoever mounts the view.
    level: State<u16>,
    /// The largest level the view has room for.
    max_px: u16,
    /// The part of a wheel movement that is not a whole level yet.
    remainder: State<f32>,
    last_wheel: State<Option<Instant>>,
    anchor: State<Option<(f32, f32)>>,
    /// The level the person last chose here, and `None` until they do. The
    /// owner setting the level (a folder opening at its remembered one) does
    /// not count: only a choice is worth remembering.
    chosen: State<Option<u16>>,
}

/// The zoom of a view whose level lives in `level`, held to `max_px` (see
/// [`cap_for_width`]). Call it from `render`.
pub fn use_zoom(level: State<u16>, max_px: u16) -> Zoom {
    Zoom {
        level,
        max_px,
        remainder: use_state(|| 0.0_f32),
        last_wheel: use_state::<Option<Instant>>(|| None),
        anchor: use_state::<Option<(f32, f32)>>(|| None),
        chosen: use_state::<Option<u16>>(|| None),
    }
}

impl Zoom {
    /// The icon size shown, in logical pixels: the chosen level, or the
    /// largest one the width allows. Reading it in `render` subscribes the
    /// view to changes.
    pub fn px(&self) -> u16 {
        limit_px(*self.level.read(), self.max_px)
    }

    /// [`Self::px`] without subscribing the caller, for event handlers.
    pub fn px_now(&self) -> u16 {
        limit_px(*self.level.peek(), self.max_px)
    }

    /// Whether the files show as a list or as a grid at the current level.
    pub fn kind(&self) -> ViewKind {
        kind_of(self.px_now())
    }

    /// The level the person chose with this zoom, if any: the keys, the
    /// wheel, a pinch, or choosing a list or a grid. Reading it in `render`
    /// subscribes the view.
    pub fn chosen(&self) -> Option<u16> {
        *self.chosen.read()
    }

    /// Goes to the level nearest `px`, held to the largest the width allows.
    pub fn set_px(&self, px: u16) {
        self.choose(limit_px(nearest_level(f32::from(px)), self.max_px));
    }

    /// Moves by `delta` levels, held between the first and the largest.
    pub fn step(&self, delta: i32) {
        // Read first: a borrow held across the write would fail it.
        let next = step_level(*self.level.peek(), delta, self.max_px);
        self.choose(next);
    }

    fn choose(&self, px: u16) {
        let mut level = self.level;
        if *level.peek() != px {
            level.set(px);
            let mut chosen = self.chosen;
            chosen.set(Some(px));
        }
    }

    /// Back to 16 px, in a list.
    pub fn reset(&self) {
        self.set_px(DEFAULT_PX);
    }

    /// Shows a list or a grid: goes to the level of that kind nearest the
    /// current size.
    pub fn show(&self, kind: ViewKind) {
        self.set_px(switch_kind(self.px_now(), kind));
    }

    /// The point the view keeps fixed while the level changes, in the
    /// coordinates of the element that handles the wheel and the touches: the
    /// pointer of the last wheel event or the midpoint of the last pinch.
    /// `None` after a key press, when the view anchors on the middle of its
    /// viewport.
    ///
    /// When the size of a row or a tile changes, the view sets its scroll
    /// offset with `interaction::anchored_offset` from this point's distance
    /// from the viewport top and the pitch and columns of the previous and
    /// the new frame (`file_list::anchor_zoom`).
    pub fn anchor(&self) -> Option<(f32, f32)> {
        *self.anchor.peek()
    }

    /// Drops the anchor, so the next zoom anchors on the viewport middle.
    pub fn clear_anchor(&self) {
        let mut anchor = self.anchor;
        anchor.set_if_modified(None);
    }

    fn set_anchor(&self, point: (f32, f32)) {
        let mut anchor = self.anchor;
        anchor.set_if_modified(Some(point));
    }

    /// Zooms by a wheel event while Ctrl is held. Without Ctrl it does
    /// nothing and leaves the event alone. With Ctrl it stops the event, so
    /// nothing above scrolls, and returns whether the level moved.
    pub fn wheel(&self, event: &Event<WheelEventData>) -> bool {
        if !ctrl_held() {
            return false;
        }
        event.stop_propagation();
        self.set_anchor((
            event.element_location.x as f32,
            event.element_location.y as f32,
        ));

        let now = Instant::now();
        let mut last_wheel = self.last_wheel;
        let stale = last_wheel
            .peek()
            .is_none_or(|last| now.duration_since(last) > WHEEL_IDLE);
        last_wheel.set(Some(now));
        let carried = if stale { 0.0 } else { *self.remainder.peek() };

        let (whole, remainder) =
            wheel_to_level_delta(event.source, event.delta_y, wheel_step(), carried);
        let before = self.px_now();
        self.step(whole);
        let moved = self.px_now() != before;
        let mut stored = self.remainder;
        // Pushing against a limit carries nothing over, so the way back starts
        // at once.
        stored.set(if moved { remainder } else { 0.0 });
        moved
    }

    /// Handles Ctrl + plus, minus and zero. It returns true when the key was
    /// a zoom command: the caller then stops the event.
    pub fn key(&self, event: &Event<KeyboardEventData>) -> bool {
        if !event.modifiers.contains(Modifiers::CONTROL) || event.modifiers.contains(Modifiers::ALT)
        {
            return false;
        }
        let Some(action) = key_action(&event.key, event.code) else {
            return false;
        };
        self.clear_anchor();
        match action {
            KeyAction::In => self.step(1),
            KeyAction::Out => self.step(-1),
            KeyAction::Reset => self.reset(),
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Pinch

#[derive(Clone, Copy, Debug, PartialEq)]
struct Finger {
    id: u64,
    at: (f32, f32),
}

/// Where a pinch began.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Grip {
    distance: f32,
    px: u16,
}

/// A two-finger pinch that drives a [`Zoom`]. Make it with [`use_pinch`] and
/// feed it the touch events of the element that wraps the view.
#[derive(Clone, Copy, PartialEq)]
pub struct Pinch {
    zoom: Zoom,
    fingers: State<[Option<Finger>; 2]>,
    grip: State<Option<Grip>>,
    /// Two fingers have been down since the last time none were.
    engaged: State<bool>,
    released: State<Option<Instant>>,
}

/// A pinch for `zoom`. Call it from `render`.
pub fn use_pinch(zoom: Zoom) -> Pinch {
    Pinch {
        zoom,
        fingers: use_state(|| [None, None]),
        grip: use_state::<Option<Grip>>(|| None),
        engaged: use_state(|| false),
        released: use_state::<Option<Instant>>(|| None),
    }
}

fn finger_of(event: &TouchEventData) -> Finger {
    Finger {
        id: event.finger_id,
        at: (
            event.element_location.x as f32,
            event.element_location.y as f32,
        ),
    }
}

fn distance(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn midpoint(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)
}

impl Pinch {
    /// A finger went down. The second finger begins the pinch.
    pub fn start(&self, event: &TouchEventData) {
        let finger = finger_of(event);
        let mut fingers = self.fingers;
        {
            let mut slots = fingers.write();
            if let Some(slot) = slots.iter_mut().flatten().find(|slot| slot.id == finger.id) {
                *slot = finger;
            } else if let Some(empty) = slots.iter_mut().find(|slot| slot.is_none()) {
                *empty = Some(finger);
            }
        }
        if let [Some(a), Some(b)] = *fingers.peek() {
            let mut engaged = self.engaged;
            engaged.set_if_modified(true);
            self.grip_if_needed(a.at, b.at);
        }
    }

    /// A finger moved. While two fingers are down, it returns the icon size
    /// the pinch set.
    pub fn moved(&self, event: &TouchEventData) -> Option<u16> {
        let finger = finger_of(event);
        let mut fingers = self.fingers;
        {
            let mut slots = fingers.write();
            let slot = slots.iter_mut().flatten().find(|slot| slot.id == finger.id)?;
            slot.at = finger.at;
        }
        let [Some(a), Some(b)] = *fingers.peek() else {
            return None;
        };
        self.grip_if_needed(a.at, b.at);
        let grip = (*self.grip.peek())?;
        let px = pinch_level(
            grip.px,
            grip.distance,
            distance(a.at, b.at),
            self.zoom.max_px,
        );
        self.zoom.set_anchor(midpoint(a.at, b.at));
        self.zoom.set_px(px);
        Some(px)
    }

    /// A finger lifted or the system cancelled the touch. Use it for both
    /// `touch_end` and `touch_cancel`.
    pub fn end(&self, event: &TouchEventData) {
        let mut fingers = self.fingers;
        let mut grip = self.grip;
        let left = {
            let mut slots = fingers.write();
            for slot in slots.iter_mut() {
                if slot.is_some_and(|slot| slot.id == event.finger_id) {
                    *slot = None;
                }
            }
            slots.iter().flatten().count()
        };
        if left < 2 {
            grip.set_if_modified(None);
        }
        if left == 0 {
            let mut engaged = self.engaged;
            if *engaged.peek() {
                let mut released = self.released;
                released.set(Some(Instant::now()));
                engaged.set(false);
            }
        }
    }

    /// Whether a pinch owns the touch: true while it is engaged and for a
    /// moment after the last finger lifts. A view ignores presses and long
    /// presses while it is true.
    pub fn active(&self) -> bool {
        *self.engaged.peek()
            || self
                .released
                .peek()
                .is_some_and(|at| at.elapsed() < PINCH_GRACE)
    }

    /// Fixes the starting distance and size, once two fingers are far enough
    /// apart for the ratio to mean something.
    fn grip_if_needed(&self, a: (f32, f32), b: (f32, f32)) {
        let mut grip = self.grip;
        if grip.peek().is_some() {
            return;
        }
        let apart = distance(a, b);
        if apart >= MIN_PINCH_DISTANCE {
            grip.set(Some(Grip {
                distance: apart,
                px: self.zoom.px_now(),
            }));
            self.zoom.set_anchor(midpoint(a, b));
        }
    }
}

// ---------------------------------------------------------------------------
// Level badge

/// The badge that shows the icon size and the view (for example `48 px ·
/// Grid`) for about 900 ms after each change: a small pill at the bottom
/// centre of its parent. It ignores pointer input and renders an empty,
/// zero-size element while hidden.
pub fn zoom_hud(zoom: Zoom) -> impl IntoElement {
    ZoomHud { zoom }
}

#[derive(Clone, PartialEq)]
struct ZoomHud {
    zoom: Zoom,
}

/// The text of the badge at `px`.
fn hud_text(px: u16) -> String {
    format!("{px} px · {}", kind_of(px).name())
}

impl Component for ZoomHud {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let px = self.zoom.px();
        let mut visible = use_state(|| false);
        let mut shown = use_state(move || px);
        let mut generation = use_state(|| 0_u64);
        use_side_effect_with_deps(&px, move |&px: &u16| {
            // The level a view starts with is not a change.
            if px == *shown.peek() {
                return;
            }
            shown.set(px);
            visible.set(true);
            let current = generation.peek().wrapping_add(1);
            generation.set(current);
            spawn(async move {
                timer(HUD_HOLD).await;
                if *generation.peek() == current {
                    visible.set(false);
                }
            });
        });

        let open = visible();
        let progress = use_presence(open, tokens.motion(MOVE_MS), || {});
        let anchor = Position::new_absolute()
            .left(0.0)
            .right(0.0)
            .bottom(S4.px());
        if !open && progress <= 0.0 {
            return rect().position(anchor).interactive(false).into_element();
        }
        rect()
            .position(anchor)
            .interactive(false)
            .tw(&tokens, &[flex_col(), items(Align::Center), w_full()])
            .child(
                rect()
                    .opacity(progress)
                    .scale(PRESENCE_SCALE + (1.0 - PRESENCE_SCALE) * progress)
                    .tw(&tokens, crate::style::TOOLTIP)
                    .tw(
                        &tokens,
                        &[border(Line::Stroke(Dim::Hairline, Paint::FloatingOutline))],
                    )
                    .child(
                        NUMERIC
                            .in_ink(Ink::OnAccent)
                            .label(&tokens, hud_text(*shown.read()))
                            .max_lines(1),
                    ),
            )
            .into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ladder_climbs_from_the_default() {
        assert_eq!(DEFAULT_PX, 16);
        assert_eq!(LADDER[0], DEFAULT_PX);
        assert!(LADDER.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(LADDER, [16, 20, 24, 32, 48, 64, 96, 128, 192]);
    }

    #[test]
    fn low_levels_are_a_list_and_high_levels_a_grid() {
        for px in [16, 20, 24, 32] {
            assert_eq!(kind_of(px), ViewKind::List, "{px}");
        }
        for px in [48, 64, 96, 128, 192] {
            assert_eq!(kind_of(px), ViewKind::Grid, "{px}");
        }
        // Once the grid is reached it stays: the kind never goes back to a
        // list on the way up.
        let kinds: Vec<ViewKind> = LADDER.iter().map(|px| kind_of(*px)).collect();
        assert!(kinds.windows(2).all(|pair| pair[0] == ViewKind::List || pair[1] == ViewKind::Grid));
    }

    #[test]
    fn the_jump_from_list_to_grid_keeps_the_icon_about_as_big() {
        let last_list = LADDER.iter().copied().filter(|px| kind_of(*px) == ViewKind::List).max();
        let first_grid = LADDER.iter().copied().find(|px| kind_of(*px) == ViewKind::Grid);
        assert_eq!((last_list, first_grid), (Some(32), Some(48)));
    }

    #[test]
    fn stepping_walks_the_ladder_and_stops_at_the_ends() {
        let top = LADDER[LADDER.len() - 1];
        let mut px = DEFAULT_PX;
        let mut seen = vec![px];
        for _ in 0..LADDER.len() + 3 {
            px = step_level(px, 1, top);
            seen.push(px);
        }
        assert_eq!(&seen[..9], &LADDER);
        assert!(seen[8..].iter().all(|px| *px == top));
        for _ in 0..LADDER.len() + 3 {
            px = step_level(px, -1, top);
        }
        assert_eq!(px, DEFAULT_PX);
        assert_eq!(step_level(16, -1, top), 16);
        assert_eq!(step_level(192, 1, top), 192);
        assert_eq!(step_level(16, 100, top), 192);
        assert_eq!(step_level(16, i32::MAX, top), 192);
        assert_eq!(step_level(192, i32::MIN, top), 16);
    }

    #[test]
    fn stepping_respects_the_cap() {
        assert_eq!(step_level(64, 1, 96), 96);
        assert_eq!(step_level(96, 1, 96), 96);
        assert_eq!(step_level(192, -1, 96), 64, "a level above the cap steps from the cap");
        assert_eq!(step_level(16, 5, 32), 32);
    }

    #[test]
    fn a_value_between_levels_counts_as_the_level_below() {
        assert_eq!(step_level(40, 1, 192), 48);
        assert_eq!(step_level(40, -1, 192), 24);
        assert_eq!(limit_px(40, 192), 32);
        assert_eq!(limit_px(1, 192), 16);
        assert_eq!(limit_px(500, 96), 96);
    }

    #[test]
    fn the_nearest_level_is_by_ratio() {
        assert_eq!(nearest_level(0.0), 16);
        assert_eq!(nearest_level(-5.0), 16);
        assert_eq!(nearest_level(f32::NAN), 16);
        assert_eq!(nearest_level(16.0), 16);
        assert_eq!(nearest_level(21.0), 20);
        assert_eq!(nearest_level(22.0), 24);
        assert_eq!(nearest_level(28.0), 32);
        assert_eq!(nearest_level(41.0), 48);
        assert_eq!(nearest_level(1000.0), 192);
    }

    #[test]
    fn choosing_a_list_or_a_grid_goes_to_the_nearest_level_of_it() {
        assert_eq!(switch_kind(16, ViewKind::Grid), 48);
        assert_eq!(switch_kind(32, ViewKind::Grid), 48);
        assert_eq!(switch_kind(192, ViewKind::List), 32);
        assert_eq!(switch_kind(64, ViewKind::List), 32);
        assert_eq!(switch_kind(20, ViewKind::List), 20);
        assert_eq!(switch_kind(96, ViewKind::Grid), 96);
        assert_eq!(switch_kind(24, ViewKind::Grid), 48);
    }

    #[test]
    fn a_narrow_view_caps_the_level_so_two_tiles_fit() {
        // A list always fits.
        assert_eq!(cap_for_width(1.0), 32);
        assert_eq!(cap_for_width(100.0), 32);
        // Not measured yet: no cap.
        assert_eq!(cap_for_width(0.0), 192);
        assert_eq!(cap_for_width(-1.0), 192);
        // 48 and 64 px share the narrowest tile, 96 px: two need
        // 2 * 96 + 3 * 8 + 16 = 232.
        assert_eq!(cap_for_width(231.0), 32);
        assert_eq!(cap_for_width(232.0), 64);
        // 96 px: 2 * 120 + 40 = 280.
        assert_eq!(cap_for_width(279.0), 64);
        assert_eq!(cap_for_width(280.0), 96);
        // A phone: 360 px holds two 128 px tiles (2 * 152 + 40 = 344).
        assert_eq!(cap_for_width(360.0), 128);
        // 192 px needs 2 * 216 + 40 = 472.
        assert_eq!(cap_for_width(471.0), 128);
        assert_eq!(cap_for_width(472.0), 192);
        assert_eq!(cap_for_width(2000.0), 192);
        // The cap never skips a level that fits: it grows with the width.
        let mut last = 0;
        for width in 0..1200 {
            let cap = cap_for_width(width as f32);
            assert!(cap >= last || width == 0 || last == 192);
            if width > 0 {
                assert!(cap >= 32);
                assert!(cap < 48 || 2.0 * tile_width(cap) + 3.0 * TILE_GAP + SCROLLBAR_WIDTH <= width as f32);
            }
            last = cap;
        }
    }

    #[test]
    fn a_tile_is_as_wide_as_its_icon_needs_and_never_narrower_than_a_name() {
        assert_eq!(tile_width(48), MIN_TILE_WIDTH);
        assert_eq!(tile_width(64), MIN_TILE_WIDTH, "88 would be narrower than a name needs");
        assert_eq!(tile_width(96), 120.0);
        assert_eq!(tile_width(192), 216.0);
        let widths: Vec<f32> = LADDER.iter().map(|px| tile_width(*px)).collect();
        assert!(widths.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn line_wheel_is_one_level_a_notch() {
        assert_eq!(wheel_to_level_delta(WheelSource::Line, 64.0, 64.0, 0.0), (1, 0.0));
        assert_eq!(wheel_to_level_delta(WheelSource::Line, -128.0, 64.0, 0.0), (-2, 0.0));
        // A high-resolution wheel sends a fraction of a notch.
        let (whole, rest) = wheel_to_level_delta(WheelSource::Line, 16.0, 64.0, 0.0);
        assert_eq!(whole, 0);
        assert!((rest - 0.25).abs() < 1e-6);
    }

    #[test]
    fn touchpad_is_one_level_per_forty_eight_pixels() {
        assert_eq!(wheel_to_level_delta(WheelSource::Pixel, 48.0, 64.0, 0.0), (1, 0.0));
        assert_eq!(wheel_to_level_delta(WheelSource::Pixel, -96.0, 64.0, 0.0), (-2, 0.0));
        assert_eq!(wheel_to_level_delta(WheelSource::Pixel, 0.0, 64.0, 0.0), (0, 0.0));
    }

    #[test]
    fn slow_touchpad_movement_adds_up() {
        let mut remainder = 0.0;
        let mut total = 0;
        // 12 px at a time is a quarter of a level: four events make one.
        for _ in 0..8 {
            let (whole, rest) = wheel_to_level_delta(WheelSource::Pixel, 12.0, 64.0, remainder);
            total += whole;
            remainder = rest;
        }
        assert_eq!(total, 2);
        assert!(remainder.abs() < 1e-6);
    }

    #[test]
    fn reversing_cancels_the_remainder() {
        let (whole, rest) = wheel_to_level_delta(WheelSource::Pixel, 36.0, 64.0, 0.0);
        assert_eq!(whole, 0);
        let (whole, rest) = wheel_to_level_delta(WheelSource::Pixel, -24.0, 64.0, rest);
        assert_eq!(whole, 0);
        assert!((rest - 0.25).abs() < 1e-6);
    }

    #[test]
    fn pinch_scales_the_starting_size_and_lands_on_a_level() {
        let top = 192;
        assert_eq!(pinch_level(16, 100.0, 100.0, top), 16);
        assert_eq!(pinch_level(16, 100.0, 300.0, top), 48);
        assert_eq!(pinch_level(64, 100.0, 50.0, top), 32);
        assert_eq!(pinch_level(64, 100.0, 1000.0, top), 192);
        assert_eq!(pinch_level(64, 100.0, 1.0, top), 16);
        assert_eq!(pinch_level(64, 0.0, 50.0, top), 64);
        // The cap holds.
        assert_eq!(pinch_level(64, 100.0, 1000.0, 96), 96);
    }

    #[test]
    fn buckets_round_up_at_the_boundaries() {
        assert_eq!(bucket_px(0.0, 1.0), 64);
        assert_eq!(bucket_px(63.0, 1.0), 64);
        assert_eq!(bucket_px(64.0, 1.0), 64);
        assert_eq!(bucket_px(64.1, 1.0), 96);
        assert_eq!(bucket_px(96.0, 1.0), 96);
        assert_eq!(bucket_px(97.0, 1.0), 128);
        assert_eq!(bucket_px(512.0, 1.0), 512);
        assert_eq!(bucket_px(513.0, 1.0), 768);
        assert_eq!(bucket_px(1024.0, 1.0), 1024);
        assert_eq!(bucket_px(5000.0, 1.0), 1024);
        // The screen's scale factor counts: 40 logical px is 80 physical.
        assert_eq!(bucket_px(40.0, 2.0), 96);
        assert_eq!(bucket_px(32.0, 2.0), 64);
    }

    #[test]
    fn every_level_decodes_a_picture_at_least_as_big_as_it_shows() {
        let mut last = 0;
        for px in LADDER {
            for scale in [1.0_f32, 1.5, 2.0] {
                let bucket = bucket_px(f32::from(px), scale);
                assert!(bucket as f32 >= f32::from(px) * scale || bucket == 1024);
            }
            let bucket = bucket_px(f32::from(px), 1.0);
            assert!(bucket >= last);
            last = bucket;
        }
    }

    #[test]
    fn zoom_keys() {
        let key = |text: &str| Key::Character(text.into());
        assert_eq!(key_action(&key("+"), Code::Equal), Some(KeyAction::In));
        assert_eq!(key_action(&key("="), Code::Equal), Some(KeyAction::In));
        assert_eq!(key_action(&key("-"), Code::Minus), Some(KeyAction::Out));
        assert_eq!(key_action(&key("0"), Code::Digit0), Some(KeyAction::Reset));
        assert_eq!(key_action(&key("a"), Code::KeyA), None);
        assert_eq!(key_action(&Key::Named(NamedKey::Enter), Code::NumpadAdd), Some(KeyAction::In));
        assert_eq!(key_action(&key("-"), Code::NumpadSubtract), Some(KeyAction::Out));
        assert_eq!(key_action(&key("0"), Code::Numpad0), Some(KeyAction::Reset));
    }

    #[test]
    fn the_badge_names_the_size_and_the_view() {
        assert_eq!(hud_text(16), "16 px · List");
        assert_eq!(hud_text(32), "32 px · List");
        assert_eq!(hud_text(48), "48 px · Grid");
        assert_eq!(hud_text(192), "192 px · Grid");
    }
}
