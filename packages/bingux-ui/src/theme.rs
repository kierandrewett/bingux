//! Design tokens for Bingux application windows.
//!
//! Accent colours and the colour-scheme rule follow `shell/bingux/Theme.qml`.
//! Surfaces are opaque: an application window has no blurred backdrop, so
//! every layer is a fixed grey and interaction states are white overlays.
//! Nothing outside this module hard-codes a colour, size, radius or duration;
//! components read them from [`Tokens`].

use crate::task::use_resource;
use freya::prelude::*;

/// The shell's own font.
const DEFAULT_FONT_FAMILY: &str = "Adwaita Sans";
/// Width below which the window is laid out as a phone: the list of pages is
/// a screen of its own and the window buttons are hidden.
pub const NARROW_BELOW: f32 = 740.0;
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
    pub accent_foreground: Color,
    pub floating: Color,
    pub floating_outline: Color,
    /// Opaque surface for menus and popovers that float over content.
    pub popover: Color,
}

impl Tokens {
    /// Builds the palette for an accent name and colour scheme.
    ///
    /// The layers follow GNOME: the content is the base layer, the sidebar
    /// sits one step off it, and the group cards, controls and popups step
    /// up from there. Hover, press and selection are white (dark) or black
    /// (light) overlays, so they read on every layer.
    pub fn new(accent_name: &'static str, scheme: Scheme, reduced_motion: bool) -> Self {
        let accent = accent_for(accent_name, scheme.is_dark());
        let p = if scheme.is_dark() {
            Palette::dark()
        } else {
            Palette::light()
        };
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
            outline: p.outline,
            text: p.text,
            muted: p.muted,
            bar_divider: p.separator,
            success: p.success,
            warning: p.warning,
            danger: p.danger,
            accent_foreground: rgba(1.0, 1.0, 1.0, 1.0),
            floating: rgba(0.12, 0.12, 0.13, 0.82),
            floating_outline: rgba(1.0, 1.0, 1.0, 0.16),
            popover: p.popup,
        }
    }

    /// Duration in milliseconds, zero when reduced motion is enabled.
    pub fn motion(&self, ms: u64) -> u64 {
        if self.reduced_motion { 0 } else { ms }
    }
}

/// The surface and ink colours of one colour scheme.
struct Palette {
    /// Page content and the header bar: the base layer.
    content: Color,
    /// Sidebar, one step off the content.
    sidebar: Color,
    /// Group cards on a page.
    card: Color,
    /// Buttons, fields and dropdowns sitting on a card.
    control: Color,
    raised: Color,
    /// Dialogs, popovers and menus: the topmost opaque layer, the
    /// `Tokens::popover` surface.
    popup: Color,
    separator: Color,
    hover: Color,
    pressed: Color,
    selection: Color,
    border: Color,
    outline: Color,
    text: Color,
    muted: Color,
    success: Color,
    warning: Color,
    danger: Color,
}

impl Palette {
    fn dark() -> Self {
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
            outline: rgba(0.314, 0.314, 0.314, 0.75),
            text: rgba(0.980, 0.980, 0.980, 0.98),
            muted: rgba(1.0, 1.0, 1.0, 0.69),
            success: hex(0x8ff0a4),
            warning: hex(0xf9f06b),
            danger: hex(0xff7b63),
        }
    }

    fn light() -> Self {
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
            outline: rgba(0.0, 0.0, 0.0, 0.20),
            text: hex(0x2e2e32),
            muted: hex(0x6b6b70),
            success: hex(0x26a269),
            warning: hex(0xcd9309),
            danger: hex(0xc01c28),
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

/// Follows `gsettings monitor org.gnome.desktop.interface` and updates the
/// shared tokens when the accent or colour scheme changes elsewhere, for
/// example in the shell's Control Centre.
///
/// The monitor runs on a background thread for the life of the window; a
/// missing `gsettings` leaves the tokens unchanged.
pub fn watch_appearance(mut tokens: State<Tokens>) {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::time::Duration;

    let (sender, receiver) = mpsc::channel::<(String, String)>();
    std::thread::spawn(move || {
        let Ok(mut child) = Command::new("gsettings")
            .args(["monitor", "org.gnome.desktop.interface"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        else {
            return;
        };
        let Some(stdout) = child.stdout.take() else {
            return;
        };
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some((key, value)) = line.split_once(':')
                && sender
                    .send((key.trim().to_string(), value.trim().to_string()))
                    .is_err()
            {
                break;
            }
        }
        let _ = child.kill();
    });
    spawn(async move {
        loop {
            match receiver.try_recv() {
                Ok((key, value)) => {
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
                Err(mpsc::TryRecvError::Empty) => timer(Duration::from_millis(200)).await,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }
    });
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
pub fn with_font(config: LaunchConfig) -> LaunchConfig {
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
