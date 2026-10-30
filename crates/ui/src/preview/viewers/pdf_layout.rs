//! The document viewer's pure parts: where each page sits at a zoom, which
//! pages to draw and render, zoom steps, the render cache, find hits and word
//! selection. No window and no host: these are tested directly.
//!
//! Pages are in points (1/72 inch). The scale is logical pixels per point; 100 %
//! is 96 dpi, so a Letter page is 816 by 1056 pixels. A page render is asked
//! for at the scale times the display scale, rounded to a step so a slow zoom
//! does not ask for a new picture on every wheel notch.

use std::collections::HashMap;
use std::ops::Range;

use bingux_preview_core::host::protocol::{Rect, Word};

/// Logical pixels between two pages, and around the stack.
pub(super) const GAP: f32 = 12.0;
pub(super) const MARGIN: f32 = 16.0;
/// Logical pixels per point at 100 %.
pub(super) const PX_PER_POINT: f32 = 96.0 / 72.0;
/// The smallest and largest zoom, as a factor of 100 %.
pub(super) const MIN_ZOOM: f32 = 0.1;
pub(super) const MAX_ZOOM: f32 = 8.0;
/// The factors a zoom step moves between.
const STEPS: [f32; 13] = [0.1, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0];
/// How far past the viewport pages are still drawn and rendered, in viewports.
const OVERSCAN: f32 = 0.75;
/// The width of a thumbnail in the sidebar, in logical pixels.
pub(super) const THUMB_WIDTH: f32 = 96.0;
/// The renders kept: pages and thumbnails together.
pub(super) const CACHE_LIMIT: usize = 14;

/// How the zoom is chosen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Zoom {
    /// The widest page fills the width.
    FitWidth,
    /// The current page fits the viewport.
    FitPage,
    /// A factor of 100 %.
    Percent(f32),
}

/// A page size in points.
pub(super) type PageSize = (f32, f32);

impl Zoom {
    /// Logical pixels per point for `sizes` in a viewport of `viewport`, with
    /// `current` the page the viewport is on.
    pub fn scale(self, sizes: &[PageSize], current: usize, viewport: (f32, f32)) -> f32 {
        let scale = match self {
            Zoom::Percent(factor) => factor.clamp(MIN_ZOOM, MAX_ZOOM) * PX_PER_POINT,
            Zoom::FitWidth => {
                let widest = sizes.iter().map(|size| size.0).fold(1.0_f32, f32::max);
                (viewport.0 - 2.0 * MARGIN) / widest
            }
            Zoom::FitPage => {
                let (width, height) = sizes.get(current).copied().unwrap_or((612.0, 792.0));
                ((viewport.0 - 2.0 * MARGIN) / width.max(1.0)).min((viewport.1 - 2.0 * MARGIN) / height.max(1.0))
            }
        };
        scale.clamp(MIN_ZOOM * PX_PER_POINT, MAX_ZOOM * PX_PER_POINT).max(0.01)
    }

    /// The zoom one step in (`1`) or out (`-1`) from the scale on screen now.
    pub fn step(scale: f32, direction: i32) -> Zoom {
        let factor = scale / PX_PER_POINT;
        let next = if direction > 0 {
            STEPS.iter().copied().find(|step| *step > factor * 1.001).unwrap_or(MAX_ZOOM)
        } else {
            STEPS.iter().rev().copied().find(|step| *step < factor / 1.001).unwrap_or(MIN_ZOOM)
        };
        Zoom::Percent(next)
    }

    /// What the zoom reads as: "Fit width", "Fit page" or "125%".
    pub fn label(self, scale: f32) -> String {
        match self {
            Zoom::FitWidth => "Fit width".to_owned(),
            Zoom::FitPage => "Fit page".to_owned(),
            Zoom::Percent(_) => format!("{}%", (scale / PX_PER_POINT * 100.0).round() as u32),
        }
    }

    /// The percentage on screen at `scale`.
    pub fn percent_at(scale: f32) -> u32 {
        (scale / PX_PER_POINT * 100.0).round() as u32
    }
}

/// Where the pages sit, top to bottom.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Layout {
    pub tops: Vec<f32>,
    pub widths: Vec<f32>,
    pub heights: Vec<f32>,
    /// The height of the whole stack, margins included.
    pub height: f32,
    /// The widest page.
    pub width: f32,
}

