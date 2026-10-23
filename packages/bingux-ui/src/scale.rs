//! The scale: spacing, radius, type, icon and box sizes.
//!
//! Every spacing, radius, type size and icon size in Bingux UI is one of
//! these names. `scripts/ui-lint.py` fails the build on a raw number in a
//! layout call, so a new value has to be added here first, on purpose.

use std::sync::atomic::{AtomicBool, Ordering};

/// Spacing, on a 4 px grid. `S0` is the half step for the gap between a
/// title and its subtitle.
pub const S0: f32 = 2.0;
pub const S1: f32 = 4.0;
pub const S2: f32 = 8.0;
pub const S3: f32 = 12.0;
pub const S4: f32 = 16.0;
pub const S5: f32 = 24.0;
pub const S6: f32 = 32.0;
pub const S7: f32 = 48.0;
/// The one horizontal inset. Row text, section titles, the page title and
/// sidebar items all start this far from their container's edge, so
/// everything lines up.
pub const GUTTER: f32 = S4;

/// Corner radius: small marks (checkbox, tick), controls and rows, and
/// cards, banners and dialogs.
pub const RADIUS_SM: f32 = 4.0;
pub const RADIUS: f32 = 12.0;
pub const RADIUS_LG: f32 = 16.0;
/// The shell's card radius: dialogs, popovers and toasts.
pub const RADIUS_XL: f32 = 20.0;

/// Type scale. Body is 14; secondary text 12; badges 11. The ramp in
/// `typography.rs` uses 11, 12, 14, 18, 28 and 36. 16 and 20 are older
/// sizes that overlays, status text and the clock still use.
pub const TEXT_XS: f32 = 11.0;
pub const TEXT_SM: f32 = 13.0;
pub const TEXT_MD: f32 = 14.0;
pub const TEXT_LG: f32 = 16.0;
pub const TEXT_XL: f32 = 20.0;
pub const TEXT_DISPLAY: f32 = 28.0;
/// The ramp's larger steps, each about 1.25x or more the one before: group
/// titles 18, page titles 28, a hero value 36.
pub const TEXT_HEADING: f32 = 18.0;
pub const TEXT_HERO: f32 = 36.0;

/// Icon sizes: chevrons and ticks, control glyphs, row and sidebar icons.
pub const ICON_SM: f32 = 12.0;
pub const ICON_MD: f32 = 16.0;
pub const ICON_LG: f32 = 18.0;
pub const ICON_XL: f32 = 48.0;
/// A full-colour application icon in a row, and at the head of a page.
pub const ICON_APP_SM: f32 = 20.0;
pub const ICON_APP: f32 = 32.0;
pub const ICON_APP_LG: f32 = 64.0;

/// Square boxes: a clear button or thumb, a small icon button, an avatar,
/// a stepper value or switch width, and a status-page tile.
pub const BOX_SM: f32 = 20.0;
pub const BOX_MD: f32 = 28.0;
pub const BOX_LG: f32 = 36.0;
pub const BOX_XL: f32 = 44.0;
pub const BOX_2XL: f32 = 96.0;
/// The width of a slider or a text field in a row.
pub const FIELD_WIDTH: f32 = 240.0;
/// The widest a field, dropdown or slider in a row gets on a phone-width
/// window, so the row's text keeps room. Read it through [`fit_field`].
pub const FIELD_WIDTH_TOUCH: f32 = 168.0;
/// The width of a text field that takes a URL, a path or a name.
pub const FIELD_WIDTH_WIDE: f32 = 440.0;
/// The width of a dropdown whose options carry application icons.
pub const FIELD_WIDTH_APPS: f32 = 260.0;
/// The width and height of a colour scheme card: a small window drawn in
/// the scheme's colours, as GNOME's Style switcher shows.
pub const PREVIEW_WIDTH: f32 = 144.0;
pub const PREVIEW_HEIGHT: f32 = 96.0;
/// Three scheme cards side by side with their gaps.
pub const PREVIEW_ROW_WIDTH: f32 = 3.0 * PREVIEW_WIDTH + 2.0 * S4;
/// The sidebar and the short accent line inside a scheme card.
pub const PREVIEW_SIDEBAR: f32 = 40.0;
pub const PREVIEW_LINE: f32 = 40.0;
/// The display arrangement preview on the Displays page. Monitor boxes scale
/// to fit inside it.
pub const ARRANGEMENT_WIDTH: f32 = 360.0;
pub const ARRANGEMENT_HEIGHT: f32 = 240.0;
/// How close, in logical pixels, a dragged display must be to another's edge
/// to snap flush with it.
pub const ARRANGEMENT_SNAP: i32 = 12;

