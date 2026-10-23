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
    Group, Icon, Status, choice_list, icon, page_column, row, row_button, skeleton, status,
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
/// an even pitch. The selected one is ringed in the text colour and carries a
/// check. Hovering shows the accent's name.
fn swatch(
    name: &'static str,
    color: Color,
    selected: bool,
    ring: Color,
    check: Color,
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
    let mark = selected.then(|| Element::from(icon(Icon::Check, ICON_MD, check)));
    let button = rect()
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
                .background(color)
                .center()
                .maybe_child(mark),
        );
    let title = format!("{}{}", name[..1].to_uppercase(), &name[1..]);
    Element::from(TooltipContainer::new(Tooltip::new_text(title)).child(button))
}

/// A grey line standing in for text in a scheme card.
fn preview_line(width: Size, color: Color) -> impl IntoElement {
    rect()
        .width(width)
        .height(Size::px(S1))
        .corner_radius(RADIUS_SM)
        .background(color)
}

/// A small window in one colour scheme: a title bar, a sidebar and content.
/// The colours are fixed so each card shows its scheme, whatever is current.
fn mock_window(dark: bool, accent: Color, width: Size) -> Element {
    let (window, bar, sidebar, line) = if dark {
        (
            Color::from_rgb(38, 38, 42),
            Color::from_rgb(56, 56, 62),
            Color::from_rgb(47, 47, 53),
            Color::from_rgb(100, 100, 108),
        )
    } else {
        (
            Color::from_rgb(250, 250, 250),
            Color::from_rgb(228, 228, 228),
            Color::from_rgb(238, 238, 238),
            Color::from_rgb(186, 186, 186),
        )
    };
    let side = rect()
        .width(Size::px(PREVIEW_SIDEBAR))
        .height(Size::fill())
        .background(sidebar)
        .padding(S2)
        .spacing(S2)
        .child(preview_line(Size::fill(), line))
        .child(preview_line(Size::fill(), line));
    let content = rect()
        .width(Size::flex(1.0))
        .height(Size::fill())
        .background(window)
        .padding(S2)
        .spacing(S2)
        .child(preview_line(Size::px(PREVIEW_LINE), accent))
        .child(preview_line(Size::fill(), line))
        .child(preview_line(Size::fill(), line));
    let body = rect()
        .width(Size::fill())
        .height(Size::flex(1.0))
        .direction(Direction::Horizontal)
        .child(side)
        .child(content);
    let top = rect()
        .width(Size::fill())
        .height(Size::px(S4))
        .background(bar);
    Element::from(
        rect()
            .width(width)
            .height(Size::fill())
            .background(window)
            .child(top)
            .child(body),
    )
}

/// One colour scheme card, drawn as a small window, with its name under it.
/// The chosen card is ringed in the accent colour, as GNOME's Style switcher.
fn scheme_card(
    title: &'static str,
    scheme: Scheme,
    tokens: Tokens,
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    let selected = tokens.scheme == scheme;
    let windows = match scheme {
        Scheme::Default => Element::from(
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .direction(Direction::Horizontal)
                .child(mock_window(false, tokens.accent, Size::flex(1.0)))
                .child(mock_window(true, tokens.accent, Size::flex(1.0))),
        ),
        Scheme::PreferLight => mock_window(false, tokens.accent, Size::fill()),
        Scheme::PreferDark => mock_window(true, tokens.accent, Size::fill()),
    };
    let border = Border::new()
        .width(RING_WIDTH)
        .alignment(BorderAlignment::Inner)
        .fill(if selected {
            tokens.accent
        } else {
            tokens.outline
        });
    let content = rect()
        .width(Size::fill())
        .height(Size::fill())
        .corner_radius(RADIUS_LG)
        .overflow(Overflow::Clip)
        .child(windows);
    let frame = rect()
        .width(Size::px(PREVIEW_WIDTH))
        .height(Size::px(PREVIEW_HEIGHT))
        .corner_radius(RADIUS_LG)
        .border(border)
        .padding((RING_WIDTH, RING_WIDTH))
        .child(content);
    let caption = label()
        .text(title)
        .font_size(TEXT_MD)
        .color(tokens.text)
        .font_weight(if selected {
            FontWeight::SEMI_BOLD
        } else {
            FontWeight::NORMAL
        });
    Element::from(
        rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("{title} colour scheme"))
            .a11y_focusable(true)
            .width(Size::px(PREVIEW_WIDTH))
            .spacing(S2)
            .cross_align(Alignment::Center)
            .on_press(on_press)
            .child(frame)
            .child(caption),
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
                    tokens.accent_foreground,
                    move |_| apply_accent(shared, error, name),
                )
            })
            .collect();

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
                    Group::titled("Colour scheme")
                        .description("Applications follow this. The desktop shell stays dark.")
                        .row(
                            rect()
                                .width(Size::fill())
                                .direction(Direction::Horizontal)
                                .main_align(Alignment::Center)
                                .padding((S3, GUTTER - S1))
                                .spacing(S4)
                                .child(scheme_card(
                                    "Default",
                                    Scheme::Default,
                                    tokens,
                                    move |_| apply_scheme(shared, error, Scheme::Default),
                                ))
                                .child(scheme_card(
                                    "Light",
                                    Scheme::PreferLight,
                                    tokens,
                                    move |_| apply_scheme(shared, error, Scheme::PreferLight),
                                ))
                                .child(scheme_card(
                                    "Dark",
                                    Scheme::PreferDark,
                                    tokens,
                                    move |_| apply_scheme(shared, error, Scheme::PreferDark),
                                )),
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
