//! The tokens: every spacing, radius, size, colour and line Bingux UI draws.
//!
//! Each kind of value is its own type with a fixed set of values. A page or
//! component cannot pass a number where a token is expected, so a value that
//! is not on the scale does not compile. Read a token's number with `px()`
//! only for geometry (a chart, a drag offset), never to style a box: boxes
//! are styled with [`super::Tw::tw`].
//!
//! On a phone-width window ([`set_touch`]) the row, control, button and
//! sidebar sizes grow to fingertip size. [`Dim`] resolves them when it is
//! applied, so a style written once is right at both sizes.

use std::sync::atomic::{AtomicBool, Ordering};

use freya::prelude::Color;

use crate::theme::Tokens;

/// A space on the 4 px grid, named as Tailwind names it: `S4` is 16 px, as
/// `p-4` is. `S0_5` is the half step between a title and its subtitle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Space(f32);

impl Space {
    pub const fn px(self) -> f32 {
        self.0
    }
}

pub const S0: Space = Space(0.0);
pub const S0_5: Space = Space(2.0);
pub const S1: Space = Space(4.0);
pub const S2: Space = Space(8.0);
pub const S3: Space = Space(12.0);
pub const S4: Space = Space(16.0);
pub const S5: Space = Space(20.0);
pub const S6: Space = Space(24.0);
pub const S7: Space = Space(28.0);
pub const S8: Space = Space(32.0);
pub const S10: Space = Space(40.0);
pub const S12: Space = Space(48.0);

/// The one horizontal inset inside a card, a header bar and the page edge
/// on a phone: row text, hairlines and controls start this far in.
pub const GUTTER: Space = S4;

/// How far a disabled control or menu item fades: clearly unavailable, still
/// readable.
pub const DISABLED_OPACITY: f32 = 0.45;

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
}

/// Text sizes. Only `typography.rs` reads them: text is set with a
/// [`crate::typography::TextStyle`], never a size.
pub(crate) const TEXT_XS: f32 = 11.0;
pub(crate) const TEXT_SM: f32 = 13.0;
pub(crate) const TEXT_MD: f32 = 14.0;
pub(crate) const TEXT_LG: f32 = 16.0;
pub(crate) const TEXT_HEADING: f32 = 18.0;
pub(crate) const TEXT_DISPLAY: f32 = 28.0;
pub(crate) const TEXT_HERO: f32 = 36.0;

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
    pub const fn px(self) -> f32 {
        match self {
            IconSize::Mark => 12.0,
            IconSize::Text => 14.0,
            IconSize::Control => 16.0,
            IconSize::Row => 18.0,
            IconSize::Hero => 48.0,
            IconSize::AppSmall => 20.0,
            IconSize::App => 32.0,
            IconSize::AppLarge => 64.0,
        }
    }
}

/// A width or height, by what it sizes. The interactive sizes follow
/// [`touch`].
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
    /// 32 px target. Back in the header uses the same target.
    WindowControl,
    WindowControlTarget,
    /// A field, dropdown or slider in a row: 240 px, at most 168 px on a
    /// phone.
    Field,
    /// A field for a URL, path or name: 440 px, at most 168 px on a phone.
    FieldWide,
    /// A dropdown whose options carry application icons: 260 px.
    FieldApps,
    /// A menu of actions: 220 px.
    Menu,
    /// A dialog card: 400 px.
    Dialog,
    /// A popup's width: 520 px.
    PopupWidth,
    /// A popup's height until it asks for another: 600 px. Its body scrolls.
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
    /// The sidebar: 280 px.
    Sidebar,
    /// The page column at its widest: 720 px.
    Page,
    /// Square boxes, named by Tailwind size step: 20, 28, 36, 44 and 96 px
    /// (`size-5`, `size-7`, `size-9`, `size-11`, `size-24`).
    Box5,
    Box7,
    Box9,
    Box11,
    Box24,
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
    /// The speaker test popup: 440 px wide and tall.
    SpeakerPopupWidth,
    SpeakerPopupHeight,
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
    /// A hairline: 1 px.
    Hairline,
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
}

