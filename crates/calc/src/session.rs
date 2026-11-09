//! A calculator session: what the window needs, with no window in it.
//!
//! The application holds one [`Session`]. It asks for a preview as the
//! person types, commits on Enter (which records the history and sets `ans`),
//! and reads back what to show. Everything here is plain data, so the rules
//! (percent, history, memory, word sizes, conversion) are tested without a
//! display.

use num_bigint::BigInt;
use num_traits::Zero;

use crate::error::{CalcError, Result};
use crate::eval::{Angle, Env, Preview, evaluate, preview};
use crate::format::{self, MINUS, Style};
use crate::locale::Locale;
use crate::number::Num;
use crate::programmer::{self, Word};
use crate::rates::Rates;
use crate::store::{Entry, Mode, Saved};
use crate::units::{self, Category};

/// A value ready to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    /// The text on screen: locale marks, a real minus sign, a unit if any.
    pub display: String,
    /// The number alone as plain text, for Copy and the history.
    pub plain: String,
    /// What a screen reader says.
    pub spoken: String,
    /// Digits were cut.
    pub rounded: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// Nothing typed.
    Empty,
    Value(Answer),
    Error(CalcError),
}

impl Outcome {
    pub fn value(&self) -> Option<&Answer> {
        match self {
            Outcome::Value(answer) => Some(answer),
            _ => None,
        }
    }
}

/// A programmer value shown in every base, and as bits.
#[derive(Clone, Debug, PartialEq)]
pub struct IntAnswer {
    pub value: BigInt,
    pub overflow: bool,
    /// The value written in the base of the entry field.
    pub display: String,
    /// `(base, digits)` for 16, 10, 8 and 2.
    pub rows: Vec<(u32, String)>,
    /// The bits, most significant first.
    pub bits: Vec<bool>,
    /// The character this code point names, if it is printable.
    pub character: Option<char>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum IntOutcome {
    Empty,
    Value(IntAnswer),
    Error(CalcError),
}

/// Where the exchange rates in use came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RatesOrigin {
    /// The snapshot built into the program.
    Bundled,
    /// A download kept from an earlier run.
    Cached,
    /// Downloaded in this run.
    Fresh,
}

pub struct Session {
    pub saved: Saved,
    pub env: Env,
    pub locale: Locale,
    pub rates: Rates,
    pub rates_origin: RatesOrigin,
    int_ans: BigInt,
}

fn plain_number(text: &str) -> Num {
    let negative = text.starts_with('-');
    let body = text.trim_start_matches('-');
    let (mantissa, exp) = match body.split_once('e') {
        Some((m, e)) => (m, e.parse::<i64>().unwrap_or(0)),
        None => (body, 0),
    };
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    Num::from_digits(negative, &format!("{int}{frac}"), frac.len(), exp)
        .unwrap_or_else(|_| Num::zero())
}

impl Session {
    pub fn new(saved: Saved, rates: Rates, rates_origin: RatesOrigin, locale: Locale) -> Session {
        let mut env = Env::new();
        env.angle = saved.settings.angle;
        env.memory = plain_number(&saved.settings.memory);
        env.ans = saved
            .history
            .iter()
            .rev()
            .find(|e| e.mode.family() == 0)
            .map_or_else(Num::zero, |e| plain_number(&e.result));
        let int_ans = saved
            .history
            .iter()
            .rev()
            .find(|e| e.mode == Mode::Programmer)
            .and_then(|e| e.result.parse::<BigInt>().ok())
            .unwrap_or_else(BigInt::zero);
        Session {
            saved,
            env,
            locale,
            rates,
            rates_origin,
            int_ans,
        }
    }

    pub fn mode(&self) -> Mode {
        self.saved.settings.mode
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.saved.settings.mode = mode;
    }

    pub fn style(&self) -> Style {
        Style {
            locale: self.locale,
            precision: self.saved.settings.precision,
            group: self.saved.settings.group,
        }
    }

