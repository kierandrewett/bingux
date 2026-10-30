//! A number whose digits roll like the shell's clock.
//!
//! The Bingux shell shows the time with `RollingNumber.qml`. Each digit that
//! changes slides out of its place while the new one slides in, in one
//! clipped column of its own: the old digit leaves upwards and fades as the
//! new one rises from below and fades in, for 140 ms along an ease-out cubic.
//! When the value goes down the digits travel the other way. Digits that do
//! not change stay still, so `12:59` becoming `13:00` moves four columns and
//! leaves the colon alone. [`AnimatedNumber`] is that, for any number.
//!
//! # Model
//!
//! A number is its text, and the text is a row of cells aligned on the
//! right, so the units are always column 0 whatever the length. A cell is a
//! digit, a separator or a sign. Every cell is one column that holds a
//! [`Strip`]: the glyphs it has shown lately, each on a numbered slot one line
//! apart, and a position that eases from slot to slot. A glyph at slot `s`
//! sits `(s - position)` lines from rest, which gives both ends of a slide:
//! the old digit at `-p` lines, the new at `1 - p`, and the opacity of each is
//! `1 - |distance|`, the cross-fade of the clock. The position is a
//! [`use_tween_with`] tween, so it retargets from where it is: a new value
//! arriving mid-slide adds a slot ahead and the digit keeps going from the
//! point it had reached; it never snaps, and nothing queues.
//!
//! A column that appears (the hundreds when 99 becomes 100) grows from no
//! width to its width while its digit rises in; one that goes (the hundreds
//! when 100 becomes 99) shrinks while its digit leaves, and stays mounted
//! with no width, ready to come back. Separators and signs cross-fade in
//! place: only digits move.
//!
//! # Layout
//!
//! The digits use tabular figures and every digit column is as wide as the
//! widest digit of the style, measured with the same text engine the labels
//! use, so counting never changes the width. Columns are as tall as one line
//! of the style, so a number sits on the same baseline as a label of that
//! style next to it and its box is the box of the text it shows. The digits
//! are always laid out left to right in Latin order, whatever the language of
//! the page: the number is a block that the surrounding layout may mirror, but
//! its digits are never reversed.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use accesskit::Live;
use freya::animation::*;
use freya::prelude::*;

use crate::controls::text_width;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{NUMERIC, TextStyle, styled_label};

use super::{ROLL_MS, use_tween_with};

/// The minus sign: U+2212, which is as wide as a plus and sits at the height of
/// the digits' crossbar, unlike the hyphen.
const MINUS: char = '\u{2212}';

/// Shown for a value that is not a number.
const NOT_A_NUMBER: char = '\u{2014}';

/// How a number is written: the mark between the thousands and the mark
/// before the decimals. The digits are always Latin digits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumberFormat {
    /// Goes between groups of three integer digits, when there are four or
    /// more of them. `None` writes `12345`.
    pub group: Option<char>,
    /// Goes before the decimals.
    pub decimal: char,
}

impl NumberFormat {
    /// No grouping and a point: `12345.6`. What a number is written like
    /// until it is asked to be grouped.
    pub const PLAIN: Self = Self {
        group: None,
        decimal: '.',
    };

    /// A comma between thousands and a point: `12,345.6`.
    pub const ENGLISH: Self = Self {
        group: Some(','),
        decimal: '.',
    };

    /// The format of a locale such as `en_GB`, `de-DE.UTF-8` or `fr`. Only
    /// the language (and, for Swiss German, the country) is read. A
    /// language not known here is written as English.
    pub fn for_locale(locale: &str) -> Self {
        let tag = locale.split(['.', '@']).next().unwrap_or("");
        let mut parts = tag.split(['_', '-']);
        let language = parts.next().unwrap_or("").to_ascii_lowercase();
        let country = parts.next().unwrap_or("").to_ascii_uppercase();
        match (language.as_str(), country.as_str()) {
            ("de", "CH") | ("it", "CH") | ("fr", "CH") => Self {
                group: Some('\u{2019}'),
                decimal: '.',
            },
            ("de" | "es" | "it" | "nl" | "pt" | "tr" | "id" | "da" | "el" | "ro" | "hr" | "sl", _) => Self {
                group: Some('.'),
                decimal: ',',
            },
            ("fr", _) => Self {
                group: Some('\u{202f}'),
                decimal: ',',
            },
            ("ru" | "uk" | "pl" | "cs" | "sk" | "sv" | "nb" | "no" | "fi" | "bg" | "hu" | "lt" | "lv", _) => Self {
                group: Some('\u{a0}'),
                decimal: ',',
            },
            _ => Self::ENGLISH,
        }
    }
}

