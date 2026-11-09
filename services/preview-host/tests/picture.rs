//! The host decoding pictures a viewer's own codecs do not read, and an outline.

mod common;

use std::io::Cursor;
use std::path::Path;
use std::time::Duration;

use bingux_preview_core::host::protocol::{ErrorCode, Message, OpenedKind};
use bingux_preview_core::testing::{self, Chapter};
use common::*;

const LONG: Duration = Duration::from_secs(20);

fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn open_image(host: &mut Host, path: &Path, raw: bool) -> Message {
    host.send_file(Message::OpenImage { raw }, &[path]);
    host.wait(LONG, |message| {
        matches!(message, Message::Opened { .. } | Message::Error { .. })
    })
    .expect("Opened or Error")
    .packet
    .message
}

fn tiff_bytes(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbImage::from_pixel(width, height, image::Rgb([10, 200, 30]));
    let mut out = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Tiff)
        .unwrap();
    out
}

#[test]
fn a_tiff_opens_as_a_picture_and_renders_at_the_size_asked_for() {
    let dir = fixture_dir().to_path_buf();
    let path = write(&dir, "picture-scan.tiff", &tiff_bytes(400, 300));
    let mut host = Host::spawn();
    let Message::Opened {
        kind,
        video_w,
        video_h,
        pages,
        ..
    } = open_image(&mut host, &path, false)
    else {
        panic!("the tiff opens")
    };
    assert_eq!(
        (kind, video_w, video_h, pages),
        (OpenedKind::Image, 400, 300, 1)
    );
    let request = host.send(Message::RenderPage {
        page: 0,
        scale_milli: 1000,
        max_w: 200,
        max_h: 200,
    });
    let ready = host
        .wait(LONG, |message| matches!(message, Message::PageReady { .. }))
        .expect("the picture is drawn");
    assert_eq!(ready.packet.request, request);
    let Message::PageReady {
        width,
        height,
        size,
        ..
    } = ready.packet.message
    else {
        unreachable!()
    };
    assert_eq!((width, height), (200, 150));
    let mut fds = ready.fds;
    let data = pixels(fds.remove(0), size);
    assert_eq!(&data[..4], &[10, 200, 30, 255]);
}

#[test]
fn something_that_is_not_a_picture_is_an_error_in_words() {
    let dir = fixture_dir().to_path_buf();
    let path = write(&dir, "picture-notes.tiff", b"this is not a picture at all");
    let mut host = Host::spawn();
    let Message::Error { code, message } = open_image(&mut host, &path, false) else {
        panic!("an error")
    };
    assert_eq!(code, ErrorCode::Unsupported);
    assert!(message.contains("not supported"), "{message}");
}

#[test]
fn a_raw_file_shows_the_jpeg_it_carries() {
    let dir = fixture_dir().to_path_buf();
    let mut jpeg = Vec::new();
    image::RgbImage::from_pixel(900, 600, image::Rgb([0, 0, 200]))
        .write_to(&mut Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
        .unwrap();
    let mut file = b"II*\0\x08\0\0\0some maker header".to_vec();
    file.extend_from_slice(&jpeg);
    let path = write(&dir, "picture-IMG_0001.cr2", &file);
    let mut host = Host::spawn();
    let Message::Opened {
        kind,
        video_w,
        video_h,
        ..
    } = open_image(&mut host, &path, true)
    else {
        panic!("the raw opens")
    };
    assert_eq!((kind, video_w, video_h), (OpenedKind::Image, 900, 600));
}

#[test]
fn an_outline_lists_chapters_and_sections_with_their_pages() {
    let dir = fixture_dir().to_path_buf();
    let pdf = testing::pdf_with_outline(
        &[
            &[("One", 72, 720)],
            &[("Two", 72, 720)],
            &[("Three", 72, 720)],
        ],
        &[
            Chapter::new("Start", 0).with(Chapter::new("Details", 1)),
            Chapter::new("End", 2),
        ],
    );
    let path = write(&dir, "picture-outlined.pdf", &pdf);
    let mut host = Host::spawn();
    host.send_file(Message::OpenDocument, &[&path]);
    host.wait(LONG, |message| matches!(message, Message::Opened { .. }))
        .expect("opened");
    host.send(Message::Outline);
    let mut entries = Vec::new();
    loop {
        let received = host
            .wait(LONG, |message| matches!(message, Message::OutlineIs { .. }))
            .expect("outline");
        let Message::OutlineIs {
            entries: more,
            more: again,
        } = received.packet.message
        else {
            unreachable!()
        };
        entries.extend(more);
        if !again {
            break;
        }
    }
    let rows: Vec<(u8, u32, &str)> = entries
        .iter()
        .map(|entry| (entry.depth, entry.page, entry.title.as_str()))
        .collect();
    assert_eq!(rows, [(0, 0, "Start"), (1, 1, "Details"), (0, 2, "End")]);
}

#[test]
fn a_document_without_an_outline_says_so() {
    let mut host = Host::spawn();
    host.send_file(Message::OpenDocument, &[three_page_pdf()]);
    host.wait(LONG, |message| matches!(message, Message::Opened { .. }))
        .expect("opened");
    host.send(Message::Outline);
    let received = host
        .wait(LONG, |message| matches!(message, Message::OutlineIs { .. }))
        .expect("outline");
    let Message::OutlineIs { entries, more } = received.packet.message else {
        unreachable!()
    };
    assert!(entries.is_empty() && !more);
}
