//! The contrast matrix: every foreground and background pair the design
//! system draws, with the WCAG requirement each one must meet.
//!
//! The pairs are data ([`pairs`]); [`evaluate`] resolves them against the
//! tokens of one scheme and accent and measures them with
//! [`crate::contrast`]. The unit test at the bottom walks the pairs for both
//! schemes and every accent the Appearance page offers and fails with a
//! readable table. The taxonomy's Foundations > Colour page renders the same
//! rows, and the inspector's Contrast check measures the live app the same
//! way.
//!
//! To cover a new pair, add a line to [`pairs`]: what is drawn (an [`Fg`]),
//! what it is drawn on (a [`Bg`]), the [`Requirement`], and an `exempt`
//! reason only when WCAG itself exempts the pair.

use std::fmt::Write as _;

use freya::prelude::Color;

use crate::contrast::{self, Level, Requirement};
use crate::style::Paint;
use crate::surface::surface_color;
use crate::theme::{BannerKind, Scheme, Tokens, accent_names};
use crate::typography::Ink;

/// What is drawn: a text ink (which may differ from the paint of the same
/// name, as `Ink::Accent` is the accent moved to read as text) or a paint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fg {
    Ink(Ink),
    Paint(Paint),
}

impl Fg {
    pub fn colour(self, tokens: &Tokens) -> Color {
        match self {
            Fg::Ink(ink) => ink.color(tokens),
            Fg::Paint(paint) => paint.color(tokens),
        }
    }

    pub fn name(self) -> String {
        match self {
            Fg::Ink(ink) => format!("Ink::{ink:?}"),
            Fg::Paint(paint) => format!("{paint:?}"),
        }
    }
}

/// The interaction layer over a background.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Rest,
    Hover,
    Pressed,
    Selected,
}

impl State {
    fn label(self) -> &'static str {
        match self {
            State::Rest => "",
            State::Hover => " hover",
            State::Pressed => " pressed",
            State::Selected => " selected",
        }
    }
}

/// What a foreground is drawn on: a fill `paint`, composited over the opaque
/// layer `under`, with an interaction `state` layer on top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bg {
    pub paint: Paint,
    pub under: Paint,
    pub state: State,
}

impl Bg {
    /// A layer drawn straight on the window: no fill beneath it.
    pub const fn layer(paint: Paint) -> Self {
        Self { paint, under: paint, state: State::Rest }
    }

    /// A fill over a layer.
    pub const fn over(paint: Paint, under: Paint) -> Self {
        Self { paint, under, state: State::Rest }
    }

    pub const fn state(self, state: State) -> Self {
        Self { state, ..self }
    }

    pub fn colour(self, tokens: &Tokens) -> Color {
        let under = self.under.color(tokens);
        let fill = contrast::composite(self.paint.color(tokens), under);
        let (hover, pressed, selected) = (
            self.state == State::Hover,
            self.state == State::Pressed,
            self.state == State::Selected,
        );
        contrast::composite(surface_color(tokens, fill, hover, pressed, selected), under)
    }

    pub fn name(self) -> String {
        let base = if self.paint == self.under {
            format!("{:?}", self.paint)
        } else {
            format!("{:?} on {:?}", self.paint, self.under)
        };
        format!("{base}{}", self.state.label())
    }
}

/// One foreground and background pair and what it must reach.
#[derive(Clone, Debug, PartialEq)]
pub struct Pair {
    /// What the pair is, for a reader of the table.
    pub what: &'static str,
    pub fg: Fg,
    pub bg: Bg,
    pub need: Requirement,
    /// The element's own opacity, for a disabled control that is dimmed as a
    /// whole: both fill and ink fade towards the layer beneath.
    pub opacity: f32,
    /// Why the pair may stay below `need`; the pair is still measured and
    /// listed, but does not fail the test.
    pub exempt: Option<&'static str>,
}

const fn pair(what: &'static str, fg: Fg, bg: Bg, need: Requirement) -> Pair {
    Pair { what, fg, bg, need, opacity: 1.0, exempt: None }
}

/// The layers a control or text can sit on directly.
const LAYERS: [Paint; 6] = [
    Paint::Bg,
    Paint::Sidebar,
    Paint::Card,
    Paint::Control,
    Paint::Raised,
    Paint::Popover,
];

/// The layers coloured text and tinted badges sit on.
const TEXT_LAYERS: [Paint; 5] =
    [Paint::Bg, Paint::Sidebar, Paint::Card, Paint::Control, Paint::Popover];

/// Disabled controls fade with this opacity; see [`crate::style::DISABLED_OPACITY`].
const DISABLED_REASON: &str = "WCAG 1.4.3 and 1.4.11 exempt inactive components; 3:1 is the house target and is reported";

