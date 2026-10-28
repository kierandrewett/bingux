//! The tokens: every spacing, radius, size, colour and line Bingux UI draws.
//!
//! Each kind of value is its own type with a fixed set of values. A page or
//! component cannot pass a number where a token is expected, so a value that
//! is not on the scale does not compile. Read a token's number with `px()`
//! only for geometry (a chart, a drag offset), never to style a box: boxes
//! are styled with [`super::Tw::tw`].
//!
//! Sizes come in two classes, [`SizeClass`]: `Regular` is the mouse density of a
//! desktop window, `Compact` the fingertip density, used on a phone-width window
//! or once a finger has touched the window. Every size token resolves against the
//! current class when it is applied, from the one table for its kind, so a style
//! written once is right at both sizes. The class is reactive: a component that
//! reads a size re-renders in place when the class changes.

use std::cell::RefCell;
use std::collections::HashMap;

use freya::prelude::{Color, State, WritableUtils, try_consume_context};

use crate::theme::{BannerKind, Tokens};

/// A space on the 4 px grid, named as Tailwind names it: `S4` is 16 px at
/// regular density, as `p-4` is. `S0_5` is the half step between a title and
/// its subtitle. Each step has a compact value too, see [`SizeClass`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Space {
    regular: f32,
    compact: f32,
}

impl Space {
    const fn new(regular: f32, compact: f32) -> Self {
        Self { regular, compact }
    }

    /// The space in logical pixels at the current size class.
    pub fn px(self) -> f32 {
        resolve(self.regular, self.compact)
    }

    /// The space in logical pixels at `class`, for a tool that reads another
    /// window's density (the inspector, showing an app at phone size).
    pub fn px_in(self, class: SizeClass) -> f32 {
        class.pick(self.regular, self.compact)
    }

    /// The regular value whatever the size class, for geometry that must not
    /// move with the density: a chart's plot margins, a menu's edge.
    pub const fn fixed_px(self) -> f32 {
        self.regular
    }
}

pub const S0: Space = Space::new(0.0, 0.0);
pub const S0_5: Space = Space::new(2.0, 2.0);
pub const S1: Space = Space::new(4.0, 4.0);
pub const S2: Space = Space::new(8.0, 8.0);
pub const S3: Space = Space::new(12.0, 14.0);
pub const S4: Space = Space::new(16.0, 18.0);
pub const S5: Space = Space::new(20.0, 22.0);
pub const S6: Space = Space::new(24.0, 26.0);
pub const S7: Space = Space::new(28.0, 32.0);
pub const S8: Space = Space::new(32.0, 36.0);
pub const S10: Space = Space::new(40.0, 44.0);
pub const S12: Space = Space::new(48.0, 54.0);

/// The space between a row's trailing button and the row's top, bottom and
/// trailing edge. [`Dim::RowButton`] is the row height less twice this, and
/// the button's radius is the card's less this ([`Radius::Inner`]).
pub const ROW_INSET: Space = S2;

/// The padding of an alert dialog's card, equal on all four sides, whatever
/// the density: 12 px. The buttons sit inside it, so a button's radius is the
/// card's less this ([`Radius::nested`] of [`Radius::Overlay`], which is
/// [`Radius::Inner`]), and the title and message sit one more inset in, so the
/// text starts 24 px from the card's edge. It does not grow on a phone: the
/// radius relation would break.
pub const DIALOG_INSET: Space = Space::new(12.0, 12.0);

/// The space between a row's trailing switch and the row's top, bottom and
/// trailing edge: half the row height less the switch height, 18 px on a
/// desktop and 16 px on a phone.
pub const SWITCH_INSET: Space = Space::new(18.0, 16.0);

/// The space between a field's trailing button (a password's reveal, a search
/// field's clear) and the field's top, bottom and trailing edge.
/// [`Dim::InlineButton`] is the field height ([`Dim::Control`]) less twice
/// this, and the button's radius is the field's less this
/// ([`Radius::nested`]): 4 px on a desktop. At compact density the button is
/// the whole 44 px touch target, so the inset is 0 and the button takes the
/// field's corner. A dense or flat field is shorter than a button, so its
/// button is a [`Dim::Box5`] with the inset that leaves.
pub const FIELD_INSET: Space = Space::new(4.0, 0.0);

/// The space above and below a one-line field in a form row: half the row
/// height less the field height, so a field row is as tall as any other row
/// and the field sits level in it: 10 px on a desktop, 8 px on a phone.
pub const FIELD_ROW_INSET: Space = Space::new(10.0, 8.0);

/// The side padding inside a sidebar or list navigation row. It matches the
/// space above and below the row's 24 px icon (40 px rows on a desktop, 56 px
/// on a phone), so the icon sits in an even margin at both densities.
pub const NAV_INSET: Space = Space::new(8.0, 16.0);

/// The space between a list of selectable items and the edge of its
/// container, on each side. A row's fill stops this far short of the edge, so
/// its rounded ends show: sidebar pages, places and file rows all keep it, and
/// a row's content then starts `LIST_EDGE + NAV_INSET` from the edge.
pub const LIST_EDGE: Space = S2;

/// The one horizontal inset inside a card, a header bar and the page edge
/// on a phone: row text, hairlines and controls start this far in.
pub const GUTTER: Space = S4;

/// The space a bottom sheet keeps clear below its last row, on top of
/// [`Dim::SafeBottom`]. It is the end of the sheet's scroll content, so the
/// last row scrolls clear of the window edge and is never cut by it.
pub const SHEET_BOTTOM: Space = S4;

/// The smallest a control's hit area may be at compact density: 44 px, the
/// touch minimum. Every [`Dim`] that is a control's size passes it (see
/// [`Dim::is_touch_target`]).
pub const TOUCH_MIN: f32 = 44.0;

/// The size of an icon button in a header bar. It is the control height, so a
/// header's icon buttons, its segmented control and its split button stand
/// on one height and rows of header controls line up: 36 px on a desktop,
/// 44 px (the touch minimum) on a phone. One token, so it cannot drift.
pub const HEADER_BUTTON: Dim = Dim::Control;

/// How far a disabled control or menu item fades: clearly unavailable, still
/// readable.
pub const DISABLED_OPACITY: f32 = 0.45;

