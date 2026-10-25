//! Appearance: the wallpaper, the font, the accent colour and the colour
//! scheme.
//!
//! The wallpaper and the font are `desktop.wallpaper` and
//! `appearance.font_family` in `bingux-config`, which the shell reads. The
//! accent and colour scheme are still GNOME's `org.gnome.desktop.interface`
//! keys, so the shell, the portal and GNOME apps all see them; without
//! gsettings those two groups are left out. Changing either updates the
//! shared palette at once, so the whole window recolours.
//!
//! The page opens on a picture of the desktop: the chosen wallpaper with the
//! top bar, a window and the dock. The colour scheme cards show the same
//! desktop in each scheme. The font subpage lives in `appearance/fonts.rs`.

mod fonts;
mod interface;
mod wallpapers;

pub use fonts::FontPage;
use wallpapers::WallpaperPicker;

use std::path::Path;
use std::time::Duration;

use freya::prelude::*;

use crate::pages::bingux_config;
use crate::pages::desktop::{
    self,
    preview::{Frame, Scene, framed, scene},
};
use crate::routes::{Route, use_navigator};
use crate::system::{
    self, CommandError, background, background_then, optimistic, optimistic_then, which,
};
use crate::theme::{Scheme, SharedTokens, Tokens, accent_names, read_gsettings};
use crate::ui::typography::{BODY, EMPHASIS};
use crate::ui::{
    Group, Icon, Overlays, PageHead, Status, focus_stop, icon, menu_item, menu_separator,
    page_column, row, skeleton_shape, status, use_overlays, use_tokens,
};
use bingux_ui::style::*;
use interface::{Interface, Setting, text_size_row, theme_group};

const INTERFACE: &str = "org.gnome.desktop.interface";
const WALLPAPER_KEY: &str = "desktop.wallpaper";
const FONT_KEY: &str = "appearance.font_family";

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Model {
    /// Absolute path of the wallpaper, or empty for the GNOME background.
    wallpaper: String,
    /// `appearance.font_family`; empty is the default font.
    font: String,
    /// Whether gsettings can read the accent and colour scheme.
    gsettings: bool,
    /// The desktop settings, which the picture's dock and sidebar follow.
    /// `None` when they could not be read.
    desktop: Option<desktop::Model>,
    /// The GNOME interface keys and installed themes; `None` when they could not be read.
    interface: Option<Interface>,
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
        desktop: desktop::load().ok(),
        interface: Interface::read(),
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

/// Loads the page again and saves the result for the next start.
fn refresh() -> Loaded {
    system::load_persistent("appearance", load)
}

fn reload(mut model: State<Option<Loaded>>) {
    model.set(None);
    background(model, refresh);
}

/// Stores the wallpaper: a picture's path, or `None` for the GNOME background.
/// The new picture shows at once. The model is reloaded from the shell once the
/// write has landed, and a failed write puts the old one back with a toast.
fn set_wallpaper(mut model: State<Option<Loaded>>, overlays: Overlays, path: Option<String>) {
    let previous = model.read().clone();
    let Some(Ok(current)) = previous.clone() else {
        return;
    };
    let mut value = current;
    value.wallpaper = path.clone().unwrap_or_default();
    let work = move || match path {
        Some(path) => config(&["set", WALLPAPER_KEY, path.as_str()]),
        None => config(&["unset", WALLPAPER_KEY]),
    };
    optimistic_then(
        model,
        Some(overlays),
        previous,
        Some(Ok(value)),
        move || work().map(|_| ()),
        move || background_then(refresh, move |loaded| model.set(Some(loaded))),
    );
}

/// Opens the picture chooser on a background thread, then stores the picked
/// file. A cancelled chooser leaves the page as it was.
fn choose_wallpaper(model: State<Option<Loaded>>, overlays: Overlays) {
    background_then(
        || system::pick_path("Choose a wallpaper", system::Pick::Image),
        move |picked| match picked {
            Ok(Some(path)) => set_wallpaper(model, overlays, Some(path)),
            Ok(None) => {}
            Err(failure) => overlays.toast(failure.0),
        },
    );
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Sets the accent at once, writes GNOME's key in the background, and rolls
/// back on failure.
fn apply_accent(shared: State<Tokens>, overlays: Overlays, name: &'static str) {
    let previous = *shared.read();
    let value = Tokens::new(name, previous.scheme, previous.reduced_motion);
    let text = format!("'{name}'");
    optimistic(shared, Some(overlays), previous, value, move || {
        system::gsettings_set(INTERFACE, "accent-color", &text).map(|_| ())
    });
}

/// Sets the colour scheme at once, writes GNOME's key in the background, and
/// rolls back on failure.
fn apply_scheme(shared: State<Tokens>, overlays: Overlays, scheme: Scheme) {
    let previous = *shared.read();
    let value = Tokens::new(previous.accent_name, scheme, previous.reduced_motion);
    let text = format!("'{}'", scheme.gsettings_value());
    optimistic(shared, Some(overlays), previous, value, move || {
        system::gsettings_set(INTERFACE, "color-scheme", &text).map(|_| ())
    });
}

/// Stores one interface key at once and writes it in the background. The model
/// is reloaded once the write has landed, and a failed write puts the old value
/// back with a toast.
fn change(mut model: State<Option<Loaded>>, overlays: Overlays, setting: Setting) {
    let previous = model.read().clone();
    let Some(Ok(current)) = previous.clone() else {
        return;
    };
    let mut value = current;
    if let Some(interface) = value.interface.as_mut() {
        interface.apply_local(&setting);
    }
    optimistic_then(
        model,
        Some(overlays),
        previous,
        Some(Ok(value)),
        move || setting.write(),
        move || background_then(refresh, move |loaded| model.set(Some(loaded))),
    );
}

/// One accent circle, set in a box of the control size so the row keeps an
/// even pitch. The selected one is ringed in the text colour and carries a
/// check. Hovering shows the accent's name.
fn swatch(
    tokens: &Tokens,
    name: &'static str,
    selected: bool,
    on_press: impl Into<EventHandler<()>>,
) -> Element {
    let mark = selected.then(|| {
        Element::from(icon(
            Icon::Check,
            IconSize::Control,
            Paint::OnAccent.color(tokens),
        ))
    });
    let button = focus_stop(
        rect()
            .tw(
                tokens,
                &[w(Dim::Box9), h(Dim::Box9), center(), rounded(Radius::Pill)],
            )
            .maybe(selected, |ring| {
                ring.tw(tokens, &[border(Line::Ring(Paint::Text))])
            })
            .child(
                rect()
                    .tw(
                        tokens,
                        &[
                            w(Dim::Box7),
                            h(Dim::Box7),
                            rounded(Radius::Pill),
                            bg(Paint::Swatch(name)),
                            center(),
                        ],
                    )
                    .maybe_child(mark),
            ),
        format!("Accent colour {name}"),
        AccessibilityRole::Button,
        true,
        Radius::Pill,
        on_press,
    );
    let title = format!("{}{}", name[..1].to_uppercase(), &name[1..]);
    Element::from(TooltipContainer::new(Tooltip::new_text(title)).child(button))
}

/// The scene for the hero and the cards: the settings' dock and sidebar, or a
/// bare picture when the desktop settings could not be read.
fn scene_for<'a>(
    desktop: Option<&'a desktop::Model>,
    dark: bool,
    wallpaper: Option<&'a str>,
) -> Scene<'a> {
    match desktop {
        Some(model) => Scene::of(model, dark, wallpaper),
        None => Scene {
            dark,
            wallpaper,
            dock: None,
            sidebar: None,
        },
    }
}

