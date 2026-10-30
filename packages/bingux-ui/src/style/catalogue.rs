//! The token catalogue: every token by name, in order, with the reverse
//! lookups a tool needs.
//!
//! Styling code names tokens in source (`gap(S3)`, `rounded(Radius::Card)`).
//! A tool that works on a running UI, such as the inspector, goes the other
//! way: it has a resolved number or colour and must say which token it is, and
//! it offers the set of tokens to pick from. Both need one list of names, kept
//! beside the tokens so a new token is added in one place.
//!
//! Lookups take the [`SizeClass`] the value was resolved at, because a size
//! token has a regular and a compact value and the window being inspected may
//! not be at the inspector's own density.

use freya::prelude::{Color, FontWeight};

use super::*;
use crate::theme::{BannerKind, Tokens};
use crate::typography::{
    BADGE, BODY, CAPTION, CODE, EMPHASIS, HEADING, HERO, LINK, NUMERIC, OVERLINE, PAGE_TITLE,
    SECTION, TITLE, TextStyle, VALUE,
};

/// The space steps, in order.
pub const SPACES: &[(&str, Space)] = &[
    ("S0", S0),
    ("S0_5", S0_5),
    ("S1", S1),
    ("S2", S2),
    ("S3", S3),
    ("S4", S4),
    ("S5", S5),
    ("S6", S6),
    ("S7", S7),
    ("S8", S8),
    ("S10", S10),
    ("S12", S12),
];

/// The corner radii. `Window` is left out: it equals `Control` in value and is
/// only the shell's own frame.
pub const RADII: &[(&str, Radius)] = &[
    ("Mark", Radius::Mark),
    ("Inner", Radius::Inner),
    ("Control", Radius::Control),
    ("Card", Radius::Card),
    ("Overlay", Radius::Overlay),
    ("Tile", Radius::Tile),
    ("Pill", Radius::Pill),
];

