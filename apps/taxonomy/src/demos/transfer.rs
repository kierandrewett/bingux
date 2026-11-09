//! Drag and drop and the clipboard: sources to drag out of the window,
//! targets that take drops from it, from another window and from other
//! programs, and the typed clipboard.

use std::path::PathBuf;

use bingux_ui::clipboard::Clipboard;
use bingux_ui::dnd::{
    DragAction, DragActions, DragEnd, DragInfo, Dropped, Format, Payload, Received, SPRING_DELAY,
    drag_source, drop_target,
};
use bingux_ui::prelude::*;
use freya::prelude::*;

use super::common::rows;
use crate::model::Spec;

/// A chip that can be picked up.
fn chip(text: &'static str) -> Element {
    let tokens = use_tokens();
    rect()
        .tw(
            &tokens,
            &[px(S3), py(S2), rounded(Radius::Pill), bg(Paint::Raised)],
        )
        .child(typography::BODY.label(&tokens, text))
        .into_element()
}

/// The file the "File" chip drags: written when the drag starts.
fn demo_file() -> PathBuf {
    let path = std::env::temp_dir().join("Dragged from Taxonomy.txt");
    let _ = std::fs::write(&path, "Dragged out of the Bingux taxonomy.\n");
    path
}

/// A small picture, encoded only when someone drops it. The picture of a
/// raster surface needs no GPU context to encode, which the deprecation is
/// about.
#[allow(deprecated)]
fn demo_png() -> Vec<u8> {
    use freya::engine::prelude::{EncodedImageFormat, SkColor, raster_n32_premul};
    let Some(mut surface) = raster_n32_premul((96, 96)) else {
        return Vec::new();
    };
    surface.canvas().clear(SkColor::from_rgb(53, 132, 228));
    surface
        .image_snapshot()
        .encode_to_data(EncodedImageFormat::PNG)
        .map(|data| data.as_bytes().to_vec())
        .unwrap_or_default()
}

