//! The picture of a film: [`video_surface`].
//!
//! The surface is a black box that draws the newest frame of its
//! [`VideoFrames`] and tells the decoder how many device pixels it covers.
//! The decoder renders at that size, letterboxed to the film's proportions,
//! so the frame fills the box exactly; while a new size has not been rendered
//! yet (the window is being resized), the last frame is drawn in its own
//! proportions, centred, never stretched.
//!
//! A frame costs one image wrapped around the decoder's buffer (no copy) and
//! one redraw. Nothing here waits: the frames arrive in a task.

use std::time::{Duration, Instant};

use freya::engine::prelude::AlphaType;
use freya::prelude::*;
use freya_core::elements::image::ImageHandle;

use super::frames::VideoFrames;
use crate::component::ComponentName;
use crate::style::*;
use crate::surface::use_tokens;

/// Two presses closer together than this are a double press.
const DOUBLE_PRESS: Duration = Duration::from_millis(400);

/// A film's picture. Build with [`video_surface`].
#[derive(PartialEq)]
pub struct VideoSurface {
    frames: VideoFrames,
    label: String,
    on_press: Option<EventHandler<()>>,
    on_double_press: Option<EventHandler<()>>,
}

/// The picture of `frames`, filling its parent.
pub fn video_surface(frames: VideoFrames) -> VideoSurface {
    VideoSurface {
        frames,
        label: "Video".to_owned(),
        on_press: None,
        on_double_press: None,
    }
}

impl VideoSurface {
    /// What a screen reader calls the picture: the film's title.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// A single press on the picture (a player plays or pauses). Waits out
    /// the double-press time only when a double press is also handled.
    pub fn on_press(mut self, on_press: impl Into<EventHandler<()>>) -> Self {
        self.on_press = Some(on_press.into());
        self
    }

    /// A double press on the picture (a player goes full screen).
    pub fn on_double_press(mut self, on_double_press: impl Into<EventHandler<()>>) -> Self {
        self.on_double_press = Some(on_double_press.into());
        self
    }
}

impl Component for VideoSurface {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut picture = use_state(|| None::<ImageHandle>);
        let frames = self.frames.clone();
        // The frames arrive in a task on this thread; each one is a redraw.
        use_hook({
            let frames = frames.clone();
            move || {
                spawn(async move {
                    loop {
                        frames.changed().await;
                        match frames.take() {
                            Some(Some(frame)) => {
                                if let Some(handle) = ImageHandle::from_rgba(
                                    frame.width,
                                    frame.height,
                                    frame.pixels,
                                    AlphaType::Opaque,
                                ) {
                                    picture.set(Some(handle));
                                }
                            }
                            Some(None) => picture.set(None),
                            None => {}
                        }
                    }
                })
            }
        });
        // Not shown any more: the decoder stops rendering for it.
        use_drop({
            let frames = frames.clone();
            move || frames.set_size((0, 0))
        });

        let mut last_press = use_state(|| None::<Instant>);
        let on_press = self.on_press.clone();
        let on_double_press = self.on_double_press.clone();
        let press = move |_: Event<PressEventData>| {
            let now = Instant::now();
            let double = last_press
                .peek()
                .is_some_and(|last| now.duration_since(last) < DOUBLE_PRESS);
            if double {
                last_press.set(None);
                if let Some(handler) = &on_double_press {
                    handler.call(());
                }
                return;
            }
            last_press.set(Some(now));
            if let Some(handler) = on_press.clone() {
                if on_double_press.is_none() {
                    handler.call(());
                } else {
                    // A second press within the double-press time cancels this one.
                    spawn(async move {
                        timer(DOUBLE_PRESS).await;
                        if last_press.peek().is_some_and(|last| last == now) {
                            handler.call(());
                        }
                    });
                }
            }
        };

        let label = self.label.clone();
        rect()
            .component("VideoSurface")
            .a11y_role(AccessibilityRole::Video)
            .a11y_alt(label)
            .expanded()
            .tw(&tokens, &[clip(), bg(Paint::Backdrop), center()])
            .on_sized(move |event: Event<SizedEventData>| {
                let scale = *Platform::get().current_window().scale_factor.peek();
                let width = (f64::from(event.area.width()) * scale).round().max(0.0) as u32;
                let height = (f64::from(event.area.height()) * scale).round().max(0.0) as u32;
                frames.set_size((width, height));
            })
            .on_press(press)
            .maybe_child(picture.read().clone().map(|handle| {
                image(handle)
                    .width(Size::fill())
                    .height(Size::fill())
                    .aspect_ratio(AspectRatio::Min)
                    .image_cover(ImageCover::Center)
                    .sampling_mode(SamplingMode::Bilinear)
            }))
    }
}