/// How far a window's content shrinks behind a popup that insets it (a phone
/// sheet): the share of the window it keeps. The card it becomes keeps the
/// `Card` radius.
pub const INSET_SCALE: f32 = 0.96;

/// Corner radius, by what it rounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Radius {
    /// Small marks: a checkbox, a skeleton bar, a scrubber.
    Mark,
    /// A box inside a rounded rail: the rail radius less its inset (12 - 4).
    Inner,
    /// Controls, rows and sidebar items.
    Control,
    /// Group cards and tiles.
    Card,
    /// Dialogs, popups, menus, popovers and toasts.
    Overlay,
    /// A large tile such as a status-page icon well.
    Tile,
    /// Fully round: switches, avatars, swatches, badges.
    Pill,
    /// The window's own corner: the shell draws its frame at this radius, so
    /// anything that touches the window's bottom edge keeps clear of it.
    Window,
}

impl Radius {
    pub const fn px(self) -> f32 {
        match self {
            Radius::Mark => 4.0,
            Radius::Control => 12.0,
            Radius::Inner => 8.0,
            Radius::Card => 16.0,
            Radius::Overlay => 20.0,
            Radius::Tile => 24.0,
            Radius::Window => 12.0,
            Radius::Pill => 999.0,
        }
    }

    /// The radius of a box that sits `inset` px inside a box of this radius
    /// and shares its corner: the outer radius less the inset, which keeps
    /// the two curves concentric. The result is the nearest token at or
    /// below that, so it is [`Radius::Control`] when the inset is 0, and
    /// [`Radius::Inner`] for a button 4 px inside a control.
    pub fn nested(self, inset: f32) -> Radius {
        let wanted = self.px() - inset;
        [Radius::Control, Radius::Inner, Radius::Mark]
            .into_iter()
            .find(|radius| radius.px() <= wanted + 0.01)
            .unwrap_or(Radius::Mark)
    }
}

/// Text sizes, by role. Only `typography.rs` reads them: text is set with a
/// [`crate::typography::TextStyle`], never a size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextSize {
    /// Badges and overlines.
    Xs,
    /// Captions and helper text.
    Sm,
    /// Row titles and body text.
    Md,
    /// Group titles.
    Lg,
    /// Dialog and subpage headings.
    Heading,
    /// Page titles.
    Display,
    /// A single large value, such as a clock.
    Hero,
}

impl TextSize {
    /// The size in logical pixels at the current size class.
    pub fn px(self) -> f32 {
        let (regular, compact) = self.sizes();
        resolve(regular, compact) * crate::stress::text_scale()
    }

    /// The size in logical pixels at `class`; see [`Space::px_in`].
    pub fn px_in(self, class: SizeClass) -> f32 {
        let (regular, compact) = self.sizes();
        class.pick(regular, compact) * crate::stress::text_scale()
    }

    /// The (regular, compact) sizes. Compact text is a step larger, so body
    /// text reads at arm's length on a phone.
    const fn sizes(self) -> (f32, f32) {
        match self {
            TextSize::Xs => (11.0, 12.0),
            TextSize::Sm => (13.0, 14.0),
            TextSize::Md => (14.0, 15.0),
            TextSize::Lg => (16.0, 17.0),
            TextSize::Heading => (18.0, 19.0),
            TextSize::Display => (28.0, 30.0),
            TextSize::Hero => (36.0, 38.0),
        }
    }
}

/// Icon sizes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IconSize {
    /// Chevrons, ticks and other marks inside a control: 12 px.
    Mark,
    /// A symbol set in a line of body text, such as before a dropdown's
    /// label: 14 px, the height of the text.
    Text,
    /// Glyphs in buttons and fields: 16 px.
    Control,
    /// Row and sidebar icons: 18 px.
    Row,
    /// The icon of an empty or status page: 48 px.
    Hero,
    /// A full-colour application icon in a dropdown: 20 px.
    AppSmall,
    /// A full-colour application icon in a row: 32 px.
    App,
    /// A full-colour application icon at the head of a page: 64 px.
    AppLarge,
}

impl IconSize {
    /// The size in logical pixels at the current size class.
    pub fn px(self) -> f32 {
        let (regular, compact) = self.sizes();
        resolve(regular, compact)
    }

    /// The (regular, compact) sizes. Icons grow about one seventh in the
    /// compact class, so they stay in step with the text beside them.
    const fn sizes(self) -> (f32, f32) {
        match self {
            IconSize::Mark => (12.0, 14.0),
            IconSize::Text => (14.0, 16.0),
            IconSize::Control => (16.0, 18.0),
            IconSize::Row => (18.0, 20.0),
            IconSize::Hero => (48.0, 56.0),
            IconSize::AppSmall => (20.0, 24.0),
            IconSize::App => (32.0, 36.0),
            IconSize::AppLarge => (64.0, 74.0),
        }
    }
}

