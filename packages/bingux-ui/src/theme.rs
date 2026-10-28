//! Design tokens for Bingux application windows.
//!
//! Accent colours and the colour-scheme rule follow `shell/bingux/Theme.qml`.
//! Surfaces are opaque: an application window has no blurred backdrop, so
//! every layer is a fixed grey and interaction states are white overlays.
//! Nothing outside this module hard-codes a colour, size, radius or duration;
//! components read them from [`Tokens`].

pub use crate::contrast::on_colour;
use crate::contrast::{AA_LARGE, AA_TEXT, composite, ensure_contrast, on_colour_over};
use crate::task::use_resource;
use freya::prelude::*;

/// The shell's own font.
const DEFAULT_FONT_FAMILY: &str = "Adwaita Sans";
/// Width below which the window is laid out as a phone: the list of pages is
/// a screen of its own and the window buttons are hidden.
pub const NARROW_BELOW: f32 = 740.0;
/// The phone preview's window size, in logical pixels. A device preview asks
/// the window for this size and lays the page out in a column this wide.
pub const PHONE_PREVIEW_SIZE: (f32, f32) = (390.0, 800.0);
/// Window default and minimum size.
pub const WINDOW_SIZE: (f32, f32) = (1000.0, 740.0);
pub const WINDOW_MIN_SIZE: (f32, f32) = (360.0, 420.0);

/// Accent names understood by GNOME's `accent-color` key, with the same RGB
/// values as `Theme.qml`'s `accentColorFor` table.
const ACCENTS: [(&str, [f32; 3]); 9] = [
    ("blue", [0.208, 0.518, 0.894]),
    ("teal", [0.129, 0.565, 0.643]),
    ("green", [0.227, 0.580, 0.290]),
    ("yellow", [0.784, 0.533, 0.000]),
    ("orange", [0.929, 0.357, 0.000]),
    ("red", [0.902, 0.176, 0.259]),
    ("pink", [0.835, 0.380, 0.600]),
    ("purple", [0.569, 0.255, 0.675]),
    ("slate", [0.435, 0.514, 0.588]),
];

/// The accent name used when GNOME reports none, or an unknown one.
pub const DEFAULT_ACCENT: &str = "blue";

/// Names of all accent colours, in picker order.
pub fn accent_names() -> impl Iterator<Item = &'static str> {
    ACCENTS.iter().map(|(name, _)| *name)
}

/// Base RGB for a GNOME accent name (`Theme.qml accentColorFor`).
fn accent_base(name: &str) -> [f32; 3] {
    ACCENTS
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, rgb)| *rgb)
        .unwrap_or(ACCENTS[0].1)
}

/// Accent colour for a GNOME accent name. In the dark colour scheme the base
/// colour is lightened 22% towards white, exactly as `Theme.qml` does.
pub fn accent_for(name: &str, dark: bool) -> Color {
    let base = accent_base(name);
    let rgb = if dark {
        base.map(|channel| channel + (1.0 - channel) * 0.22)
    } else {
        base
    };
    rgba(rgb[0], rgb[1], rgb[2], 1.0)
}

/// Converts float channels (0..1) to a Freya colour, rounding to 8 bits.
pub fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
    let to8 = |channel: f32| (channel * 255.0).round().clamp(0.0, 255.0) as u8;
    Color::from((to8(r), to8(g), to8(b), to8(a)))
}

/// Colour scheme read from `org.gnome.desktop.interface color-scheme`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    Default,
    PreferLight,
    PreferDark,
}

impl Scheme {
    /// Parses a gsettings value such as `'prefer-dark'`. Unknown values give
    /// `PreferDark`, which is the shell default in `Theme.qml`.
    pub fn from_gsettings(raw: &str) -> Self {
        match raw.trim().trim_matches('\'') {
            "default" => Self::Default,
            "prefer-light" => Self::PreferLight,
            _ => Self::PreferDark,
        }
    }

    /// The gsettings string for this scheme.
    pub fn gsettings_value(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::PreferLight => "prefer-light",
            Self::PreferDark => "prefer-dark",
        }
    }

    /// Whether the dark-scheme accent rule applies.
    pub fn is_dark(self) -> bool {
        matches!(self, Self::PreferDark)
    }
}