/// `value` written with `decimals` digits after the point, as the number
/// shows it: a sign only when the shown value is not zero, the integer digits
/// padded with zeros to `min_digits`, in groups, then the decimals.
pub(crate) fn format_number(value: f64, decimals: usize, min_digits: usize, format: NumberFormat) -> String {
    if !value.is_finite() {
        return NOT_A_NUMBER.to_string();
    }
    let plain = format!("{:.*}", decimals, value.abs());
    let (integer, fraction) = plain.split_once('.').unwrap_or((plain.as_str(), ""));
    let padded = format!("{integer:0>min_digits$}");
    let integer = padded.as_str();
    let mut text = String::with_capacity(plain.len() + 4);
    if value.is_sign_negative() && plain.bytes().any(|byte| matches!(byte, b'1'..=b'9')) {
        text.push(MINUS);
    }
    for (index, digit) in integer.chars().enumerate() {
        if let Some(group) = format.group
            && index > 0
            && (integer.len() - index) % 3 == 0
        {
            text.push(group);
        }
        text.push(digit);
    }
    if !fraction.is_empty() {
        text.push(format.decimal);
        text.push_str(fraction);
    }
    text
}

/// What a column shows: a character, or nothing.
type Glyph = Option<char>;

/// The glyph of the column `place` cells from the right, which is nothing
/// beyond the first cell of the text.
fn glyph_at(cells: &[char], place: usize) -> Glyph {
    cells.len().checked_sub(place + 1).map(|index| cells[index])
}

/// Only digits move; every other glyph holds its place and cross-fades.
fn slides(glyph: char) -> bool {
    glyph.is_ascii_digit()
}

/// How opaque a glyph is at `distance` lines from rest: the clock's
/// cross-fade, the old digit at `1 - p` and the new at `p`.
fn fade(distance: f32) -> f32 {
    (1.0 - distance.abs()).clamp(0.0, 1.0)
}

/// A glyph on show: what it is and how far from rest it is, in lines.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Placed {
    glyph: char,
    distance: f32,
}

/// The glyphs a column has shown lately, one per slot, and the slot its
/// position is heading for.
#[derive(Clone, Debug, PartialEq)]
struct Strip {
    slots: Vec<(i64, Glyph)>,
    target: i64,
}

impl Strip {
    /// A strip at rest on `glyph`.
    fn at(glyph: Glyph) -> Self {
        Self {
            slots: vec![(0, glyph)],
            target: 0,
        }
    }

    fn glyph(&self, slot: i64) -> Option<Glyph> {
        self.slots.iter().find(|(at, _)| *at == slot).map(|(_, glyph)| *glyph)
    }

    /// The glyph the strip is heading for.
    fn target_glyph(&self) -> Glyph {
        self.glyph(self.target).flatten()
    }

    /// Heads for `glyph`, one slot on in the direction of `step` (positive:
    /// the value went up and the digits travel up). Does nothing when the
    /// strip already heads for it. A glyph that is already on the next slot is
    /// reused, so a value that goes back where it came from turns the roll
    /// around where it is. Returns whether the target moved.
    fn retarget(&mut self, glyph: Glyph, step: i64) -> bool {
        if self.target_glyph() == glyph {
            return false;
        }
        let step = if step < 0 { -1 } else { 1 };
        let next = self.target + step;
        if self.glyph(next) == Some(glyph) {
            self.target = next;
            return true;
        }
        // The next slot holds something else, so the roll goes past it to
        // a free one: the glyphs in between pass by.
        let free = if step > 0 {
            self.slots.iter().map(|(slot, _)| *slot).max().unwrap_or(0).max(self.target) + 1
        } else {
            self.slots.iter().map(|(slot, _)| *slot).min().unwrap_or(0).min(self.target) - 1
        };
        self.slots.push((free, glyph));
        self.target = free;
        true
    }