/// A width or height, by what it sizes. The interactive sizes grow in the
/// compact class; their values are in the `sizes` table below.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dim {
    /// A one-line row: 56 px, 60 px for a finger.
    Row,
    /// A row with a subtitle at its smallest: 72 px.
    RowTall,
    /// A row whose subtitle is one line: its natural height, 63 px at mouse
    /// density. Lists that mount rows lazily size their slots with this.
    RowSubtitle,
    /// A control in a row (dropdown, field, segmented control, stepper,
    /// compact button): 36 px, 44 px for a finger.
    Control,
    /// A standalone button: 40 px, 48 px for a finger.
    Button,
    /// An item in a menu or dropdown list: 36 px.
    MenuItem,
    /// The space a menu gives a separator line: 8 px, the line in its middle.
    MenuSeparator,
    /// A page in the sidebar list: 40 px, 56 px for a finger.
    NavItem,
    /// The icon column of a sidebar row, shared by page icons, the search
    /// glyph and the account avatar: 24 px.
    NavIcon,
    /// The header bar: 48 px.
    Header,
    /// A window button (minimise, maximise, close): a 24 px circle in a
    /// 32 px target, 44 px at compact density.
    WindowControl,
    WindowControlTarget,
    /// A field, dropdown or slider in a row: 240 px, at most 168 px on a
    /// phone.
    Field,
    /// The least a field or dropdown in a row shrinks to before the row
    /// stacks its label above it: 120 px, which still shows a word or two.
    FieldMin,
    /// A field for a URL, path or name: 440 px, at most 168 px on a phone.
    FieldWide,
    /// A colour value beside its swatch and picker button: 144 px.
    FieldColour,
    /// A dropdown whose options carry application icons: 260 px.
    FieldApps,
    /// A menu of actions: 220 px.
    Menu,
    /// A menu with keyboard shortcut hints: 300 px, so a label and its
    /// shortcut share a row.
    MenuWide,
    /// A dialog card: 400 px.
    Dialog,
    /// The narrowest an alert dialog's card gets: 280 px.
    DialogMin,
    /// A popup's width: 520 px.
    PopupWidth,
    /// A short form's popup width: 440 px, such as the Wi-Fi password popup.
    PopupNarrow,
    /// The height of a popup that lays itself out (a file chooser) until it
    /// asks for another: 600 px. A form popup has none: it is as tall as its
    /// content.
    PopupHeight,
    /// A dropdown's option list: 240 px, never narrower than its button.
    DropdownMenu,
    /// The widest a dropdown button gets before its label is cut: 360 px.
    DropdownMax,
    /// The tallest a dropdown's option list gets before it scrolls: 320 px.
    MenuList,
    /// A switch track: 36 x 20 px, 48 x 28 px for a finger.
    SwitchWidth,
    SwitchHeight,
    /// A switch thumb: 16 px, 22 px for a finger; while pressed it
    /// widens to 18 px, 24 px for a finger.
    SwitchThumb,
    SwitchThumbPressed,
    /// A slider track: 6 px, 8 px while it is dragged.
    SliderTrack,
    SliderTrackActive,
    /// A slider thumb: 16 px, 22 px for a finger.
    SliderThumb,
    /// A stepper's square half: 28 px.
    StepperHalf,
    /// A button inside a field or a chip, such as a password's reveal or a
    /// tag's remove: 28 px, 44 px at compact density, the touch minimum.
    InlineButton,
    /// A button, dropdown or segmented control that is the trailing control
    /// of a row: the row's height less [`ROW_INSET`] above and below, so it
    /// fills the row and sits the same distance from its top, bottom and
    /// trailing edge: 40 px, 44 px (the touch minimum) at compact density.
    RowButton,
    /// The sidebar: 280 px.
    Sidebar,
    /// The page column at its widest on a window with a sidebar: 720 px.
    Page,
    /// The page column at its widest on a narrow window, where the list of
    /// pages is a screen of its own: 520 px. The column is that wide or the
    /// window less its margins, whichever is less, centred. Cards grow with
    /// the window up to the cap and then stop, so a label and its value never
    /// sit hundreds of pixels apart on a tablet-size window.
    PagePhone,
    /// Square boxes, named by Tailwind size step: 20, 28, 36, 44 and 96 px
    /// (`size-5`, `size-7`, `size-9`, `size-11`, `size-24`).
    Box5,
    Box7,
    Box9,
    Box11,
    Box24,
    /// The narrowest a tab of a tab strip gets before the strip scrolls: 96 px,
    /// 128 px on a phone, where the close slot is a whole 44 px touch target
    /// and a title still needs room beside it. The widest is [`Dim::Field`].
    TabMin,
    /// A colour scheme preview card: 144 x 96.
    PreviewWidth,
    PreviewHeight,
    /// The three colour scheme cards in one row, with the 16 px gaps
    /// between them: 464 px.
    PreviewRow,
    /// The sidebar of a scheme card's window: 40 px.
    PreviewSidebar,
    /// The accent line at the top of a scheme card's content: 40 px.
    PreviewLine,
    /// The height of a grey stand-in line in a scheme card: 4 px.
    PreviewBar,
    /// The title bar of a scheme card's window: 16 px.
    PreviewTitleBar,
    /// A compact chart: 120 px.
    Chart,
    /// The speaker test's room circle: 320 px across.
    SpeakerRoom,
    /// A speaker button in the test: 56 px.
    SpeakerButton,
    /// The speaker test popup's width: 480 px; its height follows the room.
    SpeakerPopupWidth,
    /// The display arrangement preview: 360 x 240.
    ArrangementWidth,
    ArrangementHeight,
    /// The hint under the arrangement: 260 px, so it leaves room for the
    /// display's quick controls.
    ArrangementHint,
    /// Skeleton bars standing in for a group title, a row title and a
    /// value: 120, 180 and 96 px wide, one caption line (13 px) high.
    SkeletonTitle,
    SkeletonLabel,
    SkeletonValue,
    SkeletonLine,
    /// One line of caption text, kept free under a field that validates
    /// so the row does not jump when an error appears.
    CaptionLine,
    /// An action strip cell: icon, a small gap and one line of label.
    StripCell,
    /// A box the size of a row icon, for a drawn glyph: 18 px.
    IconBox,
    /// One Wi-Fi signal bar: 3 px wide.
    SignalBar,
    /// The keyboard focus ring and selection rings: 2 px.
    Ring,
    /// A round colour-picker thumb: 12 px.
    Thumb,
    /// A colour-picker swatch: 16 px.
    Swatch,
    /// The label column of a property row, left of its controls: 96 px.
    Label,
    /// A hairline: 1 px.
    Hairline,
    /// The strip at the bottom edge of a phone-size window that the system's
    /// gesture bar may use: 16 px at compact density, none on a desktop.
    /// Pages, sheets, toasts and footers end this far above the edge, so no
    /// row, button or pill sits on it.
    SafeBottom,
    /// A selection indicator: 2 px, the underline of the selected tab.
    Indicator,
    /// A check or radio box: 18 px.
    Check,
    /// A thin progress or slider track: 6 px.
    Track,
    /// The ring of a medium spinner: 3 px.
    SpinnerMedium,
    /// A radio dot and a checkbox's mixed-state dash length: 8 px.
    Dot,
    /// The thickness of a checkbox's mixed-state dash: 2 px.
    DashHeight,
    /// The leading slot of a row holding an 18 px glyph, so text lines up
    /// with rows that have an icon.
    IconRow,
    /// The slot of a 16 px control glyph, such as a status tick.
    IconControl,
    /// A dense one-line row for tools and inspectors: 28 px. Property
    /// editors, table rows and flame graph legends use it.
    Dense,
    /// A control inside a dense row: 24 px.
    DenseControl,
    /// The name column of a dense property row: 120 px.
    PropLabel,
    /// One bar of a flame graph: 20 px.
    FlameRow,
    /// A frame timeline strip: 72 px.
    Timeline,
    /// The widest a dense inline editor (number, colour, choice) gets: 140 px.
    DenseField,
    /// A row of the DOM tree and code views: 22 px.
    CodeRow,
    /// The indent of one level in the DOM tree: 14 px.
    TreeIndent,
    /// The find strip docked at the top of a pane: 26 px.
    FlatField,
    /// A file chooser popup: 880 x 700 px, until the window is smaller.
    ChooserWidth,
    ChooserHeight,
    /// The places column of a file chooser: 200 px.
    PlacesColumn,
    /// The Size and the Modified columns of a file list: 88 and 112 px.
    FileSizeColumn,
    FileModifiedColumn,
    /// The narrowest a tile of the file grid gets: 132 px.
    GridTile,
    /// The widest a tooltip bubble gets: 280 px. Longer text wraps onto more
    /// lines inside it, up to four, then is cut.
    TooltipMax,
}

