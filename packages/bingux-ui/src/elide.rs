//! Fitting a path or a name into a width: the pure functions under
//! [`path_label`](crate::controls::path_label) and every other place that
//! shows a path in constrained space.
//!
//! Nothing here draws or measures by itself. Every function takes a
//! [`Measure`], the width of a string in px, so the UI passes the metrics of
//! the real font and size and a test passes a fake monospace measurer. The
//! design, the sources and the reasons are in `design/path-elision.md`.
//!
//! # The ladder
//!
//! [`elide_path`] walks a fixed ladder of candidates from the most to the
//! least informative and returns the first that fits. The ladder depends on
//! the path only, never on the width, so:
//!
//! * a wider width never shows less (the set of candidates that fit only
//!   grows, so the first one is never later),
//! * a result is never wider than the room, except the lone `…`,
//! * eliding an elided result with the same width changes nothing (it fits),
//! * there are no thresholds to flicker around: the same width always gives
//!   the same text.
//!
//! For `/home/kieran/Documents/Projects/bingux/design/final-report-2026.pdf`:
//!
//! 1. [`ElideStep::Whole`]: the path itself.
//! 2. [`ElideStep::Collapsed`]: directories from the middle outwards become
//!    one `…`, the anchor (`~`, or `/` and the first directory) and as many
//!    trailing directories as fit stay: `/home/kieran/…/design/final-report-2026.pdf`,
//!    down to the anchor, `…`, the directory that holds the file and the file.
//! 3. [`ElideStep::Abbreviated`]: the directories that are left give up
//!    letters, as fish's `prompt_pwd` does, longest first and from the left,
//!    to their first character (`.config` becomes `.c`); the holder of the
//!    file and the file stay: `/h/k/…/design/final-report-2026.pdf`. Then the
//!    collapse goes on with abbreviated directories: `/h/…/final-report-2026.pdf`.
//! 4. [`ElideStep::NameCut`]: the file name loses its middle and keeps its
//!    extension (`.tar.gz` whole) and the end of its stem:
//!    `/h/…/final-rep…2026.pdf`. When that leaves fewer than a few letters
//!    the directories go and the name has the room alone ([`ElideStep::NameOnly`]).
//! 5. [`ElideStep::Middle`]: the extension gives way too, and
//!    [`ElideStep::Least`] is `…`.
//!
//! Every cut is between grapheme clusters, never inside one.

use unicode_segmentation::UnicodeSegmentation;

/// The mark that stands for what is left out.
pub const ELLIPSIS: &str = "…";

/// The longest extension, dot included, that is kept whole.
const MAX_EXTENSION: usize = 12;

/// In a path, the name keeps at least this many letters of its stem before
/// the directories give up their room to it.
const NAME_FLOOR: usize = 8;

/// Step 2 collapses down to this many whole segments when the path has
/// enough: the anchor, one more, the holder of the file and the file. Below
/// it the directories are abbreviated before more are dropped.
const COLLAPSE_FLOOR: usize = 4;

/// The longest end of a stem that stays beside the extension.
const STEM_END: usize = 4;

/// Left-to-right isolate and pop directional isolate (UAX #9).
const LRI: char = '\u{2066}';
const PDI: char = '\u{2069}';

/// The width in px of a string drawn on one line. The UI measures with the
/// font and size the text is drawn in; tests use a fake. Any
/// `Fn(&str) -> f32` is a measure.
pub trait Measure {
    fn width(&self, text: &str) -> f32;
}

impl<F: Fn(&str) -> f32 + ?Sized> Measure for F {
    fn width(&self, text: &str) -> f32 {
        self(text)
    }
}

/// Which rung of the ladder a result came from, from the most to the least
/// kept. Comparing two steps compares how much they show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ElideStep {
    /// The text fits and is unchanged.
    Whole,
    /// Whole directories were replaced by `…`.
    Collapsed,
    /// Directories were shortened to their first letter.
    Abbreviated,
    /// The name lost its middle and sits after the directories that are left.
    NameCut,
    /// The directories are gone: the name alone, whole or cut.
    NameOnly,
    /// The name lost its extension as well and was cut in the middle.
    Middle,
    /// Not even a letter fits: `…`.
    Least,
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// The text to draw for `path` in at most `max_width`, as `measure` reports
/// widths. See the module docs for the ladder. A path that fits is returned
/// unchanged. An empty path gives an empty string; anything else gives at
/// least `…`.
///
/// `~` is a root, so is `/`, a drive such as `C:` and `scheme://host`. A
/// path with backslashes and no slash is a Windows path and keeps its
/// separator. A trailing separator stays. `.` and `..` are directories like
/// any other, never abbreviated.
pub fn elide_path<M: Measure + ?Sized>(path: &str, max_width: f32, measure: &M) -> String {
    elide_path_traced(path, max_width, measure).0
}