/// One line saying what a drop or a paste brought.
fn describe(data: &Received) -> String {
    let what = match data.format() {
        Format::Uris => {
            let files = data.files();
            if files.is_empty() {
                format!("{} links", data.uris().len())
            } else {
                files
                    .iter()
                    .map(|path| {
                        path.file_name().map_or_else(
                            || path.display().to_string(),
                            |name| name.to_string_lossy().into_owned(),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        }
        Format::Image => format!("A picture, {} bytes", data.bytes().len()),
        _ => data
            .text()
            .map(|text| text.chars().take(80).collect())
            .unwrap_or_else(|| format!("{} bytes", data.bytes().len())),
    };
    format!("{what} ({})", data.mime())
}

fn action_name(action: DragAction) -> &'static str {
    match action {
        DragAction::Copy => "Copy",
        DragAction::Move => "Move",
        DragAction::Ask => "Ask",
    }
}

pub const DRAG_AND_DROP: &[Spec] = &[
    Spec::live(
        "Drag out: to a target below, another window or another program",
        || {
            let mut ended = use_state(|| None::<String>);
            let report = move |end: DragEnd| {
                ended.set(Some(match end {
                    DragEnd::Dropped(action) => format!("Dropped: {}", action_name(action)),
                    DragEnd::Cancelled => "Cancelled".to_string(),
                }))
            };
            let tokens = use_tokens();
            rows(vec![
                row("Text")
                    .trailing(
                        drag_source(chip("Drag this text"), |_| {
                            Payload::new().text("Hello from the Bingux taxonomy")
                        })
                        .on_end(report),
                    )
                    .into_element(),
                row("File")
                    .trailing(
                        drag_source(chip("Drag this file"), |_| Payload::files([demo_file()]))
                            .actions(DragActions::COPY)
                            .on_end(report),
                    )
                    .into_element(),
                row("Picture")
                    .trailing(
                        drag_source(chip("Drag this picture"), |_| {
                            Payload::new().lazy("image/png", demo_png)
                        })
                        .on_end(report),
                    )
                    .into_element(),
                row("Last drag")
                    .trailing(typography::BODY.in_ink(Ink::Muted).label(
                        &tokens,
                        ended.read().clone().unwrap_or_else(|| "None yet".into()),
                    ))
                    .into_element(),
            ])
        },
    )
    .wide(),
    Spec::live(
        "Drop target: files, pictures and text from anywhere",
        || {
            let tokens = use_tokens();
            let mut last = use_state(|| None::<String>);
            let target = drop_target(
                rect()
                    .tw(&tokens, &[w_full(), p(S4), flex_col(), gap(S2)])
                    .child(typography::HEADING.label(&tokens, "Drop here"))
                    .child(
                        typography::BODY.in_ink(Ink::Muted).label(
                            &tokens,
                            last.read()
                                .clone()
                                .unwrap_or_else(|| "Files, pictures or text".into()),
                        ),
                    ),
            )
            .accepts([Format::Uris, Format::Image, Format::Text])
            .on_drop(move |dropped: Dropped| {
                let from = if dropped.data.is_local() {
                    "this program"
                } else {
                    "another program"
                };
                last.set(Some(format!(
                    "{}: {} from {from}",
                    action_name(dropped.action),
                    describe(&dropped.data)
                )));
            });
            rect().tw(&tokens, &[w_full()]).child(target).into_element()
        },
    )
    .wide(),
    Spec::live("Spring-loaded: rest a drag here to open it", || {
        let tokens = use_tokens();
        let mut opened = use_state(|| 0u32);
        let target = drop_target(
            rect()
                .tw(&tokens, &[w_full(), p(S4)])
                .child(typography::BODY.label(&tokens, format!("Opened {} times", opened.read()))),
        )
        .accepts([Format::Uris, Format::Text])
        .spring_loaded(SPRING_DELAY, move |_: DragInfo| *opened.write() += 1);
        rect().tw(&tokens, &[w_full()]).child(target).into_element()
    })
    .wide(),
];

pub const CLIPBOARD: &[Spec] = &[
    Spec::live("Copy several types, paste the one you want", || {
        let tokens = use_tokens();
        let pasted = use_state(|| None::<String>);
        let paste = move |format: Format| {
            let mut pasted = pasted;
            pasted.set(Some(match Clipboard::general().read(format) {
                Ok(data) => describe(&data),
                Err(error) => error.to_string(),
            }));
        };
        rows(vec![
            row("Copy")
                .trailing(
                    rect()
                        .tw(&tokens, &[flex_row(), gap(S2)])
                        .child(action_button("Text", |_| {
                            Clipboard::general().write_text("Copied from the Bingux taxonomy");
                        }))
                        .child(action_button("File", |_| {
                            Clipboard::general().write(Payload::copied_files([demo_file()], false));
                        }))
                        .child(action_button("Picture", |_| {
                            Clipboard::general().write(Payload::new().lazy("image/png", demo_png));
                        })),
                )
                .into_element(),
            row("Paste")
                .trailing(
                    rect()
                        .tw(&tokens, &[flex_row(), gap(S2)])
                        .child(action_button("Text", move |_| paste(Format::Text)))
                        .child(action_button("Files", move |_| paste(Format::Uris)))
                        .child(action_button("Picture", move |_| paste(Format::Image))),
                )
                .into_element(),
            row("Pasted")
                .subtitle(
                    pasted
                        .read()
                        .clone()
                        .unwrap_or_else(|| "Nothing yet".into()),
                )
                .into_element(),
            row("On the clipboard")
                .subtitle({
                    let formats = Clipboard::general().formats();
                    if formats.is_empty() {
                        "Nothing".to_string()
                    } else {
                        formats
                            .iter()
                            .map(|format| format!("{format:?}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                })
                .into_element(),
        ])
    })
    .wide(),
];