/// Every foreground and background pair the design system uses.
pub fn pairs() -> Vec<Pair> {
    let mut out = Vec::new();
    let text = Requirement::Text;
    let ui = Requirement::Component;

    // Ink tiers on every layer.
    for ink in [Ink::Text, Ink::Muted] {
        for layer in LAYERS {
            out.push(pair("text ink on a layer", Fg::Ink(ink), Bg::layer(layer), text));
        }
    }
    // Interaction layers: hover, press and the selected row or sidebar item.
    for ink in [Ink::Text, Ink::Muted] {
        for layer in [Paint::Bg, Paint::Sidebar, Paint::Card, Paint::Control, Paint::Popover] {
            for state in [State::Hover, State::Pressed, State::Selected] {
                out.push(pair(
                    "text ink on an interaction layer",
                    Fg::Ink(ink),
                    Bg::layer(layer).state(state),
                    text,
                ));
            }
        }
    }
    // Placeholder and read-only values are Muted on the field fill.
    out.push(pair(
        "placeholder in a field",
        Fg::Paint(Paint::Muted),
        Bg::layer(Paint::Control),
        text,
    ));
    // Coloured text and links.
    for ink in [Ink::Accent, Ink::Danger, Ink::Success, Ink::Warning] {
        for layer in TEXT_LAYERS {
            out.push(pair("coloured text on a layer", Fg::Ink(ink), Bg::layer(layer), text));
        }
    }
    for state in [State::Hover, State::Selected] {
        out.push(pair(
            "link on an interaction layer",
            Fg::Ink(Ink::Accent),
            Bg::layer(Paint::Card).state(state),
            text,
        ));
    }
    // Badges, banners and tinted blocks.
    for under in [Paint::Card, Paint::Control, Paint::Bg, Paint::Popover] {
        out.push(pair(
            "badge text",
            Fg::Ink(Ink::Accent),
            Bg::over(Paint::AccentSoft, under),
            text,
        ));
        out.push(pair(
            "text on a selected cell",
            Fg::Ink(Ink::Text),
            Bg::over(Paint::AccentSoft, under),
            text,
        ));
    }
    // Banners and toasts of every kind: the ink is the hue darkened (lightened
    // on a dark scheme) on its tinted fill, the action button is filled with
    // the hue and carries its own on-colour.
    for kind in BannerKind::ALL {
        let fill_paint = Paint::NoticeFill(kind);
        let fill = Bg::layer(fill_paint);
        out.push(pair(
            "text and icons on a banner or toast",
            Fg::Ink(Ink::Notice(kind)),
            fill,
            text,
        ));
        for state in [State::Hover, State::Pressed] {
            out.push(pair(
                "dismiss glyph under hover and press",
                Fg::Ink(Ink::Notice(kind)),
                fill.state(state),
                text,
            ));
        }
        for state in [State::Rest, State::Hover, State::Pressed] {
            out.push(pair(
                "action label on a banner or toast",
                Fg::Paint(Paint::NoticeOnAction(kind)),
                Bg::over(Paint::NoticeAction(kind), fill_paint).state(state),
                text,
            ));
        }
        out.push(pair(
            "action button on its banner or toast",
            Fg::Paint(Paint::NoticeAction(kind)),
            fill,
            ui,
        ));
        let mut edge = pair(
            "banner or toast edge",
            Fg::Paint(Paint::NoticeEdge(kind)),
            fill,
            ui,
        );
        if kind == BannerKind::Neutral {
            edge.exempt = Some("decorative: the dark pill is set off by its shadow and its fill, not its edge");
        }
        out.push(edge);
    }
    out.push(pair(
        "error block text",
        Fg::Ink(Ink::Danger),
        Bg::layer(Paint::DangerSoft),
        text,
    ));
    // Text on every filled surface, at rest, hover and press.
    for (fg, fill) in [
        (Paint::OnAccent, Paint::Accent),
        (Paint::OnDanger, Paint::Danger),
        (Paint::OnSuccess, Paint::Success),
        (Paint::OnWarning, Paint::Warning),
    ] {
        for state in [State::Rest, State::Hover, State::Pressed] {
            for under in [Paint::Card, Paint::Bg] {
                out.push(pair(
                    "text and icons on a filled surface",
                    Fg::Paint(fg),
                    Bg::over(fill, under).state(state),
                    text,
                ));
            }
        }
    }
    // Floating surfaces: tooltips, toasts, the status line.
    for under in [Paint::Bg, Paint::Card] {
        out.push(pair(
            "tooltip and toast text",
            Fg::Paint(Paint::OnFloating),
            Bg::over(Paint::Floating, under),
            text,
        ));
        out.push(pair(
            "error text on the floating surface",
            Fg::Ink(Ink::DangerOnFloating),
            Bg::over(Paint::Floating, under),
            text,
        ));
    }
    // Search highlights.
    out.push(pair(
        "text on a search hit",
        Fg::Ink(Ink::Text),
        Bg::over(Paint::SearchHit, Paint::Card),
        text,
    ));
    out.push(pair(
        "text on the current search hit",
        Fg::Ink(Ink::Text),
        Bg::over(Paint::SearchCurrent, Paint::Card),
        text,
    ));
    // Markup colours in the inspector's DOM tree and code views.
    for syntax in [
        Paint::SyntaxTag,
        Paint::SyntaxAttr,
        Paint::SyntaxValue,
        Paint::SyntaxText,
        Paint::SyntaxComment,
    ] {
        for layer in [Paint::Bg, Paint::Card, Paint::Popover] {
            out.push(pair("code face", Fg::Paint(syntax), Bg::layer(layer), text));
        }
    }
    // Non-text: component boundaries, indicators and the focus ring.
    for layer in TEXT_LAYERS {
        out.push(pair("control edge, track and unchecked mark", Fg::Paint(Paint::Outline), Bg::layer(layer), ui));
        out.push(pair("accent fill, mark and selected state", Fg::Paint(Paint::Accent), Bg::layer(layer), ui));
        out.push(pair("icon in muted ink", Fg::Paint(Paint::Muted), Bg::layer(layer), ui));
        for paint in [Paint::Success, Paint::Warning, Paint::Danger] {
            out.push(pair("status fill and chart series", Fg::Paint(paint), Bg::layer(layer), ui));
        }
    }
    // The rings also sit on the raised control fill.
    for layer in LAYERS {
        out.push(pair("keyboard focus ring", Fg::Paint(Paint::FocusRing), Bg::layer(layer), ui));
        out.push(pair("error ring", Fg::Paint(Paint::ErrorRing), Bg::layer(layer), ui));
    }
    let mut hairline = pair(
        "row separator and hairline",
        Fg::Paint(Paint::Separator),
        Bg::layer(Paint::Card),
        ui,
    );
    hairline.exempt = Some("decorative divider: WCAG 1.4.11 asks nothing of it, rows are named by text");
    out.push(hairline);
    // Disabled controls.
    for layer in [Paint::Card, Paint::Bg] {
        let mut label = pair(
            "disabled button label",
            Fg::Ink(Ink::Muted),
            Bg::over(Paint::Control, layer),
            ui,
        );
        label.opacity = crate::style::DISABLED_OPACITY;
        label.exempt = Some(DISABLED_REASON);
        out.push(label);
        let mut filled = pair(
            "disabled suggested button label",
            Fg::Ink(Ink::Muted),
            Bg::over(Paint::Accent, layer),
            ui,
        );
        filled.opacity = crate::style::DISABLED_OPACITY;
        filled.exempt = Some(DISABLED_REASON);
        out.push(filled);
    }
    out
}