/// The keyboard focus ring.
pub const RING_WIDTH: f32 = 2.0;

/// A control that sits in a row (dropdown, field, segmented control, stepper,
/// row button) is 32 px high, as `Theme.controlHeight`, so a 40 px row keeps
/// 4 px above and below it. A standalone `ActionButton` and a menu item are
/// 36 px. A row is 48 px, or 64 px with a subtitle, as `SettingsRow.qml`.
///
/// On a phone-width window (see [`set_touch`]) controls, buttons, rows and
/// the switch and slider handles grow to fingertip size. Read these sizes
/// through [`control_height`], [`button_height`] and [`row_height`], never
/// the constants, so the layout follows.
pub const CONTROL_HEIGHT: f32 = 36.0;
pub const BUTTON_HEIGHT: f32 = 40.0;
pub const MENU_ITEM_HEIGHT: f32 = 36.0;
/// The width of a menu of actions (see `shell::menu`).
pub const MENU_WIDTH: f32 = 220.0;
pub const ROW_HEIGHT: f32 = 56.0;
pub const TALL_ROW_HEIGHT: f32 = 72.0;
/// The fingertip sizes of the same things.
pub const CONTROL_HEIGHT_TOUCH: f32 = 44.0;
pub const BUTTON_HEIGHT_TOUCH: f32 = 48.0;
pub const ROW_HEIGHT_TOUCH: f32 = 60.0;
/// A page in the sidebar list.
pub const NAV_ITEM_HEIGHT: f32 = 40.0;
pub const NAV_ITEM_HEIGHT_TOUCH: f32 = 56.0;

static TOUCH: AtomicBool = AtomicBool::new(false);

/// Switch the sizes between mouse and fingertip. The split view sets it from
/// the window width and remounts its content when it changes.
pub fn set_touch(touch: bool) {
    TOUCH.store(touch, Ordering::Relaxed);
}

/// Whether the sizes are the fingertip sizes.
pub fn touch() -> bool {
    TOUCH.load(Ordering::Relaxed)
}

fn pick(mouse: f32, finger: f32) -> f32 {
    if touch() { finger } else { mouse }
}

/// The height of a control in a row.
pub fn control_height() -> f32 {
    pick(CONTROL_HEIGHT, CONTROL_HEIGHT_TOUCH)
}

/// The height of a standalone button.
pub fn button_height() -> f32 {
    pick(BUTTON_HEIGHT, BUTTON_HEIGHT_TOUCH)
}

/// The height of a one-line row.
pub fn row_height() -> f32 {
    pick(ROW_HEIGHT, ROW_HEIGHT_TOUCH)
}

/// A width for a control in a row, cut to [`FIELD_WIDTH_TOUCH`] on a phone.
pub fn fit_field(width: f32) -> f32 {
    if touch() {
        width.min(FIELD_WIDTH_TOUCH)
    } else {
        width
    }
}

/// The height of a page in the sidebar list.
pub fn nav_height() -> f32 {
    pick(NAV_ITEM_HEIGHT, NAV_ITEM_HEIGHT_TOUCH)
}

/// The height of a compact line or bar chart in the gallery edge cases.
pub const CHART_HEIGHT: f32 = 120.0;