/// Resolved colours and motion settings for one accent and scheme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub accent: Color,
    pub accent_name: &'static str,
    pub scheme: Scheme,
    pub reduced_motion: bool,
    /// Page background.
    pub bg: Color,
    /// Window frame and header bars.
    pub surface: Color,
    /// Sidebar background.
    pub sidebar: Color,
    /// Fill of the group cards, dialogs and popovers.
    pub elevated: Color,
    /// Fill of buttons, fields, dropdowns and segmented controls. One step
    /// lighter than `elevated`, so a control stays visible on a card.
    /// Hover and press are the state layers over it.
    pub control: Color,
    /// The selected segment of a segmented control: one step lighter than
    /// `control` in both schemes, so the choice reads as raised off its track.
    pub raised: Color,
    /// Hairline between rows and between panes.
    pub separator: Color,
    /// Group surface for lists on the page.
    pub card: Color,
    /// Overlays for pointer hover, press and the selected sidebar item.
    pub hover: Color,
    pub pressed: Color,
    pub selection: Color,
    pub border: Color,
    pub outline: Color,
    pub text: Color,
    pub muted: Color,
    pub bar_divider: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    /// Text and icons on a filled surface: the surface colour darkened (or,
    /// on a dark fill, near-white) by [`on_colour_over`] to reach AA, and AAA
    /// where the hue allows, at rest, hover and press. They follow the
    /// accent, so they follow the user's choice.
    pub on_accent: Color,
    pub on_danger: Color,
    pub on_success: Color,
    pub on_warning: Color,
    /// Text on the floating surface (tooltips, toasts, the status line).
    pub on_floating: Color,
    /// The accent, danger, success and warning colours as text and icons on
    /// the layers of the scheme: the same hue, moved in lightness until
    /// every layer gives 4.5:1. The plain tokens stay for fills.
    pub accent_ink: Color,
    pub danger_ink: Color,
    pub success_ink: Color,
    pub warning_ink: Color,
    /// The danger colour as text on the floating surface.
    pub danger_on_floating: Color,
    /// The colours of a banner or toast of each [`BannerKind`]; read one with
    /// [`Tokens::notice`].
    pub notice: [NoticePalette; 5],
    /// The keyboard focus ring and the error ring: accent and danger moved
    /// until they show 3:1 on every layer and on the raised control fill.
    /// The fills behind a search hit and the current hit: the warning colour
    /// kept dark (or light) enough that `text` reads on them.
    pub search_hit: Color,
    pub search_current: Color,
    pub focus_ring: Color,
    pub error_ring: Color,
    pub floating: Color,
    pub floating_outline: Color,
    /// Opaque surface for menus and popovers that float over content.
    pub popover: Color,
    /// Syntax colours for markup in the inspector's DOM tree.
    pub syntax_tag: Color,
    pub syntax_attr: Color,
    pub syntax_value: Color,
    pub syntax_text: Color,
    pub syntax_comment: Color,
}