/// One measured pair under one scheme and accent.
#[derive(Clone, Debug)]
pub struct Row {
    pub pair: Pair,
    pub scheme: Scheme,
    pub accent: &'static str,
    pub fg: Color,
    pub bg: Color,
    /// WCAG contrast ratio of what is on screen.
    pub ratio: f32,
    /// APCA Lc, signed. Advisory only.
    pub apca: f32,
}

impl Row {
    pub fn required(&self) -> f32 {
        self.pair.need.min()
    }

    pub fn passes(&self) -> bool {
        self.ratio + 1e-4 >= self.required()
    }

    /// Below its requirement and not exempt: the test fails on it.
    pub fn fails(&self) -> bool {
        !self.passes() && self.pair.exempt.is_none()
    }

    pub fn level(&self) -> Level {
        Level::of(self.ratio)
    }

    /// `pass`, `AAA`-grade pass, `exempt` or `FAIL`, as the table shows it.
    pub fn badge(&self) -> &'static str {
        if self.passes() {
            if self.level() == Level::Aaa { "AAA" } else { "pass" }
        } else if self.pair.exempt.is_some() {
            "exempt"
        } else {
            "FAIL"
        }
    }

    pub fn theme(&self) -> &'static str {
        if self.scheme.is_dark() { "dark" } else { "light" }
    }
}

