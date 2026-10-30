//! The Bingux desktop in miniature: the wallpaper behind a top bar, a window,
//! the sidebar and the dock, placed as the settings say. The Desktop page and
//! the Appearance page both draw it from the same model, so the picture
//! always matches the settings.
//!
//! On the Desktop page a right click on the dock or the sidebar opens the
//! settings for that part, so the same choices can be made from the picture.
//! The Appearance page draws it without menus.

use std::path::PathBuf;

use bingux_ui::component::ComponentName;
use freya::prelude::*;

use super::{Change, Ctx, Model, write};
use crate::theme::{Scheme, Tokens};
use crate::ui::{MenuItem, menu_item, menu_separator};
use bingux_ui::app_icon::app_icon;
use bingux_ui::style::IconSize;
use bingux_ui::style::*;

/// How tall the picture is drawn: the page's hero or a colour scheme card.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Frame {
    Hero,
    Card,
}

impl Frame {
    fn height(self) -> Dim {
        match self {
            Frame::Hero => Dim::ArrangementHeight,
            Frame::Card => Dim::PreviewHeight,
        }
    }
}

/// The screen edge the sidebar sits on.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Edge {
    Left,
    Right,
    Top,
}

/// The dock as the settings set it: its icon size and where it sits.
pub(crate) struct Dock<'a> {
    pub size: u32,
    pub alignment: &'a str,
}

/// What the picture shows. A part set to `None` is hidden.
pub(crate) struct Scene<'a> {
    pub dark: bool,
    /// Absolute path of the wallpaper; `None` draws a plain stand-in.
    pub wallpaper: Option<&'a str>,
    pub dock: Option<Dock<'a>>,
    pub sidebar: Option<Edge>,
}

impl<'a> Scene<'a> {
    /// The desktop as `m` sets it. An Automatic sidebar sits on the right,
    /// which is the shell's fallback when no edge is saved.
    pub(crate) fn of(m: &'a Model, dark: bool, wallpaper: Option<&'a str>) -> Self {
        Self {
            dark,
            wallpaper,
            dock: m.dock.then_some(Dock {
                size: m.dock_size,
                alignment: m.dock_alignment.as_str(),
            }),
            sidebar: m.sidebar.then_some(match m.sidebar_edge.as_deref() {
                Some("left") => Edge::Left,
                Some("top") => Edge::Top,
                _ => Edge::Right,
            }),
        }
    }
}

/// Whether a pointer press is a right click.
fn is_secondary(event: &Event<PointerEventData>) -> bool {
    matches!(event.data(), PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Right))
}

/// Opens the menu that `build` makes from the current model, at the pointer.
fn on_right_click(
    ctx: Ctx,
    build: fn(Ctx, &Model) -> Vec<MenuItem>,
) -> impl FnMut(Event<PointerEventData>) + 'static {
    move |event: Event<PointerEventData>| {
        if !is_secondary(&event) {
            return;
        }
        let Some(Ok(model)) = ctx.model.read().clone() else {
            return;
        };
        let at = event.global_location();
        ctx.overlays
            .show_menu((at.x as f32, at.y as f32), build(ctx, &model));
    }
}

/// The dock's menu: show it or hide it, then where it sits.
fn dock_items(ctx: Ctx, m: &Model) -> Vec<MenuItem> {
    let shown = m.dock;
    let alignment = m.dock_alignment.clone();
    let align = |key: &'static str, label: &'static str| {
        menu_item(label, move |_| {
            write(ctx, Change::Set("desktop.dock_alignment", key.to_string()))
        })
        .checked(alignment == key)
    };
    vec![
        menu_item("Show dock", move |_| {
            write(ctx, Change::Set("desktop.dock", (!shown).to_string()))
        })
        .checked(shown),
        menu_separator(),
        align("left", "Align left"),
        align("center", "Align centre"),
        align("right", "Align right"),
    ]
}

/// The sidebar's menu: show it or hide it, then which edge it sits on.
fn sidebar_items(ctx: Ctx, m: &Model) -> Vec<MenuItem> {
    let shown = m.sidebar;
    let edge = m.sidebar_edge.clone();
    let choose = |key: &'static str, label: &'static str| {
        menu_item(label, move |_| {
            if key == "auto" {
                write(ctx, Change::Unset("desktop.sidebar_edge"))
            } else {
                write(ctx, Change::Set("desktop.sidebar_edge", key.to_string()))
            }
        })
        .checked(match key {
            "auto" => edge.is_none(),
            _ => edge.as_deref() == Some(key),
        })
    };
    vec![
        menu_item("Show sidebar", move |_| {
            write(ctx, Change::Set("desktop.sidebar", (!shown).to_string()))
        })
        .checked(shown),
        menu_separator(),
        choose("auto", "Automatic"),
        choose("left", "Left"),
        choose("right", "Right"),
        choose("top", "Top"),
    ]
}