/// The layout of `sizes` at `scale`.
pub(super) fn layout(sizes: &[PageSize], scale: f32) -> Layout {
    let mut tops = Vec::with_capacity(sizes.len());
    let mut widths = Vec::with_capacity(sizes.len());
    let mut heights = Vec::with_capacity(sizes.len());
    let mut y = MARGIN;
    let mut widest = 0.0_f32;
    for (width, height) in sizes {
        let (w, h) = ((width * scale).round().max(1.0), (height * scale).round().max(1.0));
        tops.push(y);
        widths.push(w);
        heights.push(h);
        widest = widest.max(w);
        y += h + GAP;
    }
    Layout { tops, widths, heights, height: y - GAP + MARGIN, width: widest }
}

impl Layout {
    pub fn len(&self) -> usize {
        self.tops.len()
    }

    /// The page that contains `y`, or the nearest.
    pub fn page_at(&self, y: f32) -> usize {
        if self.tops.is_empty() {
            return 0;
        }
        match self.tops.partition_point(|top| *top <= y) {
            0 => 0,
            n => n - 1,
        }
    }

    /// The pages to draw for a viewport scrolled to `scroll` and `viewport_h`
    /// tall, with room past both ends.
    pub fn visible(&self, scroll: f32, viewport_h: f32) -> Range<usize> {
        if self.tops.is_empty() {
            return 0..0;
        }
        let slack = viewport_h * OVERSCAN;
        let first = self.page_at((scroll - slack).max(0.0));
        let last = self.page_at(scroll + viewport_h + slack);
        first..(last + 1).min(self.len())
    }

    /// The page the person is on: the one with the most of the viewport.
    pub fn current(&self, scroll: f32, viewport_h: f32) -> usize {
        if self.tops.is_empty() {
            return 0;
        }
        let (from, to) = (scroll, scroll + viewport_h);
        let mut best = (0.0_f32, 0usize);
        for page in self.page_at(scroll)..=self.page_at(to) {
            let overlap = (self.tops[page] + self.heights[page]).min(to) - self.tops[page].max(from);
            if overlap > best.0 {
                best = (overlap, page);
            }
        }
        best.1
    }

    /// The scroll offset that puts `page`'s top at the top of the viewport.
    pub fn scroll_to_page(&self, page: usize, viewport_h: f32) -> f32 {
        let top = self.tops.get(page).copied().unwrap_or(0.0);
        (top - MARGIN).clamp(0.0, self.max_scroll(viewport_h))
    }

    pub fn max_scroll(&self, viewport_h: f32) -> f32 {
        (self.height - viewport_h).max(0.0)
    }

    /// The scroll that shows a hit at `rect` (points, from the page's top) on
    /// `page`, or `None` when it is already inside the viewport. A hit is
    /// brought a third of the way down the viewport.
    pub fn scroll_to_hit(&self, page: usize, rect: Rect, scale: f32, scroll: f32, viewport_h: f32) -> Option<f32> {
        let y = self.tops.get(page)? + rect.y0 as f32 / 1000.0 * scale;
        let bottom = self.tops[page] + rect.y1 as f32 / 1000.0 * scale;
        if y >= scroll + 8.0 && bottom <= scroll + viewport_h - 8.0 {
            return None;
        }
        Some((y - viewport_h / 3.0).clamp(0.0, self.max_scroll(viewport_h)))
    }
}

/// The key a render is cached and requested under: the pixel scale, in sixteenths.
pub(super) fn render_key(scale: f32, display_scale: f64) -> u32 {
    (f64::from(scale) * display_scale * 16.0).round().max(1.0) as u32
}

/// The pixels per point a key means.
pub(super) fn key_scale(key: u32) -> f32 {
    key as f32 / 16.0
}

/// A bounded cache of renders by page and scale, least recently used out.
pub(super) struct RenderCache<T> {
    entries: HashMap<(usize, u32), (T, u64)>,
    clock: u64,
    limit: usize,
}

impl<T: Clone> RenderCache<T> {
    pub fn new(limit: usize) -> Self {
        RenderCache { entries: HashMap::new(), clock: 0, limit }
    }

    pub fn insert(&mut self, page: usize, key: u32, value: T) {
        self.clock += 1;
        self.entries.insert((page, key), (value, self.clock));
        while self.entries.len() > self.limit {
            let oldest = self.entries.iter().min_by_key(|(_, (_, used))| *used).map(|(key, _)| *key);
            match oldest {
                Some(oldest) => {
                    self.entries.remove(&oldest);
                }
                None => break,
            }
        }
    }

    pub fn has(&self, page: usize, key: u32) -> bool {
        self.entries.contains_key(&(page, key))
    }