    pub fn angle(&self) -> Angle {
        self.env.angle
    }

    pub fn set_angle(&mut self, angle: Angle) {
        self.env.angle = angle;
        self.saved.settings.angle = angle;
    }

    pub fn set_precision(&mut self, digits: usize) {
        self.saved.settings.precision = digits.clamp(6, 50);
    }

    pub fn set_group(&mut self, group: bool) {
        self.saved.settings.group = group;
    }

    pub fn word(&self) -> Word {
        Word::new(self.saved.settings.bits, self.saved.settings.signed)
    }

    pub fn base(&self) -> u32 {
        self.saved.settings.base
    }

    fn answer(&self, n: &Num) -> Answer {
        let shown = format::show(n, &self.style());
        Answer {
            spoken: shown.text.replace(MINUS, "minus "),
            plain: format::plain(n, self.saved.settings.precision.max(16)),
            display: shown.text,
            rounded: shown.rounded,
        }
    }

    /// `ans`, the last result of Basic or Scientific.
    pub fn last_answer(&self) -> Answer {
        self.answer(&self.env.ans)
    }

    // ---- Basic and Scientific ----------------------------------------------

    /// The answer to `text` and, for a plain calculation, its value. A typed
    /// conversion has no value of its own to become `ans`.
    fn evaluate_text(&mut self, text: &str) -> Result<(Answer, Option<Num>)> {
        if let Some(typed) = units::parse_typed(text) {
            let value = evaluate(&typed.expression, &mut self.env, &self.locale)?;
            let rates = Some(&self.rates);
            let converted =
                units::convert(&typed.category, typed.from.id, typed.to.id, &value, rates)?;
            let mut answer = self.answer(&converted);
            answer.display = format!("{} {}", answer.display, typed.to.symbol);
            answer.spoken = format!("{} {}", answer.spoken, typed.to.name);
            return Ok((answer, None));
        }
        let value = evaluate(text, &mut self.env, &self.locale)?;
        Ok((self.answer(&value), Some(value)))
    }

    /// What `text` comes to so far.
    pub fn preview(&mut self, text: &str) -> Outcome {
        if units::parse_typed(text).is_some() {
            return match self.evaluate_text(text) {
                Ok((answer, _)) => Outcome::Value(answer),
                Err(error) => Outcome::Error(error),
            };
        }
        match preview(text, &mut self.env, &self.locale) {
            Preview::Empty => Outcome::Empty,
            Preview::Value(n) => Outcome::Value(self.answer(&n)),
            Preview::Error(error) => Outcome::Error(error),
        }
    }

    /// Finishes `text`: records it and makes the result `ans`. An expression
    /// that cannot be finished changes nothing.
    pub fn equals(&mut self, text: &str, now: u64) -> Outcome {
        let text = text.trim();
        if text.is_empty() {
            return Outcome::Empty;
        }
        let (answer, value) = match self.evaluate_text(text) {
            Ok(done) => done,
            Err(error) => return Outcome::Error(error),
        };
        let converted = value.is_none();
        if let Some(value) = value {
            self.env.ans = value;
        }
        let mode = if self.mode() == Mode::Scientific {
            Mode::Scientific
        } else {
            Mode::Basic
        };
        self.saved.push(Entry {
            expr: text.to_string(),
            result: if converted {
                answer.display.clone()
            } else {
                answer.plain.clone()
            },
            mode,
            at: now,
            base: 0,
            bits: 0,
            signed: false,
        });
        Outcome::Value(answer)
    }

    // ---- memory ------------------------------------------------------------

    pub fn memory(&self) -> Answer {
        self.answer(&self.env.memory)
    }

    pub fn has_memory(&self) -> bool {
        !self.env.memory.is_zero()
    }

    fn store_memory(&mut self, memory: Num) {
        self.saved.settings.memory = format::plain(&memory, 60);
        self.env.memory = memory;
    }

