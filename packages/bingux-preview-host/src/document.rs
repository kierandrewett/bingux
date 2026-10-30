//! Opening and rendering a PDF with poppler.
//!
//! poppler's objects belong to one thread, so a worker thread owns the
//! document and everything the viewer asks for is a job in its queue. The
//! reader thread only enqueues: it never waits for a render. A page render
//! cancelled before it starts is skipped (a viewer that scrolled past a page
//! sends `Cancel`), and a search checks for a cancel between pages.

use std::collections::{HashSet, VecDeque};
use std::os::fd::{AsRawFd, IntoRawFd, OwnedFd};
use std::sync::{Arc, Condvar, Mutex};

use bingux_preview_core::host::protocol::{ErrorCode, MAX_PAGE_SIZES, MAX_PAGES, Message, OpenedKind, Rect, Word};
use bingux_preview_core::host::shm::{MAX_BYTES, Shared};
use poppler::Document;

use crate::limits::{self, TOO_LARGE};
use crate::link::{Link, Watchdog};

/// The most jobs waiting; the oldest page renders are dropped beyond it.
const QUEUE_LIMIT: usize = 256;
/// The most search hits sent for one query.
const MAX_HITS: u32 = 5000;
/// The most words of one page sent for selection.
const MAX_PAGE_WORDS: usize = 20_000;
/// Bytes of words in one `PageTextIs` message, under the message limit.
const WORD_BUDGET: usize = 3800;

pub enum Job {
    Open { request: u32, file: OwnedFd },
    Render { request: u32, page: u32, scale_milli: u32, max_w: u32, max_h: u32 },
    Text { request: u32, page: u32 },
    Search { request: u32, query: String },
}

struct Queue {
    jobs: Mutex<VecDeque<Job>>,
    ready: Condvar,
    cancelled: Mutex<HashSet<u32>>,
}

pub struct Worker {
    queue: Arc<Queue>,
}

impl Worker {
    pub fn start(link: Arc<Link>, watchdog: Arc<Watchdog>) -> std::io::Result<Worker> {
        let queue = Arc::new(Queue { jobs: Mutex::new(VecDeque::new()), ready: Condvar::new(), cancelled: Mutex::new(HashSet::new()) });
        let worker = Arc::clone(&queue);
        std::thread::Builder::new().name("document".into()).spawn(move || work(&link, &watchdog, &worker))?;
        Ok(Worker { queue })
    }

    pub fn submit(&self, job: Job) {
        let mut jobs = self.queue.jobs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if jobs.len() >= QUEUE_LIMIT {
            // The viewer asks again for what it still needs.
            if let Some(stale) = jobs.iter().position(|job| matches!(job, Job::Render { .. } | Job::Text { .. })) {
                jobs.remove(stale);
            }
        }
        jobs.push_back(job);
        self.queue.ready.notify_one();
    }

    /// Stop the job of `request`: a queued job is skipped, a search stops at
    /// its next page. A render already running finishes and is sent.
    pub fn cancel(&self, request: u32) {
        self.queue.cancelled.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(request);
    }
}

fn was_cancelled(queue: &Queue, request: u32) -> bool {
    queue.cancelled.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).contains(&request)
}

fn forget(queue: &Queue, request: u32) {
    queue.cancelled.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&request);
}

