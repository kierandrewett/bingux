//! A picture the preview helper decodes: HEIF, AVIF, TIFF, Radiance HDR,
//! OpenEXR and the preview inside a camera RAW file.
//!
//! The helper opens the file from the descriptor this process gives it, turns
//! the picture upright, tone maps high dynamic range, and answers `Opened` with
//! the size. This viewer then asks twice for page 0: a small render (640 pixels
//! on a side) that is on screen almost at once, and the whole picture, which
//! replaces it. Both are shown by [`ZoomView`], so everything a picture does
//! (zoom, pan, turn, flip) works the same as for a picture Freya decodes.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use bingux_preview_core::host::protocol::{ErrorCode, Message, OpenedKind, Packet};
use freya::engine::prelude::AlphaType;
use freya::prelude::*;
use freya_core::elements::image::ImageHandle;

use super::super::chrome::PreviewNav;
use super::super::host_link::{HostLink, LinkEvent};
use super::super::subject::Resolved;
use super::card::Card;
use super::image::memory_of;
use crate::controls::progress::{SpinnerSize, spinner};
use crate::style::*;
use crate::surface::use_tokens;
use crate::zoom_view::ZoomView;

/// The request ids of the two draws.
const QUICK: u32 = 10;
const SHARP: u32 = 11;
/// The longest edge of the quick draw.
const QUICK_EDGE: u32 = 640;

#[derive(Clone, Debug, PartialEq)]
enum Load {
    Opening,
    Ready { width: u32, height: u32 },
    Failed { code: ErrorCode, message: String },
}

/// What the helper has drawn so far.
#[derive(Clone, Default)]
struct Drawn {
    quick: Option<ImageHandle>,
    sharp: Option<ImageHandle>,
}

impl PartialEq for Drawn {
    fn eq(&self, other: &Self) -> bool {
        self.quick.is_some() == other.quick.is_some()
            && self.sharp.is_some() == other.sharp.is_some()
    }
}

#[derive(PartialEq)]
pub(super) struct HelperPicture {
    pub resolved: Arc<Resolved>,
    pub nav: PreviewNav,
    pub raw: bool,
}

impl Component for HelperPicture {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let path: Option<PathBuf> = self.resolved.source.as_path().map(ToOwned::to_owned);
        let raw = self.raw;
        let link = use_hook({
            let path = path.clone();
            move || {
                let link = HostLink::start();
                if let Some(path) = &path {
                    link.open_image(path, raw);
                }
                Rc::new(link)
            }
        });
        let mut load = use_state(|| Load::Opening);
        let mut drawn = use_state(Drawn::default);
        use_hook({
            let link = link.clone();
            move || {
                spawn(async move {
                    while let Some(event) = link.next().await {
                        match event {
                            LinkEvent::Packet(Packet {
                                message:
                                    Message::Opened {
                                        kind: OpenedKind::Image,
                                        video_w,
                                        video_h,
                                        ..
                                    },
                                ..
                            }) => {
                                load.set(Load::Ready {
                                    width: video_w,
                                    height: video_h,
                                });
                                // The small one first: it is on screen while the big one is made.
                                link.send(Packet {
                                    request: QUICK,
                                    message: Message::RenderPage {
                                        page: 0,
                                        scale_milli: 1000,
                                        max_w: QUICK_EDGE,
                                        max_h: QUICK_EDGE,
                                    },
                                });
                                link.send(Packet {
                                    request: SHARP,
                                    message: Message::RenderPage {
                                        page: 0,
                                        scale_milli: 1000,
                                        max_w: 0,
                                        max_h: 0,
                                    },
                                });
                            }
                            LinkEvent::Packet(Packet {
                                message: Message::Error { code, message },
                                request,
                            }) => {
                                // A failed draw leaves the quick one standing; a failed open is the end.
                                if request != QUICK && request != SHARP
                                    || *load.peek() == Load::Opening
                                {
                                    load.set(Load::Failed { code, message });
                                }
                            }
                            LinkEvent::Page(page) => {
                                if let Some(handle) = ImageHandle::from_rgba(
                                    page.width,
                                    page.height,
                                    page.pixels,
                                    AlphaType::Opaque,
                                ) {
                                    let mut now = drawn.write();
                                    match page.request {
                                        QUICK => now.quick = Some(handle),
                                        SHARP => now.sharp = Some(handle),
                                        _ => {}
                                    }
                                }
                            }
                            LinkEvent::Died(failure) => load.set(Load::Failed {
                                code: failure.code,
                                message: failure.message,
                            }),
                            LinkEvent::Packet(_) | LinkEvent::Frame => {}
                        }
                    }
                })
            }
        });
        use_drop({
            let link = link.clone();
            move || link.close()
        });

        let (start, on_mode) = memory_of(&self.nav, &self.resolved);
        let keyboard = self.nav.keyboard;
        match load.read().clone() {
            Load::Opening => rect()
                .tw(
                    &tokens,
                    &[w_full(), h_full(), flex_col(), center(), bg(Paint::Recess)],
                )
                .child(spinner(SpinnerSize::Medium))
                .into_element(),
            Load::Failed { message, code } => {
                let retry = None::<EventHandler<()>>;
                let _ = code;
                rect()
                    .tw(&tokens, &[w_full(), h_full(), bg(Paint::Card)])
                    .child(
                        Card::of(&self.resolved, &self.nav)
                            .noted(message)
                            .retrying(retry),
                    )
                    .into_element()
            }
            Load::Ready { width, height } => {
                let now = drawn.read().clone();
                let draw = move |w: f32, h: f32| -> Element {
                    match now.sharp.clone().or_else(|| now.quick.clone()) {
                        Some(handle) => image(handle)
                            .width(Size::px(w))
                            .height(Size::px(h))
                            .aspect_ratio(AspectRatio::None)
                            .sampling_mode(SamplingMode::Trilinear)
                            .into(),
                        None => rect().width(Size::px(w)).height(Size::px(h)).into(),
                    }
                };
                let mut view = ZoomView::new(Some((width as f32, height as f32)), draw)
                    .start(start)
                    .label(format!("Picture: {}", self.resolved.source.name()))
                    .keys_when(move || keyboard.held());
                if let Some(handle) = self.nav.zoom {
                    view = view.handle(handle);
                }
                if let Some(on_mode) = on_mode {
                    view = view.on_mode(on_mode);
                }
                view.into_element()
            }
        }
    }
}