impl Dim {
    /// The size in logical pixels at the current size class.
    pub fn px(self) -> f32 {
        let (regular, compact) = self.sizes();
        resolve(regular, compact)
    }

    /// The size in logical pixels at `class`; see [`Space::px_in`].
    pub fn px_in(self, class: SizeClass) -> f32 {
        let (regular, compact) = self.sizes();
        class.pick(regular, compact)
    }

    /// Whether this size is the hit area of something a finger presses: a
    /// row, a button, a menu item, a window button. Such a size is at least
    /// [`TOUCH_MIN`] at compact density, and a test walks the table to hold
    /// it to that. The match has no wildcard: a new `Dim` must say which it
    /// is. A size that is not a hit area is geometry (a box, a column width, a
    /// chart) or a mark inside a larger target: the switch and the slider
    /// thumb are pressed through their row or their track.
    pub const fn is_touch_target(self) -> bool {
        match self {
            Dim::Row
            | Dim::RowTall
            | Dim::RowSubtitle
            | Dim::Control
            | Dim::Button
            | Dim::MenuItem
            | Dim::NavItem
            | Dim::WindowControlTarget
            | Dim::StepperHalf
            | Dim::InlineButton
            | Dim::RowButton
            | Dim::SpeakerButton => true,
            Dim::MenuSeparator
            | Dim::NavIcon
            | Dim::Header
            | Dim::WindowControl
            | Dim::Field
            | Dim::FieldWide
            | Dim::FieldApps
            | Dim::FieldColour
            | Dim::FieldMin
            | Dim::Menu
            | Dim::MenuWide
            | Dim::Dialog
            | Dim::DialogMin
            | Dim::PopupWidth
            | Dim::PopupNarrow
            | Dim::PopupHeight
            | Dim::DropdownMenu
            | Dim::DropdownMax
            | Dim::MenuList
            | Dim::SwitchWidth
            | Dim::SwitchHeight
            | Dim::SwitchThumb
            | Dim::SwitchThumbPressed
            | Dim::SliderTrack
            | Dim::SliderTrackActive
            | Dim::SliderThumb
            | Dim::Sidebar
            | Dim::Page
            | Dim::PagePhone
            | Dim::Box5
            | Dim::Box7
            | Dim::Box9
            | Dim::Box11
            | Dim::Box24
            | Dim::TabMin
            | Dim::PreviewWidth
            | Dim::PreviewHeight
            | Dim::PreviewRow
            | Dim::PreviewSidebar
            | Dim::PreviewLine
            | Dim::PreviewBar
            | Dim::PreviewTitleBar
            | Dim::Chart
            | Dim::SpeakerRoom
            | Dim::SpeakerPopupWidth
            | Dim::ArrangementWidth
            | Dim::ArrangementHeight
            | Dim::ArrangementHint
            | Dim::SkeletonTitle
            | Dim::SkeletonLabel
            | Dim::SkeletonValue
            | Dim::SkeletonLine
            | Dim::CaptionLine
            | Dim::StripCell
            | Dim::IconBox
            | Dim::SignalBar
            | Dim::Ring
            | Dim::Thumb
            | Dim::Swatch
            | Dim::Label
            | Dim::Hairline
            | Dim::SafeBottom
            | Dim::Indicator
            | Dim::Check
            | Dim::Track
            | Dim::SpinnerMedium
            | Dim::Dot
            | Dim::DashHeight
            | Dim::IconRow
            | Dim::IconControl
            | Dim::Dense
            | Dim::DenseControl
            | Dim::PropLabel
            | Dim::FlameRow
            | Dim::Timeline
            | Dim::DenseField
            | Dim::CodeRow
            | Dim::TreeIndent
            | Dim::FlatField
            | Dim::ChooserWidth
            | Dim::ChooserHeight
            | Dim::PlacesColumn
            | Dim::FileSizeColumn
            | Dim::FileModifiedColumn
            | Dim::GridTile
            | Dim::TooltipMax => false,
        }
    }