fn work(link: &Link, watchdog: &Watchdog, queue: &Queue) {
    let mut document: Option<OpenDocument> = None;
    loop {
        let job = {
            let mut jobs = queue.jobs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            loop {
                if let Some(job) = jobs.pop_front() {
                    break job;
                }
                jobs = queue.ready.wait(jobs).unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        };
        let request = match &job {
            Job::Open { request, .. } | Job::Render { request, .. } | Job::Text { request, .. } | Job::Search { request, .. } => *request,
        };
        if !matches!(job, Job::Open { .. } | Job::Search { .. }) && was_cancelled(queue, request) {
            forget(queue, request);
            continue;
        }
        let _busy = watchdog.busy();
        match job {
            Job::Open { request, file } => match open(link, request, file) {
                Ok(opened) => document = Some(opened),
                Err((code, message)) => link.error(request, code, &message),
            },
            Job::Render { request, page, scale_milli, max_w, max_h } => {
                let Some(document) = &document else {
                    link.error(request, ErrorCode::Internal, "No document is open.");
                    continue;
                };
                match render(&document.inner, page, scale_milli, max_w, max_h) {
                    Ok((width, height, shared)) => {
                        let message = Message::PageReady { page, width, height, stride: width * 4, size: shared.len() as u32 };
                        link.send_with(request, message, &[shared.raw_fd()]);
                    }
                    Err((code, message)) => link.error(request, code, &message),
                }
            }
            Job::Text { request, page } => {
                let Some(document) = &document else { continue };
                for message in page_text(&document.inner, page) {
                    link.send(request, message);
                }
            }
            Job::Search { request, query } => {
                let Some(document) = &document else { continue };
                search(link, queue, &document.inner, request, &query);
                forget(queue, request);
            }
        }
    }
}

/// The document the host has open: poppler's handle, kept for the page jobs.
struct OpenDocument {
    inner: Document,
}

fn open(link: &Link, request: u32, file: OwnedFd) -> Result<OpenDocument, (ErrorCode, String)> {
    // SAFETY: fstat on a descriptor we own; `stat` is plain data.
    let size = unsafe {
        let mut stat: libc::stat = std::mem::zeroed();
        if libc::fstat(file.as_raw_fd(), &mut stat) != 0 {
            return Err((ErrorCode::NotFound, "The file could not be opened.".to_owned()));
        }
        stat.st_size as u64
    };
    if size > limits::MAX_DOCUMENT_BYTES {
        return Err((ErrorCode::Unsupported, TOO_LARGE.to_owned()));
    }
    // poppler takes the descriptor and closes it with the document.
    let document = Document::from_fd(file.into_raw_fd(), None).map_err(|error| {
        if error.matches(poppler::Error::Encrypted) {
            (ErrorCode::Unsupported, "This document is protected by a password.".to_owned())
        } else if error.matches(poppler::Error::OpenFile) {
            (ErrorCode::NotFound, "The document could not be opened.".to_owned())
        } else {
            (ErrorCode::Corrupt, "This document is damaged and cannot be shown.".to_owned())
        }
    })?;
    let pages = (document.n_pages().max(0) as u32).min(MAX_PAGES);
    if pages == 0 {
        return Err((ErrorCode::Corrupt, "This document has no pages.".to_owned()));
    }
    let title = document.title().map(|title| title.to_string()).unwrap_or_default();
    link.send(
        request,
        Message::Opened { kind: OpenedKind::Document, duration_ns: 0, video_w: 0, video_h: 0, has_audio: false, seekable: true, pages, title },
    );
    // Every page's size, so the viewer can lay out the scroll before any page is drawn.
    let mut first = 0u32;
    while first < pages {
        let end = (first + MAX_PAGE_SIZES as u32).min(pages);
        let sizes = (first..end)
            .map(|index| {
                let (width, height) = document.page(index as i32).map_or((612.0, 792.0), |page| page.size());
                (milli(width).max(1), milli(height).max(1))
            })
            .collect();
        link.send(request, Message::PageSizes { first, sizes });
        first = end;
    }
    Ok(OpenDocument { inner: document })
}

fn milli(points: f64) -> u32 {
    (points * 1000.0).round().clamp(0.0, f64::from(u32::MAX)) as u32
}

/// How big a render of a `width` by `height` point page is: `scale_milli`
/// pixels per point, cut to fit `max_w` by `max_h` (0 is no limit) and to the
/// largest frame the protocol carries.
pub fn pixel_size(width: f64, height: f64, scale_milli: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let mut scale = (f64::from(scale_milli) / 1000.0).clamp(0.02, 32.0);
    let mut fit = |limit: u32, extent: f64| {
        if limit > 0 && extent * scale > f64::from(limit) {
            scale = f64::from(limit) / extent;
        }
    };
    fit(max_w, width.max(1.0));
    fit(max_h, height.max(1.0));
    let side = 8192.0;
    fit(side as u32, width.max(1.0));
    fit(side as u32, height.max(1.0));
    let area = (MAX_BYTES / 4) as f64;
    if width * height * scale * scale > area {
        scale = (area / (width * height)).sqrt();
    }
    ((width * scale).round().max(1.0) as u32, (height * scale).round().max(1.0) as u32)
}

fn render(document: &Document, index: u32, scale_milli: u32, max_w: u32, max_h: u32) -> Result<(u32, u32, Shared), (ErrorCode, String)> {
    let page = document.page(index as i32).ok_or((ErrorCode::NotFound, format!("The document has no page {}.", index + 1)))?;
    let (page_w, page_h) = page.size();
    let (width, height) = pixel_size(page_w, page_h, scale_milli, max_w, max_h);
    let internal = |what: &str| (ErrorCode::Internal, format!("The page could not be drawn ({what})."));
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width as i32, height as i32).map_err(|_| internal("memory"))?;
    {
        let context = cairo::Context::new(&surface).map_err(|_| internal("cairo"))?;
        context.set_source_rgb(1.0, 1.0, 1.0);
        context.paint().map_err(|_| internal("paint"))?;
        context.scale(f64::from(width) / page_w.max(1.0), f64::from(height) / page_h.max(1.0));
        page.render(&context);
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().map_err(|_| internal("pixels"))?;
    let mut shared = Shared::create("bingux-page", width as usize * height as usize * 4).map_err(|_| internal("shared memory"))?;
    let out = shared.bytes_mut();
    for row in 0..height as usize {
        let source = &data[row * stride..row * stride + width as usize * 4];
        let target = &mut out[row * width as usize * 4..(row + 1) * width as usize * 4];
        for (to, from) in target.chunks_exact_mut(4).zip(source.chunks_exact(4)) {
            // cairo's ARGB32 is BGRA in memory on little-endian; the page is opaque.
            to[0] = from[2];
            to[1] = from[1];
            to[2] = from[0];
            to[3] = 255;
        }
    }
    Ok((width, height, shared))
}

/// A rectangle of the page in poppler's units as a protocol rectangle.
fn rect_of(x1: f64, y1: f64, x2: f64, y2: f64) -> Rect {
    Rect { x0: milli(x1.min(x2)) as i32, y0: milli(y1.min(y2)) as i32, x1: milli(x1.max(x2)) as i32, y1: milli(y1.max(y2)) as i32 }
}

/// Group the characters of a page into words, each with the union of its
/// characters' boxes. `chars` pairs a character with its box
/// `(x1, y1, x2, y2)`; a space or a line break ends a word.
pub fn words_of(chars: impl Iterator<Item = (char, [f64; 4])>) -> Vec<Word> {
    let mut words = Vec::new();
    let mut text = String::new();
    let mut bounds: Option<[f64; 4]> = None;
    let flush = |text: &mut String, bounds: &mut Option<[f64; 4]>, words: &mut Vec<Word>| {
        if let Some([x1, y1, x2, y2]) = bounds.take() {
            if !text.is_empty() {
                words.push(Word { rect: rect_of(x1, y1, x2, y2), text: std::mem::take(text) });
            }
        }
        text.clear();
    };
    for (character, [x1, y1, x2, y2]) in chars {
        if character.is_whitespace() || character == '\u{a0}' || character.is_control() {
            flush(&mut text, &mut bounds, &mut words);
            continue;
        }
        if text.len() >= 480 {
            flush(&mut text, &mut bounds, &mut words);
        }
        text.push(character);
        bounds = Some(match bounds {
            None => [x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2)],
            Some([a, b, c, d]) => [a.min(x1.min(x2)), b.min(y1.min(y2)), c.max(x1.max(x2)), d.max(y1.max(y2))],
        });
        if words.len() >= MAX_PAGE_WORDS {
            break;
        }
    }
    flush(&mut text, &mut bounds, &mut words);
    words
}

