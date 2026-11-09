//! The calculator the window shows: the session, and the state the controls
//! read and write.
//!
//! The rules (what an expression comes to, what the history holds) are in
//! `bingux_calc::session::Session`; this file is the glue between that and
//! the window's states, and the one place an action (a key, Enter, Escape)
//! changes them. Every control calls a method here, so a key on the keypad
//! and the same key on the keyboard do the same thing.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::OnceLock;

use bingux_calc::programmer;
use bingux_calc::rates::Rates;
use bingux_calc::session::{Answer, IntOutcome, Outcome, RatesOrigin, Session};
use bingux_calc::store::{self, Entry, Mode, Saved};
use bingux_calc::{Angle, Locale, format};
use bingux_ui::controls::FieldEdit;
use bingux_ui::runtime::background_then;
use freya::prelude::*;
use num_bigint::BigInt;

/// What the window starts with: read from the disk before it opens.
#[derive(Clone)]
pub struct Initial {
    pub saved: Saved,
    pub rates: Rates,
    pub origin: RatesOrigin,
    pub locale: Locale,
}

pub static INITIAL: OnceLock<Initial> = OnceLock::new();

impl Initial {
    /// The saved state, the cached rates (or the ones built in) and the
    /// locale of the environment.
    pub fn load() -> Initial {
        let saved = store::state_path()
            .map(|path| store::load_saved(&path))
            .unwrap_or_default();
        let cached = store::rates_path().and_then(|path| store::load_rates(&path));
        let (rates, origin) = match cached {
            Some(rates) => (rates, RatesOrigin::Cached),
            None => (Rates::bundled(), RatesOrigin::Bundled),
        };
        Initial {
            saved,
            rates,
            origin,
            locale: Locale::from_env(),
        }
    }

    /// A fresh state with no disk behind it, for a screenshot.
    pub fn clean() -> Initial {
        Initial {
            saved: Saved::default(),
            rates: Rates::bundled(),
            origin: RatesOrigin::Bundled,
            locale: Locale::EN,
        }
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// What the big readout shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub text: String,
    /// Plain text for Copy.
    pub copy: String,
    pub spoken: String,
    /// What the readout would say is wrong with the text, if anything.
    pub note: Option<String>,
    /// The text is a preview of what is typed, not the last answer.
    pub live: bool,
}

impl Shown {
    fn of(answer: &Answer, live: bool) -> Shown {
        Shown {
            text: answer.display.clone(),
            copy: answer.plain.clone(),
            spoken: answer.spoken.clone(),
            note: None,
            live,
        }
    }
}

/// One calculator: a session and the states that drive its window.
#[derive(Clone)]
pub struct Calc {
    session: Rc<RefCell<Session>>,
    /// Bumped whenever the session changes, so what reads it draws again.
    pub tick: State<u64>,
    /// What is typed.
    pub field: State<String>,
    /// Edits for the field from the keypad.
    pub edits: State<Option<FieldEdit>>,
    pub field_id: AccessibilityId,
    /// The expression that was finished last, shown above its answer until
    /// something is typed.
    pub echo: State<Option<String>>,
    /// Why the last Enter did nothing.
    pub error: State<Option<String>>,
    /// The second row of scientific functions is showing.
    pub second: State<bool>,
    /// Exchange rates are being downloaded.
    pub fetching: State<bool>,
    /// The history is showing in place of the keypad (a narrow window).
    pub history_page: State<bool>,
}

impl PartialEq for Calc {
    fn eq(&self, other: &Calc) -> bool {
        Rc::ptr_eq(&self.session, &other.session)
    }
}

/// The characters that start a continuation of the last answer.
fn is_operator_start(text: &str) -> bool {
    text.chars().next().is_some_and(|c| {
        matches!(
            c,
            '+' | '-' | '\u{2212}' | '*' | '/' | '\u{d7}' | '\u{f7}' | '^' | 'x' | 'X'
        )
    }) && !text.starts_with("x(")
}

impl Calc {
    /// Makes the calculator. Call it from a component's `render`, once.
    pub fn use_new() -> Calc {
        let session = use_hook(|| {
            let init = INITIAL.get().cloned().unwrap_or_else(Initial::clean);
            Rc::new(RefCell::new(Session::new(
                init.saved,
                init.rates,
                init.origin,
                init.locale,
            )))
        });
        let calc = Calc {
            session,
            tick: use_state(|| 0),
            field: use_state(String::new),
            edits: use_state(|| None),
            field_id: use_a11y(),
            echo: use_state(|| None),
            error: use_state(|| None),
            second: use_state(|| false),
            fetching: use_state(|| false),
            history_page: use_state(|| false),
        };
        // Exchange rates are fetched once a half day, in the background.
        let fetcher = calc.clone();
        use_hook(move || {
            if fetcher.with(|s| s.rates.is_stale(now())) {
                fetcher.refresh_rates();
            }
        });
        calc
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut Session) -> R) -> R {
        f(&mut self.session.borrow_mut())
    }

