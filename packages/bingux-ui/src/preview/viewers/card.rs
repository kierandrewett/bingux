//! The card: the type's icon, the file's name and kind, and the reason a
//! better viewer did not show it. It is the last viewer of every chain, the
//! placeholder of every viewer while it loads, and the error state of the
//! viewers that fail.

use std::path::PathBuf;
use std::time::SystemTime;

use bingux_files::thumbs::{self, Bucket};
use bingux_preview_core::ViewerId;
use freya::prelude::*;
use freya::torin::prelude::Size2D;

use super::super::chrome::PreviewNav;
use super::super::subject::Resolved;
use crate::component::ComponentName;
use crate::controls::{action_button, primary_button};
use crate::files::{FileIcon, FileIconSize, file_icon_view};
use crate::style::*;
use crate::surface::use_tokens;
use crate::task::use_resource;
use crate::typography::{CAPTION, Ink, SECTION};

/// The icon on the card, as a multiple of the 48 px hero size.
const ICON_SCALE: f32 = 3.0;
/// How long a decoded thumbnail stays cached once nothing shows it.
const KEEP: std::time::Duration = std::time::Duration::from_secs(5);

/// The size of the type's icon on the card.
fn stage_size() -> FileIconSize {
    FileIconSize::Px((IconSize::Hero.px() * ICON_SCALE).round() as u16)
}

/// The card's content, before the thumbnail.
#[derive(PartialEq)]
pub struct Card {
    icon: FileIcon,
    title: String,
    subtitle: Option<String>,
    notes: Vec<String>,
    /// The thumbnail the desktop made for a document, a song or a film.
    poster: Option<(PathBuf, Option<SystemTime>)>,
    on_open: Option<EventHandler<()>>,
    on_retry: Option<EventHandler<()>>,
    /// The card sits on the player's black backdrop, not on the window: its text takes the ink of a
    /// floating surface, which is light in both schemes (the dark ink of the light scheme was 1.2:1
    /// on it: `scripts/pixel-check.py`).
    on_backdrop: bool,
}

impl Card {
    /// The card of a resolved source, with its notes.
    pub(super) fn of(resolved: &Resolved, nav: &PreviewNav) -> Card {
        let wants_poster = matches!(resolved.viewer, ViewerId::Media | ViewerId::Document);
        Card {
            icon: super::icon(resolved),
            title: resolved.facts.name.clone(),
            subtitle: Some(resolved.facts.description.to_owned()),
            notes: resolved.notes.clone(),
            poster: resolved
                .facts
                .path
                .clone()
                .filter(|_| wants_poster)
                .map(|path| (path, resolved.facts.modified)),
            on_open: nav.on_open.clone(),
            on_retry: None,
            on_backdrop: false,
        }
    }

    /// This card drawn on the player's backdrop.
    pub(super) fn on_backdrop(mut self) -> Card {
        self.on_backdrop = true;
        self
    }

    /// This card with `note` added: why the better viewer did not show the file.
    pub(super) fn noted(mut self, note: String) -> Card {
        self.notes.push(note);
        self
    }

    /// This card with a Try again button beside Open. The card is as tall as its stage, so a button
    /// put under it is always below the stage's edge: the retry is part of the card's own actions.
    pub(super) fn retrying(mut self, retry: Option<EventHandler<()>>) -> Card {
        self.on_retry = retry;
        self
    }
}

/// The card of a file that is still loading, or could not be read.
pub struct Placeholder;

impl Placeholder {
    pub fn new(icon: FileIcon, title: String, note: Option<String>) -> Card {
        Card {
            icon,
            title,
            subtitle: None,
            notes: note.into_iter().collect(),
            poster: None,
            on_open: None,
            on_retry: None,
            on_backdrop: false,
        }
    }
}

/// The largest thumbnail the desktop has made for a file, if any.
fn find_poster(key: (PathBuf, Option<SystemTime>)) -> Result<Option<PathBuf>, String> {
    let (path, modified) = key;
    Ok([Bucket::XXLarge, Bucket::XLarge, Bucket::Large, Bucket::Normal]
        .into_iter()
        .find_map(|bucket| thumbs::lookup(&path, modified, bucket)))
}

#[derive(PartialEq)]
struct Poster {
    key: (PathBuf, Option<SystemTime>),
    fallback: Element,
}

impl Component for Poster {
    fn render(&self) -> impl IntoElement {
        let poster = use_resource(self.key.clone(), find_poster);
        let found = poster.with(|poster| poster.value().cloned());
        match found.flatten() {
            None => self.fallback.clone(),
            Some(png) => {
                let fallback = self.fallback.clone();
                ImageViewer::new(png)
                    .decode_mode(DecodeMode::Source)
                    .asset_age(KEEP)
                    .width(Size::fill())
                    .height(Size::fill())
                    .aspect_ratio(AspectRatio::Min)
                    .image_cover(ImageCover::Center)
                    .loading_placeholder(fallback.clone())
                    .error_renderer(move |_: String| fallback.clone())
                    .into()
            }
        }
    }
}

impl Component for Card {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let _ = Size2D::new(0.0, 0.0);
        let muted = if self.on_backdrop { Ink::OnFloating } else { Ink::Muted };
        let face = Element::from(
            rect()
                .tw(&tokens, &[w_full(), h_full(), flex_col(), center(), gap(S3), p(S4)])
                .child(file_icon_view(&tokens, &self.icon, stage_size(), false))
                .child(
                    SECTION
                        .in_ink(if self.on_backdrop { Ink::OnFloating } else { Ink::Text })
                        .label(&tokens, self.title.clone())
                        .max_lines(2)
                        .text_align(TextAlign::Center),
                )
                .maybe_child(self.subtitle.clone().map(|text| CAPTION.in_ink(muted).label(&tokens, text).max_lines(1)))
                .children(self.notes.iter().map(|note| {
                    Element::from(rect().tw(&tokens, &[w_full(), max_w(Dim::Dialog)]).child(
                        CAPTION
                            .in_ink(muted)
                            .paragraph(&tokens, note.clone())
                            .width(Size::fill())
                            .text_align(TextAlign::Center),
                    ))
                }))
                .maybe_child((self.on_open.is_some() || self.on_retry.is_some()).then(|| {
                    // Open and Try again side by side: stacked under the card, the second would be
                    // below the stage's edge on a short pane.
                    Element::from(
                        rect()
                            .tw(&tokens, &[flex_row(), items(Align::Center), gap(S2)])
                            .maybe_child(self.on_open.clone().map(|open| Element::from(primary_button("Open", move |_| open.call(())))))
                            .maybe_child(self.on_retry.clone().map(|retry| Element::from(action_button("Try again", move |_| retry.call(()))))),
                    )
                })),
        );
        let stage = match &self.poster {
            Some(key) => Element::from(Poster {
                key: key.clone(),
                fallback: face,
            }),
            None => face,
        };
        rect().component("PreviewCard").tw(&tokens, &[w_full(), h_full()]).child(stage)
    }
}