/// Cut words into messages that fit a datagram.
pub fn chunk_words(words: Vec<Word>) -> Vec<Vec<Word>> {
    let mut chunks: Vec<Vec<Word>> = Vec::new();
    let mut used = 0;
    for word in words {
        let size = 18 + word.text.len();
        let full = chunks.last().is_none_or(|chunk| chunk.len() >= bingux_preview_core::host::protocol::MAX_WORDS || used + size > WORD_BUDGET);
        if full {
            chunks.push(Vec::new());
            used = 0;
        }
        used += size;
        if let Some(chunk) = chunks.last_mut() {
            chunk.push(word);
        }
    }
    chunks
}

/// The words of a page with their boxes, as `PageTextIs` messages (the last
/// has `more` false).
fn page_text(document: &Document, index: u32) -> Vec<Message> {
    let Some(page) = document.page(index as i32) else {
        return vec![Message::PageTextIs { page: index, words: Vec::new(), more: false }];
    };
    let text = page.text().map(|text| text.to_string()).unwrap_or_default();
    let layout = layout_of(&page);
    // The layout is in page space from the top left, like a render; the
    // rectangles `find_text` answers with are from the bottom left.
    let chars = text.chars().zip(layout.iter()).map(|(character, rect)| (character, *rect));
    let chunks = chunk_words(words_of(chars));
    if chunks.is_empty() {
        return vec![Message::PageTextIs { page: index, words: Vec::new(), more: false }];
    }
    let last = chunks.len() - 1;
    chunks.into_iter().enumerate().map(|(at, words)| Message::PageTextIs { page: index, words, more: at < last }).collect()
}

