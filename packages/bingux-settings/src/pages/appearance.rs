//! Appearance: the wallpaper, the font, the accent colour and the colour
//! scheme.
//!
//! The wallpaper and the font are `desktop.wallpaper` and
//! `appearance.font_family` in `bingux-config`, which the shell reads. The
//! accent and colour scheme are still GNOME's `org.gnome.desktop.interface`
//! keys, so the shell, the portal and GNOME apps all see them; without
//! gsettings those two groups are left out. Changing either updates the
//! shared palette at once, so the whole window recolours.

use std::path::{Path, PathBuf};
use std::time::Duration;

use freya::prelude::*;

use crate::pages::bingux_config;
use crate::routes::{Route, use_navigator};
use crate::system::{self, CommandError, background, background_then, which};
use crate::theme::{Scheme, SharedTokens, Tokens, accent_for, accent_names, read_gsettings};
use crate::ui::{
    Group, Status, choice_list, choice_row, page_column, row, row_button, skeleton, status,
    use_tokens,
};
use bingux_ui::scale::*;

const INTERFACE: &str = "org.gnome.desktop.interface";
const WALLPAPER_KEY: &str = "desktop.wallpaper";
const FONT_KEY: &str = "appearance.font_family";

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

/// Opens the chooser, stores the picked file and reads the value back.
/// Resolves to `None` when the user cancelled.
fn choose_wallpaper() -> Result<Option<Model>, CommandError> {
    let Some(path) = system::pick_path("Choose a wallpaper", system::Pick::Image)? else {
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
        let navigator = use_navigator();

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

        let font_name = if model_value.font.is_empty() {
            "System default".to_string()
        } else {
            model_value.font.clone()
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

        page_column()
            .maybe_child(
                error
                    .read()
                    .clone()
                    .map(|message| Element::from(status(Status::Error(message)))),
            )
            .child(
                Group::titled("Wallpaper")
                    .row(match wallpaper_path {
                        Some(path) => row(file_name(&path)).trailing(
                            rect()
                                .direction(Direction::Horizontal)
                                .cross_align(Alignment::Center)
                                .spacing(S3)
                                .child(wallpaper_thumbnail(&path))
                                .child(row_button("Choose picture", move |_| {
                                    run_wallpaper_change(model, error, choose_wallpaper)
                                })),
                        ),
                        None => row("No picture chosen")
                            .subtitle("The system wallpaper is in use.")
                            .trailing(row_button("Choose picture", move |_| {
                                run_wallpaper_change(model, error, choose_wallpaper)
                            })),
                    })
                    .row(
                        row("System wallpaper")
                            .subtitle("Go back to the wallpaper that came with the system.")
                            .trailing(row_button("Use it", move |_| {
                                run_wallpaper_change(model, error, use_gnome_background)
                            })),
                    ),
            )
            .child(
                Group::titled("Font")
                    .description(
                        "The desktop uses it straight away. This window uses it after it is reopened.",
                    )
                    .row(
                        row("Font")
                            .value(font_name)
                            .navigates()
                            .on_press(move |_| navigator.push(Route::Font)),
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
                            .padding((S1, GUTTER - S1))
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
                        .subtitle("Applications follow this. The desktop shell stays dark.")
                    ),
                )
            }))
            .maybe_child((!model_value.gsettings).then(|| {
                Element::from(status(Status::Info(
                    "Accent colour and colour scheme are not available on this system.".into(),
                )))
            }))
    }
}

/// Wallpaper thumbnail: a small rounded crop of the picture, 96 by 36.
fn wallpaper_thumbnail(path: &str) -> Element {
    rect()
        .width(Size::px(BOX_2XL))
        .height(Size::px(BOX_LG))
        .corner_radius(RADIUS)
        .overflow(Overflow::Clip)
        .child(
            ImageViewer::new(PathBuf::from(path))
                .width(Size::fill())
                .height(Size::fill())
                .aspect_ratio(AspectRatio::Max),
        )
        .into()
}

/// Every installed font family, sorted, each listed once under its first name.
fn installed_fonts() -> Result<Vec<String>, CommandError> {
    let text = system::run("fc-list", &[":", "family"], Duration::from_secs(5))?;
    let mut families: Vec<String> = text
        .lines()
        .filter_map(|line| line.split(',').next())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect();
    families.sort_by_key(|name| name.to_lowercase());
    families.dedup();
    Ok(families)
}

/// The font subpage: every installed family, narrowed by typing. The first
/// choice restores the system font.
#[derive(PartialEq)]
pub struct FontPage;

impl Component for FontPage {
    fn render(&self) -> impl IntoElement {
        let model: State<Option<Loaded>> = system::cached("appearance");
        let navigator = use_navigator();
        let error = use_state::<Option<String>>(|| None);
        let fonts = use_state::<Option<Result<Vec<String>, CommandError>>>(|| None);
        use_hook(move || background(fonts, installed_fonts));

        let list = match fonts.read().clone() {
            Some(Ok(list)) => list,
            Some(Err(failure)) => {
                return page_column().child(status(Status::Error(failure.0)));
            }
            None => return page_column().child(skeleton()),
        };
        let current = match model.read().as_ref() {
            Some(Ok(data)) => data.font.clone(),
            _ => String::new(),
        };
        let mut options = vec![(String::new(), "System default".to_string())];
        options.extend(list.into_iter().map(|family| (family.clone(), family)));

        let mut page = page_column();
        if let Some(message) = error.read().clone() {
            page = page.child(status(Status::Error(message)));
        }
        page.child(
            choice_list(options, current, move |family: String| {
                let mut error = error;
                let mut model = model;
                background_then(
                    move || set_font(&family),
                    move |result| match result {
                        Ok(Some(loaded)) => {
                            error.set(None);
                            model.set(Some(Ok(loaded)));
                            navigator.pop();
                        }
                        Ok(None) => {}
                        Err(failure) => error.set(Some(failure.0)),
                    },
                );
            })
            .placeholder("Search for a font"),
        )
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("appearance", load);
}
