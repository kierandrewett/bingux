//! Appearance: the wallpaper, the font, the accent colour and the colour
//! scheme.
//!
//! The wallpaper and the font are `desktop.wallpaper` and
//! `appearance.font_family` in `bingux-config`, which the shell reads. The
//! accent and colour scheme are still GNOME's `org.gnome.desktop.interface`
//! keys, so the shell, the portal and GNOME apps all see them; without
//! gsettings those two groups are left out. Changing either updates the
//! shared palette at once, so the whole window recolours.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use freya::prelude::*;

use crate::pages::bingux_config;
use crate::system::{self, CommandError, background, background_then, which};
use crate::theme::{Scheme, SharedTokens, Tokens, accent_for, accent_names, read_gsettings};
use crate::ui::{
    Group, Status, choice_row, entry_row, page_column, row, row_button, skeleton, status,
    use_tokens,
};
use bingux_ui::scale::*;

const INTERFACE: &str = "org.gnome.desktop.interface";
const WALLPAPER_KEY: &str = "desktop.wallpaper";
const FONT_KEY: &str = "appearance.font_family";
const IMAGE_PATTERNS: &str = "*.png *.jpg *.jpeg *.webp *.jxl *.avif";

#[derive(Clone, PartialEq)]
struct Model {
    /// Absolute path of the wallpaper, or empty for the GNOME background.
    wallpaper: String,
    /// `appearance.font_family`; empty is the default font.
    font: String,
    /// Whether gsettings can read the accent and colour scheme.
    gsettings: bool,
}

type Loaded = Result<Model, CommandError>;

fn config(args: &[&str]) -> Result<String, CommandError> {
    system::run(&bingux_config(), args, Duration::from_secs(5))
}

/// Reads the wallpaper and font, and whether the GNOME keys are readable.
fn load() -> Loaded {
    let gsettings = which("gsettings")
        && ["accent-color", "color-scheme"]
            .iter()
            .all(|key| read_gsettings(key).is_some());
    let wallpaper = config(&["get", WALLPAPER_KEY])?;
    let font = config(&["get", FONT_KEY])?;
    Ok(Model {
        wallpaper,
        font,
        gsettings,
    })
}

/// Stores a font family after checking that it is installed. Empty restores
/// the default.
fn set_font(family: &str) -> Result<Option<Model>, CommandError> {
    let family = family.trim();
    if family.is_empty() {
        config(&["unset", FONT_KEY])?;
    } else {
        if crate::theme::font_files(family, 1).is_empty() {
            return Err(CommandError(format!("The font {family} is not installed.")));
        }
        config(&["set", FONT_KEY, family])?;
    }
    load().map(Some)
}

fn reload(mut model: State<Option<Loaded>>) {
    model.set(None);
    background(model, load);
}

/// Shows the file chooser and returns the picked path, or `None` when the
/// user cancels. Uses zenity when present, otherwise kdialog.
fn pick_image() -> Result<Option<String>, CommandError> {
    let mut command = if which("zenity") {
        let mut command = Command::new("zenity");
        command
            .arg("--file-selection")
            .arg("--title=Choose a wallpaper")
            .arg(format!("--file-filter=Images | {IMAGE_PATTERNS}"));
        command
    } else if which("kdialog") {
        let mut command = Command::new("kdialog");
        command
            .args(["--title", "Choose a wallpaper", "--getopenfilename"])
            .arg(std::env::var("HOME").unwrap_or_else(|_| ".".into()))
            .arg(format!("Images ({IMAGE_PATTERNS})"));
        command
    } else {
        return Err(CommandError(
            "Neither zenity nor kdialog is installed.".into(),
        ));
    };
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|error| CommandError(format!("Could not start the file chooser: {error}")))?;

    if !output.status.success() {
        // Both dialogs exit 1 with nothing on stderr when the user cancels.
        let stderr = String::from_utf8_lossy(&output.stderr);
        return match stderr.trim().lines().last() {
            None => Ok(None),
            Some(detail) => Err(CommandError(format!("File chooser: {}", detail.trim()))),
        };
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!path.is_empty()).then_some(path))
}

