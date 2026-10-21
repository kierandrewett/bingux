//! Appearance: the wallpaper, the accent colour and the colour scheme.
//!
//! The wallpaper is `desktop.wallpaper` in `bingux-config`, which the shell
//! draws. The accent and colour scheme are GNOME's `org.gnome.desktop.interface`
//! keys, so the shell, the portal and GNOME apps all see them. Changing either
//! updates the shared palette at once, so the whole window recolours.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use freya::prelude::*;

use crate::pages::bingux_config;
use crate::system::{self, CommandError, background, background_then, which};
use crate::theme::{Scheme, SharedTokens, Tokens, accent_for, accent_names, read_gsettings};
use crate::ui::{Group, Status, choice_row, page_title, row, row_button, status, use_tokens};

const INTERFACE: &str = "org.gnome.desktop.interface";
const WALLPAPER_KEY: &str = "desktop.wallpaper";
const IMAGE_PATTERNS: &str = "*.png *.jpg *.jpeg *.webp *.jxl *.avif";
const SWATCH: f32 = 24.0;
const SWATCH_RING: f32 = 36.0;

#[derive(Clone, PartialEq)]
struct Model {
    /// Absolute path of the wallpaper, or empty for the GNOME background.
    wallpaper: String,
}

type Loaded = Result<Model, CommandError>;

fn config(args: &[&str]) -> Result<String, CommandError> {
    system::run(&bingux_config(), args, Duration::from_secs(5))
}

/// Checks the GNOME keys are readable and reads the current wallpaper.
fn load() -> Loaded {
    if !which("gsettings") {
        return Err(CommandError("gsettings is not installed.".into()));
    }
    for key in ["accent-color", "color-scheme"] {
        if read_gsettings(key).is_none() {
            return Err(CommandError(format!(
                "gsettings could not read {INTERFACE} {key}."
            )));
        }
    }
    let wallpaper = config(&["get", WALLPAPER_KEY])?;
    Ok(Model { wallpaper })
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

/// One accent circle. The selected one is ringed in the text colour.
fn swatch(
    name: &'static str,
    color: Color,
    selected: bool,
    ring: Color,
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    let border = if selected {
        Border::new()
            .width(2.0)
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
            .width(Size::px(SWATCH_RING))
            .height(Size::px(SWATCH_RING))
            .center()
            .corner_radius(SWATCH_RING / 2.0)
            .border(border)
            .cursor(CursorIcon::Pointer)
            .on_press(on_press)
            .child(
                rect()
                    .width(Size::px(SWATCH))
                    .height(Size::px(SWATCH))
                    .corner_radius(SWATCH / 2.0)
                    .background(color),
            ),
    )
}

#[derive(PartialEq)]
pub struct AppearancePage;

impl Component for AppearancePage {
    fn render(&self) -> impl IntoElement {
        let model = use_state::<Option<Loaded>>(|| None);
        let error = use_state::<Option<String>>(|| None);
        let shared = use_consume::<SharedTokens>().0;
        use_hook(move || background(model, load));
        let tokens = use_tokens();

        let Some(loaded) = model.read().clone() else {
            return rect()
                .width(Size::fill())
                .child(page_title("Appearance"))
                .child(status(Status::Loading(
                    "Reading appearance settings.".into(),
                )));
        };
        let model_value = match loaded {
            Ok(value) => value,
            Err(failure) => {
                return rect()
                    .width(Size::fill())
                    .child(page_title("Appearance"))
                    .child(
                        status(Status::Error(failure.0))
                            .action("Try again", move |_| reload(model)),
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

        rect()
            .width(Size::fill())
            .child(page_title("Appearance"))
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
                        row("Choose a picture from disk")
                            .trailing(row_button("Choose image", move |_| {
                                run_wallpaper_change(model, error, choose_wallpaper)
                            })),
                    )
                    .row(
                        row("Use the GNOME background")
                            .trailing(row_button("Use GNOME background", move |_| {
                                run_wallpaper_change(model, error, use_gnome_background)
                            })),
                    ),
            )
            .child(
                Group::titled("Accent colour").row(
                    rect()
                        .width(Size::fill())
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .cross_align(Alignment::Center)
                        .padding((4.0, 16.0))
                        .spacing(10.0)
                        .children(swatches),
                ),
            )
            // The shell and this window are always dark; the scheme is what
            // GTK applications and the portal follow, and it picks the
            // accent variant.
            .child(
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
                    .subtitle("Followed by GTK applications; the shell stays dark."),
                ),
            )
            .child(
                Group::titled("Motion").row(
                    row("Reduced motion")
                        .subtitle("Set with BINGUX_REDUCED_MOTION=1 when the window starts.")
                        .value(reduced_motion),
                ),
            )
    }
}