impl Dim {
    /// The size in logical pixels at the current density.
    pub fn px(self) -> f32 {
        let touch = touch();
        let pick = |mouse: f32, finger: f32| if touch { finger } else { mouse };
        match self {
            Dim::Row => pick(56.0, 60.0),
            Dim::RowTall => 72.0,
            Dim::RowSubtitle => 63.0,
            Dim::Control => pick(36.0, 44.0),
            Dim::Button => pick(40.0, 48.0),
            Dim::MenuItem => 36.0,
            Dim::MenuSeparator => 8.0,
            Dim::NavItem => pick(40.0, 56.0),
            Dim::NavIcon => 24.0,
            Dim::Header => 48.0,
            Dim::WindowControl => 24.0,
            Dim::WindowControlTarget => 32.0,
            Dim::Field => pick(240.0, FIELD_TOUCH),
            Dim::FieldWide => pick(440.0, FIELD_TOUCH),
            Dim::FieldApps => pick(260.0, FIELD_TOUCH),
            Dim::Menu => 220.0,
            Dim::Dialog => 400.0,
            Dim::PopupWidth => 520.0,
            Dim::PopupHeight => 600.0,
            Dim::DropdownMenu => 240.0,
            Dim::DropdownMax => 360.0,
            Dim::MenuList => 320.0,
            Dim::SwitchWidth => pick(36.0, 48.0),
            Dim::SwitchHeight => pick(20.0, 28.0),
            Dim::SwitchThumb => pick(16.0, 22.0),
            Dim::SwitchThumbPressed => pick(18.0, 24.0),
            Dim::SliderTrack => 6.0,
            Dim::SliderTrackActive => 8.0,
            Dim::SliderThumb => pick(16.0, 22.0),
            Dim::StepperHalf => 28.0,
            Dim::Sidebar => 280.0,
            Dim::Page => 720.0,
            Dim::Box5 => 20.0,
            Dim::Box7 => 28.0,
            Dim::Box9 => 36.0,
            Dim::Box11 => 44.0,
            Dim::Box24 => 96.0,
            Dim::PreviewWidth => 144.0,
            Dim::PreviewHeight => 96.0,
            Dim::PreviewRow => 464.0,
            Dim::PreviewSidebar => 40.0,
            Dim::PreviewLine => 40.0,
            Dim::IconRow => 18.0,
            Dim::IconControl => 16.0,
            Dim::Dense => 28.0,
            Dim::DenseControl => 24.0,
            Dim::PropLabel => 120.0,
            Dim::FlameRow => 20.0,
            Dim::Timeline => 72.0,
            Dim::DenseField => 140.0,
            Dim::PreviewBar => 4.0,
            Dim::PreviewTitleBar => 16.0,
            Dim::Chart => 120.0,
            Dim::SpeakerRoom => 360.0,
            Dim::SpeakerButton => 56.0,
            Dim::SpeakerPopupWidth => 480.0,
            Dim::SpeakerPopupHeight => 480.0,
            Dim::ArrangementWidth => 360.0,
            Dim::ArrangementHeight => 240.0,
            Dim::ArrangementHint => 260.0,
            Dim::SkeletonTitle => 120.0,
            Dim::SkeletonLabel => 180.0,
            Dim::SkeletonValue => 96.0,
            Dim::SkeletonLine => TEXT_SM,
            Dim::CaptionLine => (TEXT_SM * 1.45).ceil(),
            Dim::StripCell => 76.0,
            Dim::IconBox => 18.0,
            Dim::SignalBar => 3.0,
            Dim::Ring => 2.0,
            Dim::Hairline => 1.0,
            Dim::Check => 18.0,
            Dim::Track => 6.0,
            Dim::SpinnerMedium => 3.0,
            Dim::Dot => 8.0,
            Dim::DashHeight => 2.0,
        }
    }
}

/// The widest a field, dropdown or slider in a row gets on a phone, so the
/// row's text keeps room.
const FIELD_TOUCH: f32 = 168.0;

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
    /// The clear layer behind a menu or dropdown list. It paints nothing,
    /// but Freya lets presses, the wheel and hover reach a box below only
    /// through a box whose fill is exactly `Color::TRANSPARENT`; this one is
    /// not, so the window behind the open menu takes no input.
    Shield,
    Accent,
    /// The accent at low strength, behind a badge.
    AccentSoft,
    /// Text and marks drawn on the accent.
    OnAccent,
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
    /// A tone at 16%: the fill of a banner.
    Tint(Tone),
    /// A tone at 28%: the edge of a banner.
    TintEdge(Tone),
}

/// The tone of a message: information, a warning or an error.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tone {
    Accent,
    Warning,
    Danger,
}

impl Tone {
    fn color(self, tokens: &Tokens) -> Color {
        match self {
            Tone::Accent => tokens.accent,
            Tone::Warning => tokens.warning,
            Tone::Danger => tokens.danger,
        }
    }
}

impl Paint {
    pub fn color(self, tokens: &Tokens) -> Color {
        match self {
            Paint::Transparent => Color::TRANSPARENT,
            Paint::Shield => Color::from_argb(0, 0, 0, 1),
            Paint::Bg => tokens.bg,
            Paint::Surface => tokens.surface,
            Paint::Frosted => tokens.bg.with_af32(0.82),
            Paint::Sidebar => tokens.sidebar,
            Paint::Card => tokens.card,
            Paint::Control => tokens.control,
            Paint::Raised => tokens.raised,
            Paint::Popover => tokens.popover,
            Paint::Scrim => Color::BLACK.with_af32(0.4),
            Paint::Accent => tokens.accent,
            Paint::AccentSoft => tokens.accent.with_af32(0.18),
            Paint::OnAccent => tokens.accent_foreground,
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
            Paint::Tint(tone) => tone.color(tokens).with_af32(0.16),
            Paint::TintEdge(tone) => tone.color(tokens).with_af32(0.28),
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
        }
    }
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
    /// A 1 px edge in a paint: a banner's tinted edge.
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
    /// A page sliding over the one under it: no offset, 24 px blur. The
    /// caller scales its opacity along the slide.
    Sheet,
    /// A slider knob: 1 px down, 3 px blur.
    Knob,
}

impl Elevation {
    /// Offset down, blur and black opacity.
    pub const fn params(self) -> (f32, f32, f32) {
        match self {
            Elevation::Raised => (8.0, 24.0, 0.35),
            Elevation::Floating => (16.0, 48.0, 0.45),
            Elevation::Sheet => (0.0, 24.0, 0.22),
            Elevation::Knob => (1.0, 3.0, 0.4),
        }
    }
}

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
