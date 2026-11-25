//! What the view tells the producer: size, pointer, wheel, keys, text, touch.
//!
//! Every position is in the view's **logical pixels** with the origin at the
//! view's top left corner; multiply by [`ViewSize::scale`] for device pixels.
//! Keys use the DOM's vocabulary: `key` and `code` print as the strings
//! `KeyboardEvent.key` and `KeyboardEvent.code` hold (`"Enter"`, `"a"`,
//! `"KeyA"`, `"ArrowLeft"`), and a wheel's `delta_y` is positive when the
//! content scrolls down.

use std::time::{Duration, Instant};

use freya::prelude::{
    Code, CursorIcon, Key, Modifiers, MouseButton, NamedKey, TouchPhase, WheelPhase, WheelSource,
};

/// The view's size in logical pixels and the scale factor of its window. The
/// device size the producer should render at is [`ViewSize::device_size`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewSize {
    pub width: f32,
    pub height: f32,
    /// Device pixels per logical pixel: 1.0, 1.25, 2.0, ...
    pub scale: f64,
}

impl ViewSize {
    /// The size in device pixels, rounded to whole pixels as the view rounds
    /// its own edges. A frame of exactly this size is drawn with no scaling,
    /// so no blur.
    pub fn device_size(&self) -> (u32, u32) {
        (
            super::geometry::device_extent(self.width, self.scale) as u32,
            super::geometry::device_extent(self.height, self.scale) as u32,
        )
    }

    /// A position inside the view in device pixels.
    pub fn to_device(&self, position: (f32, f32)) -> (f32, f32) {
        (
            (f64::from(position.0) * self.scale) as f32,
            (f64::from(position.1) * self.scale) as f32,
        )
    }

    /// Whether `other` is the same size to within a hundredth of a logical
    /// pixel, which is how far layout rounding wanders.
    pub(crate) fn same_as(&self, other: &Self) -> bool {
        (self.width - other.width).abs() < 0.01
            && (self.height - other.height).abs() < 0.01
            && (self.scale - other.scale).abs() < 1e-6
    }
}

/// How a frame that is not the size of the view is placed in it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrameFit {
    /// At its own size ([`PixelFrame::logical_size`]) in the top left corner,
    /// clipped to the view, with the backdrop around any part it leaves bare.
    /// While a producer catches up with a resize this keeps what is on screen
    /// sharp and undistorted: a view that grows shows the old frame 1:1 with
    /// the new area filled by the backdrop, and one that shrinks clips it. A
    /// frame is scaled only when its [`scale`](super::PixelFrame::scale) is not the
    /// view's. The default.
    ///
    /// [`PixelFrame::logical_size`]: super::PixelFrame::logical_size
    #[default]
    Anchored,
    /// Stretched to the whole view. The producer renders at the size
    /// [`ViewSize`] reports, so this is a 1:1 copy in the steady state and a
    /// stretch while the producer catches up with a resize: for a producer
    /// that has no size of its own and is always drawn to fit.
    Stretch,
    /// Scaled to the largest size that fits with its aspect ratio kept, centred
    /// on the backdrop: a video player's picture.
    Contain,
}

/// What Tab does inside the view. See [`FrameView::focus_escape`].
///
/// [`FrameView::focus_escape`]: super::FrameView::focus_escape
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FocusEscape {
    /// Tab and Shift+Tab leave the view to the next and previous stop, like
    /// any control. The producer never sees them. The default: a view is never
    /// a keyboard trap.
    #[default]
    Tab,
    /// Tab and Shift+Tab go to the producer (a page, an editor and a terminal
    /// use them). F6 and Shift+F6 are never forwarded and still move focus
    /// between the window's regions, so the keyboard can always leave; when
    /// the producer says its own focus order ran out, the app calls
    /// [`step_focus`](crate::focus::step_focus).
    Producer,
}