impl Tokens {
    /// Builds the palette for an accent name and colour scheme.
    ///
    /// The layers follow GNOME: the content is the base layer, the sidebar
    /// sits one step off it, and the group cards, controls and popups step
    /// up from there. Hover, press and selection are white (dark) or black
    /// (light) overlays, so they read on every layer.
    pub fn new(accent_name: &'static str, scheme: Scheme, reduced_motion: bool) -> Self {
        // Deriving the contrast-safe colours searches lightness, so each
        // accent and scheme is derived once.
        static DERIVED: std::sync::OnceLock<
            std::sync::Mutex<std::collections::HashMap<(&'static str, bool), Tokens>>,
        > = std::sync::OnceLock::new();
        let key = (accent_name, scheme.is_dark());
        let cache = DERIVED.get_or_init(Default::default);
        let cached = cache.lock().ok().and_then(|map| map.get(&key).copied());
        let derived = cached.unwrap_or_else(|| {
            let fresh = Self::derive(accent_name, scheme, false);
            if let Ok(mut map) = cache.lock() {
                map.insert(key, fresh);
            }
            fresh
        });
        Self { scheme, reduced_motion, ..derived }
    }

    pub(crate) fn derive(accent_name: &'static str, scheme: Scheme, reduced_motion: bool) -> Self {
        let accent = accent_for(accent_name, scheme.is_dark());
        let p = if scheme.is_dark() {
            Palette::dark()
        } else {
            Palette::light()
        };
        let floating = rgba(0.12, 0.12, 0.13, 1.0);
        // Fills that stand for state (accent, status colours) and the edges
        // of controls must show against every layer: 3:1 (WCAG 1.4.11). The
        // hue stays; only lightness moves, and only where it has to.
        let layers = p.layers();
        let fill = |colour: Color| fit_fill(ensure_contrast(colour, &layers, AA_LARGE), &p);
        let accent = fill(accent);
        let (success, warning, danger) = (fill(p.success), fill(p.warning), fill(p.danger));
        let outline = ensure_contrast(p.outline, &layers, AA_LARGE);
        Self {
            accent,
            accent_name,
            scheme,
            reduced_motion,
            bg: p.content,
            surface: p.content,
            sidebar: p.sidebar,
            elevated: p.card,
            control: p.control,
            raised: p.raised,
            separator: p.separator,
            card: p.card,
            hover: p.hover,
            pressed: p.pressed,
            selection: p.selection,
            border: p.border,
            outline,
            text: p.text,
            muted: p.readable(p.muted),
            bar_divider: p.separator,
            success,
            warning,
            danger,
            on_accent: on_fill(accent, &p),
            on_danger: on_fill(danger, &p),
            on_success: on_fill(success, &p),
            on_warning: on_fill(warning, &p),
            on_floating: on_colour(composite(floating, p.content)),
            notice: notice_palettes(
                &p,
                scheme.is_dark(),
                [accent, success, warning, danger],
                floating,
                on_colour(composite(floating, p.content)),
            ),
            danger_on_floating: ensure_contrast(
                danger,
                &[composite(floating, p.content)],
                AA_TEXT,
            ),
            accent_ink: text_ink(accent, &p),
            danger_ink: text_ink(danger, &p),
            success_ink: text_ink(success, &p),
            warning_ink: text_ink(warning, &p),
            search_hit: highlight(warning, 0.30, &p),
            search_current: highlight(warning, 0.65, &p),
            focus_ring: ring_colour(accent, &p),
            error_ring: ring_colour(danger, &p),
            floating,
            floating_outline: rgba(1.0, 1.0, 1.0, 0.16),
            popover: p.popup,
            syntax_tag: p.readable(p.syntax_tag),
            syntax_attr: p.readable(p.syntax_attr),
            syntax_value: p.readable(p.syntax_value),
            syntax_text: p.readable(p.syntax_text),
            syntax_comment: p.readable(p.syntax_comment),
        }
    }

    /// Duration in milliseconds, zero when reduced motion is enabled.
    pub fn motion(&self, ms: u64) -> u64 {
        if self.reduced_motion { 0 } else { ms }
    }

    /// The colours of a notice of `kind`.
    pub fn notice(&self, kind: BannerKind) -> NoticePalette {
        self.notice[kind.index()]
    }
}

/// The kind of notice a banner or toast carries. It sets every colour of
/// the notice: the fill, the ink, the edge and the action button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BannerKind {
    /// The dark floating surface of a toast, with a light action button.
    Neutral,
    /// The accent hue.
    Info,
    Success,
    Warning,
    Error,
}

impl BannerKind {
    pub const ALL: [BannerKind; 5] = [
        BannerKind::Neutral,
        BannerKind::Info,
        BannerKind::Success,
        BannerKind::Warning,
        BannerKind::Error,
    ];

    const fn index(self) -> usize {
        match self {
            BannerKind::Neutral => 0,
            BannerKind::Info => 1,
            BannerKind::Success => 2,
            BannerKind::Warning => 3,
            BannerKind::Error => 4,
        }
    }
}

/// The colours of one [`BannerKind`] in one scheme and accent. The fill is an
/// opaque low-chroma tint of the kind's hue, so the notice reads over any
/// content it floats on. The ink (text, icon and dismiss glyph) is the same
/// hue moved in lightness: darker than the fill on a light scheme, lighter on
/// a dark one, until it reaches AAA (7:1) on the fill. The edge is the hue
/// itself at 3:1 on the fill. The action button is filled with the hue and
/// carries its own on-colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoticePalette {
    pub fill: Color,
    pub ink: Color,
    pub edge: Color,
    pub action: Color,
    pub on_action: Color,
}

/// The lightness of the tinted fill, how much of the hue's chroma it keeps
/// and its chroma ceiling (OKLab), on a light and on a dark scheme.
const NOTICE_FILL_LIGHT: (f32, f32, f32) = (0.95, 0.30, 0.05);
const NOTICE_FILL_DARK: (f32, f32, f32) = (0.30, 0.35, 0.07);
/// The most chroma the ink seed keeps, so a saturated hue stays readable.
const NOTICE_INK_CHROMA_MAX: f32 = 0.13;

