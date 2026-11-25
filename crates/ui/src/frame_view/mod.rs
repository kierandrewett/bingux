//! Frames from somewhere else, in a window.
//!
//! [`FrameView`] shows what an external producer draws and forwards input to
//! it. The producer is anything that makes pictures on its own schedule and
//! wants the keyboard and the pointer back: a web engine rendering into shared
//! memory, a **video player** (a decoder thread hands over frames, and the
//! view scales them with [`FrameFit::Contain`]), a **remote desktop client**
//! (frames from the network, input to the wire) or an embedded game or VR
//! preview. The view knows nothing of any of them: it speaks to a
//! [`FrameSource`], and a source is a few methods.
//!
//! # The contract
//!
//! * **Pull, never push.** The producer calls the [waker](FrameSource::set_waker)
//!   when it has a frame; the view then calls [`take_frame`](FrameSource::take_frame)
//!   on the UI thread, once, gets the newest frame and gives back the one it
//!   replaces with [`release`](FrameSource::release). There is no timer.
//! * **Zero-copy.** A [`PixelFrame`] is a view onto the producer's memory. The
//!   view draws it where it lies; the one pass over the pixels is the draw.
//! * **Render at the size you are told.** [`FrameView::on_resize`] gives the
//!   size in logical pixels and the scale; [`ViewSize::device_size`] is the
//!   size of a frame that is drawn 1:1 and so does not blur. Until the next
//!   frame arrives the view keeps drawing the last one at its own size, in the
//!   top left corner ([`FrameFit::Anchored`]), with the backdrop in any area it
//!   leaves bare: a window that is being resized never stretches the page.
//! * **Input is in the view's logical pixels**, with DOM names for keys and
//!   a DOM sign for the wheel. See [`PointerInput`], [`WheelInput`],
//!   [`KeyInput`] and [`TextInput`].
//!
//! # A complete example
//!
//! ```ignore
//! use std::sync::Arc;
//! use bingux_ui::frame_view::{FrameView, PatternSource, frame_view};
//! use bingux_ui::prelude::*;
//! use freya::prelude::*;
//!
//! fn page() -> Element {
//!     // A producer: here, the bundled test pattern at 60 frames a second.
//!     let source = use_hook(|| PatternSource::animated(640, 360, 60));
//!     let resize_target = source.clone();
//!     frame_view(source, "Test pattern")
//!         .placeholder("Starting")
//!         // Render at the size the view really has: 1:1 on every display.
//!         .on_resize(move |size: ViewSize| {
//!             let (width, height) = size.device_size();
//!             resize_target.resize(width, height, size.scale as f32);
//!         })
//!         .on_pointer(|input: PointerInput| println!("{input:?}"))
//!         .into_element()
//! }
//! ```
//!
//! `cargo run --example frame_view` runs the same thing in a window.

mod frame;
mod geometry;
mod input;
mod pattern;
mod present;
mod source;
pub mod stats;
#[cfg(test)]
mod tests;
mod view;
#[cfg(all(test, feature = "testing"))]
mod view_tests;

pub use frame::{DamageRect, Frame, FrameError, PixelBytes, PixelFormat, PixelFrame};
pub use input::{
    CaretRect, CursorKind, FocusEscape, FrameFit, KeyInput, PointerAction, PointerButtons,
    PointerInput, TextInput, TouchInput, ViewSize, WheelDelta, WheelInput,
};
pub use freya::prelude::WheelPhase;
pub use pattern::{PATTERN_MIN_WIDTH, PatternSource, PatternStats};
pub use source::FrameSource;
pub use view::{FrameView, frame_view};