    /// Forgets the slots the position has passed and will not come back to.
    fn prune(&mut self, position: f32) {
        let low = position.min(self.target as f32) - 1.0;
        let high = position.max(self.target as f32) + 1.0;
        let target = self.target;
        self.slots
            .retain(|(slot, _)| *slot == target || (*slot as f32 >= low && *slot as f32 <= high));
    }

    /// The glyphs in view with the position at `position`, nearest the rest
    /// place first.
    fn visible(&self, position: f32) -> Vec<Placed> {
        let mut shown: Vec<Placed> = self
            .slots
            .iter()
            .filter_map(|(slot, glyph)| {
                let distance = *slot as f32 - position;
                glyph
                    .filter(|_| distance.abs() < 1.0)
                    .map(|glyph| Placed { glyph, distance })
            })
            .collect();
        shown.sort_by(|a, b| a.distance.abs().total_cmp(&b.distance.abs()));
        shown
    }
}

/// How far from the top of its column a glyph is drawn when it is `distance`
/// lines from rest and a line is `line` px tall. Only digits travel.
fn travel(glyph: char, distance: f32, line: f32) -> f32 {
    if slides(glyph) { distance * line } else { 0.0 }
}

/// The width of the widest digit of `style`: every digit column is this wide.
fn digit_advance(style: TextStyle) -> f32 {
    ('0'..='9')
        .map(|digit| text_width(&digit.to_string(), style))
        .fold(0.0, f32::max)
}

/// The height of one line of `style`: the clip window of a column.
fn line_height(style: TextStyle) -> f32 {
    style.size.px() * style.leading
}

/// A number that rolls to `value`, digit by digit, as the shell's clock does.
///
/// ```ignore
/// animated_number(battery).suffix("%")
/// animated_number(speed).decimals(1).suffix(" MB/s")
/// animated_number(bytes).grouped()
/// ```
///
/// The box is as wide as the text it shows, so a neighbour only moves when
/// the number gains or loses a digit, and then smoothly. Put it in a box of
/// its own width, aligned to the right, when neighbours must not move at all.
#[derive(Clone, PartialEq)]
pub struct AnimatedNumber {
    value: f64,
    decimals: usize,
    min_digits: usize,
    style: TextStyle,
    format: NumberFormat,
    prefix: Option<String>,
    suffix: Option<String>,
    announce: bool,
}

/// An [`AnimatedNumber`] that shows `value`, with the numeric text style.
pub fn animated_number(value: impl Into<f64>) -> AnimatedNumber {
    AnimatedNumber {
        value: value.into(),
        decimals: 0,
        min_digits: 1,
        style: NUMERIC,
        format: NumberFormat::PLAIN,
        prefix: None,
        suffix: None,
        announce: true,
    }
}

impl AnimatedNumber {
    /// Digits shown after the decimal point.
    pub fn decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals;
        self
    }

    /// At least this many integer digits, padded with zeros: `2` writes a
    /// minute as `05`. Together with a fixed text between numbers it makes a
    /// clock.
    pub fn min_digits(mut self, digits: usize) -> Self {
        self.min_digits = digits;
        self
    }

    /// The text style of the number, its unit included. The digits are always
    /// tabular.
    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    /// Separates thousands with the English comma: `12,345`.
    pub fn grouped(self) -> Self {
        self.format(NumberFormat::ENGLISH)
    }

    /// The marks of a [`NumberFormat`].
    pub fn format(mut self, format: NumberFormat) -> Self {
        self.format = format;
        self
    }

    /// The marks of a locale such as `de_DE`; see [`NumberFormat::for_locale`].
    pub fn locale(self, locale: &str) -> Self {
        self.format(NumberFormat::for_locale(locale))
    }

    /// Text before the digits that stays where it is: `$`. It is part of what
    /// assistive technology reads.
    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// A unit after the digits that stays where it is: `%`, ` MB/s`. It is part
    /// of what assistive technology reads.
    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }

    /// Keeps assistive technology quiet when the value changes. Without it
    /// the new value is announced politely, once, when it has changed, and
    /// never frame by frame.
    pub fn silent(mut self) -> Self {
        self.announce = false;
        self
    }
}