    /// The render of `page` at `key` if there is one, else the nearest scale
    /// of the same page: shown stretched until the right one arrives, so a
    /// zoom never flashes a blank page.
    pub fn best(&mut self, page: usize, key: u32) -> Option<(T, u32)> {
        let found = if self.has(page, key) {
            key
        } else {
            self.entries.keys().filter(|(p, _)| *p == page).map(|(_, k)| *k).min_by_key(|k| k.abs_diff(key))?
        };
        self.clock += 1;
        let clock = self.clock;
        let entry = self.entries.get_mut(&(page, found))?;
        entry.1 = clock;
        Some((entry.0.clone(), found))
    }
}

/// A word of a page: its page, and its place in that page's reading order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct WordRef {
    pub page: usize,
    pub index: usize,
}

/// The word at (`x`, `y`) points on a page, or the nearest within a few points.
pub(super) fn word_at(words: &[Word], x: f32, y: f32) -> Option<usize> {
    let (x, y) = ((x * 1000.0) as i64, (y * 1000.0) as i64);
    let slack = 4_000;
    let mut best: Option<(i64, usize)> = None;
    for (index, word) in words.iter().enumerate() {
        let rect = word.rect;
        let dx = (i64::from(rect.x0) - x).max(x - i64::from(rect.x1)).max(0);
        let dy = (i64::from(rect.y0) - y).max(y - i64::from(rect.y1)).max(0);
        if dx > slack || dy > slack {
            continue;
        }
        let distance = dx * dx + dy * dy;
        if best.is_none_or(|(nearest, _)| distance < nearest) {
            best = Some((distance, index));
        }
    }
    best.map(|(_, index)| index)
}

/// The words from `a` to `b` in reading order, either way round, as a list of
/// (page, first, last) runs that covers every selected word.
pub(super) fn selected_runs(a: WordRef, b: WordRef, counts: impl Fn(usize) -> usize) -> Vec<(usize, usize, usize)> {
    let (from, to) = if a <= b { (a, b) } else { (b, a) };
    let mut runs = Vec::new();
    for page in from.page..=to.page {
        let count = counts(page);
        if count == 0 {
            continue;
        }
        let first = if page == from.page { from.index } else { 0 };
        let last = if page == to.page { to.index } else { count - 1 };
        if first <= last && first < count {
            runs.push((page, first, last.min(count - 1)));
        }
    }
    runs
}

/// The text of `words` for the clipboard: a space between words on a line, a
/// line break where the next word starts a new line.
pub(super) fn text_of(words: &[&Word]) -> String {
    let mut out = String::new();
    let mut previous: Option<&Word> = None;
    for word in words {
        if let Some(before) = previous {
            let height = (before.rect.y1 - before.rect.y0).max(1);
            let new_line = (word.rect.y0 - before.rect.y0).abs() > height / 2 || word.rect.x0 < before.rect.x0 - height;
            out.push(if new_line { '\n' } else { ' ' });
        }
        out.push_str(&word.text);
        previous = Some(word);
    }
    out
}

/// Where a find hit is among all hits: which to show after `current`.
pub(super) fn next_hit(current: Option<usize>, count: usize, direction: i32) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(match current {
        None if direction >= 0 => 0,
        None => count - 1,
        Some(at) if direction >= 0 => (at + 1) % count,
        Some(at) => (at + count - 1) % count,
    })
}