/// A rectangle in the view's logical pixels where the producer's text caret
/// is, so the input method's popup opens beside it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// The pointer shape the producer asks for over the view. `#[non_exhaustive]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum CursorKind {
    #[default]
    Default,
    Pointer,
    Text,
    VerticalText,
    Crosshair,
    Move,
    Grab,
    Grabbing,
    Wait,
    Progress,
    Help,
    ContextMenu,
    Cell,
    Copy,
    Alias,
    NoDrop,
    NotAllowed,
    AllScroll,
    ZoomIn,
    ZoomOut,
    ColResize,
    RowResize,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
    /// No pointer at all while it is over the view.
    Hidden,
}

impl CursorKind {
    /// The cursor the window shows, or `None` for [`CursorKind::Hidden`].
    pub fn icon(self) -> Option<CursorIcon> {
        Some(match self {
            Self::Default => CursorIcon::Default,
            Self::Pointer => CursorIcon::Pointer,
            Self::Text => CursorIcon::Text,
            Self::VerticalText => CursorIcon::VerticalText,
            Self::Crosshair => CursorIcon::Crosshair,
            Self::Move => CursorIcon::Move,
            Self::Grab => CursorIcon::Grab,
            Self::Grabbing => CursorIcon::Grabbing,
            Self::Wait => CursorIcon::Wait,
            Self::Progress => CursorIcon::Progress,
            Self::Help => CursorIcon::Help,
            Self::ContextMenu => CursorIcon::ContextMenu,
            Self::Cell => CursorIcon::Cell,
            Self::Copy => CursorIcon::Copy,
            Self::Alias => CursorIcon::Alias,
            Self::NoDrop => CursorIcon::NoDrop,
            Self::NotAllowed => CursorIcon::NotAllowed,
            Self::AllScroll => CursorIcon::AllScroll,
            Self::ZoomIn => CursorIcon::ZoomIn,
            Self::ZoomOut => CursorIcon::ZoomOut,
            Self::ColResize => CursorIcon::ColResize,
            Self::RowResize => CursorIcon::RowResize,
            Self::EwResize => CursorIcon::EwResize,
            Self::NsResize => CursorIcon::NsResize,
            Self::NeswResize => CursorIcon::NeswResize,
            Self::NwseResize => CursorIcon::NwseResize,
            Self::Hidden => return None,
        })
    }
}

/// What the pointer did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerAction {
    Move,
    /// A button went down. [`PointerInput::button`] says which.
    Down,
    /// A button went up, wherever the pointer is: a press that began in the
    /// view always ends with an `Up`, even outside it.
    Up,
    Enter,
    Leave,
}

/// The mouse buttons held, as the DOM's `MouseEvent.buttons`: one bit each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PointerButtons(pub u8);

impl PointerButtons {
    pub const LEFT: u8 = 1;
    pub const RIGHT: u8 = 2;
    pub const MIDDLE: u8 = 4;
    pub const BACK: u8 = 8;
    pub const FORWARD: u8 = 16;

    pub fn contains(self, button: MouseButton) -> bool {
        self.0 & Self::bit(button) != 0
    }

    pub(crate) fn bit(button: MouseButton) -> u8 {
        match button {
            MouseButton::Left => Self::LEFT,
            MouseButton::Right => Self::RIGHT,
            MouseButton::Middle => Self::MIDDLE,
            MouseButton::Back => Self::BACK,
            MouseButton::Forward => Self::FORWARD,
            MouseButton::Other(_) => 0,
        }
    }

    pub(crate) fn with(self, button: MouseButton) -> Self {
        Self(self.0 | Self::bit(button))
    }

    pub(crate) fn without(self, button: MouseButton) -> Self {
        Self(self.0 & !Self::bit(button))
    }
}

/// A pointer event for the producer.
#[derive(Clone, Debug, PartialEq)]
pub struct PointerInput {
    pub action: PointerAction,
    /// In the view's logical pixels; outside the view while a button is held
    /// and the pointer has left it.
    pub position: (f32, f32),
    /// The button of a `Down` or `Up`; `None` for the other actions.
    pub button: Option<MouseButton>,
    /// The buttons held after this event.
    pub buttons: PointerButtons,
    pub modifiers: Modifiers,
    /// 1 for a single press, 2 for the second press of a double click, and so
    /// on, for a `Down` and its `Up`; 0 for the other actions.
    pub click_count: u32,
}

