//! The inline colour picker: the full view.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use freya::prelude::*;

use super::chip::Preview;
use super::core::{Link, Recents, use_recents};
use super::fields::{HexField, Values};
use super::geometry::{Geometry, Shape};
use super::model::{HUE_MAX, Hsva, Mode, alpha_from_percent, alpha_percent};
use super::palette::{Entry, PRESETS, Palette, Recent};
use super::plane::Plane;
use crate::component::ComponentName;
use crate::controls::slider::slider;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::accent_names;

/// The picker for `value`; `on_change` receives every new colour as it is
/// chosen, at most once per frame while a thumb is dragged.
///
/// Build it with [`colour_picker`] and the methods below. It is the full
/// view: a saturation and value plane, a hue strip, an opacity strip when
/// asked for, the old colour beside the new, a hex field, numeric boxes in
/// RGB, HSL or HSV, a palette and the colours chosen lately. For a colour
/// that sits in a row until it is wanted, see [`ColourButton`](super::ColourButton),
/// which opens this in a popover or a sheet.
///
/// The picker holds the colour as hue, saturation, value and alpha and reports
/// it as the RGB colour it makes. A `value` that is the colour it just
/// reported changes nothing; any other is taken in as an edit of the colour on
/// screen, so a grey keeps its hue and a black its saturation.
#[derive(PartialEq)]
pub struct ColourPicker {
    value: Color,
    on_change: EventHandler<Color>,
    alpha: bool,
    accents: bool,
    presets: Option<Vec<Color>>,
    recent: Option<(Vec<Color>, EventHandler<Vec<Color>>)>,
    recents: bool,
    original: Option<Color>,
    /// The recent colours a [`ColourButton`](super::ColourButton) shares with its picker.
    shared: Option<Recents>,
    /// Whether an edit that is over is added to the recent colours. A button
    /// that commits on Done adds the colour then, and not at every drag.
    remember: bool,
    fill: bool,
}

/// A colour picker for `value`.
pub fn colour_picker(value: Color, on_change: impl Into<EventHandler<Color>>) -> ColourPicker {
    ColourPicker {
        value,
        on_change: on_change.into(),
        alpha: false,
        accents: true,
        presets: None,
        recent: None,
        recents: true,
        original: None,
        shared: None,
        remember: true,
        fill: false,
    }
}

impl ColourPicker {
    /// Shows the opacity strip and box. Without them the picker only makes
    /// opaque colours, whatever opacity the `value` it is given has.
    pub fn alpha(mut self, alpha: bool) -> Self {
        self.alpha = alpha;
        self
    }

    /// Shows the system accent colours in the palette: on by default.
    pub fn accents(mut self, accents: bool) -> Self {
        self.accents = accents;
        self
    }

    /// The swatches after the accents, instead of the default white, grey and
    /// black. Pass none for a palette of the accents alone.
    pub fn presets(mut self, presets: Vec<Color>) -> Self {
        self.presets = Some(presets);
        self
    }

    /// The recent colours the app keeps: `colours`, newest first, and what to
    /// tell `on_change` when a new one is chosen (the list with it at the
    /// front, at most [`RECENT_MAX`](super::RECENT_MAX) long). Use this to save
    /// them in the app's settings. Without it the picker keeps them in memory
    /// for as long as the process runs.
    pub fn recent(
        mut self,
        colours: Vec<Color>,
        on_change: impl Into<EventHandler<Vec<Color>>>,
    ) -> Self {
        self.recent = Some((colours, on_change.into()));
        self
    }

    /// Shows the row of recent colours: on by default. Its room is kept
    /// whether or not a colour has been chosen yet.
    pub fn recents(mut self, recents: bool) -> Self {
        self.recents = recents;
        self
    }

    /// The colour the preview's old half shows, which a press on it restores.
    /// By default it is the `value` the picker was first given.
    pub fn original(mut self, original: Color) -> Self {
        self.original = Some(original);
        self
    }

    /// Fills the width it is given, as a phone's picker does in its sheet. A
    /// picker on a phone fills without being asked.
    pub fn fill(mut self) -> Self {
        self.fill = true;
        self
    }

    /// The recent colours of a button that owns them, and whether the picker
    /// adds to them itself.
    pub(super) fn sharing(mut self, recents: Recents, remember: bool) -> Self {
        self.shared = Some(recents);
        self.remember = remember;
        self
    }

    /// The swatches of the palette, in order.
    fn entries(&self, tokens: &crate::theme::Tokens) -> Vec<Entry> {
        let mut entries = Vec::new();
        if self.accents {
            for name in accent_names() {
                let mut label = name.to_string();
                label[..1].make_ascii_uppercase();
                entries.push(Entry {
                    colour: Paint::Swatch(name).color(tokens),
                    name: Some(label),
                });
            }
        }
        match &self.presets {
            Some(presets) => entries.extend(presets.iter().map(|colour| Entry {
                colour: *colour,
                name: None,
            })),
            None => entries.extend(PRESETS.iter().filter_map(|hex| {
                colour_from_hex(hex).map(|colour| Entry { colour, name: None })
            })),
        }
        entries
    }

    /// What the picker shows, as far as its height goes.
    fn shape(&self) -> Shape {
        let accents = if self.accents { accent_names().count() } else { 0 };
        let presets = self.presets.as_ref().map_or(PRESETS.len(), Vec::len);
        Shape {
            alpha: self.alpha,
            swatches: accents + presets,
            recents: self.recents,
        }
    }

