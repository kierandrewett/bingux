//! The host rendering and searching a PDF written by hand.

mod common;

use std::path::Path;
use std::time::Duration;

use bingux_preview_core::host::protocol::{ErrorCode, Message, OpenedKind, Word};
use common::*;

const LONG: Duration = Duration::from_secs(20);

fn open(host: &mut Host, path: &Path) -> Message {
    host.send_file(Message::OpenDocument, &[path]);
    host.wait(LONG, |message| matches!(message, Message::Opened { .. } | Message::Error { .. })).expect("Opened or Error").packet.message
}

fn render(host: &mut Host, page: u32, scale_milli: u32, max_w: u32, max_h: u32) -> (u32, u32, Vec<u8>) {
    let request = host.send(Message::RenderPage { page, scale_milli, max_w, max_h });
    let ready = host
        .wait(LONG, |message| matches!(message, Message::PageReady { .. } | Message::Error { .. }))
        .expect("the page is rendered");
    assert_eq!(ready.packet.request, request, "the render is answered under its own request");
    let Message::PageReady { page: got, width, height, stride, size } = ready.packet.message else { panic!("{:?}", ready.packet.message) };
    assert_eq!(got, page);
    assert_eq!(stride, width * 4);
    assert_eq!(size, width * height * 4);
    let mut fds = ready.fds;
    assert_eq!(fds.len(), 1);
    (width, height, pixels(fds.remove(0), size))
}

fn words(host: &mut Host, page: u32) -> Vec<Word> {
    host.send(Message::PageText { page });
    let mut all = Vec::new();
    loop {
        let received = host.wait(LONG, |message| matches!(message, Message::PageTextIs { .. })).expect("text");
        let Message::PageTextIs { words, more, .. } = received.packet.message else { unreachable!() };
        all.extend(words);
        if !more {
            return all;
        }
    }
}

#[test]
fn a_pdf_opens_with_its_page_count_and_every_page_size() {
    let mut host = Host::spawn();
    let Message::Opened { kind, pages, .. } = open(&mut host, three_page_pdf()) else { panic!("the pdf opens") };
    assert_eq!((kind, pages), (OpenedKind::Document, 3));
    let sizes = host.wait(LONG, |message| matches!(message, Message::PageSizes { .. })).expect("sizes");
    let Message::PageSizes { first, sizes } = sizes.packet.message else { unreachable!() };
    assert_eq!(first, 0);
    assert_eq!(sizes, vec![(612_000, 792_000); 3]);
}

#[test]
fn a_page_renders_at_the_requested_scale_with_text_drawn_on_white() {
    let mut host = Host::spawn();
    open(&mut host, three_page_pdf());
    let (width, height, bytes) = render(&mut host, 0, 1000, 0, 0);
    assert_eq!((width, height), (612, 792));
    // Opaque, mostly white, with dark text pixels.
    assert!(bytes.chunks_exact(4).all(|pixel| pixel[3] == 255));
    let white = bytes.chunks_exact(4).filter(|pixel| pixel[..3] == [255, 255, 255]).count();
    let dark = bytes.chunks_exact(4).filter(|pixel| pixel[..3].iter().all(|channel| *channel < 80)).count();
    assert!(white > bytes.len() / 4 * 9 / 10, "page is white");
    assert!(dark > 200, "{dark} dark pixels of text");
    // The text sits near the top: the first rows with ink are in the top 15 %.
    let first_ink_row = (0..height as usize).find(|row| bytes[row * width as usize * 4..(row + 1) * width as usize * 4].chunks_exact(4).any(|pixel| pixel[0] < 80)).unwrap();
    assert!(first_ink_row < 792 * 15 / 100, "the first ink is at row {first_ink_row}");
}

#[test]
fn a_render_is_cut_to_the_box_it_is_given_and_thumbnails_are_small() {
    let mut host = Host::spawn();
    open(&mut host, three_page_pdf());
    let (width, height, _) = render(&mut host, 1, 4000, 500, 5000);
    assert_eq!(width, 500);
    assert_eq!(height, 647, "the aspect ratio is kept");
    let (width, height, _) = render(&mut host, 2, 1000, 96, 128);
    assert!(width <= 96 && height <= 128 && width >= 90, "{width}x{height}");
}