    /// Every box size as (regular, compact) in logical pixels. This is the one
    /// table for boxes: a size that does not change with density is written
    /// with [`same`].
    fn sizes(self) -> (f32, f32) {
        match self {
            Dim::Row => (56.0, 60.0),
            Dim::RowTall => (72.0, 80.0),
            Dim::RowSubtitle => (63.0, 68.0),
            Dim::Control => (36.0, 44.0),
            Dim::Button => (40.0, 48.0),
            Dim::MenuItem => (36.0, 52.0),
            Dim::MenuSeparator => same(8.0),
            Dim::NavItem => (40.0, 56.0),
            Dim::NavIcon => same(24.0),
            Dim::Header => (48.0, 56.0),
            Dim::WindowControl => same(24.0),
            Dim::WindowControlTarget => (32.0, TOUCH_MIN),
            Dim::Field => (240.0, 168.0),
            Dim::FieldWide => (440.0, 168.0),
            Dim::FieldApps => (260.0, 168.0),
            Dim::Menu => same(220.0),
            Dim::MenuWide => same(300.0),
            Dim::Dialog => same(400.0),
            Dim::DialogMin => same(280.0),
            Dim::PopupWidth => same(520.0),
            Dim::PopupNarrow => same(440.0),
            Dim::PopupHeight => same(600.0),
            Dim::DropdownMenu => same(240.0),
            Dim::DropdownMax => same(360.0),
            Dim::MenuList => same(320.0),
            Dim::SwitchWidth => (36.0, 48.0),
            Dim::SwitchHeight => (20.0, 28.0),
            Dim::SwitchThumb => (16.0, 22.0),
            Dim::SwitchThumbPressed => (18.0, 24.0),
            Dim::SliderTrack => same(6.0),
            Dim::SliderTrackActive => same(8.0),
            Dim::SliderThumb => (16.0, 22.0),
            Dim::StepperHalf => (28.0, TOUCH_MIN),
            Dim::InlineButton => (28.0, TOUCH_MIN),
            Dim::RowButton => (40.0, TOUCH_MIN),
            Dim::Sidebar => same(280.0),
            Dim::Page => same(720.0),
            Dim::PagePhone => same(520.0),
            Dim::Box5 => same(20.0),
            Dim::Box7 => same(28.0),
            Dim::Box9 => same(36.0),
            Dim::Box11 => same(44.0),
            Dim::Box24 => same(96.0),
            Dim::TabMin => (96.0, 128.0),
            Dim::PreviewWidth => same(144.0),
            Dim::PreviewHeight => same(96.0),
            Dim::PreviewRow => same(464.0),
            Dim::PreviewSidebar => same(40.0),
            Dim::PreviewLine => same(40.0),
            Dim::PreviewBar => same(4.0),
            Dim::PreviewTitleBar => same(16.0),
            Dim::Chart => same(120.0),
            Dim::SpeakerRoom => same(360.0),
            Dim::SpeakerButton => same(56.0),
            Dim::SpeakerPopupWidth => same(480.0),
            Dim::ArrangementWidth => same(360.0),
            Dim::ArrangementHeight => same(240.0),
            Dim::ArrangementHint => same(260.0),
            Dim::SkeletonTitle => same(120.0),
            Dim::SkeletonLabel => same(180.0),
            Dim::SkeletonValue => same(96.0),
            Dim::SkeletonLine => TextSize::Sm.sizes(),
            Dim::CaptionLine => {
                let (regular, compact) = TextSize::Sm.sizes();
                ((regular * 1.45).ceil(), (compact * 1.45).ceil())
            }
            Dim::StripCell => same(76.0),
            Dim::IconBox => IconSize::Row.sizes(),
            Dim::SignalBar => same(3.0),
            Dim::Ring => same(2.0),
            Dim::Thumb => same(12.0),
            Dim::Swatch => same(16.0),
            Dim::Label => same(96.0),
            Dim::Hairline => same(1.0),
            Dim::SafeBottom => (0.0, 16.0),
            Dim::Indicator => same(2.0),
            Dim::FieldColour => same(144.0),
            Dim::FieldMin => same(120.0),
            Dim::Check => same(18.0),
            Dim::Track => same(6.0),
            Dim::SpinnerMedium => same(3.0),
            Dim::Dot => same(8.0),
            Dim::DashHeight => same(2.0),
            Dim::IconRow => IconSize::Row.sizes(),
            Dim::IconControl => IconSize::Control.sizes(),
            Dim::Dense => same(28.0),
            Dim::DenseControl => same(24.0),
            Dim::PropLabel => same(120.0),
            Dim::FlameRow => same(20.0),
            Dim::Timeline => same(72.0),
            Dim::DenseField => same(140.0),
            Dim::CodeRow => same(22.0),
            Dim::TreeIndent => same(14.0),
            Dim::FlatField => same(26.0),
            Dim::ChooserWidth => same(880.0),
            Dim::ChooserHeight => same(700.0),
            Dim::PlacesColumn => same(200.0),
            Dim::FileSizeColumn => same(88.0),
            Dim::FileModifiedColumn => same(112.0),
            Dim::GridTile => same(132.0),
            Dim::TooltipMax => same(280.0),
        }
    }
}

/// A size that does not change with density.
const fn same(px: f32) -> (f32, f32) {
    (px, px)
}

/// The value for the current size class. A pair with equal values does not
/// read the class, so a fixed size does not subscribe its component to it.
fn resolve(regular: f32, compact: f32) -> f32 {
    if regular == compact {
        regular
    } else {
        size_class().pick(regular, compact)
    }
}

/// A fill colour, by what it fills. Resolved against the live [`Tokens`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Paint {
    Transparent,
    /// The page and the window.
    Bg,
    /// Header bars.
    Surface,
    /// The header bar that floats over a scrolling page: the page colour at
    /// 82 %, so the blurred content behind it shows through faintly.
    Frosted,
    /// The sidebar and the window frame around the page panel.
    Sidebar,
    /// The strip behind a window's tabs: one step darker than [`Paint::Bg`],
    /// so the selected tab and the content under it, both `Bg`, read as one
    /// shape rising out of it.
    TabStrip,
    /// Group cards.
    Card,
    /// Buttons, fields, dropdowns and segmented tracks.
    Control,
    /// The selected segment, raised off its `Control` track.
    Raised,
    /// Menus, popovers and dialogs.
    Popover,
    /// The dimmed layer behind a dialog or popup: black at 40 %.
    Scrim,
    /// The opaque black behind the whole window, when a popup insets it.
    Backdrop,
    /// The clear layer behind a menu or dropdown list. It paints nothing,
    /// but Freya lets presses, the wheel and hover reach a box below only
    /// through a box whose fill is exactly `Color::TRANSPARENT`; this one is
    /// not, so the window behind the open menu takes no input.
    Shield,
    Accent,
    /// The keyboard focus ring and the error ring, at 3:1 on every layer.
    FocusRing,
    ErrorRing,
    /// The fainter band outside the error ring of a focused invalid control:
    /// the error ring at 34 %. It is an accent to the ring, never the only
    /// signal, so it carries no contrast requirement of its own.
    ErrorHalo,
    /// The accent at low strength, behind a badge.
    AccentSoft,
    /// Text and marks drawn on the accent.
    OnAccent,
    /// Text and marks drawn on the danger, success and warning fills, and on
    /// the floating surface.
    OnDanger,
    OnSuccess,
    OnWarning,
    OnFloating,
    /// The selected sidebar item or row.
    Selection,
    Hover,
    Pressed,
    /// Hairlines between rows and panes.
    Separator,
    /// The edge of an idle control that needs one.
    Outline,
    /// Primary text colour as a fill.
    Text,
    /// The round knob of a slider: white in both schemes, lifted by a soft
    /// shadow instead of an edge.
    Knob,
    /// Secondary text colour as a fill.
    Muted,
    /// Secondary text colour at 30%: an empty signal bar.
    MutedFaint,
    Danger,
    /// The danger colour at low strength over a card: an error block.
    DangerSoft,
    Success,
    Warning,
    /// The dark surface of anything that floats over the window for a
    /// moment: tooltips and toasts.
    Floating,
    /// The unfilled part of a spinner ring: the text colour at low strength,
    /// so it reads under any spinner ink.
    Track,
    /// The faint edge of a floating surface.
    FloatingOutline,
    /// A colour from the accent palette, shown as a swatch. The name picks
    /// the accent, tinted for the current scheme.
    Swatch(&'static str),
    /// The fixed colours of the colour scheme card previews. They do not
    /// follow the theme, so each card shows its own scheme.
    MockDarkWindow,
    MockDarkBar,
    MockDarkSidebar,
    MockDarkLine,
    MockLightWindow,
    MockLightBar,
    MockLightSidebar,
    MockLightLine,
    /// The inspector's content box, over the hovered app element.
    InspectContent,
    /// The inspector's padding band, over the hovered app element.
    InspectPadding,
    /// The inspector's margin band, over the hovered app element.
    InspectMargin,
    /// The dark tag behind the inspector's size label.
    InspectTag,
    /// The inspector's size label text.
    InspectInk,
    /// The soft shadow under the inspector's hover card.
    InspectShadow,
    /// The opaque tinted fill of a banner or toast of this kind.
    NoticeFill(BannerKind),
    /// The 1 px edge of a banner or toast: the hue at 3:1 on its fill.
    NoticeEdge(BannerKind),
    /// Syntax colours of markup: a tag name, an attribute name, an
    /// attribute value, text and a comment.
    SyntaxTag,
    SyntaxAttr,
    SyntaxValue,
    SyntaxText,
    SyntaxComment,
    /// The fill behind a search hit in a code view, and behind the current hit.
    SearchHit,
    SearchCurrent,
}