    /// The picker's height at the current density, for a popover or a sheet
    /// that must know it before the picker is drawn. The drawn picker is
    /// exactly this tall, whatever colour it holds.
    pub fn height(&self) -> f32 {
        Geometry::new(size_class()).total(self.shape())
    }

    /// The picker's width on a desktop. On a phone it fills its sheet.
    pub fn width() -> f32 {
        Dim::PickerWidth.px()
    }
}

impl Component for ColourPicker {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let g = Geometry::new(size_class());
        let alpha = self.alpha;
        let initial = Hsva::from_colour(self.value);
        let model = use_state(|| if alpha { initial } else { initial.solid() });
        let mut mode = use_state(|| Mode::Rgb);
        let first = use_hook(|| self.value);
        let out = use_hook(|| Rc::new(RefCell::new(self.on_change.clone())));
        let held = use_hook(|| Rc::new(Cell::new(false)));
        let dirty = use_hook(|| Rc::new(Cell::new(false)));
        let ended = use_hook(|| {
            Rc::new(RefCell::new(EventHandler::new(|_: Color| {})))
        });
        let own = use_recents(self.recent.clone(), self.recents);
        let recents = self.shared.clone().unwrap_or(own);
        *out.borrow_mut() = self.on_change.clone();
        let remember = self.remember;
        let keeper = recents.clone();
        *ended.borrow_mut() = EventHandler::new(move |colour: Color| {
            if remember {
                keeper.add(colour);
            }
        });
        let link = Link::new(model, out, held, dirty, ended, alpha);
        let rebasing = link.clone();
        use_side_effect_with_deps(&self.value, move |value: &Color| rebasing.rebase(*value));
        let shown = link.shown(self.value);
        let original = self.original.unwrap_or(first);
        let fill = self.fill || g.compact();

        let hue_link = link.clone();
        let hue = slider(shown.hue as f64, move |degrees: f64| {
            hue_link.set(hue_link.current().with_hue(degrees as f32));
        })
        .max(HUE_MAX as f64)
        .step(1.0)
        .fill()
        .large()
        .width(Dim::PickerWidth)
        .label("Hue")
        .thumb_fill(shown.hue_colour())
        .gradient_colours(
            (0..=6)
                .map(|step| {
                    (
                        step as f64 * (100.0 / 6.0),
                        Hsva::new(step as f32 * 60.0, 1.0, 1.0, 255).colour(),
                    )
                })
                .collect(),
        );
        let alpha_strip = alpha.then(|| {
            let alpha_link = link.clone();
            let solid = shown.opaque();
            slider(
                alpha_percent(shown.alpha) as f64,
                move |percent: f64| {
                    alpha_link.set(
                        alpha_link
                            .current()
                            .with_alpha(alpha_from_percent(percent.round() as i32)),
                    );
                },
            )
            .step(1.0)
            .fill()
            .large()
            .width(Dim::PickerWidth)
            .label("Opacity")
            .checkerboard()
            .thumb_fill(solid)
            .gradient_colours(vec![(0.0, solid.with_a(0)), (100.0, solid)])
        });

        let mode_link = link.clone();
        let entries = self.entries(&tokens);
        let settle_pointer = link.clone();
        let settle_key = link.clone();
        rect()
            .component("ColourPicker")
            .a11y_role(AccessibilityRole::Group)
            .a11y_alt("Colour picker")
            .maybe(fill, |picker| picker.width(Size::fill()))
            .maybe(!fill, |picker| picker.tw(&tokens, &[w(Dim::PickerWidth)]))
            .tw(&tokens, &[flex_col(), gap(S3)])
            // An edit is over when the pointer or a key comes up anywhere.
            .on_global_pointer_up(move |_: Event<PointerEventData>| settle_pointer.settle())
            .on_key_up(move |_: Event<KeyboardEventData>| settle_key.settle())
            .child(
                rect()
                    .component("ColourSpectrum")
                    .tw(&tokens, &[w_full(), flex_col(), gap(S1)])
                    .child(Plane {
                        hsva: shown,
                        link: link.clone(),
                        geometry: g,
                        fill,
                    })
                    .child(hue)
                    .maybe_child(alpha_strip),
            )
            .child(
                rect()
                    .component("ColourEntry")
                    .tw(&tokens, &[w_full(), flex_row(), grow_children(), gap(S2)])
                    .child(Preview {
                        old: original,
                        new: shown.colour(),
                        link: link.clone(),
                        geometry: g,
                    })
                    .child(HexField {
                        colour: shown,
                        link: link.clone(),
                        geometry: g,
                    }),
            )
            .child(Values {
                colour: shown,
                mode: mode(),
                link: link.clone(),
                geometry: g,
                on_mode: EventHandler::new(move |next: Mode| {
                    mode.set(next);
                    // The boxes change; the colour does not.
                    mode_link.settle();
                }),
            })
            .maybe_child((!entries.is_empty()).then(|| {
                Palette {
                    entries,
                    editing: shown.colour(),
                    link: link.clone(),
                }
            }))
            .maybe_child(recents.enabled().then(|| Recent {
                colours: recents.list(),
                editing: shown.colour(),
                link,
                geometry: g,
            }))
    }
}
