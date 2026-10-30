//! The environment every specimen is shown in, and the stage that holds one.
//!
//! The header switcher edits one [`Env`] (colour scheme, accent, density and
//! display scale). The window applies it to itself, so the whole app is the
//! "real environment": its own palette, its own density, its own scale. A
//! [`Stage`] then re-provides tokens and density to one specimen, which is how
//! a specimen is shown in light and dark, or at desktop and phone density,
//! side by side in a single window.

use bingux_ui::prelude::*;
use bingux_ui::style::{PointerKind, SizeClass, force_density, window_density};
use bingux_ui::theme::{accent_names, freya_theme, load_tokens};
use freya::prelude::*;

use crate::model::{Body, Frame, Kind};
use crate::registry::ENTRIES;

/// The colour scheme the window and its specimens use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    /// The scheme the desktop reports.
    System,
    Light,
    Dark,
    /// The window follows the desktop; every specimen shows light and dark.
    Both,
}

impl ThemeMode {
    pub const ALL: [ThemeMode; 4] = [
        ThemeMode::System,
        ThemeMode::Light,
        ThemeMode::Dark,
        ThemeMode::Both,
    ];

    pub fn key(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
            ThemeMode::Both => "both",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::System => "System",
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
            ThemeMode::Both => "Both",
        }
    }

    pub fn from_key(key: &str) -> Option<ThemeMode> {
        ThemeMode::ALL.into_iter().find(|mode| mode.key() == key)
    }
}

/// The density of the whole window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DensityMode {
    /// The window's own: wide is desktop, narrow is phone.
    Auto,
    /// Mouse density, whatever the width.
    Desktop,
    /// Fingertip density in a phone-wide column (the inspector's device
    /// preview).
    Phone,
}

impl DensityMode {
    pub const ALL: [DensityMode; 3] = [DensityMode::Auto, DensityMode::Desktop, DensityMode::Phone];

    pub fn key(self) -> &'static str {
        match self {
            DensityMode::Auto => "auto",
            DensityMode::Desktop => "desktop",
            DensityMode::Phone => "phone",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DensityMode::Auto => "Auto",
            DensityMode::Desktop => "Desktop",
            DensityMode::Phone => "Phone",
        }
    }

    pub fn from_key(key: &str) -> Option<DensityMode> {
        DensityMode::ALL.into_iter().find(|mode| mode.key() == key)
    }
}

/// What the header switcher edits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Env {
    pub theme: ThemeMode,
    /// An accent name, or `None` for the desktop's own.
    pub accent: Option<&'static str>,
    pub density: DensityMode,
    /// Display scale in percent.
    pub scale: u32,
    /// Whether every string is pseudo-localised (`bingux_ui::stress`).
    pub pseudo: bool,
    /// The factor the type ramp is multiplied by, in percent.
    pub text_scale: u32,
}

/// The display scales the switcher offers, in percent.
pub const SCALES: [u32; 4] = [100, 125, 150, 200];

/// The text scales the switcher offers, in percent: those of
/// `bingux_ui::stress::TEXT_SCALES`.
pub const TEXT_SCALES: [u32; 4] = [100, 125, 150, 200];

