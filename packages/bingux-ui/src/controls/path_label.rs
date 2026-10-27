//! A single line of text that does not fit is cut in the middle, so the start
//! and the end both stay readable, as the macOS Finder does:
//! `~/Documents/…/Reports/final.pdf`.
//!
//! [`path_label`] is the control. [`elide_path`], [`elide_name`] and
//! [`elide_middle`] are the pure functions under it: they take a width and a
//! `measure` function and return the text to draw. [`measure_text`] measures a
//! string with the font a label in a [`TextStyle`] renders with, so the
//! decision agrees with the pixels. A breadcrumb or a list row can use the
//! functions directly when it lays text out itself.
//!
//! The rules for a path, in order:
//!
//! 1. A path that fits is shown whole.
//! 2. Whole segments from the middle are replaced by one `…` segment. The
//!    first and the last segment stay, then the segments nearest the end and
//!    the start in turn, as many as fit.
//! 3. When even `first/…/last` does not fit, the last segment is shortened in
//!    the middle and keeps its extension (`final-rep…2026.pdf`).
//! 4. Only then is the first segment shortened too.
//!
//! The text is never empty: the least that is returned is `…`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::LazyLock;

use freya::engine::prelude::{
    FontCollection, FontMgr, FontStyle, ParagraphBuilder, ParagraphStyle, Slant,
    TextStyle as SkTextStyle, Width,
};
use freya::prelude::*;
use freya_core::current_context::CurrentContext;

use super::press::{Press, use_press};
use super::tooltip::hover_tip;
use crate::component::ComponentName;
use crate::shell::{MenuItem, Overlays, menu_item};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, Ink, TextStyle, mono_family};

const ELLIPSIS: &str = "…";

/// The longest extension, dot included, that `elide_name` keeps whole.
const MAX_EXTENSION: usize = 12;

// ---------------------------------------------------------------------------
// Elision
// ---------------------------------------------------------------------------

/// The text to draw for `path` in at most `max_width`, as measured by
/// `measure` (the width of a string in px).
///
/// A path that fits is returned unchanged. Otherwise whole middle segments
/// give way to a `…` segment, then the last segment is shortened, then the
/// first (see the module docs). `scheme://host` counts as one first segment,
/// `/` and `~` as roots. A trailing slash stays. An empty path gives an empty
/// string; anything else gives at least `…`.
///
/// A wider `max_width` never keeps less of the path. It measures
/// `O(log segments)` candidate strings, not one per segment.
pub fn elide_path(path: &str, max_width: f32, measure: &dyn Fn(&str) -> f32) -> String {
    if path.is_empty() || measure(path) <= max_width {
        return path.to_owned();
    }
    let parts = Parts::of(path);
    let count = parts.items.len();
    let out = match count {
        0 => path.to_owned(),
        1 => {
            let room = max_width - measure(parts.lead) - measure(parts.trail);
            let name = elide_name(parts.items[0], room, measure);
            compose(&parts, &[name.as_str()], false, &[])
        }
        _ if count >= 3 && fits(&parts, 2, max_width, measure) => {
            // The most segments kept, found by halving: every step keeps one
            // more segment, so the width only grows.
            let (mut low, mut high) = (2, count - 1);
            while low < high {
                let mid = (low + high).div_ceil(2);
                if fits(&parts, mid, max_width, measure) {
                    low = mid;
                } else {
                    high = mid - 1;
                }
            }
            keep(&parts, low)
        }
        _ => shorten_ends(&parts, max_width, measure),
    };
    // Nothing that is left fits: the least there is, not a sliver of the path.
    if out != ELLIPSIS && measure(&out) > max_width {
        return ELLIPSIS.to_owned();
    }
    out
}

/// The text to draw for a file or folder `name` in at most `max_width`.
/// A name that does not fit loses its middle and keeps its extension:
/// `final-rep…2026.pdf`. `.tar.gz` and any `.tar.*` count as one extension.
/// When even one letter and the extension do not fit, the extension goes
/// too. Never empty: the least is `…`.
pub fn elide_name(name: &str, max_width: f32, measure: &dyn Fn(&str) -> f32) -> String {
    if name.is_empty() || measure(name) <= max_width {
        return name.to_owned();
    }
    let ext_at = extension_start(name);
    if ext_at < name.len() {
        let stem_chars = name[..ext_at].chars().count();
        let shape = Shape::Extension(ext_at);
        if stem_chars >= 2 && measure(&shape.cut(name, 1)) <= max_width {
            return largest_fit(name, shape, stem_chars - 1, max_width, measure);
        }
    }
    let chars = name.chars().count();
    largest_fit(name, Shape::Plain, chars.saturating_sub(1), max_width, measure)
}

/// The text to draw for any `text` in at most `max_width`, cut in the middle
/// with no special care for extensions. For labels that are not file names.
pub fn elide_middle(text: &str, max_width: f32, measure: &dyn Fn(&str) -> f32) -> String {
    if text.is_empty() || measure(text) <= max_width {
        return text.to_owned();
    }
    let chars = text.chars().count();
    largest_fit(text, Shape::Plain, chars.saturating_sub(1), max_width, measure)
}