    /// Reading this in `render` subscribes the component to every change of
    /// the session.
    pub fn watch(&self) {
        let _ = self.tick.read();
    }

    /// The session changed: draw again and save.
    pub fn changed(&self) {
        let mut tick = self.tick;
        let next = *tick.peek() + 1;
        tick.set(next);
        self.save();
    }

    fn save(&self) {
        let saved = self.with(|s| s.saved.clone());
        if let Some(path) = store::state_path() {
            background_then(move || store::save_saved(&path, &saved), |_| {});
        }
    }

    pub fn mode(&self) -> Mode {
        self.watch();
        self.with(|s| s.mode())
    }

    pub fn set_mode(&self, mode: Mode) {
        if self.with(|s| s.mode()) == mode {
            return;
        }
        self.with(|s| s.set_mode(mode));
        let (mut error, mut echo) = (self.error, self.echo);
        error.set(None);
        echo.set(None);
        // Text typed for one mode does not mean the same in another.
        self.set_field("");
        let mut page = self.history_page;
        page.set(false);
        self.changed();
    }

    // ---- the field ---------------------------------------------------------

    fn send(&self, edit: FieldEdit) {
        let mut edits = self.edits;
        edits.set(Some(edit));
    }

    /// Replaces the whole text and puts the caret at its end.
    pub fn set_field(&self, text: &str) {
        let mut field = self.field;
        field.set(text.to_string());
        self.send(FieldEdit::SelectRange {
            start: text.chars().count(),
            end: text.chars().count(),
        });
        let mut error = self.error;
        error.set(None);
    }

    /// Types `text` at the caret, as the keyboard would.
    pub fn type_text(&self, text: &str) {
        self.clear_error();
        // After an answer an operator carries on from it; anything else
        // starts a new entry.
        if self.field.peek().is_empty() && !is_operator_start(text) {
            let mut echo = self.echo;
            echo.set(None);
        }
        self.send(FieldEdit::Type(text.to_string()));
    }

    pub fn backspace(&self) {
        self.clear_error();
        self.send(FieldEdit::Backspace);
    }

    fn clear_error(&self) {
        let mut error = self.error;
        if error.peek().is_some() {
            error.set(None);
        }
    }

    /// The field was edited by the person.
    pub fn edited(&self, text: &str) {
        self.clear_error();
        if !text.is_empty() && !is_operator_start(text.trim_start()) {
            let mut echo = self.echo;
            if echo.peek().is_some() {
                echo.set(None);
            }
        }
    }

    /// Escape and C: clears what is typed; with nothing typed, clears the
    /// answer too.
    pub fn clear(&self) {
        if self.field.peek().is_empty() {
            self.all_clear();
        } else {
            self.set_field("");
        }
    }

    pub fn all_clear(&self) {
        let (mut echo, mut error) = (self.echo, self.error);
        echo.set(None);
        error.set(None);
        self.set_field("");
        self.with(|s| {
            s.env.ans = bingux_calc::Num::zero();
        });
        self.changed();
    }

    pub fn can_clear_entry(&self) -> bool {
        !self.field.read().is_empty()
    }