/// [`elide_path`] with the rung that produced the text.
pub fn elide_path_traced<M: Measure + ?Sized>(
    path: &str,
    max_width: f32,
    measure: &M,
) -> (String, ElideStep) {
    let fits = |text: &str| measure.width(text) <= max_width;
    if path.is_empty() || fits(path) {
        return (path.to_owned(), ElideStep::Whole);
    }
    let parts = Parts::of(path);
    let count = parts.items.len();
    if count == 0 {
        // Only separators: nothing to collapse.
        return cut_plain(path, "", &fits).unwrap_or_else(least);
    }

    // 2. Collapse directories from the middle outwards.
    let none = vec![false; count];
    let floor = if count >= 3 { (count - 1).min(COLLAPSE_FLOOR) } else { count };
    if count >= 3 {
        for kept in (floor..count).rev() {
            let candidate = parts.keep(kept, &none);
            if fits(&candidate) {
                return (candidate, ElideStep::Collapsed);
            }
        }
    }

    // 3. Abbreviate the directories that are left, then collapse further.
    let skeleton = |abbreviated: &[bool]| {
        if count >= 3 {
            parts.keep(floor, abbreviated)
        } else {
            parts.keep(count, abbreviated)
        }
    };
    let (head, tail) = if count >= 3 { split_kept(floor) } else { (count, 0) };
    let present = (0..head).chain(count - tail..count);
    let mut abbreviated = none.clone();
    for index in parts.abbreviation_order(present) {
        abbreviated[index] = true;
        let candidate = skeleton(&abbreviated);
        if fits(&candidate) {
            return (candidate, ElideStep::Abbreviated);
        }
    }
    let all = parts.abbreviation_order(0..count);
    let mut abbreviated = none;
    for index in all {
        abbreviated[index] = true;
    }
    if count >= 3 {
        for kept in (2..floor).rev() {
            let candidate = parts.keep(kept, &abbreviated);
            if fits(&candidate) {
                return (candidate, ElideStep::Abbreviated);
            }
        }
    }

    // 4. Cut the name in the middle, keeping its extension.
    let last = parts.items[count - 1];
    let name = Name::split(last, !parts.trail.is_empty());
    let prefix = parts.prefix(&abbreviated);
    let most = name.most();
    let low = if count == 1 { 1 } else { NAME_FLOOR.min(most).max(1) };
    if let Some(k) = widest(low, most, |k| {
        fits(&format!("{prefix}{}{}", name.cut(k), parts.trail))
    }) {
        return (format!("{prefix}{}{}", name.cut(k), parts.trail), ElideStep::NameCut);
    }
    if count == 1 {
        // The name is all there is: the same ladder without an extension.
        return finish_plain(last, parts.trail, &fits);
    }

    // The directories give their room to the name.
    if let Some(found) = name_only(&name, parts.trail, &fits) {
        return found;
    }
    finish_plain(last, parts.trail, &fits)
}

/// `…` alone, the least there is.
fn least() -> (String, ElideStep) {
    (ELLIPSIS.to_owned(), ElideStep::Least)
}

/// The last rungs: `name` without an extension cut in the middle, then `…`.
fn finish_plain(name: &str, suffix: &str, fits: &dyn Fn(&str) -> bool) -> (String, ElideStep) {
    cut_plain(name, suffix, fits).unwrap_or_else(least)
}

/// `text` cut in the middle, with the most letters that fit beside `suffix`.
fn cut_plain(
    text: &str,
    suffix: &str,
    fits: &dyn Fn(&str) -> bool,
) -> Option<(String, ElideStep)> {
    let plain = Name::plain(text);
    let k = widest(1, plain.most(), |k| fits(&format!("{}{suffix}", plain.cut(k))))?;
    Some((format!("{}{suffix}", plain.cut(k)), ElideStep::Middle))
}