/// The window, bar and line colours of the picture for the scheme.
fn paints(dark: bool) -> (Paint, Paint, Paint) {
    if dark {
        (
            Paint::MockDarkWindow,
            Paint::MockDarkBar,
            Paint::MockDarkLine,
        )
    } else {
        (
            Paint::MockLightWindow,
            Paint::MockLightBar,
            Paint::MockLightLine,
        )
    }
}

/// A grey bar standing in for text in the picture.
fn stub_line(tokens: &Tokens, width: Util, paint: Paint) -> Rect {
    rect().tw(
        tokens,
        &[width, h(Dim::PreviewBar), rounded(Radius::Mark), bg(paint)],
    )
}

/// The side of a dock icon for the dock size setting (32 to 80 px).
fn icon_box(size: u32) -> Dim {
    match size {
        0..=47 => Dim::Box5,
        48..=63 => Dim::Box7,
        _ => Dim::Box9,
    }
}

/// Makes `panel` open its menu on a right click, when there are menus.
fn interactive(
    panel: Rect,
    menus: Option<Ctx>,
    label: &'static str,
    build: fn(Ctx, &Model) -> Vec<MenuItem>,
) -> Element {
    match menus {
        Some(ctx) => Element::from(
            panel
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(label)
                .on_pointer_down(on_right_click(ctx, build)),
        ),
        None => Element::from(panel.a11y_alt(label)),
    }
}

/// The apps shown in the preview dock.
const DOCK_APPS: [&str; 4] = [
    "firefox",
    "org.gnome.Nautilus",
    "org.gnome.Ptyxis",
    "org.gnome.Settings",
];

/// One dock slot: a real app icon, with a dot under it for a running app.
fn dock_slot(tokens: &Tokens, icon: Dim, app: &str, running: bool) -> Element {
    Element::from(
        rect()
            .tw(tokens, &[flex_col(), items(Align::Center), gap(S0_5)])
            .child(
                rect()
                    .tw(tokens, &[w(icon), h(icon), center()])
                    // The glyph is never bigger than its slot (a 32 px icon in a 20 px slot
                    // stuck out of the pill and over its neighbours).
                    .child(app_icon(
                        app.to_string(),
                        if matches!(icon, Dim::Box9) { IconSize::App } else { IconSize::AppSmall },
                    )),
            )
            .child(rect().tw(
                tokens,
                &[
                    w(Dim::PreviewBar),
                    h(Dim::PreviewBar),
                    rounded(Radius::Pill),
                    bg(if running {
                        Paint::Accent
                    } else {
                        Paint::Transparent
                    }),
                ],
            )),
    )
}

/// The dock: a pill of app icons at the bottom, placed as the alignment
/// setting says. The row's vertical padding keeps the running dot off the
/// frame edge.
///
/// A scheme card is a picture a third of a phone's width: its dock is the miniature of the
/// dock, three small icons with the half step between them, so the pill stays inside the
/// picture (the hero, which is as wide as the page, shows the dock at the size the setting says).
fn dock_row(tokens: &Tokens, dock: &Dock, bar: Paint, frame: Frame, menus: Option<Ctx>) -> Element {
    let miniature = frame == Frame::Card;
    let icon = if miniature { Dim::Box5 } else { icon_box(dock.size) };
    let apps = if miniature { &DOCK_APPS[..3] } else { &DOCK_APPS[..] };
    let (between, inset) = if miniature { (S0_5, S0_5) } else { (S1, S1) };
    let placed = match dock.alignment {
        "left" => Align::Start,
        "right" => Align::End,
        _ => Align::Center,
    };
    let mut pill = rect().component("DockPreview").tw(
        tokens,
        &[
            flex_row(),
            items(Align::Center),
            gap(between),
            p(inset),
            rounded(Radius::Pill),
            border(Line::Hairline),
            bg(bar),
        ],
    );
    for (index, app) in apps.iter().enumerate() {
        pill = pill.child(dock_slot(tokens, icon, app, index == 0));
    }
    let row = rect()
        .tw(
            tokens,
            &[
                w_full(),
                flex_row(),
                justify(placed),
                items(Align::Center),
                px(S2),
                py(S1),
            ],
        )
        .child(pill);
    interactive(row, menus, "Dock", dock_items)
}

