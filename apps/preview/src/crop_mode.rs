//! Crop mode: the picture at fit, a crop box over it, and the bar that says
//! what to do with it.
//!
//! The picture is a `ZoomView` with its own keys and pill switched off, turned
//! the way the person had it; the box, its handles, the shape list and the
//! keys are `bingux_ui::crop`. This file puts the two together and hands the
//! box, as fractions of the picture as it looks, to the window.

use std::path::PathBuf;

use bingux_ui::crop::{CropBar, CropOverlay, use_crop};
use bingux_ui::prelude::*;
use bingux_ui::component::ComponentName;
use bingux_ui::surface::use_tokens;
use bingux_ui::zoom_view::{ZoomView, turned, use_zoom_handle};
use freya::prelude::*;

use crate::copy;
use crate::edit::Crop;

/// What a crop is about: the picture, its size as it is drawn, and how it was
/// turned when the person began.
#[derive(Clone, Debug, PartialEq)]
pub struct CropJob {
    pub file: PathBuf,
    /// Pixels, as drawn (the file's own orientation tag applied), before the turn below.
    pub size: (f32, f32),
    /// Quarter turns clockwise, and whether mirrored, as the person had it.
    pub orientation: (u8, bool),
}

#[derive(PartialEq)]
pub struct CropEditor {
    pub job: CropJob,
    pub on_save: EventHandler<Crop>,
    pub on_cancel: EventHandler<()>,
}

impl Component for CropEditor {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let (turns, mirrored) = self.job.orientation;
        let crop = use_crop(turned(self.job.size, turns));
        let zoom = use_zoom_handle();
        let file = self.job.file.clone();
        let picture = ZoomView::new(Some(self.job.size), move |width, height| {
            ImageViewer::new(file.clone())
                .width(Size::px(width))
                .height(Size::px(height))
                .aspect_ratio(AspectRatio::Min)
                .image_cover(ImageCover::Center)
                .into()
        })
        .handle(zoom)
        .orientation(turns, mirrored)
        .controls(false)
        .keys_when(|| false)
        .label(copy::picture_label(
            &self
                .job
                .file
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        ));

        let save = {
            let on_save = self.on_save.clone();
            move |_: ()| on_save.call(crop.fractions())
        };
        let cancel = {
            let on_cancel = self.on_cancel.clone();
            move |_: ()| on_cancel.call(())
        };
        rect()
            .component("CropEditor")
            .tw(&tokens, &[w_full(), h_full(), flex_col(), grow_children()])
            .child(
                rect().tw(&tokens, &[w_full(), h_flex()]).child(
                    CropOverlay::new(crop)
                        .content(picture.into_element())
                        .label(copy::CROP_AREA)
                        .on_commit(save.clone())
                        .on_cancel(cancel.clone()),
                ),
            )
            .child(
                rect()
                    .tw(
                        &tokens,
                        &[w_full(), flex_row(), justify(Align::Center), p(S2), bg(Paint::Bg)],
                    )
                    .child(
                        CropBar::new(crop)
                            .commit_label(copy::CROP_SAVE)
                            .cancel_label(copy::CANCEL)
                            .on_commit(save)
                            .on_cancel(cancel),
                    ),
            )
    }
}