/// The palettes of every kind, indexed by [`BannerKind::index`]: `hues` are the
/// accent, success, warning and danger fills.
fn notice_palettes(
    p: &Palette,
    dark: bool,
    hues: [Color; 4],
    floating: Color,
    on_floating: Color,
) -> [NoticePalette; 5] {
    use crate::contrast::{AAA_TEXT, Oklch};
    let layers = p.layers();
    let (fill_l, keep, max) = if dark { NOTICE_FILL_DARK } else { NOTICE_FILL_LIGHT };
    let tinted = |hue: Color| {
        let h = Oklch::from_colour(hue);
        let fill = Oklch { l: fill_l, c: (h.c * keep).min(max), h: h.h }.to_colour();
        let seed = Oklch {
            l: if dark { 0.88 } else { 0.42 },
            c: h.c.min(NOTICE_INK_CHROMA_MAX),
            h: h.h,
        }
        .to_colour();
        let ink = ensure_contrast(seed, &[fill], AAA_TEXT);
        let edge = ensure_contrast(hue, &[fill], AA_LARGE);
        let mut surfaces = layers.to_vec();
        surfaces.push(fill);
        let action = fit_fill(ensure_contrast(hue, &surfaces, AA_LARGE), p);
        NoticePalette { fill, ink, edge, action, on_action: on_fill(action, p) }
    };
    let [info, success, warning, error] = hues.map(tinted);
    let neutral = NoticePalette {
        fill: floating,
        ink: on_floating,
        edge: composite(rgba(1.0, 1.0, 1.0, 0.16), floating),
        action: on_floating,
        on_action: on_colour(on_floating),
    };
    [neutral, info, success, warning, error]
}

/// The ink for text and icons on `fill`, holding at rest and under the hover,
/// press and selection layers of the scheme.
fn on_fill(fill: Color, p: &Palette) -> Color {
    on_colour_over(&fill_states(fill, p))
}

/// A filled surface at rest and under the hover, press and selection layers.
fn fill_states(fill: Color, p: &Palette) -> [Color; 4] {
    [
        fill,
        composite(p.hover, fill),
        composite(p.pressed, fill),
        composite(p.selection, fill),
    ]
}

/// The most the lightness of a fill moves to make its ink reach AA.
const FILL_NUDGE_MAX: f32 = 0.12;
const FILL_NUDGE_STEP: f32 = 0.004;

/// `fill` as it is drawn: unchanged when its on-colour reaches AA in every
/// state, else moved the least in lightness (lighter for dark ink, darker for
/// light ink) that makes it so while it still shows 3:1 on every layer.
fn fit_fill(fill: Color, p: &Palette) -> Color {
    let layers = p.layers();
    let reads = |candidate: Color| {
        let states = fill_states(candidate, p);
        crate::contrast::worst_ratio(on_colour_over(&states), &states) >= AA_TEXT
            && crate::contrast::worst_ratio(candidate, &layers) >= AA_LARGE
    };
    if reads(fill) {
        return fill;
    }
    let base = crate::contrast::Oklch::from_colour(fill);
    let mut step = FILL_NUDGE_STEP;
    while step <= FILL_NUDGE_MAX {
        for direction in [-1.0, 1.0] {
            let candidate = crate::contrast::Oklch { l: base.l + direction * step, ..base }.to_colour();
            if reads(candidate) {
                return candidate;
            }
        }
        step += FILL_NUDGE_STEP;
    }
    fill
}

/// `colour` at `alpha` over the card, then moved in lightness until the
/// scheme's text reads on it at AA: the fill behind a search hit.
fn highlight(colour: Color, alpha: f32, p: &Palette) -> Color {
    let fill = composite(colour.with_af32(alpha), p.card);
    ensure_contrast(fill, &[p.text, p.syntax_text], AA_TEXT)
}

/// A ring colour that shows on every layer and on the raised fill.
fn ring_colour(colour: Color, p: &Palette) -> Color {
    let mut surfaces = p.layers().to_vec();
    surfaces.push(p.raised);
    ensure_contrast(colour, &surfaces, AA_LARGE)
}

/// `colour` as text: moved in lightness until it reads at AA on every layer
/// of the scheme and on its own 16% tint over a card and a control, which is
/// where badges and banners set it.
fn text_ink(colour: Color, p: &Palette) -> Color {
    let tint = |under: Color| composite(colour.with_af32(0.18), under);
    let surfaces = [
        p.content,
        p.sidebar,
        p.card,
        p.control,
        p.raised,
        p.popup,
        tint(p.card),
        tint(p.control),
        tint(p.content),
        tint(p.popup),
    ];
    ensure_contrast(colour, &surfaces, AA_TEXT)
}