/// A path split into the pieces the rules work on.
struct Parts<'a> {
    /// `/` for an absolute path, else empty.
    lead: &'static str,
    /// The segments, first to last. The first may be `scheme://host`.
    items: Vec<&'a str>,
    /// `/` when the path ends in one.
    trail: &'static str,
}

impl<'a> Parts<'a> {
    fn of(path: &'a str) -> Self {
        let (lead, items) = match scheme_end(path) {
            Some(end) => {
                let mut items = vec![&path[..end]];
                items.extend(path[end..].split('/').filter(|part| !part.is_empty()));
                ("", items)
            }
            None => (
                if path.starts_with('/') { "/" } else { "" },
                path.split('/').filter(|part| !part.is_empty()).collect(),
            ),
        };
        let trail = if path.ends_with('/') && !items.is_empty() {
            "/"
        } else {
            ""
        };
        Self { lead, items, trail }
    }
}

/// Where the first segment of `scheme://host/..` ends, when the path has a
/// scheme: the byte index of the `/` after the host, or the path's end.
fn scheme_end(path: &str) -> Option<usize> {
    let marker = path.find("://")?;
    let scheme = &path[..marker];
    let valid = !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !valid {
        return None;
    }
    let host = marker + 3;
    Some(path[host..].find('/').map_or(path.len(), |at| host + at))
}

/// Joins the kept segments with `/`, with `…` between `head` and `tail` when
/// `gap` is set, inside the path's lead and trail.
fn compose(parts: &Parts, head: &[&str], gap: bool, tail: &[&str]) -> String {
    let pieces = head
        .iter()
        .copied()
        .chain(gap.then_some(ELLIPSIS))
        .chain(tail.iter().copied());
    let mut out = String::with_capacity(
        parts.lead.len() + parts.trail.len() + parts.items.iter().map(|s| s.len() + 1).sum::<usize>(),
    );
    out.push_str(parts.lead);
    for (index, piece) in pieces.enumerate() {
        if index > 0 {
            out.push('/');
        }
        out.push_str(piece);
    }
    out.push_str(parts.trail);
    out
}

/// The path with `kept` whole segments and one `…`: the end gets the extra
/// segment when `kept` is odd, so the nearest folders to the name stay.
fn keep(parts: &Parts, kept: usize) -> String {
    let head = kept / 2;
    let tail = kept - head;
    let count = parts.items.len();
    compose(parts, &parts.items[..head], true, &parts.items[count - tail..])
}

fn fits(parts: &Parts, kept: usize, max_width: f32, measure: &dyn Fn(&str) -> f32) -> bool {
    measure(&keep(parts, kept)) <= max_width
}

/// `first/…/last` is too wide: shortens the last segment, and the first
/// only when the last is already as short as it goes.
fn shorten_ends(parts: &Parts, max_width: f32, measure: &dyn Fn(&str) -> f32) -> String {
    let count = parts.items.len();
    let (head, last) = (parts.items[0], parts.items[count - 1]);
    let gap = count >= 3;
    let separator = if gap { "/…/" } else { "/" };
    let room = max_width - measure(parts.lead) - measure(separator) - measure(parts.trail);

    let head_whole = measure(head);
    let head_least = least_width(head, measure);
    let last_least = least_width(last, measure);
    if room < head_least + last_least {
        // Not both: the last segment alone, which is the part that names the
        // place. This is never wider than the least form that follows it.
        return elide_name(last, max_width - measure(parts.trail), measure) + parts.trail;
    }
    // The first segment keeps its width while the last can still be shown in
    // what is left; below that the first gives up room, down to its least.
    let head_room = if room >= head_whole + last_least {
        head_whole
    } else {
        room - last_least
    };
    let head = elide_name(head, head_room, measure);
    let last = elide_name(last, room - head_room, measure);
    compose(parts, &[head.as_str()], gap, &[last.as_str()])
}

/// The width of `name` cut as short as `elide_name` goes before it gives up
/// its extension: one letter, `…` and the extension.
fn least_width(name: &str, measure: &dyn Fn(&str) -> f32) -> f32 {
    let ext_at = extension_start(name);
    let stem_chars = name[..ext_at].chars().count();
    if stem_chars < 2 {
        return measure(name);
    }
    let shape = if ext_at < name.len() {
        Shape::Extension(ext_at)
    } else {
        Shape::Plain
    };
    measure(&shape.cut(name, 1)).min(measure(name))
}

/// How a name loses its middle.
#[derive(Clone, Copy)]
enum Shape {
    /// Letters from both ends, half each.
    Plain,
    /// The stem loses its middle and the extension, from this byte index,
    /// stays whole. The end of the stem gets a third of what is kept.
    Extension(usize),
}