    /// Changes the sign of the whole entry: `5` becomes `-5`, `-5` becomes
    /// `5`, and `2+3` becomes `-(2+3)`.
    pub fn negate(&self) {
        let text = self.field.peek().trim().to_string();
        let next = if text.is_empty() {
            "-".to_string()
        } else if let Some(rest) = text
            .strip_prefix('-')
            .or_else(|| text.strip_prefix('\u{2212}'))
        {
            let rest = rest.trim();
            if rest.starts_with('(') && rest.ends_with(')') && balanced_inner(rest) {
                rest[1..rest.len() - 1].to_string()
            } else if rest
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
            {
                rest.to_string()
            } else {
                format!("-({rest})")
            }
        } else if text
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
        {
            format!("-{text}")
        } else {
            format!("-({text})")
        };
        self.set_field(&next);
    }

    /// The text to evaluate: after an answer, a leading operator carries on
    /// from `ans`.
    pub fn effective(&self, text: &str) -> String {
        let continuing = self.echo.peek().is_some();
        let trimmed = text.trim_start();
        if continuing && is_operator_start(trimmed) && self.with(|s| s.mode()) != Mode::Programmer {
            let ans = self.with(|s| s.last_answer().plain);
            if ans.starts_with('-') {
                format!("({ans}){trimmed}")
            } else {
                format!("{ans}{trimmed}")
            }
        } else {
            text.to_string()
        }
    }

    // ---- Enter -------------------------------------------------------------

    /// Finishes the entry.
    pub fn equals(&self) {
        let text = self.field.peek().clone();
        let mode = self.with(|s| s.mode());
        let (mut echo, mut error) = (self.echo, self.error);
        match mode {
            Mode::Basic | Mode::Scientific => {
                let effective = self.effective(&text);
                let outcome = self.with(|s| s.equals(&effective, now()));
                self.finish(outcome, &effective);
            }
            Mode::Convert => {
                let outcome = self.with(|s| s.convert_equals(text.trim(), now()));
                match outcome {
                    Outcome::Value(_) => {
                        let shown = text.trim().to_string();
                        echo.set(Some(shown));
                        error.set(None);
                        self.changed();
                    }
                    Outcome::Error(e) => error.set(Some(e.to_string())),
                    Outcome::Empty => {}
                }
            }
            Mode::Programmer => {
                let effective =
                    if self.echo.peek().is_some() && is_operator_start(text.trim_start()) {
                        format!("ans{}", text.trim_start())
                    } else {
                        text.clone()
                    };
                match self.with(|s| s.int_equals(&effective, now())) {
                    IntOutcome::Value(_) => {
                        echo.set(Some(effective.trim().to_string()));
                        error.set(None);
                        self.set_field("");
                        self.changed();
                    }
                    IntOutcome::Error(e) => error.set(Some(e.to_string())),
                    IntOutcome::Empty => {}
                }
            }
        }
    }

    fn finish(&self, outcome: Outcome, expression: &str) {
        let (mut echo, mut error) = (self.echo, self.error);
        match outcome {
            Outcome::Value(_) => {
                let (locale, group) = self.with(|s| (s.locale, s.saved.settings.group));
                echo.set(Some(bingux_calc::format::pretty_expression(
                    expression, &locale, group,
                )));
                error.set(None);
                self.set_field("");
                self.changed();
            }
            Outcome::Error(e) => error.set(Some(e.to_string())),
            Outcome::Empty => {}
        }
    }

    // ---- what is shown -----------------------------------------------------