/// One colour scheme's desktop picture. Cards leave out the wallpaper, so each
/// scheme reads as its colours alone.
fn card_picture(
    tokens: &Tokens,
    dark: bool,
    desktop: Option<&desktop::Model>,
    width: Util,
) -> Element {
    Element::from(rect().tw(tokens, &[width]).child(scene(
        tokens,
        &scene_for(desktop, dark, None),
        Frame::Card,
        None,
    )))
}

/// One colour scheme card: the desktop drawn in that scheme, with its name
/// under it. The chosen card is ringed in the accent colour, as GNOME's Style
/// switcher. The default scheme shows the light and the dark desktop side by
/// side.
fn scheme_card(
    title: &'static str,
    scheme: Scheme,
    tokens: Tokens,
    desktop: Option<&desktop::Model>,
    on_press: impl Into<EventHandler<()>>,
) -> Element {
    let selected = tokens.scheme == scheme;
    let picture = match scheme {
        Scheme::Default => Element::from(
            rect()
                .tw(&tokens, &[flex_row(), w_full(), gap(S2)])
                .child(card_picture(&tokens, false, desktop, w_flex()))
                .child(card_picture(&tokens, true, desktop, w_flex())),
        ),
        Scheme::PreferLight => card_picture(&tokens, false, desktop, w_full()),
        Scheme::PreferDark => card_picture(&tokens, true, desktop, w_full()),
    };
    let ring = if selected {
        Paint::Accent
    } else {
        Paint::Outline
    };
    let frame = rect()
        .tw(
            &tokens,
            &[
                w_full(),
                rounded(Radius::Card),
                border(Line::Ring(ring)),
                p(S1),
            ],
        )
        .child(picture);
    let caption = if selected { EMPHASIS } else { BODY };
    let card = rect()
        .tw(
            &tokens,
            &[flex_col(), w_flex(), gap(S2), items(Align::Center)],
        )
        .child(frame)
        .child(caption.label(&tokens, title));
    Element::from(focus_stop(
        card,
        format!("{title} colour scheme"),
        AccessibilityRole::Button,
        true,
        Radius::Card,
        on_press,
    ))
}