/// Opens the chooser, stores the picked file and reads the value back.
/// Resolves to `None` when the user cancelled.
fn choose_wallpaper() -> Result<Option<Model>, CommandError> {
    let Some(path) = pick_image()? else {
        return Ok(None);
    };
    config(&["set", WALLPAPER_KEY, path.as_str()])?;
    load().map(Some)
}

/// Removes the wallpaper setting so the GNOME background applies again.
fn use_gnome_background() -> Result<Option<Model>, CommandError> {
    config(&["unset", WALLPAPER_KEY])?;
    load().map(Some)
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Sets the accent immediately, writes GNOME's key, and rolls back on failure.
fn apply_accent(mut shared: State<Tokens>, mut error: State<Option<String>>, name: &'static str) {
    let previous = *shared.read();
    shared.set(Tokens::new(name, previous.scheme, previous.reduced_motion));
    let value = format!("'{name}'");
    background_then(
        move || system::gsettings_set(INTERFACE, "accent-color", &value),
        move |result| match result {
            Ok(_) => error.set(None),
            Err(failure) => {
                shared.set(previous);
                error.set(Some(failure.0));
            }
        },
    );
}

/// Sets the colour scheme immediately, writes GNOME's key, and rolls back on
/// failure.
fn apply_scheme(mut shared: State<Tokens>, mut error: State<Option<String>>, scheme: Scheme) {
    let previous = *shared.read();
    shared.set(Tokens::new(
        previous.accent_name,
        scheme,
        previous.reduced_motion,
    ));
    let value = format!("'{}'", scheme.gsettings_value());
    background_then(
        move || system::gsettings_set(INTERFACE, "color-scheme", &value),
        move |result| match result {
            Ok(_) => error.set(None),
            Err(failure) => {
                shared.set(previous);
                error.set(Some(failure.0));
            }
        },
    );
}

/// Runs a wallpaper change in the background and stores the reloaded model.
/// A cancelled chooser leaves the page as it was.
fn run_wallpaper_change(
    mut model: State<Option<Loaded>>,
    mut error: State<Option<String>>,
    work: fn() -> Result<Option<Model>, CommandError>,
) {
    background_then(work, move |result| match result {
        Ok(Some(loaded)) => {
            error.set(None);
            model.set(Some(Ok(loaded)));
        }
        Ok(None) => {}
        Err(failure) => error.set(Some(failure.0)),
    });
}

/// One accent circle, set in a box of the icon-button size so the row keeps
/// an even pitch. The selected one is ringed in the text colour.
fn swatch(
    name: &'static str,
    color: Color,
    selected: bool,
    ring: Color,
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    let border = if selected {
        Border::new()
            .width(RING_WIDTH)
            .alignment(BorderAlignment::Inner)
            .fill(ring)
    } else {
        Border::new()
    };
    Element::from(
        rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("Accent colour {name}"))
            .a11y_focusable(true)
            .width(Size::px(BOX_LG))
            .height(Size::px(BOX_LG))
            .center()
            .corner_radius(BOX_LG / 2.0)
            .border(border)
            .on_press(on_press)
            .child(
                rect()
                    .width(Size::px(BOX_MD))
                    .height(Size::px(BOX_MD))
                    .corner_radius(BOX_MD / 2.0)
                    .background(color),
            ),
    )
}

#[derive(PartialEq)]
pub struct AppearancePage;