/// The surface and ink colours of one colour scheme.
pub(crate) struct Palette {
    /// Page content and the header bar: the base layer.
    pub(crate) content: Color,
    /// Sidebar, one step off the content.
    pub(crate) sidebar: Color,
    /// Group cards on a page.
    pub(crate) card: Color,
    /// Buttons, fields and dropdowns sitting on a card.
    pub(crate) control: Color,
    pub(crate) raised: Color,
    /// Dialogs, popovers and menus: the topmost opaque layer, the
    /// `Tokens::popover` surface.
    pub(crate) popup: Color,
    pub(crate) separator: Color,
    pub(crate) hover: Color,
    pub(crate) pressed: Color,
    pub(crate) selection: Color,
    pub(crate) border: Color,
    pub(crate) outline: Color,
    pub(crate) text: Color,
    pub(crate) muted: Color,
    pub(crate) success: Color,
    pub(crate) warning: Color,
    pub(crate) danger: Color,
    pub(crate) syntax_tag: Color,
    pub(crate) syntax_attr: Color,
    pub(crate) syntax_value: Color,
    pub(crate) syntax_text: Color,
    pub(crate) syntax_comment: Color,
}

impl Palette {
    /// The opaque layers a control or text can sit on.
    fn layers(&self) -> [Color; 5] {
        [self.content, self.sidebar, self.card, self.control, self.popup]
    }

    /// The layers and each of them under the hover, press and selection
    /// layers, which is every ground ordinary text sits on.
    fn grounds(&self) -> Vec<Color> {
        let mut layers = self.layers().to_vec();
        layers.push(self.raised);
        let mut grounds = layers.clone();
        for layer in layers {
            for state in [self.hover, self.pressed, self.selection] {
                grounds.push(composite(state, layer));
            }
        }
        grounds
    }

    /// Ink that must read on every ground: secondary text and the code face.
    fn readable(&self, ink: Color) -> Color {
        let grounds = self.grounds();
        if ink.a() == 255 {
            return ensure_contrast(ink, &grounds, AA_TEXT);
        }
        // A translucent ink gains opacity until it reads on every ground.
        (ink.a()..=255)
            .map(|alpha| Color::from((ink.r(), ink.g(), ink.b(), alpha)))
            .find(|candidate| crate::contrast::worst_ratio(*candidate, &grounds) >= AA_TEXT)
            .unwrap_or_else(|| Color::from((ink.r(), ink.g(), ink.b(), 255u8)))
    }

    pub(crate) fn dark() -> Self {
        Self {
            content: hex(0x222226),
            sidebar: hex(0x2e2e32),
            card: hex(0x343438),
            control: hex(0x424246),
            raised: hex(0x5a5a5f),
            popup: hex(0x3a3a3e),
            separator: rgba(1.0, 1.0, 1.0, 0.08),
            hover: rgba(1.0, 1.0, 1.0, 0.06),
            pressed: rgba(1.0, 1.0, 1.0, 0.10),
            selection: rgba(1.0, 1.0, 1.0, 0.10),
            border: rgba(1.0, 1.0, 1.0, 1.0),
            outline: hex(0x8c8c92),
            text: rgba(0.980, 0.980, 0.980, 0.98),
            muted: rgba(1.0, 1.0, 1.0, 0.69),
            success: hex(0x8ff0a4),
            warning: hex(0xf9f06b),
            danger: hex(0xff7b63),
            syntax_tag: hex(0x8ec6ff),
            syntax_attr: hex(0xf9cb8f),
            syntax_value: hex(0x8fe2a4),
            syntax_text: hex(0xf0f0f2),
            syntax_comment: hex(0x8a8a92),
        }
    }

    pub(crate) fn light() -> Self {
        Self {
            content: hex(0xfafafb),
            sidebar: hex(0xebebed),
            card: hex(0xffffff),
            control: hex(0xf1f1f3),
            raised: hex(0xffffff),
            popup: hex(0xffffff),
            separator: rgba(0.0, 0.0, 0.0, 0.10),
            hover: rgba(0.0, 0.0, 0.0, 0.05),
            pressed: rgba(0.0, 0.0, 0.0, 0.08),
            selection: rgba(0.0, 0.0, 0.0, 0.08),
            border: rgba(0.0, 0.0, 0.0, 1.0),
            outline: hex(0x8c8c92),
            text: hex(0x2e2e32),
            muted: hex(0x6b6b70),
            success: hex(0x26a269),
            warning: hex(0xcd9309),
            danger: hex(0xc01c28),
            syntax_tag: hex(0x1f5fa8),
            syntax_attr: hex(0x9a5a00),
            syntax_value: hex(0x1d7a3a),
            syntax_text: hex(0x2e2e32),
            syntax_comment: hex(0x7a7a80),
        }
    }
}