/// The box sizes, by name. Sorted by value at lookup time with [`dims`].
const DIMS: &[(&str, Dim)] = &[
    ("Hairline", Dim::Hairline),
    ("SafeBottom", Dim::SafeBottom),
    ("Indicator", Dim::Indicator),
    ("Ring", Dim::Ring),
    ("DashHeight", Dim::DashHeight),
    ("SpinnerMedium", Dim::SpinnerMedium),
    ("SignalBar", Dim::SignalBar),
    ("Dot", Dim::Dot),
    ("Track", Dim::Track),
    ("SliderTrack", Dim::SliderTrack),
    ("SliderTrackActive", Dim::SliderTrackActive),
    ("MenuSeparator", Dim::MenuSeparator),
    ("PickerPlane", Dim::PickerPlane),
    ("PickerKnob", Dim::PickerKnob),
    ("PickerHandle", Dim::PickerHandle),
    ("PickerSwatch", Dim::PickerSwatch),
    ("PickerWidth", Dim::PickerWidth),
    ("SliderTrackLarge", Dim::SliderTrackLarge),
    ("SliderThumbLarge", Dim::SliderThumbLarge),
    ("SwitchThumb", Dim::SwitchThumb),
    ("SwitchThumbPressed", Dim::SwitchThumbPressed),
    ("SliderThumb", Dim::SliderThumb),
    ("Swatch", Dim::Swatch),
    ("Check", Dim::Check),
    ("IconBox", Dim::IconBox),
    ("IconRow", Dim::IconRow),
    ("IconControl", Dim::IconControl),
    ("Box5", Dim::Box5),
    ("SwitchHeight", Dim::SwitchHeight),
    ("DenseControl", Dim::DenseControl),
    ("NavIcon", Dim::NavIcon),
    ("WindowControl", Dim::WindowControl),
    ("FlameRow", Dim::FlameRow),
    ("CodeRow", Dim::CodeRow),
    ("FlatField", Dim::FlatField),
    ("Dense", Dim::Dense),
    ("Box7", Dim::Box7),
    ("StepperHalf", Dim::StepperHalf),
    ("InlineButton", Dim::InlineButton),
    ("WindowControlTarget", Dim::WindowControlTarget),
    ("Control", Dim::Control),
    ("Box9", Dim::Box9),
    ("SwitchWidth", Dim::SwitchWidth),
    ("MenuItem", Dim::MenuItem),
    ("NavItem", Dim::NavItem),
    ("Button", Dim::Button),
    ("PreviewLine", Dim::PreviewLine),
    ("PreviewSidebar", Dim::PreviewSidebar),
    ("PreviewBar", Dim::PreviewBar),
    ("PreviewTitleBar", Dim::PreviewTitleBar),
    ("Box11", Dim::Box11),
    ("Header", Dim::Header),
    ("SpeakerButton", Dim::SpeakerButton),
    ("Row", Dim::Row),
    ("RowSubtitle", Dim::RowSubtitle),
    ("RowTall", Dim::RowTall),
    ("RowButton", Dim::RowButton),
    ("StripCell", Dim::StripCell),
    ("Timeline", Dim::Timeline),
    ("Box24", Dim::Box24),
    ("TabMin", Dim::TabMin),
    ("TabBar", Dim::TabBar),
    ("TabButton", Dim::TabButton),
    ("TextFade", Dim::TextFade),
    ("Label", Dim::Label),
    ("PropLabel", Dim::PropLabel),
    ("SkeletonTitle", Dim::SkeletonTitle),
    ("Chart", Dim::Chart),
    ("DenseField", Dim::DenseField),
    ("FieldColour", Dim::FieldColour),
    ("PreviewWidth", Dim::PreviewWidth),
    ("PreviewHeight", Dim::PreviewHeight),
    ("SkeletonValue", Dim::SkeletonValue),
    ("SkeletonLabel", Dim::SkeletonLabel),
    ("SkeletonLine", Dim::SkeletonLine),
    ("CaptionLine", Dim::CaptionLine),
    ("Field", Dim::Field),
    ("FieldApps", Dim::FieldApps),
    ("FieldMin", Dim::FieldMin),
    ("ArrangementHint", Dim::ArrangementHint),
    ("Menu", Dim::Menu),
    ("MenuWide", Dim::MenuWide),
    ("DropdownMenu", Dim::DropdownMenu),
    ("ArrangementHeight", Dim::ArrangementHeight),
    ("MenuList", Dim::MenuList),
    ("Sidebar", Dim::Sidebar),
    ("SpeakerRoom", Dim::SpeakerRoom),
    ("ArrangementWidth", Dim::ArrangementWidth),
    ("DropdownMax", Dim::DropdownMax),
    ("Dialog", Dim::Dialog),
    ("DialogMin", Dim::DialogMin),
    ("FieldWide", Dim::FieldWide),
    ("PreviewRow", Dim::PreviewRow),
    ("SpeakerPopupWidth", Dim::SpeakerPopupWidth),
    ("PopupWidth", Dim::PopupWidth),
    ("PopupNarrow", Dim::PopupNarrow),
    ("PopupHeight", Dim::PopupHeight),
    ("Page", Dim::Page),
    ("PagePhone", Dim::PagePhone),
];

/// The box sizes at `class`, smallest first, so a list of them reads as a
/// scale. Sizes of equal value keep the order they are written in, so the
/// first of a run is the one a reverse lookup names.
pub fn dims(class: SizeClass) -> Vec<(&'static str, Dim)> {
    let mut list = DIMS.to_vec();
    list.sort_by(|a, b| a.1.px_in(class).total_cmp(&b.1.px_in(class)));
    list
}