impl Component for AppearancePage {
    fn render(&self) -> impl IntoElement {
        let model = system::use_cached("appearance", load);
        let error = use_state::<Option<String>>(|| None);
        let shared = use_consume::<SharedTokens>().0;
        let tokens = use_tokens();

        let Some(loaded) = model.read().clone() else {
            return page_column().child(skeleton());
        };
        let model_value = match loaded {
            Ok(value) => value,
            Err(failure) => {
                return page_column().child(
                    status(Status::Error(failure.0)).action("Try again", move |_| reload(model)),
                );
            }
        };

        let wallpaper_name = if model_value.wallpaper.is_empty() {
            "GNOME background".to_string()
        } else {
            file_name(&model_value.wallpaper)
        };
        let wallpaper_path = Some(model_value.wallpaper.clone()).filter(|path| !path.is_empty());

        let swatches: Vec<Element> = accent_names()
            .map(|name| {
                swatch(
                    name,
                    accent_for(name, tokens.scheme.is_dark()),
                    name == tokens.accent_name,
                    tokens.text,
                    move |_| apply_accent(shared, error, name),
                )
            })
            .collect();

        let scheme_options: Vec<(String, String)> = vec![
            (Scheme::Default.gsettings_value().into(), "Default".into()),
            (Scheme::PreferLight.gsettings_value().into(), "Light".into()),
            (Scheme::PreferDark.gsettings_value().into(), "Dark".into()),
        ];

        let reduced_motion = if tokens.reduced_motion { "On" } else { "Off" };

        page_column()
            .maybe_child(
                error
                    .read()
                    .clone()
                    .map(|message| Element::from(status(Status::Error(message)))),
            )
            .child(
                Group::titled("Wallpaper")
                    .row(
                        row("Current wallpaper")
                            .maybe_subtitle(wallpaper_path)
                            .value(wallpaper_name),
                    )
                    .row(
                        row("Choose a picture").trailing(row_button("Choose image", move |_| {
                            run_wallpaper_change(model, error, choose_wallpaper)
                        })),
                    )
                    .row(
                        row("GNOME background").trailing(row_button("Use GNOME", move |_| {
                            run_wallpaper_change(model, error, use_gnome_background)
                        })),
                    ),
            )
            .child(
                Group::titled("Font")
                    .description("The shell uses it at once. This window uses it after a restart.")
                    .row(
                        entry_row("Font family", model_value.font.clone(), {
                            move |family: String| {
                                background_then(
                                    move || set_font(&family),
                                    move |result| match result {
                                        Ok(Some(loaded)) => {
                                            let mut error = error;
                                            let mut model = model;
                                            error.set(None);
                                            model.set(Some(Ok(loaded)));
                                        }
                                        Ok(None) => {}
                                        Err(failure) => {
                                            let mut error = error;
                                            error.set(Some(failure.0));
                                        }
                                    },
                                );
                            }
                        })
                        .placeholder("Adwaita Sans"),
                    ),
            )
            .maybe_child(model_value.gsettings.then(|| {
                Element::from(
                    Group::titled("Accent colour").row(
                        rect()
                            .width(Size::fill())
                            .direction(Direction::Horizontal)
                            .content(Content::Flex)
                            .cross_align(Alignment::Center)
                            .padding((S1, S0))
                            .spacing(S1)
                            .children(swatches),
                    ),
                )
            }))
            // The shell and this window are always dark; the scheme is what
            // GTK applications and the portal follow, and it picks the
            // accent variant.
            .maybe_child(model_value.gsettings.then(|| {
                Element::from(
                    Group::titled("Colour scheme").row(
                        choice_row(
                            "Applications",
                            scheme_options,
                            tokens.scheme.gsettings_value(),
                            move |key: String| {
                                let scheme = match key.as_str() {
                                    "prefer-light" => Scheme::PreferLight,
                                    "prefer-dark" => Scheme::PreferDark,
                                    _ => Scheme::Default,
                                };
                                apply_scheme(shared, error, scheme);
                            },
                        )
                        .subtitle("GTK applications follow this. The shell stays dark."),
                    ),
                )
            }))
            .maybe_child((!model_value.gsettings).then(|| {
                Element::from(status(Status::Info(
                    "Accent colour and colour scheme need gsettings, which is not available."
                        .into(),
                )))
            }))
            .child(
                Group::titled("Motion").row(
                    row("Reduced motion")
                        .subtitle("Set BINGUX_REDUCED_MOTION=1 to turn it on.")
                        .value(reduced_motion),
                ),
            )
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("appearance", load);
}