/// The name on its own: whole, then cut with its extension kept.
fn name_only(name: &Name, suffix: &str, fits: &dyn Fn(&str) -> bool) -> Option<(String, ElideStep)> {
    let whole = format!("{}{suffix}", name.cut(name.stem.len()));
    if fits(&whole) {
        return Some((whole, ElideStep::NameOnly));
    }
    let k = widest(1, name.most(), |k| fits(&format!("{}{suffix}", name.cut(k))))?;
    Some((format!("{}{suffix}", name.cut(k)), ElideStep::NameOnly))
}

/// A path split into the pieces the ladder works with.
struct Parts<'a> {
    /// The root separator (`/`, or `\\` of a UNC path), else empty.
    lead: &'a str,
    /// The segments, first to last. The first may be `scheme://host`.
    items: Vec<&'a str>,
    /// The separator when the path ends in one.
    trail: &'a str,
    sep: &'static str,
    /// The first segment is a root (`~`, a drive, `scheme://host`): it is
    /// never abbreviated.
    anchor_fixed: bool,
}

impl<'a> Parts<'a> {
    fn of(path: &'a str) -> Self {
        let windows = path.contains('\\') && !path.contains('/');
        let (sep_char, sep) = if windows { ('\\', "\\") } else { ('/', "/") };
        let (lead, items, anchor_fixed) = match scheme_end(path) {
            Some(end) => {
                let mut items = vec![&path[..end]];
                items.extend(path[end..].split('/').filter(|part| !part.is_empty()));
                ("", items, true)
            }
            None => {
                let lead_len = if windows && path.starts_with("\\\\") {
                    2
                } else {
                    usize::from(path.starts_with(sep_char))
                };
                let items: Vec<&str> = path[lead_len..]
                    .split(sep_char)
                    .filter(|part| !part.is_empty())
                    .collect();
                let fixed = items.first().is_some_and(|first| first.starts_with('~') || is_drive(first));
                (&path[..lead_len], items, fixed)
            }
        };
        let trail = if path.ends_with(sep_char) && !items.is_empty() {
            &path[path.len() - 1..]
        } else {
            ""
        };
        Self { lead, items, trail, sep, anchor_fixed }
    }

    /// Whether the segment at `index` may be shortened to its first letter:
    /// not the last one, not the directory that holds it, not a root.
    fn abbreviable(&self, index: usize) -> Option<&'a str> {
        let count = self.items.len();
        if index + 2 >= count || (index == 0 && self.anchor_fixed) {
            return None;
        }
        abbreviate(self.items[index])
    }

    /// The segments among `indices` that can be abbreviated, longest first
    /// and from the left among equals.
    fn abbreviation_order(&self, indices: impl Iterator<Item = usize>) -> Vec<usize> {
        let mut order: Vec<(usize, usize)> = indices
            .filter(|&index| self.abbreviable(index).is_some())
            .map(|index| (index, self.items[index].graphemes(true).count()))
            .collect();
        order.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        order.into_iter().map(|(index, _)| index).collect()
    }

    /// The segment at `index` as drawn.
    fn shown(&self, index: usize, abbreviated: &[bool]) -> &'a str {
        if abbreviated[index] {
            self.abbreviable(index).unwrap_or(self.items[index])
        } else {
            self.items[index]
        }
    }

    /// `kept` whole segments (all of them when `kept` is the count) and one
    /// `…` where the middle ones were, inside the lead and the trail.
    fn keep(&self, kept: usize, abbreviated: &[bool]) -> String {
        let count = self.items.len();
        let (head, tail) = if kept >= count { (count, 0) } else { split_kept(kept) };
        let mut out = String::from(self.lead);
        let mut first = true;
        let mut push = |out: &mut String, piece: &str| {
            if !first {
                out.push_str(self.sep);
            }
            first = false;
            out.push_str(piece);
        };
        for index in 0..head {
            push(&mut out, self.shown(index, abbreviated));
        }
        if kept < count {
            push(&mut out, ELLIPSIS);
            for index in count - tail..count {
                push(&mut out, self.shown(index, abbreviated));
            }
        }
        out.push_str(self.trail);
        out
    }

    /// What stands before the last segment when only the anchor is left:
    /// `/home/…/`, or `/home/` for two segments, or just the lead.
    fn prefix(&self, abbreviated: &[bool]) -> String {
        let count = self.items.len();
        if count == 1 {
            return self.lead.to_owned();
        }
        let mut out = String::from(self.lead);
        out.push_str(self.shown(0, abbreviated));
        out.push_str(self.sep);
        if count >= 3 {
            out.push_str(ELLIPSIS);
            out.push_str(self.sep);
        }
        out
    }
}