/// The colour tokens, in the order a reverse lookup tries them: the surfaces
/// first, so a colour two tokens share is named by the plainer one.
pub const PAINTS: &[(&str, Paint)] = &[
    ("Transparent", Paint::Transparent),
    ("Bg", Paint::Bg),
    ("Surface", Paint::Surface),
    ("Sidebar", Paint::Sidebar),
    ("Recess", Paint::Recess),
    ("Card", Paint::Card),
    ("Control", Paint::Control),
    ("Raised", Paint::Raised),
    ("Popover", Paint::Popover),
    ("Frosted", Paint::Frosted),
    ("Accent", Paint::Accent),
    ("FocusRing", Paint::FocusRing),
    ("ErrorRing", Paint::ErrorRing),
    ("ErrorHalo", Paint::ErrorHalo),
    ("AccentSoft", Paint::AccentSoft),
    ("OnAccent", Paint::OnAccent),
    ("OnDanger", Paint::OnDanger),
    ("OnSuccess", Paint::OnSuccess),
    ("OnWarning", Paint::OnWarning),
    ("OnFloating", Paint::OnFloating),
    ("Selection", Paint::Selection),
    ("Hover", Paint::Hover),
    ("Pressed", Paint::Pressed),
    ("Separator", Paint::Separator),
    ("Outline", Paint::Outline),
    ("Text", Paint::Text),
    ("Muted", Paint::Muted),
    ("MutedFaint", Paint::MutedFaint),
    ("Danger", Paint::Danger),
    ("DangerSoft", Paint::DangerSoft),
    ("Success", Paint::Success),
    ("Warning", Paint::Warning),
    ("Floating", Paint::Floating),
    ("FloatingOutline", Paint::FloatingOutline),
    ("Scrim", Paint::Scrim),
    ("Knob", Paint::Knob),
    ("Track", Paint::Track),
    ("NoticeFill(Neutral)", Paint::NoticeFill(BannerKind::Neutral)),
    ("NoticeFill(Info)", Paint::NoticeFill(BannerKind::Info)),
    ("NoticeFill(Success)", Paint::NoticeFill(BannerKind::Success)),
    ("NoticeFill(Warning)", Paint::NoticeFill(BannerKind::Warning)),
    ("NoticeFill(Error)", Paint::NoticeFill(BannerKind::Error)),
    ("SyntaxTag", Paint::SyntaxTag),
    ("SyntaxAttr", Paint::SyntaxAttr),
    ("SyntaxValue", Paint::SyntaxValue),
    ("SyntaxText", Paint::SyntaxText),
    ("SyntaxComment", Paint::SyntaxComment),
];

/// The shadows a box can float with.
pub const ELEVATIONS: &[(&str, Elevation)] = &[
    ("Raised", Elevation::Raised),
    ("Floating", Elevation::Floating),
    ("Knob", Elevation::Knob),
];

/// The alignments `items` and `justify` take.
pub const ALIGNS: &[(&str, Align)] = &[
    ("Start", Align::Start),
    ("Center", Align::Center),
    ("End", Align::End),
    ("Between", Align::Between),
    ("Around", Align::Around),
];

/// The type ramp, largest first.
pub const TEXT_STYLES: &[(&str, TextStyle)] = &[
    ("HERO", HERO),
    ("TITLE", TITLE),
    ("PAGE_TITLE", PAGE_TITLE),
    ("HEADING", HEADING),
    ("SECTION", SECTION),
    ("EMPHASIS", EMPHASIS),
    ("BODY", BODY),
    ("VALUE", VALUE),
    ("NUMERIC", NUMERIC),
    ("LINK", LINK),
    ("CAPTION", CAPTION),
    ("BADGE", BADGE),
    ("OVERLINE", OVERLINE),
    ("CODE", CODE),
];

/// The borders a box can draw, by name.
pub const LINES: &[(&str, Line)] = &[
    ("Hairline", Line::Hairline),
    ("HairlineTop", Line::HairlineTop),
    ("HairlineRight", Line::HairlineRight),
    ("HairlineBottom", Line::HairlineBottom),
    ("Outline", Line::Outline),
    ("Edge(Accent)", Line::Edge(Paint::Accent)),
    ("Ring(Accent)", Line::Ring(Paint::Accent)),
];