impl Component for AnimatedNumber {
    fn render(&self) -> impl IntoElement {
        let style = TextStyle {
            tabular: true,
            ..self.style
        };
        let text = format_number(self.value, self.decimals, self.min_digits, self.format);
        let cells: Vec<char> = text.chars().collect();
        let spoken = format!(
            "{}{}{}",
            self.prefix.as_deref().unwrap_or(""),
            text,
            self.suffix.as_deref().unwrap_or("")
        );

        // The most columns this number has needed: a column that is no longer
        // needed stays, with no width, so it can grow back without mounting.
        let places = use_hook(|| Rc::new(Cell::new(0_usize)));
        places.set(places.get().max(cells.len()));
        // Columns that mount after the first draw appear: they grow in.
        let first = use_hook(|| Rc::new(Cell::new(true)));
        let appear = !first.replace(false);
        // The way the digits travel is the way the value went; it stays
        // while the value stays.
        let last = use_hook(|| Rc::new(Cell::new((self.value, 1_i64))));
        let (before, mut step) = last.get();
        if before != self.value {
            step = if self.value < before { -1 } else { 1 };
            last.set((self.value, step));
        }

        let digit_width = digit_advance(style);
        let line = line_height(style);
        let mut row = rect().horizontal().height(Size::px(line));
        if let Some(prefix) = &self.prefix {
            row = row.child(fixed_text(style, prefix));
        }
        for place in (0..places.get()).rev() {
            let glyph = glyph_at(&cells, place);
            row = row.child(
                DigitColumn {
                    glyph,
                    step,
                    style,
                    digit_width,
                    line,
                    appear,
                    place,
                }
                .into_element(),
            );
        }
        if let Some(suffix) = &self.suffix {
            row = row.child(fixed_text(style, suffix));
        }
        let announce = self.announce;
        let value = spoken.clone();
        row.a11y_role(if announce {
            AccessibilityRole::Status
        } else {
            AccessibilityRole::Label
        })
        .a11y_alt(spoken)
        .a11y_builder(move |node| {
            node.set_value(value);
            if announce {
                node.set_live(Live::Polite);
            }
        })
    }
}

/// A unit or a prefix: still text, hidden from assistive technology because
/// the number itself says it.
fn fixed_text(style: TextStyle, text: &str) -> Rect {
    rect()
        .a11y_builder(|node| node.set_hidden())
        .child(styled_label(style, text))
}

/// One cell of a number: a clipped window one line tall that a [`Strip`]
/// rolls through.
#[derive(Clone, PartialEq)]
struct DigitColumn {
    glyph: Glyph,
    /// Which way a change travels: positive for up.
    step: i64,
    style: TextStyle,
    digit_width: f32,
    line: f32,
    /// Mounted after the number was first drawn: grow in.
    appear: bool,
    /// How many cells from the right: 0 is the units.
    place: usize,
}

impl DigitColumn {
    /// The width this column has when it shows `glyph`.
    fn width_of(&self, glyph: Glyph) -> f32 {
        match glyph {
            None => 0.0,
            Some(glyph) if slides(glyph) => self.digit_width,
            Some(glyph) => text_width(&glyph.to_string(), self.style),
        }
    }
}

impl Component for DigitColumn {
    /// The place is the identity: a column is the same column when the number
    /// gains a digit on its left, and keeps its strip and its motion.
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(self.place as u64)
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ms = tokens.motion(ROLL_MS);
        let strip = use_hook(|| {
            Rc::new(RefCell::new(Strip::at(if self.appear { None } else { self.glyph })))
        });
        strip.borrow_mut().retarget(self.glyph, self.step);
        let target = strip.borrow().target as f32;
        let wanted = self.width_of(self.glyph);

        // Both begin at rest on the first glyph, so only a column that
        // appears (empty at first) has anywhere to go when it mounts.
        let position = use_tween_with(0.0, target, ms, Function::Cubic);
        let width = use_tween_with(if self.appear { 0.0 } else { wanted }, wanted, ms, Function::Cubic);
        let (position, width) = if ms == 0 { (target, wanted) } else { (position, width) };