    pub fn memory_clear(&mut self) {
        self.store_memory(Num::zero());
    }

    /// Adds `delta` to the memory; `None` leaves it as it is.
    pub fn memory_add(&mut self, delta: Option<&Answer>, subtract: bool) {
        let Some(delta) = delta else { return };
        let value = plain_number(&delta.plain);
        let next = if subtract {
            self.env.memory.sub(&value)
        } else {
            self.env.memory.add(&value)
        };
        if let Ok(next) = next {
            self.store_memory(next);
        }
    }

    // ---- Programmer --------------------------------------------------------

    fn int_answer(&self, value: BigInt, overflow: bool) -> IntAnswer {
        let word = self.word();
        let base = self.base();
        let group = self.saved.settings.group;
        IntAnswer {
            display: programmer::format_int(&value, word, base, group, &self.locale),
            rows: [16, 10, 8, 2]
                .iter()
                .map(|b| {
                    (
                        *b,
                        programmer::format_int(&value, word, *b, true, &self.locale),
                    )
                })
                .collect(),
            bits: programmer::bits_of(&value, word),
            character: programmer::character(&value),
            overflow,
            value,
        }
    }

    pub fn int_value(&self, value: &BigInt) -> IntAnswer {
        self.int_answer(self.word().wrap(value), false)
    }

    pub fn int_preview(&self, text: &str) -> IntOutcome {
        match programmer::preview_int(text, self.base(), self.word(), &self.int_ans) {
            Ok(None) => IntOutcome::Empty,
            Ok(Some(v)) => IntOutcome::Value(self.int_answer(v.value, v.overflow)),
            Err(error) => IntOutcome::Error(error),
        }
    }

    pub fn int_equals(&mut self, text: &str, now: u64) -> IntOutcome {
        let text = text.trim();
        if text.is_empty() {
            return IntOutcome::Empty;
        }
        match programmer::evaluate_int(text, self.base(), self.word(), &self.int_ans) {
            Ok(v) => {
                self.int_ans = v.value.clone();
                self.saved.push(Entry {
                    expr: text.to_string(),
                    result: v.value.to_string(),
                    mode: Mode::Programmer,
                    at: now,
                    base: self.base(),
                    bits: self.word().bits,
                    signed: self.word().signed,
                });
                IntOutcome::Value(self.int_answer(v.value, v.overflow))
            }
            Err(error) => IntOutcome::Error(error),
        }
    }

    pub fn last_int(&self) -> IntAnswer {
        self.int_answer(self.int_ans.clone(), false)
    }

    /// Makes `value` the programmer's current value (a bit was flipped).
    pub fn set_int(&mut self, value: BigInt) {
        self.int_ans = self.word().wrap(&value);
    }

    /// A different word size or signedness: the value keeps its bits.
    pub fn set_word(&mut self, bits: u32, signed: bool) {
        let from = self.word();
        let to = Word::new(bits, signed);
        self.int_ans = programmer::convert_word(&self.int_ans, from, to);
        self.saved.settings.bits = to.bits;
        self.saved.settings.signed = to.signed;
    }

    pub fn set_base(&mut self, base: u32) {
        self.saved.settings.base = match base {
            2 | 8 | 16 => base,
            _ => 10,
        };
    }

    // ---- Convert -----------------------------------------------------------

    pub fn category(&self) -> Category {
        units::category(&self.saved.settings.category).unwrap_or_else(|| units::categories()[0])
    }

    pub fn set_category(&mut self, id: &str) {
        if let Some(category) = units::category(id) {
            self.saved.settings.category = category.id.to_string();
            self.saved.settings.from = category.from.to_string();
            self.saved.settings.to = category.to.to_string();
        }
    }

    pub fn set_units(&mut self, from: &str, to: &str) {
        let category = self.category();
        if category.unit(from).is_some() && category.unit(to).is_some() {
            self.saved.settings.from = from.to_string();
            self.saved.settings.to = to.to_string();
        }
    }