    /// The readout of Basic and Scientific: the live value of what is typed,
    /// or the last answer when nothing is.
    pub fn shown(&self) -> Shown {
        self.watch();
        let text = self.field.read().clone();
        let mode = self.with(|s| s.mode());
        if text.trim().is_empty() {
            return match mode {
                Mode::Programmer => {
                    let last = self.with(|s| s.last_int());
                    Shown {
                        text: last.display.clone(),
                        copy: last.value.to_string(),
                        spoken: last.display,
                        note: None,
                        live: false,
                    }
                }
                _ => {
                    let answer = self.with(|s| s.last_answer());
                    Shown::of(&answer, false)
                }
            };
        }
        match mode {
            Mode::Programmer => {
                let effective =
                    if self.echo.peek().is_some() && is_operator_start(text.trim_start()) {
                        format!("ans{}", text.trim_start())
                    } else {
                        text.clone()
                    };
                match self.with(|s| s.int_preview(&effective)) {
                    IntOutcome::Value(v) => Shown {
                        text: v.display.clone(),
                        copy: v.value.to_string(),
                        spoken: v.display,
                        note: v
                            .overflow
                            .then(|| "The result does not fit the word size.".to_string()),
                        live: true,
                    },
                    IntOutcome::Empty => self.last_shown(),
                    IntOutcome::Error(e) => {
                        let mut shown = self.last_shown();
                        shown.note = Some(e.to_string());
                        shown
                    }
                }
            }
            Mode::Convert => match self.with(|s| s.convert_preview(&text)) {
                Outcome::Value(a) => Shown::of(&a, true),
                Outcome::Empty => self.last_shown(),
                Outcome::Error(e) => {
                    let mut shown = self.last_shown();
                    shown.note = Some(e.to_string());
                    shown
                }
            },
            _ => {
                let effective = self.effective(&text);
                match self.with(|s| s.preview(&effective)) {
                    Outcome::Value(a) => Shown::of(&a, true),
                    Outcome::Empty => self.last_shown(),
                    Outcome::Error(e) => {
                        let mut shown = self.last_shown();
                        shown.note = Some(e.to_string());
                        shown
                    }
                }
            }
        }
    }

    fn last_shown(&self) -> Shown {
        match self.with(|s| s.mode()) {
            Mode::Programmer => {
                let last = self.with(|s| s.last_int());
                Shown {
                    text: last.display.clone(),
                    copy: last.value.to_string(),
                    spoken: last.display,
                    note: None,
                    live: false,
                }
            }
            Mode::Convert => Shown {
                text: "0".into(),
                copy: "0".into(),
                spoken: "0".into(),
                note: None,
                live: false,
            },
            _ => Shown::of(&self.with(|s| s.last_answer()), false),
        }
    }

    // ---- scientific --------------------------------------------------------

    pub fn toggle_angle(&self) {
        let next = match self.with(|s| s.angle()) {
            Angle::Degrees => Angle::Radians,
            Angle::Radians => Angle::Degrees,
        };
        self.with(|s| s.set_angle(next));
        self.changed();
    }

    pub fn toggle_second(&self) {
        let mut second = self.second;
        let next = !*second.peek();
        second.set(next);
    }

    pub fn random_text(&self) -> String {
        self.with(|s| match s.preview("rand") {
            Outcome::Value(a) => a.plain,
            _ => "0.5".to_string(),
        })
    }

    // ---- memory ------------------------------------------------------------

    pub fn memory_clear(&self) {
        self.with(|s| s.memory_clear());
        self.changed();
    }

    /// Adds (or takes away) what the readout shows.
    pub fn memory_add(&self, subtract: bool) {
        let shown = self.shown();
        let answer = Answer {
            display: shown.text,
            plain: shown.copy,
            spoken: shown.spoken,
            rounded: false,
        };
        self.with(|s| s.memory_add(Some(&answer), subtract));
        self.changed();
    }

    pub fn memory_recall(&self) {
        let text = self.with(|s| s.memory().plain);
        self.type_text(&text);
    }

    // ---- programmer --------------------------------------------------------

    pub fn set_base(&self, base: u32) {
        // The text typed in one base is read in another: the value stays.
        let value = self.with(|s| match s.int_preview(self.field.peek().as_str()) {
            IntOutcome::Value(v) => Some(v.value),
            _ => None,
        });
        self.with(|s| s.set_base(base));
        if let Some(value) = value {
            let text = self.with(|s| programmer::plain_int(&value, s.word(), s.base()));
            self.set_field(&text);
        }
        self.changed();
    }

    pub fn set_word(&self, bits: u32, signed: bool) {
        self.with(|s| s.set_word(bits, signed));
        self.changed();
    }