pub fn space(name: &str) -> Option<Space> {
    SPACES.iter().find(|(key, _)| *key == name).map(|(_, v)| *v)
}

pub fn radius(name: &str) -> Option<Radius> {
    RADII.iter().find(|(key, _)| *key == name).map(|(_, v)| *v)
}

pub fn dim(name: &str) -> Option<Dim> {
    DIMS.iter().find(|(key, _)| *key == name).map(|(_, v)| *v)
}

pub fn paint(name: &str) -> Option<Paint> {
    PAINTS.iter().find(|(key, _)| *key == name).map(|(_, v)| *v)
}

pub fn elevation(name: &str) -> Option<Elevation> {
    ELEVATIONS
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, v)| *v)
}

pub fn align(name: &str) -> Option<Align> {
    ALIGNS.iter().find(|(key, _)| *key == name).map(|(_, v)| *v)
}

pub fn text_style(name: &str) -> Option<TextStyle> {
    TEXT_STYLES
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, v)| *v)
}

pub fn line(name: &str) -> Option<Line> {
    LINES.iter().find(|(key, _)| *key == name).map(|(_, v)| *v)
}

/// Whether two sizes are the same to the pixel grid the layout works on.
fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

/// The space token worth `px` at `class`.
pub fn space_name(px: f32, class: SizeClass) -> Option<&'static str> {
    SPACES
        .iter()
        .find(|(_, space)| near(space.px_in(class), px))
        .map(|(name, _)| *name)
}

/// The radius token worth `px`. `Control` is named before `Window`.
pub fn radius_name(px: f32) -> Option<&'static str> {
    RADII
        .iter()
        .find(|(_, radius)| near(radius.px(), px))
        .map(|(name, _)| *name)
}

/// The box size worth `px` at `class`.
pub fn dim_name(px: f32, class: SizeClass) -> Option<&'static str> {
    dims(class)
        .into_iter()
        .find(|(_, dim)| near(dim.px_in(class), px))
        .map(|(name, _)| name)
}

/// Every box size worth `px` at `class`, the first of which `dim_name` names.
pub fn dim_names(px: f32, class: SizeClass) -> Vec<&'static str> {
    dims(class)
        .into_iter()
        .filter(|(_, dim)| near(dim.px_in(class), px))
        .map(|(name, _)| name)
        .collect()
}

/// The colour token that paints `colour` in `tokens`.
pub fn paint_name(colour: Color, tokens: &Tokens) -> Option<&'static str> {
    PAINTS
        .iter()
        .find(|(_, paint)| paint.color(tokens) == colour)
        .map(|(name, _)| *name)
}

/// The type style with this size, weight and face at `class`. Styles that
/// differ only in ink or tracking share a name; the first listed wins.
pub fn text_style_name(
    size: f32,
    weight: FontWeight,
    mono: bool,
    class: SizeClass,
) -> Option<&'static str> {
    TEXT_STYLES
        .iter()
        .find(|(_, style)| {
            near(style.size.px_in(class), size) && style.weight == weight && style.mono == mono
        })
        .map(|(name, _)| *name)
}

/// The regular text weight, what text has when it declares none.
pub const REGULAR: FontWeight = FontWeight::NORMAL;

/// A weight by name, or its number when it has none.
pub fn weight_text(weight: FontWeight) -> String {
    let named = [
        (FontWeight::LIGHT, "light"),
        (FontWeight::NORMAL, "regular"),
        (FontWeight::MEDIUM, "medium"),
        (FontWeight::SEMI_BOLD, "semibold"),
        (FontWeight::BOLD, "bold"),
    ];
    match named.iter().find(|(w, _)| *w == weight) {
        Some((_, name)) => name.to_string(),
        None => format!("{weight:?}"),
    }
}