impl Shape {
    /// `name` with `keep` stem characters and a `…` where the rest was.
    fn cut(self, name: &str, keep: usize) -> String {
        let (stem, extension, tail) = match self {
            Shape::Plain => (name, "", keep / 2),
            Shape::Extension(at) => (&name[..at], &name[at..], keep / 3),
        };
        let head = keep - tail;
        let head_end = head_end(stem, head);
        let tail_start = tail_start(stem, tail).max(head_end);
        let mut out = String::with_capacity(head_end + (stem.len() - tail_start) + extension.len() + 3);
        out.push_str(&stem[..head_end]);
        out.push_str(ELLIPSIS);
        out.push_str(&stem[tail_start..]);
        out.push_str(extension);
        out
    }
}

/// The widest cut of `name` that fits, trying `0..=most` kept characters.
/// Keeping nothing gives `…` alone, which is returned even when it does not
/// fit.
fn largest_fit(
    name: &str,
    shape: Shape,
    most: usize,
    max_width: f32,
    measure: &dyn Fn(&str) -> f32,
) -> String {
    let (mut low, mut high) = (0, most);
    while low < high {
        let mid = (low + high).div_ceil(2);
        if measure(&shape.cut(name, mid)) <= max_width {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    shape.cut(name, low)
}

/// The byte index where the extension of `name` starts, or `name.len()` when
/// it has none. A leading dot (`.bashrc`) is not an extension. A long or
/// spaced tail is a sentence, not an extension.
fn extension_start(name: &str) -> usize {
    let Some(dot) = name.rfind('.') else {
        return name.len();
    };
    let extension = &name[dot..];
    if dot == 0
        || extension.len() == 1
        || extension.chars().count() > MAX_EXTENSION
        || extension.chars().any(char::is_whitespace)
    {
        return name.len();
    }
    match name[..dot].rfind('.') {
        Some(previous) if previous > 0 && name[previous + 1..dot].eq_ignore_ascii_case("tar") => {
            previous
        }
        _ => dot,
    }
}

/// Whether `c` joins the character before it, so a cut must not come between
/// them: combining marks, variation selectors, skin tones and the joiner.
fn joins_previous(c: char) -> bool {
    matches!(
        c,
        '\u{0300}'..='\u{036F}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{200D}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FE20}'..='\u{FE2F}'
            | '\u{1F3FB}'..='\u{1F3FF}'
            | '\u{E0100}'..='\u{E01EF}'
    )
}

/// The byte index after the first `count` characters of `text`, moved on
/// past anything that belongs to the last of them.
fn head_end(text: &str, count: usize) -> usize {
    let mut end = 0;
    let mut taken = 0;
    let mut previous = None;
    for (index, c) in text.char_indices() {
        let attached = joins_previous(c) || previous == Some('\u{200D}');
        if taken >= count && !attached {
            break;
        }
        if !attached {
            taken += 1;
        }
        end = index + c.len_utf8();
        previous = Some(c);
    }
    end
}

/// The byte index where the last `count` characters of `text` start, moved on
/// past anything that would hang on the cut-off character before it.
fn tail_start(text: &str, count: usize) -> usize {
    if count == 0 {
        return text.len();
    }
    let start = text
        .char_indices()
        .rev()
        .nth(count - 1)
        .map_or(0, |(index, _)| index);
    let rest = &text[start..];
    let skipped: usize = rest
        .chars()
        .take_while(|c| joins_previous(*c))
        .map(char::len_utf8)
        .sum();
    start + skipped
}

// ---------------------------------------------------------------------------
// Measuring
// ---------------------------------------------------------------------------

/// What makes two styles measure differently.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct StyleKey {
    size: u32,
    weight: i32,
    tracking: u32,
    tabular: bool,
    mono: bool,
}

impl StyleKey {
    fn of(style: TextStyle, size: f32) -> Self {
        Self {
            size: size.to_bits(),
            weight: i32::from(style.weight),
            tracking: style.tracking.to_bits(),
            tabular: style.tabular,
            mono: style.mono,
        }
    }
}

/// The font size of `style` at `scale`: what `TextStyle::label_scaled` draws,
/// a quarter pixel step. At scale 1 it is the style's own size.
fn scaled_size(style: TextStyle, scale: f32) -> f32 {
    if scale == 1.0 {
        style.size.px()
    } else {
        (style.size.px() * scale * 4.0).round() / 4.0
    }
}

/// A style ready to lay out, and the widths measured with it.
struct Measured {
    paragraph: ParagraphStyle,
    text: SkTextStyle,
    widths: HashMap<Box<str>, f32>,
}

/// How many strings one style remembers before it starts again. A page of
/// rows repeats a few hundred segments; this is generous.
const MEMORY: usize = 8192;

thread_local! {
    static MEASURED: RefCell<HashMap<StyleKey, Measured>> = RefCell::new(HashMap::new());
    /// The system fonts, for measuring outside a running app.
    static SYSTEM_FONTS: FontCollection = {
        let mut fonts = FontCollection::new();
        fonts.set_default_font_manager(FontMgr::default(), None);
        fonts
    };
}

/// The families a label falls back through: the interface font first, as the
/// app registers it as the default.
static FAMILIES: LazyLock<[Vec<String>; 2]> = LazyLock::new(|| {
    let rest = || freya_core::style::default_fonts::default_fonts().into_iter().map(|font| font.to_string());
    [
        std::iter::once(crate::theme::font_family().to_owned())
            .chain(rest())
            .collect(),
        std::iter::once(mono_family().to_owned()).chain(rest()).collect(),
    ]
});

/// The width in px of `text` on one line in `style`: the width the same text
/// has in a `style.label(..)`, laid out by the same text engine with the same
/// fonts. Call it while a component renders so it finds the app's fonts; with
/// no app running it uses the system's.
///
/// Results are remembered per style, so measuring a hundred rows on every
/// frame costs hash lookups.
pub(crate) fn measure_text(text: &str, style: TextStyle) -> f32 {
    measure_text_scaled(text, style, 1.0)
}

/// [`measure_text`] for text drawn with `TextStyle::label_scaled(.., scale)`:
/// the font size is scaled and rounded to a quarter pixel, as the label does.
pub(crate) fn measure_text_scaled(text: &str, style: TextStyle, scale: f32) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let size = scaled_size(style, scale);
    let key = StyleKey::of(style, size);
    if let Some(width) = MEASURED.with_borrow(|all| {
        all.get(&key)
            .and_then(|measured| measured.widths.get(text).copied())
    }) {
        return width;
    }
    let app_fonts = CurrentContext::try_with(|_| try_consume_root_context::<FontCollection>()).flatten();
    let remember = app_fonts.is_some();
    let fonts = app_fonts.unwrap_or_else(|| SYSTEM_FONTS.with(Clone::clone));
    MEASURED.with_borrow_mut(|all| {
        let measured = all.entry(key).or_insert_with(|| measured_style(style, size));
        let mut builder = ParagraphBuilder::new(&measured.paragraph, fonts);
        builder.push_style(&measured.text);
        builder.add_text(text);
        let mut paragraph = builder.build();
        paragraph.layout(f32::MAX);
        let width = paragraph.longest_line();
        if remember {
            if measured.widths.len() >= MEMORY {
                measured.widths.clear();
            }
            measured.widths.insert(text.into(), width);
        }
        width
    })
}