impl Env {
    /// The starting environment. `BINGUX_TAXONOMY_THEME` (`system`, `light`,
    /// `dark`, `both`), `BINGUX_TAXONOMY_ACCENT` (an accent name),
    /// `BINGUX_TAXONOMY_DENSITY` (`auto`, `desktop`, `phone`) and
    /// `BINGUX_TAXONOMY_SCALE` (a percent) set it, so a snapshot or a test
    /// can start in any combination.
    pub fn initial() -> Self {
        let var = |name: &str| std::env::var(name).ok();
        Self {
            theme: var("BINGUX_TAXONOMY_THEME")
                .and_then(|key| ThemeMode::from_key(&key))
                .unwrap_or(ThemeMode::System),
            accent: var("BINGUX_TAXONOMY_ACCENT")
                .and_then(|name| accent_names().find(|candidate| *candidate == name)),
            density: var("BINGUX_TAXONOMY_DENSITY")
                .and_then(|key| DensityMode::from_key(&key))
                .unwrap_or(DensityMode::Auto),
            scale: var("BINGUX_TAXONOMY_SCALE")
                .and_then(|text| text.parse().ok())
                .filter(|scale| SCALES.contains(scale))
                .unwrap_or(100),
            // `BINGUX_PSEUDO_LOCALE` and `BINGUX_TEXT_SCALE` are read by
            // bingux_ui::stress, which this starts from.
            pseudo: bingux_ui::stress::pseudo_locale(),
            text_scale: TEXT_SCALES
                .into_iter()
                .find(|percent| *percent == (bingux_ui::stress::text_scale() * 100.0).round() as u32)
                .unwrap_or(100),
        }
    }

    /// The window's palette: the desktop's, with this scheme and accent.
    pub fn tokens(&self, system: &Tokens) -> Tokens {
        let scheme = match self.theme {
            ThemeMode::Light => Scheme::PreferLight,
            ThemeMode::Dark => Scheme::PreferDark,
            ThemeMode::System | ThemeMode::Both => system.scheme,
        };
        let accent = self.accent.unwrap_or(system.accent_name);
        Tokens::new(accent, scheme, system.reduced_motion)
    }

    /// The schemes a specimen is shown in: the window's, or both.
    pub fn variants(&self) -> &'static [Option<bool>] {
        match self.theme {
            ThemeMode::Both => &[Some(false), Some(true)],
            _ => &[None],
        }
    }
}

/// The palette the desktop reported at start, before the switcher.
#[derive(Clone, Copy, PartialEq)]
pub struct SystemTokens(pub Tokens);

/// The environment state, from anywhere under the window root.
pub fn use_env() -> State<Env> {
    use_consume::<State<Env>>()
}

/// Provide the environment and the palette it resolves to. Call once in the
/// window root. Replaces `provide_tokens`: the palette comes from the
/// switcher, not from `gsettings`.
pub fn provide_env() -> Tokens {
    let env = use_provide_root_context(|| State::create(Env::initial()));
    let system = use_hook(|| SystemTokens(load_tokens()));
    use_provide_root_context(move || system);
    let tokens = env.read().tokens(&system.0);
    let shared = use_hook(|| SharedTokens(State::create(tokens)));
    use_provide_root_context(move || shared);
    let mut palette = shared.0;
    palette.set_if_modified(tokens);
    let mut theme_state = use_init_theme(|| freya_theme(&tokens));
    use_side_effect(move || {
        let tokens = *shared.0.read();
        theme_state.set(freya_theme(&tokens));
    });
    // Density and scale belong to the window, not to the palette: apply them
    // when the switcher changes them, and once at start.
    use_hook(move || apply_window(*env.peek()));
    tokens
}

/// Apply the density and scale of `env` to the window.
pub fn apply_window(env: Env) {
    let window = Platform::get().current_window().id;
    force_density(
        window,
        match env.density {
            DensityMode::Auto => None,
            DensityMode::Desktop => Some((SizeClass::Regular, PointerKind::Mouse)),
            DensityMode::Phone => Some((SizeClass::Compact, PointerKind::Touch)),
        },
    );
    Platform::get()
        .current_window()
        .set_custom_scale_factor(f64::from(env.scale) / 100.0);
    // The stress switches are text-making state, not window state, but they
    // follow the same switcher; the root is keyed on them, so it rebuilds.
    bingux_ui::stress::set_pseudo_locale(env.pseudo);
    bingux_ui::stress::set_text_scale(env.text_scale as f32 / 100.0);
}