/// How many of `kept` segments stand before the `…`: the end has the extra
/// one, so the folders nearest the file stay.
fn split_kept(kept: usize) -> (usize, usize) {
    let head = kept / 2;
    (head, kept - head)
}

/// A drive: `C:`.
fn is_drive(segment: &str) -> bool {
    let mut chars = segment.chars();
    matches!((chars.next(), chars.next(), chars.next()), (Some(letter), Some(':'), None) if letter.is_ascii_alphabetic())
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

/// A directory name shortened to its first letter, keeping a leading dot
/// (`.config` is `.c`); `None` when that is not shorter, or for `.` and `..`.
pub fn abbreviate(directory: &str) -> Option<&str> {
    if directory.chars().all(|c| c == '.') {
        return None;
    }
    let dot = usize::from(directory.starts_with('.'));
    let first = directory[dot..].graphemes(true).next()?;
    let end = dot + first.len();
    (end < directory.len()).then(|| &directory[..end])
}

// ---------------------------------------------------------------------------
// Names
// ---------------------------------------------------------------------------

/// The text to draw for a file or folder `name` in at most `max_width`. A
/// name that does not fit loses its middle and keeps its extension and the
/// end of its stem: `final-rep…2026.pdf`. `.tar.gz` and any `.tar.*` count
/// as one extension. When one letter and the extension do not fit, the
/// extension goes too. Never empty: the least is `…`.
pub fn elide_name<M: Measure + ?Sized>(name: &str, max_width: f32, measure: &M) -> String {
    elide_name_traced(name, max_width, measure).0
}

/// [`elide_name`] with the rung that produced the text.
pub fn elide_name_traced<M: Measure + ?Sized>(
    name: &str,
    max_width: f32,
    measure: &M,
) -> (String, ElideStep) {
    let fits = |text: &str| measure.width(text) <= max_width;
    if name.is_empty() || fits(name) {
        return (name.to_owned(), ElideStep::Whole);
    }
    let split = Name::split(name, false);
    name_only(&split, "", &fits).unwrap_or_else(|| finish_plain(name, "", &fits))
}

/// The text to draw for any `text` in at most `max_width`, cut in the
/// middle with no special care for extensions or separators. For labels that
/// are not file names.
pub fn elide_middle<M: Measure + ?Sized>(text: &str, max_width: f32, measure: &M) -> String {
    let fits = |candidate: &str| measure.width(candidate) <= max_width;
    if text.is_empty() || fits(text) {
        return text.to_owned();
    }
    cut_plain(text, "", &fits).unwrap_or_else(least).0
}

/// A name as graphemes of its stem and the extension that stays whole.
struct Name<'a> {
    stem: Vec<&'a str>,
    ext: &'a str,
}

impl<'a> Name<'a> {
    /// `name` with its extension apart. `directory` names have none.
    fn split(name: &'a str, directory: bool) -> Self {
        let at = if directory { name.len() } else { extension_start(name) };
        Self {
            stem: name[..at].graphemes(true).collect(),
            ext: &name[at..],
        }
    }

    /// `name` as one run of graphemes, whatever its dots.
    fn plain(name: &'a str) -> Self {
        Self { stem: name.graphemes(true).collect(), ext: "" }
    }

    /// The most letters a cut can keep: one fewer than the stem has.
    fn most(&self) -> usize {
        self.stem.len().saturating_sub(1)
    }

    /// The name with `keep` letters of the stem and `…` for the rest. Beside
    /// an extension the end of the stem is a quarter of the letters at most
    /// four; without one the ends are even.
    fn cut(&self, keep: usize) -> String {
        if keep >= self.stem.len() {
            return format!("{}{}", self.stem.concat(), self.ext);
        }
        let tail = if self.ext.is_empty() { keep / 2 } else { (keep / 2).min(STEM_END) };
        let head = keep - tail;
        let mut out = String::new();
        out.extend(self.stem[..head].iter().copied());
        out.push_str(ELLIPSIS);
        out.extend(self.stem[self.stem.len() - tail..].iter().copied());
        out.push_str(self.ext);
        out
    }
}

