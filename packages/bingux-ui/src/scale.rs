//! The scale: spacing, radius, type, icon and box sizes.
//!
//! Every spacing, radius, type size and icon size in Bingux UI is one of
//! these names. `scripts/ui-lint.py` fails the build on a raw number in a
//! layout call, so a new value has to be added here first, on purpose.

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
pub const GUTTER: f32 = S3;

/// Corner radius: small marks (checkbox, tick), controls and rows, and
/// cards, banners and dialogs.
pub const RADIUS_SM: f32 = 4.0;
pub const RADIUS: f32 = 8.0;
pub const RADIUS_LG: f32 = 12.0;
/// The shell's card radius: dialogs, popovers and toasts.
pub const RADIUS_XL: f32 = 20.0;

/// Type scale. Body is 14; secondary text 12; badges 11; section titles
/// 16; status-page titles 20; the page title 28.
pub const TEXT_XS: f32 = 11.0;
pub const TEXT_SM: f32 = 12.0;
pub const TEXT_MD: f32 = 14.0;
pub const TEXT_LG: f32 = 16.0;
pub const TEXT_XL: f32 = 20.0;
pub const TEXT_DISPLAY: f32 = 28.0;

/// Icon sizes: chevrons and ticks, control glyphs, row and sidebar icons.
pub const ICON_SM: f32 = 12.0;
pub const ICON_MD: f32 = 16.0;
pub const ICON_LG: f32 = 18.0;
pub const ICON_XL: f32 = 48.0;

/// Square boxes: a clear button or thumb, a small icon button, an avatar,
/// a stepper value or switch width, and a status-page tile.
pub const BOX_SM: f32 = 20.0;
pub const BOX_MD: f32 = 28.0;
pub const BOX_LG: f32 = 36.0;
pub const BOX_XL: f32 = 44.0;
pub const BOX_2XL: f32 = 96.0;
/// The width of a slider or a text field in a row.
pub const FIELD_WIDTH: f32 = 220.0;

/// The keyboard focus ring.
pub const RING_WIDTH: f32 = 2.0;

/// A control (button, dropdown, field, segmented control) is 36 px high, as
/// `ActionButton.qml`; a row is 40 px, or 56 px with a subtitle, as
/// `ControlRow.qml`.
pub const CONTROL_HEIGHT: f32 = 36.0;
pub const BUTTON_HEIGHT: f32 = 36.0;
pub const ROW_HEIGHT: f32 = 40.0;
pub const TALL_ROW_HEIGHT: f32 = 56.0;