/// The text style a `style.label(..)` has, in the engine's terms: what
/// Freya's label builds for its paragraph, minus what does not change width.
fn measured_style(style: TextStyle, size: f32) -> Measured {
    let mut text = SkTextStyle::new();
    text.set_font_size(size);
    text.set_font_families(&FAMILIES[usize::from(style.mono)]);
    text.set_font_style(FontStyle::new(style.weight.into(), Width::NORMAL, Slant::Upright));
    text.set_letter_spacing(style.tracking);
    if style.tabular {
        text.add_font_feature("tnum", 1);
    }
    let mut paragraph = ParagraphStyle::new();
    paragraph.set_text_style(&text);
    Measured {
        paragraph,
        text,
        widths: HashMap::new(),
    }
}

// ---------------------------------------------------------------------------
// The control
// ---------------------------------------------------------------------------

/// Which cut a [`PathLabel`] makes.
#[derive(Clone, Copy, PartialEq)]
enum Cut {
    Path,
    Name,
    Middle,
}

/// A path, a file name or other text on one line that is cut in the middle
/// to fit the width it is given. Build it with [`path_label`].
///
/// It fills the width of its parent and is one line tall, so a cut never
/// changes the layout around it. Put it in a box that has a width of its own
/// (a row cell, a column), not in one that is as wide as its content.
#[derive(Clone, PartialEq)]
pub struct PathLabel {
    text: String,
    style: TextStyle,
    scale: f32,
    cut: Cut,
    selectable: bool,
    copyable: bool,
    centre: bool,
    tooltip: bool,
}

/// A path on one line, ellipsed in the middle when it does not fit, as in
/// `~/Documents/…/Reports/final.pdf`. The full path is the tooltip while it
/// is cut.
///
/// ```ignore
/// path_label("/home/kieran/Documents/Reports/final.pdf")
///     .style(CAPTION)
///     .copyable(true)
/// ```
pub fn path_label(path: impl Into<String>) -> PathLabel {
    PathLabel {
        text: path.into(),
        style: BODY,
        scale: 1.0,
        cut: Cut::Path,
        selectable: false,
        copyable: false,
        centre: false,
        tooltip: true,
    }
}

impl PathLabel {
    /// The text style. The default is [`BODY`].
    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    /// The colour of the text.
    pub fn ink(mut self, ink: Ink) -> Self {
        self.style = self.style.in_ink(ink);
        self
    }

    /// The text is a file or folder name, not a path: only the name is cut,
    /// keeping its extension (`final-rep…2026.pdf`).
    pub fn name(mut self) -> Self {
        self.cut = Cut::Name;
        self
    }

    /// The text is neither: cut in the middle with no special care.
    pub fn middle(mut self) -> Self {
        self.cut = Cut::Middle;
        self
    }