#[test]
fn words_come_with_boxes_in_page_space_from_the_top_left() {
    let mut host = Host::spawn();
    open(&mut host, three_page_pdf());
    let all = words(&mut host, 0);
    let texts: Vec<&str> = all.iter().map(|word| word.text.as_str()).collect();
    assert_eq!(texts, vec!["Invoice", "105", "Total", "due", "40.00"]);
    let invoice = &all[0].rect;
    // 24 pt Helvetica at baseline y=720 of 792: its box is near the top, 72 pt in.
    assert!((60_000..90_000).contains(&invoice.x0), "x0 {}", invoice.x0);
    assert!((40_000..75_000).contains(&invoice.y0), "y0 {} (from the top)", invoice.y0);
    assert!(invoice.y1 > invoice.y0 && invoice.x1 > invoice.x0);
    let total = &all[2].rect;
    assert!(total.y0 > 600_000, "the line near the bottom is far from the top: {}", total.y0);
}

#[test]
fn search_finds_every_hit_ignoring_case_and_agrees_with_the_word_boxes() {
    let mut host = Host::spawn();
    open(&mut host, three_page_pdf());
    let word_boxes = words(&mut host, 0);
    let request = host.send(Message::Search { query: "invoice".into() });
    let packets = host.collect_until(LONG, |message| matches!(message, Message::SearchDone { .. }));
    let hits: Vec<(u32, usize)> = packets
        .iter()
        .filter_map(|received| match &received.packet.message {
            Message::SearchHit { page, rects } => Some((*page, rects.len())),
            _ => None,
        })
        .collect();
    assert_eq!(hits, vec![(0, 1), (2, 1)]);
    assert!(packets.iter().all(|received| received.packet.request == request));
    assert!(matches!(packets.last().unwrap().packet.message, Message::SearchDone { hits: 2 }));
    let Message::SearchHit { rects, .. } = &packets[0].packet.message else { panic!("a hit") };
    let hit = rects[0];
    let word = word_boxes[0].rect;
    assert!(hit.x0.abs_diff(word.x0) < 4000 && hit.y0.abs_diff(word.y0) < 4000, "hit {hit:?} vs word {word:?}");
    // No hits is still answered.
    host.send(Message::Search { query: "zebra".into() });
    let done = host.wait(LONG, |message| matches!(message, Message::SearchDone { .. })).expect("done");
    assert!(matches!(done.packet.message, Message::SearchDone { hits: 0 }));
}

#[test]
fn renders_that_are_cancelled_before_they_start_are_skipped() {
    let mut host = Host::spawn();
    open(&mut host, three_page_pdf());
    let mut requests = Vec::new();
    for _ in 0..40 {
        requests.push(host.send(Message::RenderPage { page: 0, scale_milli: 6000, max_w: 4000, max_h: 4000 }));
    }
    for request in &requests[1..] {
        host.send(Message::Cancel { target: *request });
    }
    // A page asked for afterwards is answered: the queue is not stuck.
    let last = host.send(Message::RenderPage { page: 1, scale_milli: 500, max_w: 400, max_h: 400 });
    let mut answered = Vec::new();
    let deadline = std::time::Instant::now() + LONG;
    while std::time::Instant::now() < deadline {
        let Some(received) = host.next(Duration::from_millis(500)) else { continue };
        if matches!(received.packet.message, Message::PageReady { .. }) {
            answered.push(received.packet.request);
            if received.packet.request == last {
                break;
            }
        }
    }
    assert!(answered.contains(&last));
    assert!(answered.len() < 20, "{} renders ran although 39 were cancelled", answered.len());
}

#[test]
fn a_damaged_document_is_an_error_and_a_huge_one_is_refused_unread() {
    let mut host = Host::spawn();
    let Message::Error { code, message } = open(&mut host, &junk("junk.pdf")) else { panic!("junk is refused") };
    assert_eq!(code, ErrorCode::Corrupt, "{message}");
    let mut host = Host::spawn();
    let Message::Error { code, message } = open(&mut host, &huge_pdf()) else { panic!("too large is refused") };
    assert_eq!(code, ErrorCode::Unsupported);
    assert_eq!(message, "This file is too large to preview.");
}

#[test]
fn asking_for_a_page_that_is_not_there_is_an_error_not_a_crash() {
    let mut host = Host::spawn();
    open(&mut host, three_page_pdf());
    host.send(Message::RenderPage { page: 99, scale_milli: 1000, max_w: 0, max_h: 0 });
    let error = host.wait(LONG, |message| matches!(message, Message::Error { .. })).expect("an error");
    assert!(matches!(error.packet.message, Message::Error { code: ErrorCode::NotFound, .. }));
    // The next render is fine.
    let (width, _, _) = render(&mut host, 0, 500, 0, 0);
    assert_eq!(width, 306);
}