/// Whether a pointer press is a right click, which opens the picture menu.
fn is_secondary(event: &Event<PointerEventData>) -> bool {
    matches!(event.data(), PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Right))
}

#[derive(PartialEq)]
pub struct AppearancePage;

impl Component for AppearancePage {
    fn render(&self) -> impl IntoElement {
        let model = system::use_cached_persistent("appearance", load);
        let overlays = use_overlays();
        let shared = use_consume::<SharedTokens>().0;
        let tokens = use_tokens();
        let navigator = use_navigator();
        let on_setting = move |setting: Setting| change(model, overlays, setting);

        let Some(loaded) = model.read().clone() else {
            // The picture comes first, then the groups, so the hero keeps its
            // place while the data loads.
            return page_column()
                .child(PageHead::new())
                .child(framed(
                    &tokens,
                    Element::from(rect().tw(
                        &tokens,
                        &[w_full(), h(Dim::ArrangementHeight), bg(Paint::Control)],
                    )),
                ))
                .child(skeleton_shape(&[2, 2, 3, 3, 1, 1]));
        };
        let model_value = match loaded {
            Ok(value) => value,
            Err(failure) => {
                return page_column().child(PageHead::new()).child(
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
        let dark = tokens.scheme != Scheme::PreferLight;

        let swatches: Vec<Element> = accent_names()
            .map(|name| {
                swatch(&tokens, name, name == tokens.accent_name, move |_| {
                    apply_accent(shared, overlays, name)
                })
            })
            .collect();

        let picture = rect()
            .tw(&tokens, &[w_full()])
            .on_pointer_down(move |event: Event<PointerEventData>| {
                if !is_secondary(&event) {
                    return;
                }
                let at = event.global_location();
                overlays.show_menu(
                    (at.x as f32, at.y as f32),
                    vec![
                        menu_item("Choose picture", move |_| choose_wallpaper(model, overlays))
                            .icon(Icon::ImageGeneric),
                        menu_separator(),
                        menu_item("Use system wallpaper", move |_| {
                            set_wallpaper(model, overlays, None)
                        }),
                    ],
                );
            })
            .child(framed(
                &tokens,
                scene(
                    &tokens,
                    &scene_for(
                        model_value.desktop.as_ref(),
                        dark,
                        wallpaper_path.as_deref(),
                    ),
                    Frame::Hero,
                    None,
                ),
            ));

        page_column()
            .child(PageHead::new())
            .child(picture)
            .child(WallpaperPicker {
                current: model_value.wallpaper.clone(),
            })
            .child(
                Group::titled("Font")
                    .description(
                        "The desktop applies it immediately. This window applies it when reopened.",
                    )
                    .rows(
                        model_value
                            .interface
                            .as_ref()
                            .map(|interface| text_size_row(interface, on_setting)),
                    )
                    .row(
                        row("Font")
                            .value(font_name)
                            .navigates()
                            .on_press(move |_| navigator.push(Route::Font)),
                    ),
            )
            .maybe_child(
                model_value
                    .interface
                    .as_ref()
                    .map(|interface| Element::from(theme_group(interface, on_setting))),
            )
            .maybe_child(model_value.gsettings.then(|| {
                Element::from(
                    Group::titled("Accent colour").row(
                        rect()
                            .tw(&tokens, BOX_BODY)
                            .tw(
                                &tokens,
                                &[
                                    flex_row(),
                                    wrap(S2),
                                    justify(Align::Center),
                                    items(Align::Center),
                                    gap(S2),
                                ],
                            )
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
                                .tw(&tokens, BOX_BODY)
                                .tw(&tokens, &[items(Align::Center)])
                                .child(
                                    rect()
                                        .tw(
                                            &tokens,
                                            &[
                                                flex_row(),
                                                grow_children(),
                                                w_full(),
                                                max_w(Dim::PreviewRow),
                                                gap(S4),
                                            ],
                                        )
                                        .child(scheme_card(
                                            "Default",
                                            Scheme::Default,
                                            tokens,
                                            model_value.desktop.as_ref(),
                                            move |_| {
                                                apply_scheme(shared, overlays, Scheme::Default)
                                            },
                                        ))
                                        .child(scheme_card(
                                            "Light",
                                            Scheme::PreferLight,
                                            tokens,
                                            model_value.desktop.as_ref(),
                                            move |_| {
                                                apply_scheme(shared, overlays, Scheme::PreferLight)
                                            },
                                        ))
                                        .child(scheme_card(
                                            "Dark",
                                            Scheme::PreferDark,
                                            tokens,
                                            model_value.desktop.as_ref(),
                                            move |_| {
                                                apply_scheme(shared, overlays, Scheme::PreferDark)
                                            },
                                        )),
                                ),
                        ),
                )
            }))
            .maybe_child((!model_value.gsettings).then(|| {
                Element::from(status(Status::Info(
                    "\"Accent colour\" and \"Colour scheme\" are not available on this system."
                        .into(),
                )))
            }))
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch_persistent("appearance", load);
}