/// A word for the environment, for the header and a snapshot's file name.
pub fn describe(env: &Env) -> String {
    let mut text = format!(
        "{} · {} · {} · {}%",
        env.theme.label(),
        env.accent.unwrap_or("system accent"),
        env.density.label(),
        env.scale
    );
    if env.pseudo {
        text.push_str(" · pseudo-locale");
    }
    if env.text_scale != 100 {
        text.push_str(&format!(" · text {}%", env.text_scale));
    }
    text
}

/// The key of the window density a stage borrows: a window of its own that
/// is held at one density, so a desktop specimen stays desktop in a phone
/// window and the other way round.
fn stage_key(compact: bool) -> u64 {
    if compact { 0xB1A0_0002 } else { 0xB1A0_0001 }
}

/// One specimen of one entry at one density and scheme.
#[derive(PartialEq)]
pub struct Stage {
    pub entry: usize,
    pub spec: usize,
    pub compact: bool,
    /// `Some(true)` dark, `Some(false)` light, `None` the window's own.
    pub dark: Option<bool>,
}

impl Component for Stage {
    fn render(&self) -> impl IntoElement {
        let window_tokens = use_tokens();
        let tokens = match self.dark {
            None => window_tokens,
            Some(dark) => Tokens::new(
                window_tokens.accent_name,
                if dark {
                    Scheme::PreferDark
                } else {
                    Scheme::PreferLight
                },
                window_tokens.reduced_motion,
            ),
        };
        let shared = use_provide_context(|| SharedTokens(State::create(tokens)));
        let mut palette = shared.0;
        palette.set_if_modified(tokens);
        let mut theme_state = use_init_theme(|| freya_theme(&tokens));
        use_side_effect(move || {
            let tokens = *shared.0.read();
            theme_state.set(freya_theme(&tokens));
        });
        let compact = self.compact;
        let key = stage_key(compact);
        let density = use_hook(move || {
            force_density(
                key,
                Some(if compact {
                    (SizeClass::Compact, PointerKind::Touch)
                } else {
                    (SizeClass::Regular, PointerKind::Mouse)
                }),
            );
            window_density(key)
        });
        use_provide_context(move || density);
        // A specimen is a picture of a window, not a second window: its window
        // buttons and header drag must not act on this one.
        use_provide_context(|| bingux_ui::shell::InertControls);
        let frame = frame_of(self.entry, self.spec);
        let width = frame.width(compact);
        let stage = rect()
            .tw(
                &tokens,
                &[
                    flex_col(),
                    p(S3),
                    rounded(Radius::Card),
                    bg(Paint::Bg),
                    border(Line::Hairline),
                    ink(Paint::Text),
                    clip(),
                ],
            )
            .font_family(bingux_ui::theme::font_family())
            .width(Size::px(width))
            // A phone-sized window has a page column narrower than the phone
            // stage: the stage gives way, so it is not cut by the window edge.
            .max_width(Size::percent(100.0));
        let stage = match frame.height() {
            Some(height) => stage.height(Size::px(height)),
            None => stage,
        };
        stage.child(crate::view::keyed(
            (self.entry, self.spec),
            Specimen {
                entry: self.entry,
                spec: self.spec,
            },
        ))
    }
}

/// The demo itself, called inside a component so it may use hooks.
#[derive(PartialEq)]
struct Specimen {
    entry: usize,
    spec: usize,
}

impl Component for Specimen {
    fn render(&self) -> impl IntoElement {
        match ENTRIES[self.entry].body {
            Body::States(specs) => (specs[self.spec].build)(),
            Body::Custom(build) => build(),
        }
    }
}

fn frame_of(entry: usize, spec: usize) -> Frame {
    match ENTRIES[entry].body {
        Body::States(specs) => specs[spec].frame,
        Body::Custom(_) => Frame::Wide,
    }
}

/// "live" or "forced", as the tag beside a state's name.
pub fn kind_tag(kind: Kind) -> &'static str {
    match kind {
        Kind::Live => "live",
        Kind::Forced => "forced",
    }
}