        let mut strip = strip.borrow_mut();
        strip.prune(position);
        let mut column = rect()
            .width(Size::px(width.max(0.0)))
            .height(Size::px(self.line))
            .tw(&tokens, &[clip()])
            .a11y_builder(|node| node.set_hidden());
        for placed in strip.visible(position) {
            let x = if slides(placed.glyph) {
                (self.digit_width - text_width(&placed.glyph.to_string(), self.style)) / 2.0
            } else {
                0.0
            };
            column = column.child(
                rect()
                    .position(
                        Position::new_absolute()
                            .top(travel(placed.glyph, placed.distance, self.line))
                            .left(x),
                    )
                    .opacity(fade(placed.distance))
                    .child(styled_label(self.style, placed.glyph.to_string())),
            );
        }
        column
    }
}

#[cfg(test)]
mod math {
    use super::*;

    fn visible(strip: &Strip, position: f32) -> Vec<(char, f32)> {
        strip
            .visible(position)
            .into_iter()
            .map(|placed| (placed.glyph, placed.distance))
            .collect()
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn text_is_plain_grouped_padded_and_signed() {
        let plain = NumberFormat::PLAIN;
        assert_eq!(format_number(0.0, 0, 1, plain), "0");
        assert_eq!(format_number(1234.5, 1, 1, plain), "1234.5");
        assert_eq!(format_number(1234.5, 1, 1, NumberFormat::ENGLISH), "1,234.5");
        assert_eq!(format_number(999.0, 0, 1, NumberFormat::ENGLISH), "999");
        assert_eq!(format_number(1_000_000.0, 0, 1, NumberFormat::ENGLISH), "1,000,000");
        assert_eq!(format_number(5.0, 0, 2, plain), "05");
        assert_eq!(format_number(5.25, 2, 3, plain), "005.25");
        // The grouping runs over the digits of the integer part only.
        assert_eq!(format_number(1234.5678, 3, 1, NumberFormat::ENGLISH), "1,234.568");
        // Rounding carries into a new digit.
        assert_eq!(format_number(999.96, 1, 1, NumberFormat::ENGLISH), "1,000.0");
    }

    #[test]
    fn a_sign_shows_only_for_a_value_that_is_not_zero_as_written() {
        let plain = NumberFormat::PLAIN;
        assert_eq!(format_number(-5.0, 0, 1, plain), "\u{2212}5");
        assert_eq!(format_number(-0.4, 0, 1, plain), "0", "rounds to zero: no sign");
        assert_eq!(format_number(-0.0, 1, 1, plain), "0.0");
        assert_eq!(format_number(-0.04, 1, 1, plain), "0.0");
        assert_eq!(format_number(-0.06, 1, 1, plain), "\u{2212}0.1");
        assert_eq!(format_number(-1234.0, 0, 1, NumberFormat::ENGLISH), "\u{2212}1,234");
        assert_eq!(format_number(f64::NAN, 1, 1, plain), "\u{2014}");
        assert_eq!(format_number(f64::INFINITY, 0, 1, plain), "\u{2014}");
    }

    #[test]
    fn locales_pick_their_own_marks() {
        let english = NumberFormat::for_locale("en_GB.UTF-8");
        assert_eq!(format_number(1234567.5, 1, 1, english), "1,234,567.5");
        let german = NumberFormat::for_locale("de_DE");
        assert_eq!(format_number(1234567.5, 1, 1, german), "1.234.567,5");
        let swiss = NumberFormat::for_locale("de-CH");
        assert_eq!(format_number(1234.5, 1, 1, swiss), "1\u{2019}234.5");
        let french = NumberFormat::for_locale("fr_FR");
        assert_eq!(format_number(1234.5, 1, 1, french), "1\u{202f}234,5");
        let russian = NumberFormat::for_locale("ru");
        assert_eq!(format_number(1234.5, 1, 1, russian), "1\u{a0}234,5");
        assert_eq!(NumberFormat::for_locale("xx_YY"), NumberFormat::ENGLISH);
        assert_eq!(NumberFormat::for_locale(""), NumberFormat::ENGLISH);
    }

    #[test]
    fn cells_are_aligned_on_the_right() {
        let cells: Vec<char> = "1,204".chars().collect();
        assert_eq!(glyph_at(&cells, 0), Some('4'));
        assert_eq!(glyph_at(&cells, 3), Some(','));
        assert_eq!(glyph_at(&cells, 4), Some('1'));
        assert_eq!(glyph_at(&cells, 5), None);
        // 99 becomes 100: the units and tens change, the hundreds appear.
        let before: Vec<char> = "99".chars().collect();
        let after: Vec<char> = "100".chars().collect();
        let moved: Vec<(usize, Glyph, Glyph)> = (0..3)
            .map(|place| (place, glyph_at(&before, place), glyph_at(&after, place)))
            .filter(|(_, from, to)| from != to)
            .collect();
        assert_eq!(
            moved,
            vec![
                (0, Some('9'), Some('0')),
                (1, Some('9'), Some('0')),
                (2, None, Some('1')),
            ]
        );
    }

    #[test]
    fn a_strip_at_rest_shows_its_glyph_in_place() {
        let strip = Strip::at(Some('5'));
        assert_eq!(visible(&strip, 0.0), vec![('5', 0.0)]);
        assert_eq!(strip.target_glyph(), Some('5'));
    }

    #[test]
    fn going_up_the_old_digit_leaves_upwards_as_the_new_one_rises() {
        let mut strip = Strip::at(Some('5'));
        assert!(strip.retarget(Some('6'), 1));
        let at = |position: f32| visible(&strip, position);
        assert_eq!(at(0.0), vec![('5', 0.0)], "the new digit is a line below, out of the window");
        let middle = at(0.5);
        assert_eq!(middle.len(), 2);
        assert!(middle.contains(&('5', -0.5)), "the old digit is half a line up: {middle:?}");
        assert!(middle.contains(&('6', 0.5)), "the new digit is half a line below: {middle:?}");
        assert_eq!(at(1.0), vec![('6', 0.0)]);
    }

    #[test]
    fn going_down_the_digits_travel_the_other_way() {
        let mut strip = Strip::at(Some('5'));
        assert!(strip.retarget(Some('4'), -1));
        let middle = visible(&strip, -0.25);
        assert!(middle.contains(&('5', 0.25)), "the old digit sinks: {middle:?}");
        assert!(middle.contains(&('4', -0.75)), "the new digit comes from above: {middle:?}");
        assert_eq!(visible(&strip, -1.0), vec![('4', 0.0)]);
    }

    #[test]
    fn a_wrap_from_nine_to_zero_still_rises() {
        let mut strip = Strip::at(Some('9'));
        strip.retarget(Some('0'), 1);
        let middle = visible(&strip, 0.5);
        assert!(middle.contains(&('9', -0.5)));
        assert!(middle.contains(&('0', 0.5)), "zero comes up from below, not down from above: {middle:?}");
        // 59 to 00 is two such rolls at once, each in its own strip.
        let mut tens = Strip::at(Some('5'));
        tens.retarget(Some('0'), 1);
        assert_eq!(tens.target, 1);
    }

    #[test]
    fn a_digit_that_does_not_change_does_not_move() {
        let mut strip = Strip::at(Some('7'));
        assert!(!strip.retarget(Some('7'), 1));
        assert!(!strip.retarget(Some('7'), -1));
        assert_eq!(strip, Strip::at(Some('7')));
    }

    #[test]
    fn a_new_value_mid_roll_keeps_every_digit_where_it_is() {
        // 5 is rolling to 6 and is 0.4 of the way when 7 arrives.
        let mut strip = Strip::at(Some('5'));
        strip.retarget(Some('6'), 1);
        let before = visible(&strip, 0.4);
        assert!(strip.retarget(Some('7'), 1));
        let after = visible(&strip, 0.4);
        assert_eq!(before, after, "nothing on screen moves when the target does");
        // The roll carries on from there, through six, to seven.
        assert_eq!(strip.target, 2);
        let later = visible(&strip, 1.5);
        assert!(later.contains(&('6', -0.5)) && later.contains(&('7', 0.5)), "{later:?}");
        assert_eq!(visible(&strip, 2.0), vec![('7', 0.0)]);
    }

    #[test]
    fn a_value_that_comes_back_turns_the_roll_around_on_the_spot() {
        let mut strip = Strip::at(Some('5'));
        strip.retarget(Some('6'), 1);
        let before = visible(&strip, 0.7);
        assert!(strip.retarget(Some('5'), -1));
        assert_eq!(strip.target, 0, "the five that is still there is the target: no new slot");
        assert_eq!(strip.slots.len(), 2);
        assert_eq!(visible(&strip, 0.7), before);
        assert_eq!(visible(&strip, 0.0), vec![('5', 0.0)]);
    }

    #[test]
    fn a_value_that_turns_to_something_else_goes_past_what_is_on_screen() {
        let mut strip = Strip::at(Some('5'));
        strip.retarget(Some('6'), 1);
        // Down to a three, though a five is on the slot below: the three
        // gets its own slot beyond it and the roll goes through the five.
        strip.retarget(Some('3'), -1);
        assert_eq!(strip.target, -1);
        assert_eq!(visible(&strip, -1.0), vec![('3', 0.0)]);
        assert_eq!(strip.glyph(0), Some(Some('5')));
    }

    #[test]
    fn rapid_updates_never_pile_up_slots() {
        let mut strip = Strip::at(Some('0'));
        let mut position = 0.0_f32;
        let mut most = 0;
        for count in 1..400_u32 {
            let digit = char::from_digit(count % 10, 10).unwrap();
            strip.retarget(Some(digit), 1);
            // The position closes a third of the way to the target per update.
            position += (strip.target as f32 - position) / 3.0;
            strip.prune(position);
            most = most.max(strip.slots.len());
        }
        assert!(most <= 6, "the strip keeps what is near the position, not the history: {most}");
        // Let it settle: one slot is left, at rest.
        position = strip.target as f32;
        strip.prune(position);
        assert_eq!(visible(&strip, position).len(), 1);
    }

    #[test]
    fn offsets_are_continuous_in_the_position() {
        let mut strip = Strip::at(Some('2'));
        strip.retarget(Some('3'), 1);
        let mut previous = visible(&strip, 0.0);
        for step in 1..=100 {
            let now = visible(&strip, step as f32 / 100.0);
            for (glyph, distance) in &now {
                if let Some((_, before)) = previous.iter().find(|(g, _)| g == glyph) {
                    assert!((distance - before).abs() < 0.02, "{glyph}: {before} to {distance}");
                }
            }
            previous = now;
        }
    }

    #[test]
    fn the_fade_is_the_clocks_cross_fade() {
        assert!(close(fade(0.0), 1.0));
        assert!(close(fade(0.5), 0.5));
        assert!(close(fade(-0.25), 0.75));
        assert!(close(fade(1.0), 0.0));
        assert!(close(fade(3.0), 0.0));
        // The old and new digit always add up to one: nothing blinks.
        for step in 0..=20 {
            let progress = step as f32 / 20.0;
            assert!(close(fade(-progress) + fade(1.0 - progress), 1.0));
        }
    }

    #[test]
    fn only_digits_travel() {
        assert!(close(travel('4', -0.5, 20.0), -10.0));
        assert!(close(travel('4', 1.0, 20.0), 20.0));
        assert!(close(travel(',', -0.5, 20.0), 0.0));
        assert!(close(travel(MINUS, 0.5, 20.0), 0.0));
    }

    #[test]
    fn an_appearing_column_rises_in_from_nothing() {
        let mut strip = Strip::at(None);
        assert!(strip.retarget(Some('1'), 1));
        assert!(visible(&strip, 0.0).is_empty(), "nothing is there at first");
        assert_eq!(visible(&strip, 0.5), vec![('1', 0.5)]);
        assert_eq!(visible(&strip, 1.0), vec![('1', 0.0)]);
    }

    #[test]
    fn a_leaving_column_empties() {
        let mut strip = Strip::at(Some('1'));
        assert!(strip.retarget(None, 1));
        assert_eq!(visible(&strip, 0.5), vec![('1', -0.5)]);
        assert!(visible(&strip, 1.0).is_empty());
        assert_eq!(strip.target_glyph(), None);
    }
}

#[cfg(all(test, feature = "testing"))]
#[path = "animated_number_tests.rs"]
mod frames;
