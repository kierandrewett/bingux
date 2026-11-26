//! The two cards that hang from the toolbar: what is known about the page's
//! connection and permissions, and the downloads.

use bingux_ui::prelude::*;
use bingux_ui::web_view::DownloadId;
use bingux_ui::web_view::requests::{DownloadInfo, DownloadState};
use freya::prelude::*;

use crate::model::downloads;
use crate::site_info::SiteInfoPopover;

use super::ctx::Ctx;

/// Both popovers, open or not.
#[derive(PartialEq)]
pub struct Popovers {
    pub ctx: Ctx,
}

impl Component for Popovers {
    fn render(&self) -> impl IntoElement {
        let ctx = self.ctx;
        let chrome = ctx.chrome;
        let mut downloads_open = chrome.downloads_open;
        let downloads_anchor = chrome.downloads_anchor.read().clone();
        rect()
            .width(Size::px(0.0))
            .height(Size::px(0.0))
            .child(SiteInfoPopover::element(ctx))
            .child(
                popover(downloads_open(), &downloads_anchor, download_list(ctx))
                    .width_px(ctx.popover_width())
                    .on_dismiss(move |_: ()| downloads_open.set(false)),
            )
    }
}

/// One download's controls: pause or resume, and cancel; Retry for one that
/// failed or was cancelled (the engine's Resume takes it from where it stopped).
fn download_controls(ctx: Ctx, download: &DownloadInfo) -> Option<Element> {
    let id: DownloadId = download.id;
    // An engine with no call for it gets no button that does nothing.
    if !ctx.browser.read().engine.capabilities().download_control {
        return None;
    }
    let backend = ctx.browser.read().engine.backend().clone();
    match download.state {
        DownloadState::Running => {
            let pause = backend.clone();
            Some(
                rect()
                    .tw_layout(&[flex_row(), items(Align::Center), gap(S1)])
                    .child(
                        icon_button(Icon::Pause, move |_: ()| {
                            let _ = pause.pause_download(id);
                        })
                        .tooltip("Pause"),
                    )
                    .child(
                        icon_button(Icon::WindowClose, move |_: ()| {
                            let _ = backend.cancel_download(id);
                        })
                        .tooltip("Cancel download"),
                    )
                    .into_element(),
            )
        }
        DownloadState::Paused => Some(
            rect()
                .tw_layout(&[flex_row(), items(Align::Center), gap(S1)])
                .child({
                    let resume = backend.clone();
                    icon_button(Icon::MediaPlaybackStart, move |_: ()| {
                        let _ = resume.resume_download(id);
                    })
                    .tooltip("Resume")
                })
                .child(
                    icon_button(Icon::WindowClose, move |_: ()| {
                        let _ = backend.cancel_download(id);
                    })
                    .tooltip("Cancel download"),
                )
                .into_element(),
        ),
        DownloadState::Failed | DownloadState::Cancelled => Some(
            rect()
                .tw_layout(&[flex_row(), items(Align::Center), gap(S1)])
                .child(
                    icon_button(Icon::Refresh, move |_: ()| {
                        let _ = backend.resume_download(id);
                    })
                    .tooltip("Retry"),
                )
                .into_element(),
        ),
        DownloadState::Finished => None,
    }
}

/// The downloads of this window.
fn download_list(ctx: Ctx) -> Element {
    let list: Vec<DownloadInfo> = ctx.browser.read().downloads.list().to_vec();
    if list.is_empty() {
        return rect()
            .tw_layout(&[flex_col(), w_full()])
            .child(
                Group::new(None::<String>).row(
                    row("No downloads")
                        .subtitle("Files you save appear here.")
                        .leading(Icon::FolderDownload),
                ),
            )
            .into_element();
    }
    let mut group = Group::new("Downloads".to_string());
    for download in &list {
        let mut line = row(download.filename.clone())
            .subtitle(downloads::status_line(download))
            .leading(Icon::FolderDownload);
        if download.state == DownloadState::Failed {
            line = line.leading_tone(bingux_ui::style::Tone::Danger);
        }
        let controls = download_controls(ctx, download);
        let fraction =
            downloads::fraction(download).filter(|_| download.state == DownloadState::Running);
        if controls.is_some() || fraction.is_some() {
            let mut trailing = rect().tw_layout(&[flex_row(), items(Align::Center), gap(S2)]);
            if let Some(fraction) = fraction.filter(|_| !is_compact()) {
                trailing = trailing.child(
                    rect()
                        .tw_layout(&[w(Dim::Box9)])
                        .child(progress_bar(fraction)),
                );
            }
            if let Some(controls) = controls {
                trailing = trailing.child(controls);
            }
            line = line.trailing(trailing);
        }
        group = group.row(line);
    }
    let finished = list.iter().any(|download| {
        !matches!(
            download.state,
            DownloadState::Running | DownloadState::Paused
        )
    });
    let mut column = rect()
        .tw_layout(&[flex_col(), w_full(), gap(S3)])
        .child(group);
    if finished {
        column = column.child(
            action_button("Clear finished", move |_: ()| {
                ctx.browser.clone().write().clear_finished_downloads();
            })
            .flat()
            .compact(),
        );
    }
    column.into_element()
}