    pub fn swap_units(&mut self) {
        std::mem::swap(&mut self.saved.settings.from, &mut self.saved.settings.to);
    }

    fn convert_text(&mut self, text: &str) -> Result<Answer> {
        let category = self.category();
        let (from, to) = (
            self.saved.settings.from.clone(),
            self.saved.settings.to.clone(),
        );
        let to_unit = category
            .unit(&to)
            .ok_or_else(|| CalcError::Unit("That unit is not known.".into()))?;
        let value = match preview(text, &mut self.env, &self.locale) {
            Preview::Value(n) => n,
            Preview::Empty => return Err(CalcError::Incomplete),
            Preview::Error(error) => return Err(error),
        };
        let converted = units::convert(&category, &from, &to, &value, Some(&self.rates))?;
        let mut answer = self.answer(&converted);
        answer.display = format!("{} {}", answer.display, to_unit.symbol);
        answer.spoken = format!("{} {}", answer.spoken, to_unit.name);
        Ok(answer)
    }

    pub fn convert_preview(&mut self, text: &str) -> Outcome {
        match self.convert_text(text) {
            Ok(answer) => Outcome::Value(answer),
            Err(CalcError::Incomplete) => Outcome::Empty,
            Err(error) => Outcome::Error(error),
        }
    }

    pub fn convert_equals(&mut self, text: &str, now: u64) -> Outcome {
        let text = text.trim();
        if text.is_empty() {
            return Outcome::Empty;
        }
        match self.convert_text(text) {
            Ok(answer) => {
                let category = self.category();
                let name = |id: &str| {
                    category
                        .unit(id)
                        .map_or(id.to_string(), |u| u.symbol.to_string())
                };
                self.saved.push(Entry {
                    expr: format!(
                        "{text} {} to {}",
                        name(&self.saved.settings.from),
                        name(&self.saved.settings.to)
                    ),
                    result: answer.display.clone(),
                    mode: Mode::Convert,
                    at: now,
                    base: 0,
                    bits: 0,
                    signed: false,
                });
                Outcome::Value(answer)
            }
            Err(error) => Outcome::Error(error),
        }
    }

    /// The rates in use, as a sentence: "Rates from 6 Nov 2026 (European
    /// Central Bank)".
    pub fn rates_note(&self) -> String {
        let date = pretty_date(&self.rates.date);
        match self.rates_origin {
            RatesOrigin::Fresh => format!("Rates from {date}, European Central Bank"),
            RatesOrigin::Cached => format!(
                "Rates from {date}, European Central Bank. Offline: showing the last download."
            ),
            RatesOrigin::Bundled => {
                format!("Rates from {date}, built into this app. They update when you are online.")
            }
        }
    }

    pub fn set_rates(&mut self, rates: Rates, origin: RatesOrigin) {
        self.rates = rates;
        self.rates_origin = origin;
    }

    // ---- History -----------------------------------------------------------

    /// The entries of the current mode's family, newest first, with their
    /// index in the saved list.
    pub fn history(&self) -> Vec<(usize, &Entry)> {
        let family = self.mode().family();
        self.saved
            .history
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, e)| e.mode.family() == family)
            .collect()
    }

    pub fn remove_history(&mut self, index: usize) {
        if index < self.saved.history.len() {
            self.saved.history.remove(index);
        }
    }

    /// Clears the history of the current mode's family.
    pub fn clear_history(&mut self) {
        let family = self.mode().family();
        self.saved.history.retain(|e| e.mode.family() != family);
    }

    /// How an entry reads in the list: the expression made readable, and the
    /// result as it was shown.
    pub fn describe(&self, entry: &Entry) -> (String, String) {
        let group = self.saved.settings.group;
        match entry.mode {
            Mode::Programmer => {
                let word = Word::new(entry.bits.max(8), entry.signed);
                let result = entry.result.parse::<BigInt>().map_or_else(
                    |_| entry.result.clone(),
                    |v| programmer::format_int(&v, word, entry.base.max(2), group, &self.locale),
                );
                (entry.expr.clone(), result)
            }
            Mode::Convert => (entry.expr.clone(), entry.result.clone()),
            _ => {
                let shown = if entry.result.contains(' ') {
                    entry.result.clone()
                } else {
                    format::show(&plain_number(&entry.result), &self.style()).text
                };
                (
                    format::pretty_expression(&entry.expr, &self.locale, group),
                    shown,
                )
            }
        }
    }
}