/// The sidebar: a column on the left or right, or a strip along the top.
fn sidebar_panel(
    tokens: &Tokens,
    edge: Edge,
    bar: Paint,
    stub: Paint,
    menus: Option<Ctx>,
) -> Element {
    let panel = if edge == Edge::Top {
        rect()
            .tw(
                tokens,
                &[
                    w_full(),
                    h(Dim::PreviewTitleBar),
                    flex_row(),
                    items(Align::Center),
                    gap(S1),
                    px(S2),
                    bg(bar),
                ],
            )
            .child(stub_line(tokens, w(Dim::PreviewLine), Paint::Accent))
            .child(stub_line(tokens, w_flex(), stub))
    } else {
        rect()
            .tw(
                tokens,
                &[
                    w(Dim::PreviewSidebar),
                    h_full(),
                    flex_col(),
                    gap(S1),
                    p(S1),
                    bg(bar),
                ],
            )
            .child(stub_line(tokens, w_full(), Paint::Accent))
            .child(stub_line(tokens, w_full(), stub))
            .child(stub_line(tokens, w_full(), stub))
    };
    interactive(panel, menus, "Sidebar", sidebar_items)
}

/// The top bar: a launcher stand-in and a title stand-in on the left, a clock
/// and a status cluster on the right.
fn top_bar(tokens: &Tokens, bar: Paint, stub: Paint) -> Element {
    Element::from(
        rect()
            .tw(
                tokens,
                &[
                    w_full(),
                    h(Dim::PreviewTitleBar),
                    flex_row(),
                    items(Align::Center),
                    justify(Align::Between),
                    px(S2),
                    bg(bar),
                ],
            )
            .child(stub_line(tokens, w(Dim::Box5), stub))
            .child(stub_line(tokens, w(Dim::PreviewLine), stub))
            .child(
                rect()
                    .tw(tokens, &[flex_row(), items(Align::Center), gap(S1)])
                    .child(stub_line(tokens, w(Dim::Box5), stub))
                    .child(stub_line(tokens, w(Dim::Box5), stub))
                    .child(stub_line(tokens, w(Dim::PreviewLine), stub)),
            ),
    )
}

/// The window in the middle: a title strip with three small control dots,
/// then a sidebar column beside the content. In a scheme card (`miniature`) the
/// window is about 32 px wide, less than the sidebar column and a content line
/// of the full-size picture, so it keeps the content alone: the column was laid
/// out 4 px (on a phone, 29 px) outside the window.
fn window_card(tokens: &Tokens, window: Paint, stub: Paint, miniature: bool) -> Element {
    let control = |paint: Paint| {
        rect().tw(
            tokens,
            &[
                w(Dim::PreviewBar),
                h(Dim::PreviewBar),
                rounded(Radius::Pill),
                bg(paint),
            ],
        )
    };
    let title = rect()
        .tw(
            tokens,
            &[
                w_full(),
                h(Dim::PreviewTitleBar),
                flex_row(),
                items(Align::Center),
                gap(S1),
                px(S1),
                border(Line::Hairline),
            ],
        )
        .child(control(stub))
        .child(control(stub))
        .child(control(stub));
    let sidebar = rect()
        .tw(
            tokens,
            &[
                w(Dim::Box9),
                h_full(),
                flex_col(),
                gap(S1),
                p(S1),
                border(Line::Hairline),
            ],
        )
        .child(stub_line(tokens, w_full(), stub))
        .child(stub_line(tokens, w_full(), stub))
        .child(stub_line(tokens, w_full(), stub));
    let content = rect()
        .tw(tokens, &[w_flex(), h_full(), flex_col(), gap(S1), p(S1)])
        .child(stub_line(tokens, if miniature { w_full() } else { w(Dim::PreviewLine) }, Paint::Accent))
        .child(stub_line(tokens, w_full(), stub))
        .child(stub_line(tokens, w_full(), stub));
    let body = rect()
        .tw(tokens, &[w_full(), h_flex(), flex_row()])
        .maybe_child((!miniature).then_some(sidebar))
        .child(content);
    Element::from(
        rect()
            .tw(
                tokens,
                &[
                    w_flex(),
                    h_full(),
                    flex_col(),
                    rounded(Radius::Card),
                    border(Line::Hairline),
                    bg(window),
                    clip(),
                ],
            )
            .child(title)
            .child(body),
    )
}

/// The longest side a wallpaper is decoded to in the picture: enough for a
/// full-width hero on a HiDPI screen, and the same for every use of one
/// wallpaper so the hero and the scheme cards share one decode.
const WALLPAPER_DECODE: f32 = 2048.0;