    /// Lets the pointer select the text and Ctrl+C copy it, for a properties
    /// dialog. The selection is of what is shown, so it is the cut text while
    /// the text is cut; pair it with [`copyable`](Self::copyable) for the
    /// whole. It is not a Tab stop. A list row leaves this off, because a
    /// selection would take the press that opens the row.
    pub fn selectable(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }

    /// A right click, or a long press on a touch screen, offers to copy the
    /// whole text, even while it is cut.
    pub fn copyable(mut self, copyable: bool) -> Self {
        self.copyable = copyable;
        self
    }

    /// Draws the text at `scale` times its size, rounded to a quarter pixel,
    /// as `TextStyle::label_scaled` does, and cuts it by the scaled width.
    /// A view that zooms its text uses it. The line height scales too.
    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    /// Centres the text in its width, as a grid caption is. Text that is cut
    /// fills the width anyway.
    pub fn centre(mut self) -> Self {
        self.centre = true;
        self
    }

    /// Whether the full text shows as a hover tooltip while it is cut. On by
    /// default; turn it off where the surroundings already show the detail.
    pub fn tooltip(mut self, tooltip: bool) -> Self {
        self.tooltip = tooltip;
        self
    }

    /// The text to draw in `width` px.
    fn elided(&self, width: f32) -> String {
        let (style, scale) = (self.style, self.scale);
        let measure = move |text: &str| measure_text_scaled(text, style, scale);
        match self.cut {
            Cut::Path => elide_path(&self.text, width, &measure),
            Cut::Name => elide_name(&self.text, width, &measure),
            Cut::Middle => elide_middle(&self.text, width, &measure),
        }
    }
}

/// The last cut a label made, so a render with the same text, width and
/// scale does not cut again.
struct Memo {
    text: String,
    width: f32,
    scale: f32,
    cut: Cut,
    style: TextStyle,
    out: String,
}

impl Component for PathLabel {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut width = use_state(|| None::<f32>);
        let memo = use_hook(|| Rc::new(RefCell::new(None::<Memo>)));
        // Before the first size is known the whole text is drawn on one line
        // and clipped, so the first frame is never empty. Each size event
        // after that sets the width, and the next frame draws the cut for it.
        let shown = match width() {
            Some(room) => {
                let mut memo = memo.borrow_mut();
                match memo.as_ref() {
                    Some(last)
                        if last.width == room
                            && last.scale == self.scale
                            && last.cut == self.cut
                            && last.style == self.style
                            && last.text == self.text =>
                    {
                        last.out.clone()
                    }
                    _ => {
                        let out = self.elided(room);
                        *memo = Some(Memo {
                            text: self.text.clone(),
                            width: room,
                            scale: self.scale,
                            cut: self.cut,
                            style: self.style,
                            out: out.clone(),
                        });
                        out
                    }
                }
            }
            None => self.text.clone(),
        };
        let cut = shown != self.text;

        let text: Element = if self.selectable {
            SelectableText::new()
                .span(self.style.span(&tokens, shown))
                .max_lines(1)
                .line_height(self.style.leading)
                .a11y_focusable(false)
                .into()
        } else {
            self.style
                .label_scaled(&tokens, shown, self.scale)
                .max_lines(1)
                .into()
        };
        // The tooltip is the whole text, and only while it is cut.
        let text = if cut && self.tooltip {
            hover_tip(text, self.text.clone())
        } else {
            text
        };

        // One line of the style, so the row does not change height with the
        // text. The label's own height can differ by a fraction of a pixel.
        let line = (scaled_size(self.style, self.scale) * self.style.leading).ceil();
        let body = rect()
            .component("PathLabel")
            .a11y_role(AccessibilityRole::Label)
            .a11y_alt(self.text.clone())
            .width(Size::fill())
            .height(Size::px(line))
            .tw(&tokens, &[clip()])
            .maybe(self.centre && width().is_some(), |element| {
                element.tw(&tokens, &[flex_col(), items(Align::Center)])
            })
            .on_sized(move |event: Event<SizedEventData>| {
                width.set_if_modified(Some(event.area.width()));
            })
            .child(text);
        if self.copyable {
            Element::from(CopyMenu {
                child: body.into(),
                text: self.text.clone(),
                label: match self.cut {
                    Cut::Path => "Copy path",
                    Cut::Name => "Copy name",
                    Cut::Middle => "Copy",
                },
            })
        } else {
            body.into()
        }
    }
}

/// Wraps a label with the right click and long press that offer to copy its
/// whole text. Only labels that ask for it carry the press state.
#[derive(Clone, PartialEq)]
struct CopyMenu {
    child: Element,
    text: String,
    label: &'static str,
}