/// How far a wheel turned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WheelDelta {
    /// Logical pixels, from a touchpad.
    Pixels { x: f32, y: f32 },
    /// Notches, from a wheel: one is one line or a step the producer picks.
    Lines { x: f32, y: f32 },
}

/// A wheel event for the producer. Positive `y` scrolls the content down and
/// positive `x` to the right, as the DOM's `deltaY` and `deltaX`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelInput {
    pub delta: WheelDelta,
    pub position: (f32, f32),
    pub modifiers: Modifiers,
    /// Where a touchpad gesture is: `Began` with the first delta, `Changed`,
    /// then `Ended` (or `Cancelled`) with a zero delta when the fingers lift.
    /// `None` for a wheel's notch.
    pub phase: WheelPhase,
}

/// A key event for the producer.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyInput {
    /// `true` for a key going down, `false` for it going up.
    pub pressed: bool,
    /// The meaning of the key with the layout and Shift applied; prints as the
    /// DOM's `key` ("a", "A", "Enter", "ArrowLeft", "Dead").
    pub key: Key,
    /// The physical key, as the DOM's `code` ("KeyA", "Enter", "ArrowLeft").
    pub code: Code,
    pub modifiers: Modifiers,
    /// The key went down again while held, as the system's key repeat does.
    pub repeat: bool,
    /// The characters the key types, for a key down that types text: `"a"`,
    /// not "Enter" or "ArrowLeft" and nothing while Ctrl, Alt or Meta is held.
    pub text: Option<String>,
}

impl KeyInput {
    /// The DOM `key` string.
    pub fn key_string(&self) -> String {
        self.key.to_string()
    }

    /// The DOM `code` string.
    pub fn code_string(&self) -> String {
        self.code.to_string()
    }
}

/// Text from an input method, or typed with no physical key (an on-screen
/// keyboard).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextInput {
    /// The text being composed, to show underlined at the caret. `cursor` is
    /// the byte range inside `text` the input method marks, if any. An empty
    /// `text` ends the composition.
    Preedit {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    /// Finished text to insert.
    Commit(String),
}

/// A touch event for the producer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchInput {
    pub phase: TouchPhase,
    /// Which finger; stable from `Started` to `Ended` or `Cancelled`.
    pub id: u64,
    pub position: (f32, f32),
}

/// The position of a pointer event relative to the view, in logical pixels.
/// Freya hands every event over in logical pixels, so this only moves the
/// origin: `origin` is the view's top left in the window.
pub(crate) fn relative(global: (f64, f64), origin: (f32, f32)) -> (f32, f32) {
    (
        (global.0 - f64::from(origin.0)) as f32,
        (global.1 - f64::from(origin.1)) as f32,
    )
}

/// Two presses are one multiple click when they are this close in time.
pub(crate) const MULTI_CLICK: Duration = Duration::from_millis(400);
/// And this close together, in logical pixels.
pub(crate) const MULTI_CLICK_SLOP: f32 = 4.0;

/// Counts consecutive presses of one button.
#[derive(Clone, Debug, Default)]
pub(crate) struct ClickCounter {
    last: Option<(Instant, MouseButton, (f32, f32), u32)>,
}

impl ClickCounter {
    /// Registers a press and returns its click count.
    pub(crate) fn press(&mut self, now: Instant, button: MouseButton, at: (f32, f32)) -> u32 {
        let count = match self.last {
            Some((when, previous, from, count))
                if previous == button
                    && now.saturating_duration_since(when) <= MULTI_CLICK
                    && (at.0 - from.0).abs() <= MULTI_CLICK_SLOP
                    && (at.1 - from.1).abs() <= MULTI_CLICK_SLOP =>
            {
                count + 1
            }
            _ => 1,
        };
        self.last = Some((now, button, at, count));
        count
    }