    /// Flips one bit of the value on show.
    pub fn toggle_bit(&self, index: u32) {
        let value = self.with(|s| {
            let current = match s.int_preview(self.field.peek().as_str()) {
                IntOutcome::Value(v) => v.value,
                _ => s.last_int().value,
            };
            programmer::toggle_bit(&current, s.word(), index)
        });
        let text = self.with(|s| {
            s.set_int(value.clone());
            programmer::plain_int(&value, s.word(), s.base())
        });
        self.set_field(&text);
        self.changed();
    }

    // ---- convert -----------------------------------------------------------

    pub fn refresh_rates(&self) {
        let mut fetching = self.fetching;
        if *fetching.peek() {
            return;
        }
        fetching.set(true);
        let calc = self.clone();
        background_then(
            move || {
                let rates = crate::rates_fetch::fetch(now());
                if let (Some(rates), Some(path)) = (&rates, store::rates_path()) {
                    let _ = store::save_rates(&path, rates);
                }
                rates
            },
            move |rates| {
                let mut fetching = calc.fetching;
                fetching.set(false);
                if let Some(rates) = rates {
                    calc.with(|s| s.set_rates(rates, RatesOrigin::Fresh));
                    calc.changed();
                }
            },
        );
    }

    // ---- history -----------------------------------------------------------

    /// Brings an entry back into the field.
    pub fn reuse(&self, entry: &Entry) {
        let mut echo = self.echo;
        echo.set(None);
        match entry.mode {
            Mode::Convert => {
                if let Some(typed) = bingux_calc::units::parse_typed(&entry.expr) {
                    self.with(|s| {
                        s.set_category(typed.category.id);
                        s.set_units(typed.from.id, typed.to.id);
                    });
                    self.set_field(&typed.expression);
                    self.changed();
                }
            }
            Mode::Programmer => {
                self.with(|s| {
                    s.set_word(entry.bits.max(8), entry.signed);
                    s.set_base(entry.base);
                });
                self.set_field(&entry.expr);
                self.changed();
            }
            _ => self.set_field(&entry.expr),
        }
        let mut page = self.history_page;
        page.set(false);
    }

    /// Types an entry's result at the caret.
    pub fn insert_result(&self, entry: &Entry) {
        let text = match entry.mode {
            Mode::Programmer => entry
                .result
                .parse::<BigInt>()
                .map(|v| {
                    programmer::plain_int(&v, self.with(|s| s.word()), self.with(|s| s.base()))
                })
                .unwrap_or_else(|_| entry.result.clone()),
            Mode::Convert => entry
                .result
                .split(' ')
                .next()
                .unwrap_or("")
                .replace(self.with(|s| s.locale.group), "")
                .replace(self.with(|s| s.locale.decimal), ".")
                .replace('\u{2212}', "-"),
            _ => entry.result.clone(),
        };
        self.type_text(&text);
        let mut page = self.history_page;
        page.set(false);
    }

    pub fn delete_history(&self, index: usize) {
        self.with(|s| s.remove_history(index));
        self.changed();
    }

    pub fn clear_history(&self) {
        self.with(|s| s.clear_history());
        self.changed();
    }

    pub fn copy_text(&self, text: String, overlays: bingux_ui::prelude::Overlays) {
        match freya::clipboard::Clipboard::set(text) {
            Ok(()) => overlays.toast("Copied"),
            Err(_) => overlays.toast("Could not copy. Try again."),
        }
    }

    pub fn style(&self) -> format::Style {
        self.with(|s| s.style())
    }
}

/// Whether the brackets of `text` close at its last character and nowhere
/// before: `(2+3)` but not `(2)+(3)`.
fn balanced_inner(text: &str) -> bool {
    let mut depth = 0i32;
    let last = text.chars().count() - 1;
    for (i, c) in text.chars().enumerate() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && i != last {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brackets_that_wrap_the_whole_text() {
        assert!(balanced_inner("(2+3)"));
        assert!(balanced_inner("((2)+(3))"));
        assert!(!balanced_inner("(2)+(3)"));
    }

    #[test]
    fn an_operator_continues_an_answer() {
        assert!(is_operator_start("+5"));
        assert!(is_operator_start("\u{d7}2"));
        assert!(is_operator_start("-3"));
        assert!(!is_operator_start("5+"));
        assert!(!is_operator_start("(1)"));
        assert!(!is_operator_start(""));
    }
}
