//! The two cards that hang from the toolbar: what is known about the page's
//! connection and permissions, and the downloads.

use bingux_ui::prelude::*;
use bingux_ui::web_view::DownloadId;
use bingux_ui::web_view::requests::{DownloadInfo, DownloadState};
use freya::prelude::*;

use crate::address;
use crate::model::downloads;
use crate::model::security::{self, Tone};
use crate::model::tab::TabKind;

use super::ctx::Ctx;

/// The width of both: wide enough for a sentence, and a phone's own column.
fn width(ctx: Ctx) -> f32 {
    if is_compact() {
        // The phone's whole column, less the margin a floating card keeps.
        (ctx.window.size().0 - 2.0 * S2.px()).max(Dim::MenuWide.px())
    } else {
        Dim::Dialog.px()
    }
}

fn tone_of(tone: Tone) -> Option<bingux_ui::style::Tone> {
    match tone {
        Tone::Neutral => None,
        Tone::Good => None,
        Tone::Warning => Some(bingux_ui::style::Tone::Warning),
        Tone::Danger => Some(bingux_ui::style::Tone::Danger),
    }
}

/// Both popovers, open or not.
#[derive(PartialEq)]
pub struct Popovers {
    pub ctx: Ctx,
}

impl Component for Popovers {
    fn render(&self) -> impl IntoElement {
        let ctx = self.ctx;
        let chrome = ctx.chrome;
        let mut info_open = chrome.info_open;
        let mut downloads_open = chrome.downloads_open;
        let info_anchor = chrome.info_anchor.read().clone();
        let downloads_anchor = chrome.downloads_anchor.read().clone();
        rect()
            .width(Size::px(0.0))
            .height(Size::px(0.0))
            .child(
                popover(info_open(), &info_anchor, page_info(ctx))
                    .width_px(width(ctx))
                    .on_dismiss(move |_: ()| info_open.set(false)),
            )
            .child(
                popover(downloads_open(), &downloads_anchor, download_list(ctx))
                    .width_px(width(ctx))
                    .on_dismiss(move |_: ()| downloads_open.set(false)),
            )
    }
}

/// The connection of the page, and what the site may do.
fn page_info(ctx: Ctx) -> Element {
    let (summary, origin, remembered, key, internal) = {
        let browser = ctx.browser.read();
        let Some(tab) = browser.active() else {
            return rect().into_element();
        };
        let summary = security::summarize(&tab.security, &tab.url);
        let origin = address::origin(&tab.url);
        (
            summary,
            origin.clone(),
            browser.permissions.for_origin(&origin),
            tab.key,
            tab.kind == TabKind::Settings || tab.is_blank(),
        )
    };
    let mut status_row = row(summary.headline.clone())
        .subtitle(summary.detail.clone())
        .leading(summary.icon);
    if let Some(tone) = tone_of(summary.tone) {
        status_row = status_row.leading_tone(tone);
    }
    let mut column = rect()
        .tw_layout(&[flex_col(), w_full(), gap(S3)])
        .child(Group::new(None::<String>).row(status_row));
    if !internal && origin.contains("://") {
        let site = Group::new("Site".to_string())
            .row(property_row("Site", address::host(&origin)))
            .row(property_row("Address", {
                let browser = ctx.browser.read();
                browser
                    .tabs
                    .get(key)
                    .map(|tab| address::display(&tab.url))
                    .unwrap_or_default()
            }));
        column = column.child(site);
    }
    if !remembered.is_empty() {
        let mut group = Group::new("Permissions".to_string())
            .footer("Choices you made for this site. Reset one to be asked again.");
        for (name, allowed) in remembered {
            let site = origin.clone();
            let forget = name.clone();
            group = group.row(
                row(name)
                    .value(if allowed { "Allowed" } else { "Blocked" })
                    .trailing(
                        action_button("Reset", move |_: ()| {
                            ctx.with(|browser| browser.permissions.forget(&site, &forget));
                        })
                        .compact()
                        .flat(),
                    ),
            );
        }
        column = column.child(group);
    }
    column.into_element()
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