    /// The count of the press in progress, for its release.
    pub(crate) fn current(&self) -> u32 {
        self.last.map_or(0, |last| last.3)
    }
}

/// The text a key down types: a character key with no Ctrl, Alt or Meta held.
pub(crate) fn typed_text(key: &Key, modifiers: Modifiers) -> Option<String> {
    if modifiers.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META) {
        return None;
    }
    match key {
        Key::Character(text) if !text.is_empty() => Some(text.clone()),
        _ => None,
    }
}

/// How the window scaled a wheel event's delta on its way to the view, so the
/// view can undo it and hand the producer what the device sent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WheelScaling {
    /// Pixels the window counts for one notch (`BINGUX_WHEEL_STEP`).
    pub step: f64,
    /// The factor the window multiplies a touchpad's pixels by
    /// (`BINGUX_TOUCHPAD_FACTOR`).
    pub touchpad: f64,
    /// Device pixels per logical pixel. A touchpad's pixels are device pixels.
    pub scale: f64,
}

/// A wheel event's delta in the DOM's convention, as the device sent it.
///
/// Freya reports a notch as `step` pixels already and a touchpad's movement in
/// device pixels times a factor, with positive `y` for the content moving
/// down. All of that is undone here: a notch is a count of notches (the
/// producer decides what one is worth, as every browser does) and a touchpad
/// delta is in the view's logical pixels. Nothing is accelerated or smoothed
/// on the way: the engine owns scrolling.
pub(crate) fn wheel_delta(source: WheelSource, x: f64, y: f64, scaling: WheelScaling) -> WheelDelta {
    match source {
        WheelSource::Line => WheelDelta::Lines {
            x: (-x / scaling.step) as f32,
            y: (-y / scaling.step) as f32,
        },
        WheelSource::Pixel | WheelSource::Custom => {
            let per_logical = scaling.touchpad * scaling.scale;
            WheelDelta::Pixels {
                x: (-x / per_logical) as f32,
                y: (-y / per_logical) as f32,
            }
        }
    }
}

/// What a key event means to the view.
pub(crate) enum KeyRoute {
    /// Not the producer's: it goes on to the rest of the window.
    Leave,
    Key(KeyInput),
    Text(TextInput),
}

/// Tells repeats from first presses, and keys from typed text.
#[derive(Debug, Default)]
pub(crate) struct KeyTracker {
    held: Vec<Code>,
}

impl KeyTracker {
    pub(crate) fn route(
        &mut self,
        key: &Key,
        code: Code,
        modifiers: Modifiers,
        pressed: bool,
        escape: FocusEscape,
    ) -> KeyRoute {
        match key {
            // F6 and Shift+F6 cycle the window's regions: the keyboard can
            // always leave the view by them.
            Key::Named(NamedKey::F6) => return KeyRoute::Leave,
            Key::Named(NamedKey::Tab)
                if escape == FocusEscape::Tab
                    && (modifiers.is_empty() || modifiers == Modifiers::SHIFT) =>
            {
                return KeyRoute::Leave;
            }
            _ => {}
        }
        // Freya reports text from an input method, or from an on-screen
        // keyboard, as a character key with no physical key behind it.
        if let (true, Code::Unidentified, Key::Character(text)) = (pressed, code, key)
            && !text.is_empty()
        {
            return KeyRoute::Text(TextInput::Commit(text.clone()));
        }
        let repeat = if code == Code::Unidentified {
            false
        } else if pressed {
            let held = self.held.contains(&code);
            if !held {
                self.held.push(code);
            }
            held
        } else {
            self.held.retain(|held| *held != code);
            false
        };
        KeyRoute::Key(KeyInput {
            pressed,
            key: key.clone(),
            code,
            modifiers,
            repeat,
            text: if pressed {
                typed_text(key, modifiers)
            } else {
                None
            },
        })
    }

    /// Forgets the keys held: focus left, and their releases will not arrive.
    pub(crate) fn clear(&mut self) {
        self.held.clear();
    }
}