/// The tone of a message: information, a warning or an error.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tone {
    Accent,
    Warning,
    Danger,
}

impl Tone {
    /// The tone's colour in the given tokens.
    pub fn color(self, tokens: &Tokens) -> Color {
        match self {
            Tone::Accent => tokens.accent,
            Tone::Warning => tokens.warning,
            Tone::Danger => tokens.danger,
        }
    }
}

impl Paint {
    /// The paint for text and marks drawn on this fill: the derived on-colour
    /// of a coloured fill, else `None`. Callers that take a fill from data
    /// (a tree badge) use it so the label stays readable.
    pub fn on(self) -> Option<Paint> {
        match self {
            Paint::Accent => Some(Paint::OnAccent),
            Paint::Danger => Some(Paint::OnDanger),
            Paint::Success => Some(Paint::OnSuccess),
            Paint::Warning => Some(Paint::OnWarning),
            Paint::Floating => Some(Paint::OnFloating),
            _ => None,
        }
    }

    pub fn color(self, tokens: &Tokens) -> Color {
        match self {
            Paint::Transparent => Color::TRANSPARENT,
            Paint::Shield => Color::from_argb(0, 0, 0, 1),
            Paint::Backdrop => Color::BLACK,
            Paint::Bg => tokens.bg,
            Paint::Surface => tokens.surface,
            Paint::Frosted => tokens.bg.with_af32(0.82),
            Paint::Sidebar => tokens.sidebar,
            Paint::TabStrip => tokens.tab_strip,
            Paint::Card => tokens.card,
            Paint::Control => tokens.control,
            Paint::Raised => tokens.raised,
            Paint::Popover => tokens.popover,
            Paint::Scrim => Color::BLACK.with_af32(0.4),
            Paint::Accent => tokens.accent,
            Paint::FocusRing => tokens.focus_ring,
            Paint::ErrorRing => tokens.error_ring,
            Paint::ErrorHalo => tokens.error_ring.with_af32(0.34),
            Paint::AccentSoft => tokens.accent.with_af32(0.18),
            Paint::OnAccent => tokens.on_accent,
            Paint::OnDanger => tokens.on_danger,
            Paint::OnSuccess => tokens.on_success,
            Paint::OnWarning => tokens.on_warning,
            Paint::OnFloating => tokens.on_floating,
            Paint::Selection => tokens.selection,
            Paint::Hover => tokens.hover,
            Paint::Pressed => tokens.pressed,
            Paint::Separator => tokens.separator,
            Paint::Outline => tokens.outline,
            Paint::Text => tokens.text,
            Paint::Knob => Color::WHITE,
            Paint::Muted => tokens.muted,
            Paint::MutedFaint => tokens.muted.with_a(77),
            Paint::Danger => tokens.danger,
            Paint::DangerSoft => crate::surface::over(tokens.danger.with_af32(0.12), tokens.card),
            Paint::Success => tokens.success,
            Paint::Warning => tokens.warning,
            Paint::Floating => tokens.floating,
            Paint::FloatingOutline => tokens.floating_outline,
            Paint::NoticeFill(kind) => tokens.notice(kind).fill,
            Paint::NoticeEdge(kind) => tokens.notice(kind).edge,
            Paint::Track => tokens.text.with_af32(0.22),
            Paint::Swatch(name) => crate::theme::accent_for(name, tokens.scheme.is_dark()),
            Paint::MockDarkWindow => Color::from_rgb(38, 38, 42),
            Paint::MockDarkBar => Color::from_rgb(56, 56, 62),
            Paint::MockDarkSidebar => Color::from_rgb(47, 47, 53),
            Paint::MockDarkLine => Color::from_rgb(100, 100, 108),
            Paint::MockLightWindow => Color::from_rgb(250, 250, 250),
            Paint::MockLightBar => Color::from_rgb(228, 228, 228),
            Paint::MockLightSidebar => Color::from_rgb(238, 238, 238),
            Paint::MockLightLine => Color::from_rgb(186, 186, 186),
            // Chrome's overlay bands: rgba(111,168,220,.66), rgba(147,196,125,.55)
            // and rgba(246,178,107,.66).
            Paint::InspectContent => Color::from_argb(0xa8, 0x6f, 0xa8, 0xdc),
            Paint::InspectPadding => Color::from_argb(0x8c, 0x93, 0xc4, 0x7d),
            Paint::InspectMargin => Color::from_argb(0xa8, 0xf6, 0xb2, 0x6b),
            Paint::InspectTag => Color::from_argb(0xee, 0x20, 0x21, 0x24),
            Paint::InspectInk => Color::WHITE,
            Paint::InspectShadow => Color::from_argb(0x66, 0x00, 0x00, 0x00),
            Paint::SyntaxTag => tokens.syntax_tag,
            Paint::SyntaxAttr => tokens.syntax_attr,
            Paint::SyntaxValue => tokens.syntax_value,
            Paint::SyntaxText => tokens.syntax_text,
            Paint::SyntaxComment => tokens.syntax_comment,
            Paint::SearchHit => tokens.search_hit,
            Paint::SearchCurrent => tokens.search_current,
        }
    }
}

