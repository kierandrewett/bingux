//! The popup and full-screen frames: a preview in a dialog.
//!
//! [`show`] puts any preview content (a [`PreviewHost`] or a component that
//! builds one from its owner's state) into the window's dialog layer, as a
//! card or, in full screen and in a compact window, as a sheet that fills the
//! window. [`open`] is the one-call form for a source that does not change.

use bingux_preview_core::PreviewSource;
use freya::prelude::*;

use super::chrome::PreviewNav;
use super::host::PreviewHost;
use super::mode::Mode;
use crate::shell::{Overlays, framed_dialog};

/// The largest and the smallest card, in logical pixels. It is cut down to
/// fit a small window.
const CARD: (f32, f32) = (1040.0, 760.0);
const CARD_MIN: (f32, f32) = (420.0, 360.0);

/// The card for a view whose scrolled area is `view` wide and tall: as wide as
/// the view and a little taller, so over the chooser dialog it stays inside
/// the dialog and over a full window it is large.
pub fn card_size(view: (f32, f32)) -> (f32, f32) {
    if view.0 < 1.0 || view.1 < 1.0 {
        return CARD;
    }
    (view.0.clamp(CARD_MIN.0, CARD.0), (view.1 + 96.0).clamp(CARD_MIN.1, CARD.1))
}

/// Show `content` in the frame of `mode`. `size` is the card's size (see
/// [`card_size`]); `compact` is whether the window is a phone's, which only
/// has a full-window sheet. `on_closed` runs once the dialog has gone, to
/// give the keyboard back to the owner.
///
/// A pane is not a dialog: ask [`PreviewDock`](super::PreviewDock) for that.
/// Showing in `Mode::Pane` shows a popup.
pub fn show(overlays: Overlays, mode: Mode, size: (f32, f32), compact: bool, content: impl Into<Element>, on_closed: impl Into<EventHandler<()>>) {
    let dialog = framed_dialog("Preview", content, size.0, size.1).on_closed(on_closed);
    let dialog = if compact || mode == Mode::Fullscreen { dialog.fill_window() } else { dialog };
    overlays.show_dialog(dialog);
}

/// Preview `source` in a popup over the window. The header offers Info, full
/// screen and Close; Escape and Space close it.
pub fn open(overlays: Overlays, source: PreviewSource) {
    show_fixed(overlays, source, Mode::Popup);
}

fn show_fixed(overlays: Overlays, source: PreviewSource, mode: Mode) {
    let again = source.clone();
    let nav = PreviewNav::new(move |_| overlays.dismiss_dialog()).on_mode(move |next: Mode| show_fixed(overlays, again.clone(), next));
    let host = PreviewHost::new(Some(source), mode, nav);
    show(overlays, mode, CARD, crate::style::is_compact(), host, |_| {});
}