impl Component for CopyMenu {
    fn render(&self) -> impl IntoElement {
        let overlays = try_consume_context::<Overlays>();
        let press = use_press();
        let items = overlays.map(|overlays| {
            let text = self.text.clone();
            vec![menu_item(self.label, move |_| {
                match freya::clipboard::Clipboard::set(text.clone()) {
                    Ok(()) => overlays.toast("Copied."),
                    Err(_) => overlays.toast("The text could not be copied."),
                }
            })]
        });
        let items = items.unwrap_or_default();
        let touch_items = items.clone();
        rect()
            .width(Size::fill())
            .on_pointer_down(move |event: Event<PointerEventData>| {
                open_menu(overlays, &items, &event);
                begin_touch(press, overlays, &touch_items, &event);
            })
            .on_global_pointer_up(move |_| press.release())
            .on_global_pointer_move(move |event: Event<PointerEventData>| press.moved(&event))
            .child(self.child.clone())
    }
}

/// On a right press opens `items` at the pointer and stops the press there.
fn open_menu(overlays: Option<Overlays>, items: &[MenuItem], event: &Event<PointerEventData>) {
    let right = matches!(
        event.data(),
        PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Right)
    );
    if let Some(overlays) = overlays
        && right
        && !items.is_empty()
    {
        event.stop_propagation();
        let at = event.global_location();
        overlays.show_menu((at.x as f32, at.y as f32), items.to_vec());
    }
}