/// The largest `k` in `low..=high` for which `fits(k)`, assuming the width
/// only grows with `k`. `None` when not even `low` fits.
fn widest(low: usize, high: usize, mut fits: impl FnMut(usize) -> bool) -> Option<usize> {
    if low > high || !fits(low) {
        return None;
    }
    let (mut low, mut high) = (low, high);
    while low < high {
        let mid = (low + high).div_ceil(2);
        if fits(mid) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    Some(low)
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

// ---------------------------------------------------------------------------
// Lists
// ---------------------------------------------------------------------------

/// A file name and what tells it from other files of the same name in the
/// same list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Distinct {
    pub name: String,
    /// The nearest parent directories that make the name unique, as
    /// `parent` or `outer/parent`; empty for a name that is unique already.
    pub context: String,
}

impl Distinct {
    /// `name`, or `name — context` for a name that needs it.
    pub fn label(&self) -> String {
        if self.context.is_empty() {
            self.name.clone()
        } else {
            format!("{} — {}", self.name, self.context)
        }
    }
}

/// The shortest labels that tell `paths` apart, as VS Code does for tabs:
/// the file name alone while it is unique, else the name with the fewest
/// parent directories that make it unique among the paths that share it.
/// The same path twice gets the same label. Paths with other separators
/// work as in [`elide_path`].
pub fn distinguish(paths: &[&str]) -> Vec<Distinct> {
    fn name_of<'a>(parts: &Parts<'a>) -> &'a str {
        parts.items.last().copied().unwrap_or("")
    }
    let split: Vec<Parts> = paths.iter().map(|path| Parts::of(path)).collect();
    let count = paths.len();
    // Two different paths with one name.
    let same_name = |a: usize, b: usize| {
        a != b && paths[a] != paths[b] && name_of(&split[a]) == name_of(&split[b])
    };
    // A name shared with another path shows at least one parent, so the
    // reader sees why it is not alone.
    let mut depth: Vec<usize> = (0..count)
        .map(|index| {
            usize::from(split[index].items.len() > 1 && (0..count).any(|other| same_name(index, other)))
        })
        .collect();
    // Paths that show the same thing grow together, so `app/src` is not set
    // beside a bare `src` of the same kind.
    loop {
        let clashing: Vec<usize> = (0..count)
            .filter(|&index| {
                let label = context_of(&split[index], depth[index]);
                (0..count).any(|other| {
                    same_name(index, other) && context_of(&split[other], depth[other]) == label
                })
            })
            .filter(|&index| depth[index] + 1 < split[index].items.len())
            .collect();
        if clashing.is_empty() {
            break;
        }
        for index in clashing {
            depth[index] += 1;
        }
    }
    split
        .iter()
        .zip(&depth)
        .map(|(parts, &depth)| Distinct {
            name: name_of(parts).to_owned(),
            context: context_of(parts, depth),
        })
        .collect()
}

/// The `depth` parents nearest the last segment, joined.
fn context_of(parts: &Parts, depth: usize) -> String {
    let count = parts.items.len();
    if depth == 0 || count < 2 {
        return String::new();
    }
    parts.items[count.saturating_sub(1 + depth)..count - 1].join(parts.sep)
}

// ---------------------------------------------------------------------------
// Direction
// ---------------------------------------------------------------------------

/// Whether `text` has a letter that is written right to left.
pub fn has_rtl(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            c as u32,
            0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF
        )
    })
}

/// `text` as one left-to-right unit that the letters around it do not
/// reorder, for a path shown in a right-to-left sentence. A path is read
/// from its root to its end whatever the language of its names, so the
/// segments keep their order on screen; the isolate stops the neighbours of
/// a `/` from joining the run next to it. Text without right-to-left
/// letters is returned as it is.
pub fn isolate_ltr(text: &str) -> String {
    if !has_rtl(text) {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() + 6);
    out.push(LRI);
    out.push_str(text);
    out.push(PDI);
    out
}

#[cfg(test)]
mod tests;