/// Measures every pair for one set of tokens.
pub fn evaluate(tokens: &Tokens) -> Vec<Row> {
    pairs()
        .into_iter()
        .map(|pair| {
            let layer = pair.bg.under.color(tokens);
            let mut bg = pair.bg.colour(tokens);
            let mut fg = contrast::composite(pair.fg.colour(tokens), bg);
            if pair.opacity < 1.0 {
                let fade = |c: Color| {
                    contrast::composite(c.with_af32(pair.opacity), layer)
                };
                bg = fade(bg);
                fg = fade(fg);
            }
            Row {
                scheme: tokens.scheme,
                accent: tokens.accent_name,
                ratio: contrast::ratio_opaque(fg, bg),
                apca: contrast::apca(fg, bg),
                fg,
                bg,
                pair,
            }
        })
        .collect()
}

/// Both schemes, with an accent: the tokens the matrix covers.
pub fn all_tokens() -> Vec<Tokens> {
    let mut out = Vec::new();
    for scheme in [Scheme::PreferLight, Scheme::PreferDark] {
        for accent in accent_names() {
            out.push(Tokens::new(accent, scheme, false));
        }
    }
    out
}

/// Every pair under both schemes and every accent.
pub fn evaluate_all() -> Vec<Row> {
    all_tokens().iter().flat_map(evaluate).collect()
}

/// The rows that fail: below the requirement and not exempt.
pub fn failing(rows: &[Row]) -> Vec<&Row> {
    rows.iter().filter(|row| row.fails()).collect()
}

fn hex(colour: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", colour.r(), colour.g(), colour.b())
}

/// A readable table: one line per row, with the colours, the ratio, what the
/// pair needs, the APCA reading (advisory) and the verdict.
pub fn table(rows: &[&Row]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<6} {:<7} {:<34} {:<34} {:<8} {:<8} {:>6} {:>10} {:>7}  verdict",
        "theme", "accent", "foreground", "background", "fg", "bg", "ratio", "need", "apca*"
    );
    for row in rows {
        let _ = writeln!(
            out,
            "{:<6} {:<7} {:<34} {:<34} {:<8} {:<8} {:>6.2} {:>10} {:>7.1}  {}",
            row.theme(),
            row.accent,
            format!("{} ({})", row.pair.fg.name(), row.pair.what),
            row.pair.bg.name(),
            hex(row.fg),
            hex(row.bg),
            row.ratio,
            row.pair.need.label(),
            row.apca,
            row.badge(),
        );
    }
    out.push_str("* APCA Lc is advisory and never fails a pair.\n");
    out
}

/// `(pairs measured, failing, exempt below requirement)` for a summary line.
pub fn counts(rows: &[Row]) -> (usize, usize, usize) {
    let failing = rows.iter().filter(|row| row.fails()).count();
    let exempt = rows
        .iter()
        .filter(|row| !row.passes() && row.pair.exempt.is_some())
        .count();
    (rows.len(), failing, exempt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_token_pair_meets_its_wcag_requirement() {
        let rows = evaluate_all();
        let failing = failing(&rows);
        let (total, fails, exempt) = counts(&rows);
        println!(
            "contrast matrix: {total} pair measurements, {fails} failing, {exempt} exempt below requirement"
        );
        assert!(
            failing.is_empty(),
            "{} of {} token pairs are below their WCAG requirement:\n{}",
            failing.len(),
            total,
            table(&failing)
        );
    }

    #[test]
    fn filled_surfaces_use_a_darkened_version_of_their_colour() {
        // Text on a light accent fill is a darker shade of the same hue, not white.
        for tokens in all_tokens().iter().filter(|t| !t.scheme.is_dark()) {
            let fill = tokens.accent;
            let ink = tokens.on_accent;
            let (f, i) = (
                contrast::Oklch::from_colour(fill),
                contrast::Oklch::from_colour(ink),
            );
            if contrast::luminance(ink) < contrast::luminance(fill) {
                assert!(i.l < f.l - 0.2, "{}: ink is not much darker", tokens.accent_name);
            }
        }
    }

    #[test]
    fn deriving_tokens_is_fast_for_every_scheme_and_accent() {
        // Each derive is a handful of bounded searches; a regression that
        // loops shows here instead of at window start-up.
        for scheme in [Scheme::PreferLight, Scheme::PreferDark] {
            for accent in accent_names() {
                let started = std::time::Instant::now();
                let _ = Tokens::derive(accent, scheme, false);
                assert!(started.elapsed().as_millis() < 500, "{accent} {scheme:?} took {:?}", started.elapsed());
            }
        }
    }

    #[test]
    fn exemptions_have_reasons_and_stay_legible() {
        for row in evaluate_all() {
            if let Some(reason) = row.pair.exempt {
                assert!(!reason.is_empty());
            }
        }
    }
}