/// `2026-11-06` as `6 Nov 2026`; the text unchanged when it is not a date.
pub fn pretty_date(date: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let parts: Vec<&str> = date.split('-').collect();
    if let [year, month, day] = parts.as_slice() {
        if let (Ok(month), Ok(day)) = (month.parse::<usize>(), day.parse::<u32>()) {
            if (1..=12).contains(&month) {
                return format!("{day} {} {year}", MONTHS[month - 1]);
            }
        }
    }
    date.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session::new(
            Saved::default(),
            Rates::bundled(),
            RatesOrigin::Bundled,
            Locale::EN,
        )
    }

    fn shown(outcome: Outcome) -> String {
        match outcome {
            Outcome::Value(a) => a.display,
            Outcome::Empty => "empty".into(),
            Outcome::Error(e) => format!("error: {e}"),
        }
    }

    #[test]
    fn equals_records_history_and_sets_ans() {
        let mut s = session();
        assert_eq!(shown(s.equals("1234 * 5", 10)), "6,170");
        assert_eq!(s.saved.history.len(), 1);
        assert_eq!(s.saved.history[0].result, "6170");
        assert_eq!(shown(s.preview("ans + 1")), "6,171");
        assert_eq!(s.last_answer().display, "6,170");
    }

    #[test]
    fn a_failed_equals_changes_nothing() {
        let mut s = session();
        s.equals("2+2", 1);
        assert!(shown(s.equals("5/0", 2)).starts_with("error: Cannot divide"));
        assert!(shown(s.equals("5+", 2)).starts_with("error"));
        assert_eq!(s.saved.history.len(), 1);
        assert_eq!(s.last_answer().display, "4");
        assert_eq!(shown(s.equals("   ", 3)), "empty");
    }

    #[test]
    fn history_survives_the_session() {
        let mut s = session();
        s.equals("0.1+0.2", 1);
        s.equals("2^10", 2);
        let again = Session::new(
            s.saved.clone(),
            Rates::bundled(),
            RatesOrigin::Cached,
            Locale::EN,
        );
        assert_eq!(again.history().len(), 2);
        assert_eq!(again.last_answer().display, "1,024");
        let (expr, result) = again.describe(again.history()[1].1);
        assert_eq!((expr.as_str(), result.as_str()), ("0.1 + 0.2", "0.3"));
    }

    #[test]
    fn typed_conversions_in_the_expression_field() {
        let mut s = session();
        assert_eq!(shown(s.preview("5 km to mi")), "3.10685596118667 mi");
        assert_eq!(shown(s.preview("100 degC in degF")), "212 \u{b0}F");
        assert_eq!(shown(s.preview("(2+3)*2 kg to g")), "10,000 g");
        assert!(shown(s.equals("1 kg to lb", 1)).ends_with("lb"));
        assert_eq!(s.saved.history.len(), 1);
        // A conversion does not become `ans`.
        assert_eq!(s.last_answer().display, "0");
    }

    #[test]
    fn typed_currency_uses_the_rates() {
        let mut s = session();
        let mut rates = Rates::empty();
        rates.set("USD", "1.25");
        rates.set("GBP", "0.8");
        rates.date = "2026-11-06".into();
        s.set_rates(rates, RatesOrigin::Fresh);
        assert_eq!(shown(s.preview("100 usd to gbp")), "64 GBP");
        s.set_rates(Rates::empty(), RatesOrigin::Bundled);
        assert!(shown(s.preview("100 usd to gbp")).starts_with("error: No exchange rate for USD"));
    }

    #[test]
    fn memory_adds_subtracts_and_clears() {
        let mut s = session();
        assert!(!s.has_memory());
        let five = s.equals("2+3", 1);
        s.memory_add(five.value(), false);
        s.memory_add(five.value(), false);
        assert_eq!(s.memory().display, "10");
        assert!(s.has_memory());
        s.memory_add(five.value(), true);
        assert_eq!(s.memory().display, "5");
        assert_eq!(shown(s.preview("mem * 2")), "10");
        s.memory_clear();
        assert!(!s.has_memory());
        // The memory is part of what is saved.
        s.memory_add(five.value(), false);
        let again = Session::new(
            s.saved.clone(),
            Rates::bundled(),
            RatesOrigin::Cached,
            Locale::EN,
        );
        assert_eq!(again.memory().display, "5");
    }

    #[test]
    fn the_angle_unit_is_kept() {
        let mut s = session();
        assert_eq!(shown(s.preview("sin(30)")), "0.5");
        s.set_angle(Angle::Radians);
        assert_eq!(shown(s.preview("sin(pi/6)")), "0.5");
        let again = Session::new(
            s.saved.clone(),
            Rates::bundled(),
            RatesOrigin::Cached,
            Locale::EN,
        );
        assert_eq!(again.angle(), Angle::Radians);
    }

    #[test]
    fn the_precision_and_grouping_follow_the_settings() {
        let mut s = session();
        assert_eq!(shown(s.preview("1/3")), "0.3333333333333333");
        s.set_precision(8);
        assert_eq!(shown(s.preview("1/3")), "0.33333333");
        s.set_group(false);
        assert_eq!(shown(s.preview("1234567")), "1234567");
        s.set_precision(500);
        assert_eq!(s.saved.settings.precision, 50);
    }

    #[test]
    fn the_locale_shapes_the_answer() {
        let mut s = Session::new(
            Saved::default(),
            Rates::bundled(),
            RatesOrigin::Bundled,
            Locale::from_tag("de_DE"),
        );
        assert_eq!(shown(s.preview("1234,5*2")), "2.469");
        let history = {
            s.equals("1234,5*2", 1);
            s.describe(&s.saved.history[0].clone())
        };
        assert_eq!(
            history,
            ("1.234,5 \u{d7} 2".to_string(), "2.469".to_string())
        );
    }

    #[test]
    fn programmer_values_wrap_and_show_in_every_base() {
        let mut s = session();
        s.set_word(8, false);
        match s.int_equals("255+1", 1) {
            IntOutcome::Value(v) => {
                assert_eq!(v.value, BigInt::from(0));
                assert!(v.overflow);
                assert_eq!(v.rows[0], (16, "00".to_string()));
                assert_eq!(v.bits, vec![false; 8]);
            }
            other => panic!("{other:?}"),
        }
        match s.int_preview("0xAB") {
            IntOutcome::Value(v) => {
                assert_eq!(v.rows[0].1, "AB");
                assert_eq!(v.rows[1].1, "171");
                assert_eq!(v.rows[2].1, "253");
                assert_eq!(v.rows[3].1, "1010 1011");
                assert_eq!(v.character, Some('\u{ab}'));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(s.saved.history.len(), 1);
        assert_eq!(s.saved.history[0].bits, 8);
    }

    #[test]
    fn the_entry_base_decides_how_digits_are_read() {
        let mut s = session();
        s.set_base(16);
        s.set_word(32, false);
        match s.int_preview("ff + 1") {
            IntOutcome::Value(v) => {
                assert_eq!(v.value, BigInt::from(256));
                assert_eq!(v.display, "0000 0100");
            }
            other => panic!("{other:?}"),
        }
        s.set_base(10);
        assert!(matches!(s.int_preview("ff"), IntOutcome::Error(_)));
    }

    #[test]
    fn changing_the_word_keeps_the_value_it_can() {
        let mut s = session();
        s.set_word(16, true);
        s.int_equals("-2", 1);
        s.set_word(32, true);
        assert_eq!(s.last_int().value, BigInt::from(-2));
        s.set_word(8, false);
        assert_eq!(s.last_int().value, BigInt::from(254));
        assert_eq!(
            s.int_preview("ans + 1").clone(),
            IntOutcome::Value(s.int_value(&BigInt::from(255)))
        );
    }

    #[test]
    fn programmer_history_keeps_its_base_and_word() {
        let mut s = session();
        s.set_mode(Mode::Programmer);
        s.set_base(16);
        s.set_word(16, false);
        s.int_equals("ff * 2", 1);
        let (expr, result) = s.describe(&s.saved.history[0].clone());
        assert_eq!(expr, "ff * 2");
        assert_eq!(result, "01FE");
    }

    #[test]
    fn conversion_mode_converts_and_records() {
        let mut s = session();
        s.set_category("temperature");
        assert_eq!(shown(s.convert_preview("100")), "212 \u{b0}F");
        assert_eq!(shown(s.convert_preview("")), "empty");
        assert_eq!(shown(s.convert_preview("5 +")), "41 \u{b0}F");
        s.swap_units();
        assert_eq!(shown(s.convert_preview("212")), "100 \u{b0}C");
        s.convert_equals("212", 5);
        assert_eq!(s.saved.history[0].expr, "212 \u{b0}F to \u{b0}C");
        s.set_mode(Mode::Convert);
        assert_eq!(s.history().len(), 1);
        s.set_mode(Mode::Basic);
        assert_eq!(s.history().len(), 0);
    }

    #[test]
    fn changing_the_category_picks_its_default_units() {
        let mut s = session();
        s.set_category("mass");
        assert_eq!(
            (s.saved.settings.from.as_str(), s.saved.settings.to.as_str()),
            ("kg", "lb")
        );
        s.set_units("g", "oz");
        assert_eq!(shown(s.convert_preview("28.349523125")), "1 oz");
        s.set_units("nonsense", "oz");
        assert_eq!(s.saved.settings.from, "g");
        s.set_category("nonsense");
        assert_eq!(s.saved.settings.category, "mass");
    }

    #[test]
    fn history_is_per_family_and_can_be_edited() {
        let mut s = session();
        s.equals("1+1", 1);
        s.equals("2+2", 2);
        s.set_mode(Mode::Scientific);
        s.equals("3+3", 3);
        assert_eq!(s.history().len(), 3);
        assert_eq!(s.history()[0].1.expr, "3+3");
        let index = s.history()[1].0;
        s.remove_history(index);
        assert_eq!(s.history().len(), 2);
        s.set_mode(Mode::Programmer);
        s.int_equals("1", 4);
        s.set_mode(Mode::Basic);
        s.clear_history();
        assert!(s.history().is_empty());
        s.set_mode(Mode::Programmer);
        assert_eq!(s.history().len(), 1);
    }

    #[test]
    fn rates_notes_say_where_the_rates_are_from() {
        let mut s = session();
        assert!(s.rates_note().contains("6 Nov 2026"));
        assert!(s.rates_note().contains("built into this app"));
        s.rates_origin = RatesOrigin::Cached;
        assert!(s.rates_note().contains("Offline"));
        s.rates_origin = RatesOrigin::Fresh;
        assert!(!s.rates_note().contains("Offline"));
        assert_eq!(pretty_date("2026-01-09"), "9 Jan 2026");
        assert_eq!(pretty_date("soon"), "soon");
    }

    #[test]
    fn the_spoken_form_says_minus() {
        let mut s = session();
        let v = s.equals("-1234.5", 1);
        assert_eq!(v.value().unwrap().spoken, "minus 1,234.5");
        assert_eq!(v.value().unwrap().plain, "-1234.5");
    }
}