/// The desktop picture: the wallpaper (or a stand-in) behind the top bar, the
/// window, the sidebar and the dock.
///
/// The picture is clipped to its own rounded box, and callers put it in
/// [`framed`] so the wallpaper never draws over a border.
///
/// `menus` is `Some` on the Desktop page, where the dock and the sidebar open
/// their menus on a right click.
pub(crate) fn scene(tokens: &Tokens, picture: &Scene, frame: Frame, menus: Option<Ctx>) -> Element {
    let (window, bar, stub) = paints(picture.dark);

    // The backdrop is the picture's final box, filled with the stand-in colour,
    // from the first frame. A wallpaper decodes on a worker (to a bounded size,
    // so a 4K picture does not become a 33 MB upload on the UI thread) and
    // fades in over the stand-in when it lands; nothing in the picture moves
    // or changes edge.
    let mut backdrop_layer = rect()
        .position(Position::new_absolute().left(0.).top(0.))
        .tw(tokens, &[w_full(), h_full(), clip(), bg(Paint::AccentSoft)]);
    if let Some(path) = picture.wallpaper {
        backdrop_layer = backdrop_layer.child(
            ImageViewer::new(PathBuf::from(path))
                .decode_mode(DecodeMode::Custom(Size2D::new(
                    WALLPAPER_DECODE,
                    WALLPAPER_DECODE,
                )))
                .loading_placeholder(rect().tw(tokens, &[w_full(), h_full()]))
                .fade_in(tokens.motion(bingux_ui::motion::MOVE_MS))
                .width(Size::fill())
                .height(Size::fill())
                .aspect_ratio(AspectRatio::Max),
        );
    }

    let edge = picture.sidebar;
    // The picture's own parts sit one layer above the backdrop. Siblings in
    // one layer are painted in no fixed order (Freya keeps a layer's nodes in a
    // hash set), so without this a wallpaper that lands after the first frame,
    // a new node, is painted over the top bar.
    let mut desktop_layer = rect()
        .position(Position::new_absolute().left(0.).top(0.))
        .tw(tokens, &[w_full(), h_full(), flex_col(), grow_children()])
        .layer(Layer::Relative(1))
        .child(top_bar(tokens, bar, stub));
    if edge == Some(Edge::Top) {
        desktop_layer = desktop_layer.child(sidebar_panel(tokens, Edge::Top, bar, stub, menus));
    }

    let mut middle = rect().tw(
        tokens,
        &[
            w_full(),
            h_flex(),
            flex_row(),
            grow_children(),
            gap(S2),
            p(S2),
        ],
    );
    if edge == Some(Edge::Left) {
        middle = middle.child(sidebar_panel(tokens, Edge::Left, bar, stub, menus));
    }
    middle = middle
        .child(window_card(tokens, window, stub, frame == Frame::Card))
        .child(rect().tw(tokens, &[w_flex(), h_full()]));
    if edge == Some(Edge::Right) {
        middle = middle.child(sidebar_panel(tokens, Edge::Right, bar, stub, menus));
    }
    desktop_layer = desktop_layer.child(middle);

    if let Some(dock) = &picture.dock {
        desktop_layer = desktop_layer.child(dock_row(tokens, dock, bar, frame, menus));
    }

    Element::from(
        rect()
            .tw(
                tokens,
                &[
                    w_full(),
                    h(frame.height()),
                    rounded(Radius::Control),
                    clip(),
                ],
            )
            .child(backdrop_layer)
            .child(desktop_layer),
    )
}

/// Frames a picture with a hairline and an inset, so the picture stays inside
/// the border.
pub(crate) fn framed(tokens: &Tokens, picture: Element) -> Element {
    Element::from(
        rect()
            .tw(
                tokens,
                &[
                    w_full(),
                    rounded(Radius::Card),
                    border(Line::Hairline),
                    p(S1),
                ],
            )
            .child(picture),
    )
}

/// The desktop picture at the top of the Desktop page. It reads the same
/// model as the rows below it, so the picture and the settings agree.
pub(crate) fn desktop_preview(tokens: &Tokens, ctx: Ctx, m: &Model) -> Element {
    let dark = tokens.scheme != Scheme::PreferLight;
    let wallpaper = (!m.wallpaper.is_empty()).then_some(m.wallpaper.as_str());
    let picture = scene(
        tokens,
        &Scene::of(m, dark, wallpaper),
        Frame::Hero,
        Some(ctx),
    );
    framed(tokens, picture)
}
