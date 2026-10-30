//! Choosing a colour: [`ColourPicker`], the full view, and [`ColourButton`], a
//! swatch that opens it.
//!
//! Both forms are one picker over one state model. The model is
//! [`Hsva`]: hue, saturation, value and alpha, never an RGB colour converted
//! back, because RGB cannot say which hue a grey was on. The picker reports
//! the RGB colour the model makes (a Freya `Color`) to `on_change`, and takes
//! the caller's `value` back as an edit of what is on screen.
//!
//! ```ignore
//! // Inline, in a page: the whole picker, with an opacity strip.
//! colour_picker(colour(), move |next| colour.set(next)).alpha(true)
//!
//! // In a row: a swatch that opens the same picker in a popover or a sheet.
//! colour_button(colour(), move |next| colour.set(next))
//!     .title("Accent colour")
//!     .live(true)
//! ```
//!
//! # The picker
//!
//! From the top: a saturation and value plane with a large handle, a hue
//! strip, an opacity strip when [`alpha`](ColourPicker::alpha) is on, the old
//! colour beside the new one, a hex field that takes `#rgb`, `#rgba`,
//! `#rrggbb` and `#rrggbbaa`, numeric boxes in RGB, HSL or HSV, a palette of
//! the system accents and the app's presets, and the colours chosen lately.
//! Nothing in it changes size when a colour is chosen or an entry is wrong:
//! [`ColourPicker::height`] is the height it is drawn at.
//!
//! - **Pointer.** A press on the plane or a strip takes the pointer until the
//!   button comes up; the handle is where the pointer is, held to the plane at
//!   its edges. The handle's target is 32 px across on a desktop and 44 px on
//!   a phone; its knob is smaller.
//! - **Keyboard.** The plane takes the arrow keys, 1 % a press and 10 % with
//!   Shift; Home and End go to no saturation and full saturation, and Page
//!   Up and Page Down move the value 10 %. A strip takes the same keys, with
//!   degrees for the hue. Tab visits the plane, the strips, the old colour,
//!   the hex field, the mode switch, the boxes, then the swatches. Up and
//!   Down in a box step it.
//! - **Assistive technology.** The plane and the strips are sliders with a
//!   value; each swatch is a button named by its colour and says whether it is
//!   selected; the preview says both colours; the hex field's message is a
//!   status.
//! - **Direction.** A colour axis is not text: the plane and the strips run
//!   the same way in every language, and right is more saturation, as it is on
//!   a screen with the strip's red at its left end. The layout is symmetric,
//!   with no left or right inset that could be the wrong way round.
//!
//! # The button
//!
//! [`ColourButton`] is a control-high square with the colour in a ring. It
//! opens the picker in a popover anchored to it on a desktop and in a bottom
//! sheet on a phone. Done commits; Escape, a press outside and Back close it.
//! With [`live`](ColourButton::live) the colour is reported as it is
//! dragged and nothing is held back; without it the colour is reported on Done
//! and every other way out drops the draft.
//!
//! # Recent colours
//!
//! A colour is added to the recent row when an edit of it is over. The app
//! decides where they live: pass `recent(list, on_change)` and save the list
//! it is told; without it the picker keeps them in memory for the life of the
//! process. There is no eyedropper: the one way to read a screen pixel from an
//! app, `org.freedesktop.portal.Screenshot.PickColor`, answers through a
//! signal on the caller's own D-Bus connection, and this crate has no D-Bus
//! client to hold one.

mod button;
mod chip;
mod core;
mod fields;
mod geometry;
mod model;
mod palette;
mod picker;
mod plane;

#[cfg(all(test, feature = "testing"))]
mod live_tests;

pub use button::{ColourButton, colour_button};
pub use chip::colour_chip;
pub use core::RECENT_MAX;
pub use model::{Gamut, Hex, HexError, Hsva, Mode, parse_hex, push_recent};
pub use palette::PRESETS;
pub use picker::{ColourPicker, colour_picker};