/// The box of every character of a page's text.
fn layout_of(page: &poppler::Page) -> Vec<[f64; 4]> {
    use glib::translate::ToGlibPtr;
    let mut rectangles: *mut poppler::ffi::PopplerRectangle = std::ptr::null_mut();
    let mut count: std::os::raw::c_uint = 0;
    // SAFETY: poppler fills `rectangles` with `count` boxes it allocated with
    // g_malloc; they are copied out and freed with g_free.
    unsafe {
        let page_ptr: *mut poppler::ffi::PopplerPage = page.to_glib_none().0;
        if poppler::ffi::poppler_page_get_text_layout(page_ptr, &mut rectangles, &mut count) == 0 || rectangles.is_null() {
            return Vec::new();
        }
        let boxes = std::slice::from_raw_parts(rectangles, count as usize)
            .iter()
            .map(|rect| [rect.x1, rect.y1, rect.x2, rect.y2])
            .collect();
        glib::ffi::g_free(rectangles.cast());
        boxes
    }
}

fn search(link: &Link, queue: &Queue, document: &Document, request: u32, query: &str) {
    let pages = (document.n_pages().max(0) as u32).min(MAX_PAGES);
    let mut hits = 0u32;
    for index in 0..pages {
        if was_cancelled(queue, request) {
            return;
        }
        let Some(page) = document.page(index as i32) else { continue };
        let (_, page_h) = page.size();
        let rects: Vec<Rect> = page
            .find_text(query)
            .iter()
            // find_text answers in PDF space like the layout: flip to the top.
            .map(|found| rect_of(found.x1(), page_h - found.y2(), found.x2(), page_h - found.y1()))
            .collect();
        if rects.is_empty() {
            continue;
        }
        hits = hits.saturating_add(rects.len() as u32);
        for chunk in rects.chunks(64) {
            link.send(request, Message::SearchHit { page: index, rects: chunk.to_vec() });
        }
        if hits >= MAX_HITS {
            break;
        }
    }
    link.send(request, Message::SearchDone { hits });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str, width: f64) -> Vec<(char, [f64; 4])> {
        text.chars().enumerate().map(|(index, c)| (c, [index as f64 * width, 10.0, (index + 1) as f64 * width, 20.0])).collect()
    }

    #[test]
    fn words_are_split_at_spaces_and_carry_the_union_of_their_boxes() {
        let words = words_of(chars("hello big  world\nnext", 5.0).into_iter());
        let texts: Vec<_> = words.iter().map(|word| word.text.as_str()).collect();
        assert_eq!(texts, vec!["hello", "big", "world", "next"]);
        assert_eq!(words[0].rect, Rect { x0: 0, y0: 10_000, x1: 25_000, y1: 20_000 });
        assert_eq!(words[1].rect.x0, 30_000);
    }

    #[test]
    fn no_text_is_no_words() {
        assert!(words_of(std::iter::empty()).is_empty());
        assert!(words_of(chars("   \n ", 5.0).into_iter()).is_empty());
    }

    #[test]
    fn a_very_long_word_is_split_and_chunks_fit_a_datagram() {
        let long = "x".repeat(1200);
        let words = words_of(chars(&long, 1.0).into_iter());
        assert_eq!(words.len(), 3, "480, 480 and 240 characters");
        let many: Vec<Word> = (0..500).map(|n| Word { rect: Rect { x0: 0, y0: 0, x1: 1, y1: 1 }, text: format!("word{n}") }).collect();
        let chunks = chunk_words(many);
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            let message = Message::PageTextIs { page: 0, words: chunk.clone(), more: true };
            let bytes = bingux_preview_core::host::protocol::encode(&bingux_preview_core::host::protocol::Packet { request: 1, message });
            assert!(bytes.len() <= bingux_preview_core::host::protocol::MAX_MESSAGE);
            let back = bingux_preview_core::host::protocol::decode(&bytes).unwrap();
            let Message::PageTextIs { words, .. } = back.message else { panic!("text") };
            assert_eq!(words.len(), chunk.len(), "nothing was cut to fit");
        }
        assert_eq!(chunks.iter().map(Vec::len).sum::<usize>(), 500);
    }

    #[test]
    fn a_page_render_is_cut_to_the_limits() {
        // 612 x 792 points at 1.0 is the page itself.
        assert_eq!(pixel_size(612.0, 792.0, 1000, 0, 0), (612, 792));
        // Twice the scale, but the box is 900 wide: scale down to fit.
        assert_eq!(pixel_size(612.0, 792.0, 2000, 900, 0).0, 900);
        let (w, h) = pixel_size(612.0, 792.0, 2000, 4000, 1000);
        assert_eq!(h, 1000);
        assert!(w < 4000);
        // A poster page never exceeds the frame cap.
        let (w, h) = pixel_size(20_000.0, 20_000.0, 32_000, 0, 0);
        assert!(w <= 8192 && h <= 8192);
        assert_eq!(pixel_size(0.0, 0.0, 1000, 0, 0), (1, 1));
    }
}