/// A touch that rests opens the menu where it began.
fn begin_touch(
    press: Press,
    overlays: Option<Overlays>,
    items: &[MenuItem],
    event: &Event<PointerEventData>,
) {
    let long = overlays.map(|overlays| {
        let items = items.to_vec();
        EventHandler::new(move |at: (f32, f32)| overlays.show_menu(at, items.clone()))
    });
    press.begin(event, long);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character is 8 px, the ellipsis too.
    fn fixed(text: &str) -> f32 {
        text.chars().count() as f32 * 8.0
    }

    /// Letters differ in width, as in a proportional font.
    fn proportional(text: &str) -> f32 {
        text.chars()
            .map(|c| match c {
                'i' | 'l' | 'j' | '.' | '/' | ',' | '\'' | ' ' | '-' | 't' | 'f' | 'I' | '1' => 4.0,
                'm' | 'w' | 'M' | 'W' | '@' => 12.0,
                'A'..='Z' => 9.5,
                '…' => 9.0,
                '\u{0300}'..='\u{036F}' => 0.0,
                c if c as u32 > 0x2E80 => 16.0,
                _ => 7.0,
            })
            .sum()
    }

    fn path(text: &str, chars: usize) -> String {
        elide_path(text, chars as f32 * 8.0, &fixed)
    }

    fn name(text: &str, chars: usize) -> String {
        elide_name(text, chars as f32 * 8.0, &fixed)
    }

    const DEEP: &str = "~/Documents/Work/Clients/Reports/2026/final.pdf";

    #[test]
    fn a_path_that_fits_is_unchanged() {
        assert_eq!(path(DEEP, DEEP.chars().count()), DEEP);
        assert_eq!(path(DEEP, 200), DEEP);
        assert_eq!(path("/usr/bin/", 20), "/usr/bin/");
    }

    #[test]
    fn middle_segments_go_first() {
        assert_eq!(path(DEEP, 44), "~/Documents/Work/…/Reports/2026/final.pdf");
        assert_eq!(path(DEEP, 40), "~/Documents/…/Reports/2026/final.pdf");
        assert_eq!(path(DEEP, 33), "~/Documents/…/2026/final.pdf");
        assert_eq!(path(DEEP, 25), "~/…/2026/final.pdf");
    }

    #[test]
    fn drops_run_from_the_middle_outward_and_the_end_keeps_the_extra_segment() {
        let long = "/mnt/disk/a/b/c/d/e/last";
        let kept: Vec<String> = (11..=26).map(|chars| path(long, chars)).collect();
        assert_eq!(kept[0], "/mnt/…/last");
        assert!(kept.contains(&"/mnt/…/e/last".to_string()));
        assert!(kept.contains(&"/mnt/disk/…/e/last".to_string()));
        assert!(kept.contains(&"/mnt/disk/…/d/e/last".to_string()));
        assert!(kept.contains(&"/mnt/disk/a/…/d/e/last".to_string()));
    }

    #[test]
    fn the_first_and_last_segments_stay_while_they_fit() {
        assert_eq!(path(DEEP, 18), "~/…/2026/final.pdf");
        assert_eq!(path(DEEP, 15), "~/…/final.pdf");
        assert_eq!(path("/mnt/kieran/data/downloads", 21), "/mnt/…/data/downloads");
        assert_eq!(path("/mnt/kieran/data/downloads", 20), "/mnt/…/downloads");
    }

    #[test]
    fn the_root_is_part_of_the_first_segment() {
        let long = "/home/kieran/Documents/Reports/final.pdf";
        assert_eq!(path(long, 25), "/home/…/Reports/final.pdf");
        assert_eq!(path(long, 24), "/home/…/final.pdf");
        assert_eq!(path("/", 1), "/");
        assert_eq!(path("/", 0), "…");
    }

    #[test]
    fn a_trailing_slash_stays() {
        assert_eq!(path("~/Documents/Work/Reports/", 16), "~/…/Reports/");
        assert_eq!(path("~/Documents/Work/Reports/", 200), "~/Documents/Work/Reports/");
    }

    #[test]
    fn a_relative_path_has_no_root() {
        assert_eq!(path("src/controls/deep/path_label.rs", 20), "src/…/path_label.rs");
        assert_eq!(path("src/controls/deep/path_label.rs", 16), "src/…/path…el.rs");
    }

    #[test]
    fn a_scheme_and_host_are_one_segment() {
        let share = "smb://nas.local/share/a/b/report.pdf";
        assert_eq!(path(share, 30), "smb://nas.local/…/b/report.pdf");
        assert_eq!(path(share, 29), "smb://nas.local/…/report.pdf");
        assert_eq!(path("file:///home/kieran/x/y/z", 20), "file:///home/…/x/y/z");
        assert_eq!(path("file:///home/kieran/x/y/z", 19), "file:///home/…/y/z");
    }

    #[test]
    fn empty_and_single_segment_paths() {
        assert_eq!(path("", 10), "");
        assert_eq!(path("~", 1), "~");
        assert_eq!(path("readme.md", 100), "readme.md");
        assert_eq!(path("a-very-long-single-segment-name.txt", 20), "a-very-lon…-name.txt");
    }

    #[test]
    fn two_segments_shorten_the_last_then_the_first() {
        assert_eq!(path("~/final-report-2026.pdf", 14), "~/final…26.pdf");
        // Last segment at its least, so the first gives way.
        let tiny = path("Documents/final-report-2026.pdf", 12);
        assert!(tiny.contains('…'));
        assert!(fixed(&tiny) <= 12.0 * 8.0, "{tiny}");
    }

    #[test]
    fn the_last_segment_loses_its_middle_and_keeps_its_extension() {
        let cut = path("~/a/b/c/final-report-2026-edition.pdf", 24);
        assert!(cut.starts_with("~/…/"), "{cut}");
        assert!(cut.ends_with(".pdf"), "{cut}");
        assert!(cut.contains('…'));
        assert_eq!(cut.matches('…').count(), 2, "{cut}");
    }

    #[test]
    fn very_narrow_widths_give_the_ellipsis() {
        for chars in 0..2 {
            assert_eq!(path(DEEP, chars), "…", "{chars}");
        }
        assert_eq!(elide_path(DEEP, -5.0, &fixed), "…");
        assert_eq!(elide_path(DEEP, 0.0, &proportional), "…");
        assert_eq!(elide_name("a-very-long-name.txt", 0.0, &fixed), "…");
        assert_eq!(elide_middle("anything at all", 1.0, &fixed), "…");
    }

    #[test]
    fn too_narrow_for_both_ends_the_last_segment_stays() {
        let cut = path(DEEP, 8);
        assert_eq!(cut, "fi…l.pdf");
        assert_eq!(path("smb://nas.local/share/a/clip.mp4", 6), "c….mp4");
    }

    #[test]
    fn the_result_is_never_empty_and_fits() {
        for text in [DEEP, "/", "/mnt/a/b/c/d", "smb://host/x/y/z", "plain", "a/b", "~/", "///a//b//"] {
            for chars in 0..60 {
                let out = path(text, chars);
                assert!(!out.is_empty(), "{text} at {chars}");
                assert!(out == "…" || fixed(&out) <= chars as f32 * 8.0 || out == text, "{text} at {chars}: {out}");
            }
        }
    }

    #[test]
    fn a_wider_width_never_keeps_less() {
        let texts = [
            DEEP,
            "/home/kieran/Documents/Reports/final.pdf",
            "smb://nas.local/share/Videos/Holidays/2026/clip-final.mp4",
            "/mnt/Ünïcödé/日本語のフォルダ/ファイル名の長い書類.pdf",
            "relative/dir/with/many/segments/and-a-long-final-name.tar.gz",
            "a-single-segment-that-is-very-long.txt",
        ];
        for text in texts {
            for measure in [&fixed as &dyn Fn(&str) -> f32, &proportional] {
                let mut previous = 0.0_f32;
                let mut width = 0.0;
                while width < measure(text) + 24.0 {
                    let out = elide_path(text, width, measure);
                    let kept = measure(&out);
                    assert!(
                        kept + 0.001 >= previous,
                        "{text}: {width} px gave `{out}` ({kept}) after {previous}"
                    );
                    previous = kept;
                    width += 1.0;
                }
            }
        }
    }

    #[test]
    fn a_proportional_measure_decides_by_width_not_count() {
        let text = "/home/iii/lll/mmm/WWW/final.pdf";
        let narrow = elide_path(text, 120.0, &proportional);
        let by_count = elide_path(text, 120.0, &fixed);
        assert!(proportional(&narrow) <= 120.0);
        assert_ne!(narrow, by_count);
    }

    #[test]
    fn a_name_keeps_its_extension() {
        assert_eq!(name("final-report-2026-edition.pdf", 18), "final-rep…tion.pdf");
        let cut = name("final-report-2026-edition.pdf", 18);
        assert!(cut.ends_with(".pdf"), "{cut}");
        assert!(cut.starts_with("final-"), "{cut}");
        assert!(cut.chars().count() <= 18, "{cut}");
        assert!(cut.contains('…'));
    }

    #[test]
    fn the_end_of_the_stem_stays_beside_the_extension() {
        assert_eq!(name("final-report-2026-edition.pdf", 19), "final-repo…tion.pdf");
    }

    #[test]
    fn tar_archives_keep_both_parts() {
        let cut = name("backup-of-the-home-folder-2026-10-26.tar.gz", 20);
        assert!(cut.ends_with(".tar.gz"), "{cut}");
        assert!(cut.chars().count() <= 20, "{cut}");
        assert_eq!(extension_start("a.b.tar.xz"), 3);
        assert_eq!(extension_start("a.tar"), 1);
        assert_eq!(extension_start("a.b.c"), 3);
    }

    #[test]
    fn dotfiles_and_odd_dots_have_no_extension() {
        assert_eq!(extension_start(".bashrc"), ".bashrc".len());
        assert_eq!(extension_start("trailing."), "trailing.".len());
        assert_eq!(extension_start("no-dot"), 6);
        assert_eq!(extension_start("Mr. Smith went to town"), "Mr. Smith went to town".len());
        assert_eq!(extension_start("file.averyveryverylongextension"), "file.averyveryverylongextension".len());
    }

    #[test]
    fn a_name_without_room_for_its_extension_gives_it_up() {
        let cut = name("final-report-2026.pdf", 4);
        assert!(cut.chars().count() <= 4, "{cut}");
        assert!(cut.contains('…'));
        assert_eq!(name("x.pdf", 1), "…");
        assert!(name("x.pdf", 3).chars().count() <= 3);
    }

    #[test]
    fn a_name_that_fits_is_unchanged() {
        assert_eq!(name("report.pdf", 10), "report.pdf");
        assert_eq!(name("", 0), "");
    }

    #[test]
    fn unicode_is_cut_between_characters() {
        let text = "日本語のとても長いファイル名.pdf";
        for chars in 0..text.chars().count() {
            let cut = name(text, chars);
            assert!(!cut.is_empty());
            assert!(cut.chars().count() <= chars.max(1), "{cut} in {chars}");
        }
        assert_eq!(name(text, 10), "日本語の…名.pdf");
    }

    #[test]
    fn combining_marks_stay_with_their_letter() {
        // "é" as e + U+0301, repeated.
        let text = "re\u{301}sume\u{301}-final-re\u{301}port.pdf";
        for chars in 6..text.chars().count() {
            let cut = name(text, chars);
            let mut previous = None;
            for c in cut.chars() {
                if joins_previous(c) {
                    assert!(previous.is_some_and(|p| p != '…' && p != '/'), "{cut:?}");
                }
                previous = Some(c);
            }
            assert!(
                !cut.starts_with(|c: char| joins_previous(c)),
                "{cut:?} starts with a mark"
            );
        }
    }

    #[test]
    fn a_joiner_keeps_an_emoji_sequence_together() {
        let text = "family-👨\u{200D}👩\u{200D}👧-photos-of-the-whole-year.png";
        for chars in 8..text.chars().count() {
            let cut = name(text, chars);
            let parts: Vec<char> = cut.chars().collect();
            for (index, c) in parts.iter().enumerate() {
                if *c == '\u{200D}' {
                    let before = parts.get(index.wrapping_sub(1)).copied();
                    let after = parts.get(index + 1).copied();
                    assert!(
                        before.is_some_and(|b| b != '…') && after.is_some_and(|a| a != '…'),
                        "{cut:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_name_is_monotone_in_width() {
        for text in [
            "final-report-2026-edition.pdf",
            "backup-of-the-home-folder.tar.gz",
            "日本語のとても長いファイル名.pdf",
            "no-extension-at-all-just-a-long-name",
        ] {
            for measure in [&fixed as &dyn Fn(&str) -> f32, &proportional] {
                let mut previous = 0;
                let mut width = 0.0;
                while width < measure(text) + 10.0 {
                    let out = elide_name(text, width, measure);
                    let letters = out.chars().filter(|c| *c != '…').count();
                    assert!(letters >= previous, "{text}: {width} px gave `{out}` after {previous}");
                    previous = letters;
                    width += 1.0;
                }
            }
        }
    }

    #[test]
    fn middle_cuts_ignore_extensions() {
        let cut = elide_middle("Wireless network at home", 14.0 * 8.0, &fixed);
        assert_eq!(cut, "Wireles…t home");
        assert_eq!(elide_middle("short", 100.0, &fixed), "short");
    }

    #[test]
    fn the_least_width_of_a_name_is_one_letter_and_its_extension() {
        assert_eq!(least_width("final-report.pdf", &fixed), 6.0 * 8.0);
        assert_eq!(least_width("~", &fixed), 8.0);
        assert_eq!(least_width("Documents", &fixed), 16.0);
    }
}