/// An opaque colour from a 0xRRGGBB value.
fn hex(value: u32) -> Color {
    let channel = |shift: u32| ((value >> shift) & 0xff) as f32 / 255.0;
    rgba(channel(16), channel(8), channel(0), 1.0)
}

/// Reads `BINGUX_REDUCED_MOTION=1` from the environment.
pub fn reduced_motion_from_env() -> bool {
    std::env::var("BINGUX_REDUCED_MOTION").is_ok_and(|value| value == "1")
}

/// Accent name normalised to a known entry, falling back to the default.
pub fn normalise_accent(raw: &str) -> &'static str {
    let trimmed = raw.trim().trim_matches('\'');
    ACCENTS
        .iter()
        .find(|(name, _)| *name == trimmed)
        .map(|(name, _)| *name)
        .unwrap_or(DEFAULT_ACCENT)
}

/// Reads one GNOME interface key with `gsettings`, returning its raw text.
/// Returns `None` when gsettings is missing or the key cannot be read.
pub fn read_gsettings(key: &str) -> Option<String> {
    let output = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", key])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Reads the accent and colour scheme from GNOME settings and builds tokens.
pub fn load_tokens() -> Tokens {
    let accent = read_gsettings("accent-color")
        .map(|raw| normalise_accent(&raw))
        .unwrap_or(DEFAULT_ACCENT);
    // The snapshot harness pins the scheme so both palettes can be rendered
    // without changing the desktop setting.
    let scheme = std::env::var("BINGUX_SETTINGS_SNAPSHOT_SCHEME")
        .ok()
        .map(|raw| Scheme::from_gsettings(&raw))
        .or_else(|| read_gsettings("color-scheme").map(|raw| Scheme::from_gsettings(&raw)))
        .unwrap_or(Scheme::PreferDark);
    Tokens::new(accent, scheme, reduced_motion_from_env())
}

/// Tokens shown until the GNOME settings have been read on the task pool.
/// The default accent and dark scheme match the shell's own defaults.
fn startup_tokens() -> Tokens {
    Tokens::new(
        DEFAULT_ACCENT,
        Scheme::PreferDark,
        reduced_motion_from_env(),
    )
}

/// Pool job for [`provide_tokens`]: reads the settings with `gsettings`.
fn load_settings_tokens(_: ()) -> Result<Tokens, String> {
    Ok(load_tokens())
}

/// Shared, live palette for the whole window. Pages that change the accent
/// or colour scheme write to it; every component reads it through
/// `crate::ui::use_tokens`.
#[derive(Clone, Copy, PartialEq)]
pub struct SharedTokens(pub State<Tokens>);

/// Loads the tokens, provides them as root context, bridges them into
/// Freya's component theme and follows live accent changes. Call once in
/// the window root's `render`; it uses hooks. Returns the current tokens.
pub fn provide_tokens() -> Tokens {
    let tokens = use_hook(|| SharedTokens(State::create(startup_tokens())));
    use_hook(move || watch_appearance(tokens.0));
    let loaded = use_resource((), load_settings_tokens);
    let mut shared = tokens.0;
    use_side_effect(move || {
        loaded.with(|resource| {
            if let Some(next) = resource.value() {
                shared.set_if_modified(*next);
            }
        });
    });
    use_provide_root_context(|| tokens);
    let current = *tokens.0.read();
    let mut theme_state = use_init_theme(|| freya_theme(&current));
    use_side_effect(move || {
        let tokens = *tokens.0.read();
        theme_state.set(freya_theme(&tokens));
    });
    current
}

/// Builds Freya's component theme from the tokens so built-in controls
/// (switches, sliders, selects, inputs) use the Bingux palette.
pub fn freya_theme(tokens: &Tokens) -> Theme {
    let dark = tokens.scheme.is_dark();
    let mut theme = if dark { dark_theme() } else { light_theme() };
    let accent = tokens.accent;
    theme.colors = ColorsSheet {
        primary: accent,
        secondary: Color::lerp(accent, Color::WHITE, 0.35),
        tertiary: Color::lerp(accent, Color::BLACK, 0.23),
        success: tokens.success,
        warning: tokens.warning,
        error: tokens.danger,
        info: accent,
        background: tokens.bg,
        surface_primary: tokens.card,
        surface_secondary: tokens.control,
        surface_tertiary: tokens.sidebar,
        surface_inverse: rgba(0.78, 0.78, 0.78, 1.0),
        surface_inverse_secondary: rgba(0.85, 0.85, 0.85, 1.0),
        surface_inverse_tertiary: rgba(0.92, 0.92, 0.92, 1.0),
        border: tokens.outline,
        border_focus: accent,
        border_disabled: tokens.outline.with_a(120),
        text_primary: tokens.text,
        text_secondary: tokens.muted,
        text_placeholder: tokens.muted.with_a(170),
        text_inverse: if dark {
            rgba(0.08, 0.08, 0.08, 1.0)
        } else {
            rgba(1.0, 1.0, 1.0, 1.0)
        },
        text_highlight: accent,
        focus: accent,
        active: tokens.pressed,
        disabled: rgba(0.2, 0.2, 0.2, 1.0),
        overlay: Color::from_af32rgb(0.2, 255, 255, 255),
        shadow: Color::from_af32rgb(0.6, 0, 0, 0),
    };
    // Freya's own tooltips (icon buttons, the busy spinner) take the same dark
    // bubble as Bingux's tooltip_icon, not the sidebar colour Freya uses.
    theme.set(
        "tooltip",
        TooltipThemePreference {
            background: Preference::Specific(tokens.floating),
            color: Preference::Reference("text_primary"),
            border_fill: Preference::Specific(tokens.floating_outline),
            font_size: Preference::Specific(crate::typography::CAPTION.size.px()),
        },
    );
    theme
}

/// Whether a real `gsettings monitor` may be started. Test processes and the
/// headless snapshot build never start one: a test process that exits
/// without running destructors would leave the child, and its session bus
/// connection, behind. `BINGUX_APPEARANCE_MONITOR=1` forces it on (for a test
/// about the monitor itself) and `=0` forces it off.
fn appearance_monitor_enabled() -> bool {
    match std::env::var("BINGUX_APPEARANCE_MONITOR").as_deref() {
        Ok("1") => true,
        Ok("0") => false,
        _ => !(cfg!(test) || cfg!(feature = "testing")),
    }
}

/// Owns the `gsettings monitor` child of [`watch_appearance`]. Dropping it
/// (the window root unmounts, the window closes) kills and reaps the child.
/// The child also carries `PR_SET_PDEATHSIG(SIGTERM)`, so it dies with this
/// process even when no destructor runs.
#[derive(Clone)]
pub struct AppearanceMonitor(std::sync::Arc<MonitorInner>);

struct MonitorInner {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    child: std::sync::Arc<std::sync::Mutex<Option<std::process::Child>>>,
}

impl Drop for MonitorInner {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        let child = self
            .child
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take();
        if let Some(mut child) = child {
            let _ = child.kill();
            // Reaping waits for the child, so it runs off the UI thread.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }
}

/// Follows `gsettings monitor org.gnome.desktop.interface` and updates the
/// shared tokens when the accent or colour scheme changes elsewhere, for
/// example in the shell's Control Centre.
///
/// The monitor runs on a background thread for as long as the returned
/// guard lives; a missing `gsettings` leaves the tokens unchanged. Keep the
/// guard in a hook so it drops with the window. Nothing is started when
/// [`appearance_monitor_enabled`] says no.
pub fn watch_appearance(mut tokens: State<Tokens>) -> AppearanceMonitor {
    use std::io::{BufRead, BufReader};

    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use crate::task::Feed;

    let feed = Arc::new(Feed::<(String, String)>::default());
    let stop = Arc::new(AtomicBool::new(false));
    let slot: Arc<Mutex<Option<std::process::Child>>> = Arc::default();
    let guard = AppearanceMonitor(Arc::new(MonitorInner {
        stop: Arc::clone(&stop),
        child: Arc::clone(&slot),
    }));
    if !appearance_monitor_enabled() {
        feed.close();
        return guard;
    }
    let sender = Arc::clone(&feed);
    std::thread::spawn(move || {
        let mut command = Command::new("gsettings");
        command
            .args(["monitor", "org.gnome.desktop.interface"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        // The child dies with the thread that spawned it (see the helper):
        // that thread lives as long as the child.
        crate::runtime::die_with_parent(&mut command);
        let Ok(mut child) = command.spawn() else {
            sender.close();
            return;
        };
        let stdout = child.stdout.take();
        // The guard may already be gone: then nobody will kill the child.
        {
            let mut held = slot.lock().unwrap_or_else(|poison| poison.into_inner());
            if stop.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                sender.close();
                return;
            }
            *held = Some(child);
        }
        if let Some(stdout) = stdout {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                if let Some((key, value)) = line.split_once(':') {
                    sender.push((key.trim().to_string(), value.trim().to_string()));
                }
            }
        }
        let child = slot
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take();
        if let Some(mut child) = child {
            let _ = child.kill();
            let _ = child.wait();
        }
        sender.close();
    });
    // The task sleeps until the monitor says something: no timer runs while
    // the appearance does not change.
    spawn(async move {
        while let Some((key, value)) = feed.next().await {
            let current = *tokens.read();
            let next = match key.as_str() {
                "accent-color" => Tokens::new(
                    normalise_accent(&value),
                    current.scheme,
                    current.reduced_motion,
                ),
                "color-scheme" => Tokens::new(
                    current.accent_name,
                    Scheme::from_gsettings(&value),
                    current.reduced_motion,
                ),
                _ => continue,
            };
            tokens.set_if_modified(next);
        }
    });
    guard
}

/// The interface font, read once. It is `appearance.font_family` from
/// `bingux.toml`. When that is empty the family is Adwaita Sans, the shell's
/// own font, or the fontconfig default sans-serif when Adwaita Sans is not
/// installed. GNOME's font setting is not consulted.
pub fn font_family() -> &'static str {
    static FAMILY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    FAMILY.get_or_init(|| {
        let configured = crate::runtime::run(
            &crate::runtime::bingux_config(),
            &["get", "appearance.font_family"],
            std::time::Duration::from_secs(2),
        )
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
        configured.unwrap_or_else(|| {
            if !font_files(DEFAULT_FONT_FAMILY, 1).is_empty() {
                return DEFAULT_FONT_FAMILY.to_string();
            }
            std::process::Command::new("fc-match")
                .args(["-f", "%{family}", "sans-serif"])
                .output()
                .ok()
                .and_then(|output| {
                    let text = String::from_utf8_lossy(&output.stdout).into_owned();
                    text.split(',').next().map(|name| name.trim().to_string())
                })
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| DEFAULT_FONT_FAMILY.to_string())
        })
    })
}