/// Parses `#rrggbb` or `#rrggbbaa` (the `#` optional), the form CSS writes and
/// the inspector shows and commits.
pub fn colour_from_hex(text: &str) -> Option<Color> {
    let text = text.trim();
    let digits = text.strip_prefix('#').unwrap_or(text);
    if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    match digits.len() {
        6 => Some(colour_from_argb(
            255,
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
        )),
        8 => Some(colour_from_argb(
            value as u8,
            (value >> 24) as u8,
            (value >> 16) as u8,
            (value >> 8) as u8,
        )),
        _ => None,
    }
}
/// The colour as CSS writes it: `#rrggbb` when opaque, `#rrggbbaa` when
/// translucent. `colour_from_hex` reads both back.
pub fn colour_to_hex(colour: Color) -> String {
    if colour.a() == 255 {
        format!("#{:02x}{:02x}{:02x}", colour.r(), colour.g(), colour.b())
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            colour.r(),
            colour.g(),
            colour.b(),
            colour.a()
        )
    }
}
/// A colour from its channels, read from a file or a field at run time.
pub fn colour_from_argb(alpha: u8, red: u8, green: u8, blue: u8) -> Color {
    Color::from_argb(alpha, red, green, blue)
}
/// A colour from hue, saturation and value, chosen in a colour picker.
pub fn colour_from_hsv(hue: f32, saturation: f32, value: f32) -> Color {
    Color::from_hsv(hue, saturation, value)
}

/// A border, by what it draws. All are drawn inside the box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Line {
    /// A 1 px separator-coloured edge all round: a card.
    Hairline,
    /// A 1 px separator line on one side.
    HairlineTop,
    HairlineRight,
    HairlineBottom,
    /// A 1 px outline-coloured edge: an idle control that needs one.
    Outline,
    /// A ring of a `Dim` width in a paint: the spinner's ring.
    Stroke(Dim, Paint),
    /// A 1 px edge in a paint: a notice's tinted edge.
    Edge(Paint),
    /// A 2 px ring in a paint: the selected swatch or preview card.
    Ring(Paint),
}

/// A drop shadow, by how high the box floats.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Elevation {
    /// Menus, dropdown lists, toasts and chart tooltips: 8 px down, 24 px
    /// blur.
    Raised,
    /// Dialogs and popups: 16 px down, 48 px blur.
    Floating,
    /// A slider knob: 1 px down, 3 px blur.
    Knob,
}

impl Elevation {
    /// Offset down, blur and black opacity.
    pub const fn params(self) -> (f32, f32, f32) {
        match self {
            Elevation::Raised => (8.0, 24.0, 0.35),
            Elevation::Floating => (16.0, 48.0, 0.45),
            Elevation::Knob => (1.0, 3.0, 0.4),
        }
    }
}

/// The density of the window. `Regular` is the mouse density of a desktop
/// window; `Compact` is the fingertip density. Every size token resolves
/// against the current class when it is applied (see the `sizes` tables).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeClass {
    Regular,
    Compact,
}

impl SizeClass {
    pub fn is_compact(self) -> bool {
        self == SizeClass::Compact
    }

    /// `regular` at regular density, `compact` at compact density.
    pub fn pick<T>(self, regular: T, compact: T) -> T {
        match self {
            SizeClass::Regular => regular,
            SizeClass::Compact => compact,
        }
    }
}

/// What the latest pointer event came from. A finger latches `Touch`; a real
/// mouse press or move latches `Mouse` again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Mouse,
    Touch,
}

thread_local! {
    // Created once per thread and never dropped, so the handles stay valid for
    // the life of the UI. The globals mirror the window `WindowHost` last
    // rendered, and are read only outside a window's tree. Inside one, the
    // window's own [`WindowDensity`] answers.
    static CLASS: State<SizeClass> = State::create_global(SizeClass::Regular);
    static POINTER: State<PointerKind> = State::create_global(PointerKind::Mouse);
    static WINDOWS: RefCell<HashMap<u64, WindowDensity>> = RefCell::new(HashMap::new());
}

/// The density of one window: its size class, the latest pointer kind, and an
/// optional forced pair that wins over both (a device preview). `WindowHost`
/// provides it to the window's tree and writes the first two from the window
/// width and the pointer. A window without a host, such as the inspector, keeps
/// the value it was made with. Every field is a `State`, so a reader re-renders
/// when the density changes.
#[derive(Clone, Copy)]
pub struct WindowDensity {
    class: State<SizeClass>,
    pointer: State<PointerKind>,
    forced: State<Option<(SizeClass, PointerKind)>>,
}

impl WindowDensity {
    pub fn new(class: SizeClass, pointer: PointerKind) -> Self {
        Self {
            class: State::create_global(class),
            pointer: State::create_global(pointer),
            forced: State::create_global(None),
        }
    }

    /// The class this window renders at, forced or measured. Reading it
    /// subscribes the caller.
    pub fn class(self) -> SizeClass {
        match *self.forced.read() {
            Some((class, _)) => class,
            None => *self.class.read(),
        }
    }

    /// The pointer kind this window renders for, forced or latched.
    pub fn pointer(self) -> PointerKind {
        match *self.forced.read() {
            Some((_, pointer)) => pointer,
            None => *self.pointer.read(),
        }
    }

    /// The pointer kind of the latest pointer event in this window, ignoring
    /// any forced pair. Reading it subscribes the caller.
    pub(crate) fn latched_pointer(self) -> PointerKind {
        *self.pointer.read()
    }