/// "3 of 12" for a find, or "No matches" / "Searching…".
pub(super) fn find_status(current: Option<usize>, count: usize, searching: bool, query: &str) -> String {
    if query.is_empty() {
        String::new()
    } else if count == 0 {
        if searching { "Searching\u{2026}".to_owned() } else { "No matches".to_owned() }
    } else {
        format!("{} of {}{}", current.map_or(1, |at| at + 1), count, if searching { "+" } else { "" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LETTER: PageSize = (612.0, 792.0);

    fn pages(count: usize) -> Vec<PageSize> {
        vec![LETTER; count]
    }

    fn rect(x0: i32, y0: i32, x1: i32, y1: i32) -> Rect {
        Rect { x0, y0, x1, y1 }
    }

    fn word(text: &str, x0: i32, y0: i32, x1: i32, y1: i32) -> Word {
        Word { rect: rect(x0, y0, x1, y1), text: text.to_owned() }
    }

    #[test]
    fn a_hundred_percent_page_is_96_dpi() {
        let scale = Zoom::Percent(1.0).scale(&pages(1), 0, (800.0, 600.0));
        assert!((scale - 4.0 / 3.0).abs() < 1e-6);
        let layout = layout(&pages(1), scale);
        assert_eq!((layout.widths[0], layout.heights[0]), (816.0, 1056.0));
    }

    #[test]
    fn fit_width_fills_the_width_of_the_widest_page_and_fit_page_shows_one_whole() {
        let sizes = vec![LETTER, (792.0, 612.0)];
        let width = Zoom::FitWidth.scale(&sizes, 0, (900.0, 700.0));
        assert!((792.0 * width - (900.0 - 2.0 * MARGIN)).abs() < 0.01, "the widest page fills the width");
        let page = Zoom::FitPage.scale(&sizes, 0, (900.0, 700.0));
        assert!(792.0 * page <= 700.0 - 2.0 * MARGIN + 0.01 && 612.0 * page <= 900.0 - 2.0 * MARGIN + 0.01);
        // The landscape page is limited by the width instead.
        let landscape = Zoom::FitPage.scale(&sizes, 1, (900.0, 700.0));
        assert!(landscape > page);
    }

    #[test]
    fn zoom_steps_walk_the_ladder_from_wherever_the_scale_is() {
        let at = |factor: f32| factor * PX_PER_POINT;
        assert_eq!(Zoom::step(at(1.0), 1), Zoom::Percent(1.25));
        assert_eq!(Zoom::step(at(1.0), -1), Zoom::Percent(0.75));
        // From a fit scale of 87 % in goes to 100 and out to 75.
        assert_eq!(Zoom::step(at(0.87), 1), Zoom::Percent(1.0));
        assert_eq!(Zoom::step(at(0.87), -1), Zoom::Percent(0.75));
        assert_eq!(Zoom::step(at(8.0), 1), Zoom::Percent(MAX_ZOOM));
        assert_eq!(Zoom::step(at(0.1), -1), Zoom::Percent(MIN_ZOOM));
    }

    #[test]
    fn labels_name_the_mode_or_the_percentage() {
        assert_eq!(Zoom::FitWidth.label(1.0), "Fit width");
        assert_eq!(Zoom::FitPage.label(1.0), "Fit page");
        assert_eq!(Zoom::Percent(1.25).label(1.25 * PX_PER_POINT), "125%");
        assert_eq!(Zoom::percent_at(PX_PER_POINT * 0.5), 50);
    }

    #[test]
    fn pages_stack_with_gaps_and_the_stack_has_a_height() {
        let layout = layout(&pages(3), 1.0);
        assert_eq!(layout.tops, vec![MARGIN, MARGIN + 792.0 + GAP, MARGIN + 2.0 * (792.0 + GAP)]);
        assert_eq!(layout.height, MARGIN + 3.0 * 792.0 + 2.0 * GAP + MARGIN);
        assert_eq!(layout.width, 612.0);
        assert_eq!(layout.page_at(0.0), 0);
        assert_eq!(layout.page_at(layout.tops[1]), 1);
        assert_eq!(layout.page_at(layout.tops[1] - 1.0), 0);
        assert_eq!(layout.page_at(1e9), 2);
    }

    #[test]
    fn only_the_pages_near_the_viewport_are_visible() {
        let layout = layout(&pages(100), 1.0);
        let shown = layout.visible(0.0, 800.0);
        assert_eq!(shown.start, 0);
        assert!(shown.end >= 2 && shown.end <= 3, "{shown:?}");
        let middle = layout.visible(layout.tops[50], 800.0);
        assert!(middle.contains(&50) && middle.len() <= 4, "{middle:?}");
        assert!(layout.visible(0.0, 800.0).len() < 100);
        assert_eq!(super::layout(&[], 1.0).visible(0.0, 800.0), 0..0);
    }

    #[test]
    fn the_current_page_is_the_one_with_most_of_the_viewport() {
        let layout = layout(&pages(5), 1.0);
        assert_eq!(layout.current(0.0, 800.0), 0);
        // Scrolled so page 2 fills the viewport and a sliver of page 1 shows.
        let scroll = layout.tops[2] - 40.0;
        assert_eq!(layout.current(scroll, 800.0), 2);
        assert_eq!(layout.current(layout.tops[4], 800.0), 4);
    }

    #[test]
    fn jumping_to_a_page_scrolls_its_top_to_the_top_and_stays_in_range() {
        let layout = layout(&pages(5), 1.0);
        assert_eq!(layout.scroll_to_page(0, 800.0), 0.0);
        assert_eq!(layout.scroll_to_page(2, 800.0), layout.tops[2] - MARGIN);
        // The last page cannot go past the end.
        assert_eq!(layout.scroll_to_page(4, 800.0), layout.tops[4] - MARGIN);
        assert!(layout.scroll_to_page(4, 800.0) <= layout.max_scroll(800.0));
        assert_eq!(layout.scroll_to_page(99, 800.0), 0.0, "a page that is not there stays at the top");
    }

    #[test]
    fn a_find_hit_scrolls_into_view_only_when_it_is_not_already() {
        let layout = layout(&pages(3), 1.0);
        let hit = rect(72_000, 100_000, 140_000, 120_000);
        assert_eq!(layout.scroll_to_hit(0, hit, 1.0, 0.0, 800.0), None, "already visible");
        let on_page_two = layout.scroll_to_hit(1, hit, 1.0, 0.0, 800.0).expect("scrolls");
        assert!((on_page_two - (layout.tops[1] + 100.0 - 800.0 / 3.0)).abs() < 0.01);
        assert_eq!(layout.scroll_to_hit(9, hit, 1.0, 0.0, 800.0), None);
    }

    #[test]
    fn render_keys_quantise_the_scale_and_the_cache_forgets_the_oldest() {
        assert_eq!(render_key(1.0, 1.0), 16);
        assert_eq!(render_key(1.0, 2.0), 32);
        assert_eq!(render_key(1.01, 1.0), 16, "a hair of zoom is the same render");
        assert_eq!(key_scale(32), 2.0);
        let mut cache = RenderCache::new(2);
        cache.insert(0, 16, "a");
        cache.insert(1, 16, "b");
        assert_eq!(cache.best(0, 16), Some(("a", 16)), "a is now the freshest");
        cache.insert(2, 16, "c");
        assert!(cache.has(0, 16) && cache.has(2, 16) && !cache.has(1, 16));
    }

    #[test]
    fn the_nearest_scale_of_a_page_stands_in_until_the_right_one_comes() {
        let mut cache = RenderCache::new(8);
        cache.insert(3, 16, "small");
        cache.insert(3, 48, "big");
        assert_eq!(cache.best(3, 20), Some(("small", 16)));
        assert_eq!(cache.best(3, 40), Some(("big", 48)));
        assert_eq!(cache.best(3, 48), Some(("big", 48)));
        assert_eq!(cache.best(4, 16), None);
    }

    #[test]
    fn a_point_picks_the_word_under_it_or_the_nearest_within_a_few_points() {
        let words = vec![word("one", 10_000, 10_000, 40_000, 22_000), word("two", 50_000, 10_000, 80_000, 22_000)];
        assert_eq!(word_at(&words, 20.0, 15.0), Some(0));
        assert_eq!(word_at(&words, 60.0, 15.0), Some(1));
        assert_eq!(word_at(&words, 44.0, 15.0), Some(0), "in the gap, the nearer");
        assert_eq!(word_at(&words, 300.0, 300.0), None);
    }

    #[test]
    fn a_selection_covers_whole_pages_between_its_ends_in_either_direction() {
        let counts = |page: usize| [5usize, 0, 4, 6][page];
        let a = WordRef { page: 0, index: 3 };
        let b = WordRef { page: 3, index: 1 };
        let forward = selected_runs(a, b, counts);
        assert_eq!(forward, vec![(0, 3, 4), (2, 0, 3), (3, 0, 1)], "the empty page has no run");
        assert_eq!(selected_runs(b, a, counts), forward);
        assert_eq!(selected_runs(a, a, counts), vec![(0, 3, 3)]);
    }

    #[test]
    fn copied_text_breaks_lines_where_the_words_do() {
        let words = vec![
            word("Invoice", 72_000, 50_000, 140_000, 70_000),
            word("105", 150_000, 50_000, 180_000, 70_000),
            word("Total", 72_000, 400_000, 120_000, 420_000),
            word("due", 125_000, 400_000, 160_000, 420_000),
        ];
        let refs: Vec<&Word> = words.iter().collect();
        assert_eq!(text_of(&refs), "Invoice 105\nTotal due");
        assert_eq!(text_of(&[]), "");
    }

    #[test]
    fn next_and_previous_hit_wrap_around() {
        assert_eq!(next_hit(None, 3, 1), Some(0));
        assert_eq!(next_hit(None, 3, -1), Some(2));
        assert_eq!(next_hit(Some(2), 3, 1), Some(0));
        assert_eq!(next_hit(Some(0), 3, -1), Some(2));
        assert_eq!(next_hit(Some(1), 3, 1), Some(2));
        assert_eq!(next_hit(None, 0, 1), None);
    }

    #[test]
    fn the_find_status_counts_or_says_why_not() {
        assert_eq!(find_status(Some(2), 12, false, "inv"), "3 of 12");
        assert_eq!(find_status(Some(0), 4, true, "inv"), "1 of 4+");
        assert_eq!(find_status(None, 0, false, "zzz"), "No matches");
        assert_eq!(find_status(None, 0, true, "zzz"), "Searching\u{2026}");
        assert_eq!(find_status(None, 0, false, ""), "");
    }
}