/// The font files fontconfig has for `family`, regular first, at most
/// `limit` of them. Only files whose family really is `family` are kept, so
/// a missing family gives an empty list instead of a substitute.
pub fn font_files(family: &str, limit: usize) -> Vec<Vec<u8>> {
    let Ok(output) = std::process::Command::new("fc-list")
        .args(["-f", "%{file}\t%{family}\t%{style}\n", family])
        .output()
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut entries: Vec<(&str, &str)> = text
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let (file, names, style) = (fields.next()?, fields.next()?, fields.next()?);
            names
                .split(',')
                .any(|name| name.trim().eq_ignore_ascii_case(family))
                .then_some((file, style))
        })
        .collect();
    // Upright faces only. Registered under one family name, an italic file
    // can win the lookup and set every line in italics. Regular files go
    // first so the limit keeps them.
    entries.retain(|(file, style)| {
        !style.to_ascii_lowercase().contains("italic")
            && !file.to_ascii_lowercase().contains("italic")
    });
    entries.sort_by_key(|(file, _)| *file);
    let mut seen = Vec::new();
    let mut fonts = Vec::new();
    for (file, _) in entries {
        if fonts.len() == limit || seen.contains(&file) {
            continue;
        }
        seen.push(file);
        if let Ok(bytes) = std::fs::read(file) {
            fonts.push(bytes);
        }
    }
    fonts
}

/// Registers the interface font with Freya from its font files.
///
/// Many system fonts, Adwaita Sans and Cantarell among them, are variable.
/// Freya picks the weight axis only for fonts it loads itself. For a system
/// font the text engine asks for Bold, gets the Regular instance and fakes
/// the weight by smearing the outlines, which looks heavy and blotchy.
/// Loading the files gives real instances for every weight. When the family
/// has no file, Freya falls back to its own system lookup by name.
pub(crate) fn with_font(config: LaunchConfig) -> LaunchConfig {
    let family = font_family();
    let mut config = config;
    let mut found = false;
    for bytes in font_files(family, 4) {
        config = config.with_font(family, bytes);
        found = true;
    }
    if found {
        config.with_default_font(family)
    } else {
        config
    }
}