    /// The class and pointer kind forced on this window, if any.
    pub fn forced(self) -> Option<(SizeClass, PointerKind)> {
        *self.forced.read()
    }

    /// Set the class this window renders at. Subscribers are marked at once.
    pub(crate) fn set_class(self, class: SizeClass) {
        let mut state = self.class;
        state.set_if_modified(class);
    }

    /// Record the kind of the latest pointer event in this window.
    pub(crate) fn latch_pointer(self, kind: PointerKind) {
        let mut state = self.pointer;
        state.set_if_modified(kind);
    }
}

/// The density of the window with this key (the `u64` the platform knows the
/// window by), made on first use and kept for the life of the UI.
pub fn window_density(key: impl Into<u64>) -> WindowDensity {
    WINDOWS.with(|windows| {
        *windows
            .borrow_mut()
            .entry(key.into())
            .or_insert_with(|| WindowDensity::new(SizeClass::Regular, PointerKind::Mouse))
    })
}

/// Force the density of a window to `forced`, or release it with `None`. A
/// forced window renders at the forced class and pointer kind wherever its
/// width and pointer would say otherwise.
pub fn force_density(key: impl Into<u64>, forced: Option<(SizeClass, PointerKind)>) {
    let mut state = window_density(key).forced;
    state.set_if_modified(forced);
}

/// The current size class. Reading it while a component renders subscribes
/// that component, so it re-renders in place when the class changes.
pub fn size_class() -> SizeClass {
    match try_consume_context::<WindowDensity>() {
        Some(density) => density.class(),
        None => CLASS.with(|class| *class.read()),
    }
}

/// Whether the current size class is compact.
pub fn is_compact() -> bool {
    size_class().is_compact()
}

/// Whether the window this tree renders in is held at compact density by a
/// device preview, whatever its width. Reading it subscribes the caller.
pub fn device_preview() -> bool {
    try_consume_context::<WindowDensity>()
        .is_some_and(|density| matches!(density.forced(), Some((SizeClass::Compact, _))))
}

/// The width the page of a window `window_width` px wide is drawn at: the
/// phone preview's column in a device preview, which holds the page to a
/// phone-wide column in the middle of the window, and the whole window
/// otherwise. What a window lays out against the page's edges (a bottom
/// sheet) uses it, so a sheet is as wide as the page and not the window.
pub fn page_column(window_width: f32) -> f32 {
    if device_preview() {
        window_width.min(crate::theme::PHONE_PREVIEW_SIZE.0)
    } else {
        window_width
    }
}

/// `regular` at regular density, `compact` at compact density, read now.
pub fn pick<T>(regular: T, compact: T) -> T {
    size_class().pick(regular, compact)
}

/// The kind of the latest pointer event in the current window.
pub fn pointer_kind() -> PointerKind {
    match try_consume_context::<WindowDensity>() {
        Some(density) => density.pointer(),
        None => POINTER.with(|pointer| *pointer.read()),
    }
}

/// Mirror the density of the window `WindowHost` just rendered into the
/// globals, for readers outside any window tree.
pub(crate) fn mirror_density(class: SizeClass, pointer: PointerKind) {
    CLASS.with(|state| {
        let mut state = *state;
        state.set_if_modified(class);
    });
    POINTER.with(|state| {
        let mut state = *state;
        state.set_if_modified(pointer);
    });
}

#[cfg(test)]
mod touch_tests {
    use super::*;
    use crate::style::catalogue::dims;

    /// Every control size that a finger presses, at compact density, is at
    /// least the touch minimum, and at no density does it shrink going
    /// from regular to compact.
    #[test]
    fn every_touch_target_token_reaches_the_touch_minimum_at_compact_density() {
        let table = dims(SizeClass::Compact);
        let mut walked = 0;
        for (name, dim) in table {
            if !dim.is_touch_target() {
                continue;
            }
            walked += 1;
            assert!(
                dim.px_in(SizeClass::Compact) >= TOUCH_MIN,
                "{name} is {} px at compact density, under the {TOUCH_MIN} px touch minimum",
                dim.px_in(SizeClass::Compact)
            );
            assert!(
                dim.px_in(SizeClass::Compact) >= dim.px_in(SizeClass::Regular),
                "{name} shrinks at compact density"
            );
        }
        // The walk saw the controls it is meant to hold: a catalogue that
        // dropped them would pass with nothing checked.
        assert!(walked >= 11, "only {walked} touch targets are in the catalogue");
    }

    #[test]
    fn the_header_button_and_back_are_touch_targets() {
        assert!(HEADER_BUTTON.is_touch_target());
        assert!(Dim::WindowControlTarget.is_touch_target());
        assert!(HEADER_BUTTON.px_in(SizeClass::Compact) >= TOUCH_MIN);
        assert!(Dim::WindowControlTarget.px_in(SizeClass::Compact) >= TOUCH_MIN);
    }

    #[test]
    fn the_phone_page_column_is_narrower_than_the_desktop_one() {
        assert!(Dim::PagePhone.px_in(SizeClass::Regular) < Dim::Page.px_in(SizeClass::Regular));
    }

    /// A field's trailing button is the field less the inset on every side,
    /// and a one-line field row is as tall as any other row.
    #[test]
    fn field_insets_fit_the_field_and_the_row() {
        for class in [SizeClass::Regular, SizeClass::Compact] {
            assert_eq!(
                Dim::Control.px_in(class) - Dim::InlineButton.px_in(class),
                2.0 * FIELD_INSET.px_in(class),
                "{class:?}: the inline button is the field less two insets"
            );
            assert_eq!(
                Dim::Row.px_in(class) - Dim::Control.px_in(class),
                2.0 * FIELD_ROW_INSET.px_in(class),
                "{class:?}: a field row is a row tall"
            );
        }
    }

    #[test]
    fn a_nested_radius_is_the_outer_less_the_inset() {
        assert_eq!(Radius::Control.nested(0.0), Radius::Control);
        assert_eq!(Radius::Control.nested(4.0), Radius::Inner);
        assert_eq!(Radius::Card.nested(ROW_INSET.px_in(SizeClass::Regular)), Radius::Inner);
        assert_eq!(Radius::Control.nested(2.0), Radius::Inner);
        assert_eq!(Radius::Mark.nested(2.0), Radius::Mark);
    }
}
