//! The keys of each mode, as `bingux_ui::controls::Keypad`s.
//!
//! A key's label is what is printed on it; its name is what a screen reader
//! says. Each key calls a `Calc` method, the same one the keyboard reaches.

use bingux_calc::programmer::Word;
use bingux_calc::store::Mode;
use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::model::Calc;

/// A key that types `text`.
fn typed(calc: &Calc, label: &str, text: &'static str) -> KeypadKey {
    let calc = calc.clone();
    keypad_key(label, move |_: ()| calc.type_text(text))
}

fn digit(calc: &Calc, digit: &'static str) -> KeypadKey {
    typed(calc, digit, digit)
}

/// An operator key that types `text`, named by the message `name` (an ID).
fn operator(calc: &Calc, label: &str, name: &'static str, text: &'static str) -> KeypadKey {
    let name = bingux_ui::i18n::text(name);
    typed(calc, label, text)
        .name(name.clone())
        .tooltip(name)
        .tone(KeyTone::Operator)
}

/// A key that types `text`, named by the message `name` (an ID).
fn function(calc: &Calc, label: &str, name: &'static str, text: &'static str) -> KeypadKey {
    let name = bingux_ui::i18n::text(name);
    typed(calc, label, text)
        .name(name.clone())
        .tooltip(name)
        .tone(KeyTone::Plain)
}

fn delete_key(calc: &Calc) -> KeypadKey {
    let calc = calc.clone();
    keypad_key(tr!("calculator-key-delete"), move |_: ()| calc.backspace())
        .icon(Icon::EditClear)
        .name(tr!("calculator-key-delete.a11y"))
        .tooltip(tr!("calculator-key-delete.tooltip"))
        .tone(KeyTone::Plain)
}

fn clear_key(calc: &Calc) -> KeypadKey {
    let entry = calc.can_clear_entry();
    let press = calc.clone();
    let (name, tip) = if entry {
        (
            tr!("calculator-key-clear-entry.a11y"),
            tr!("calculator-key-clear-entry.tooltip"),
        )
    } else {
        (
            tr!("calculator-key-clear-all.a11y"),
            tr!("calculator-key-clear-all.tooltip"),
        )
    };
    keypad_key(if entry { "C" } else { "AC" }, move |_: ()| press.clear())
        .name(name)
        .tooltip(tip)
        .tone(KeyTone::Destructive)
}

fn equals_key(calc: &Calc) -> KeypadKey {
    let calc = calc.clone();
    keypad_key("=", move |_: ()| calc.equals())
        .name(tr!("calculator-key-equals.a11y"))
        .tooltip(tr!("calculator-key-equals.tooltip"))
        .tone(KeyTone::Suggested)
}

fn negate_key(calc: &Calc) -> KeypadKey {
    let calc = calc.clone();
    keypad_key("\u{b1}", move |_: ()| calc.negate())
        .name(tr!("calculator-key-negate"))
        .tooltip(tr!("calculator-key-negate"))
        .tone(KeyTone::Plain)
}

fn point_key(calc: &Calc, decimal: char) -> KeypadKey {
    let calc = calc.clone();
    let text: &'static str = if decimal == ',' { "," } else { "." };
    keypad_key(decimal.to_string(), move |_: ()| calc.type_text(text))
        .name(tr!("calculator-key-point"))
        .tone(KeyTone::Digit)
}

/// The four-column keypad of Basic, and the numbers of the other modes.
pub fn basic_keys(calc: &Calc) -> Vec<KeypadKey> {
    let decimal = calc.with(|s| s.locale.decimal);
    vec![
        clear_key(calc),
        delete_key(calc),
        operator(calc, "%", i18n_id!("calculator-key-percent"), "%"),
        operator(calc, "\u{f7}", i18n_id!("calculator-key-divide"), "\u{f7}"),
        digit(calc, "7"),
        digit(calc, "8"),
        digit(calc, "9"),
        operator(
            calc,
            "\u{d7}",
            i18n_id!("calculator-key-multiply"),
            "\u{d7}",
        ),
        digit(calc, "4"),
        digit(calc, "5"),
        digit(calc, "6"),
        operator(
            calc,
            "\u{2212}",
            i18n_id!("calculator-key-subtract"),
            "\u{2212}",
        ),
        digit(calc, "1"),
        digit(calc, "2"),
        digit(calc, "3"),
        operator(calc, "+", i18n_id!("calculator-key-add"), "+"),
        negate_key(calc),
        digit(calc, "0"),
        point_key(calc, decimal),
        equals_key(calc),
    ]
}

/// The memory and function keys of Scientific: six columns, five rows.
pub fn scientific_keys(calc: &Calc) -> Vec<KeypadKey> {
    let second = *calc.second.read();
    let radians = calc.with(|s| s.angle()) == bingux_calc::Angle::Radians;
    let remember = |label: &str, name: &'static str, f: fn(&Calc)| {
        let calc = calc.clone();
        keypad_key(label, move |_: ()| f(&calc))
            .name(bingux_ui::i18n::text(name))
            .tone(KeyTone::Plain)
    };
    let toggle_second = {
        let calc = calc.clone();
        keypad_key("2nd", move |_: ()| calc.toggle_second())
            .name(tr!("calculator-key-second"))
            .tone(KeyTone::Plain)
            .active(second)
    };
    let angle = {
        let calc = calc.clone();
        let (label, name) = if radians {
            (
                tr!("calculator-angle-radians"),
                tr!("calculator-key-angle-radians.a11y"),
            )
        } else {
            (
                tr!("calculator-angle-degrees"),
                tr!("calculator-key-angle-degrees.a11y"),
            )
        };
        keypad_key(label, move |_: ()| calc.toggle_angle())
            .name(name)
            .tone(KeyTone::Plain)
    };
    let random = {
        let calc = calc.clone();
        keypad_key("Rand", move |_: ()| {
            let number = calc.random_text();
            calc.type_text(&number);
        })
        .name(tr!("calculator-key-random"))
        .tone(KeyTone::Plain)
    };
    let pair = |plain: (&'static str, &'static str, &'static str),
                inverse: (&'static str, &'static str, &'static str)| {
        let (label, name, text) = if second { inverse } else { plain };
        function(calc, label, name, text)
    };
    vec![
        function(calc, "(", i18n_id!("calculator-key-open-bracket"), "("),
        function(calc, ")", i18n_id!("calculator-key-close-bracket"), ")"),
        remember(
            "mc",
            i18n_id!("calculator-key-clear-memory"),
            Calc::memory_clear,
        ),
        remember("m+", i18n_id!("calculator-key-add-to-memory"), |c| {
            c.memory_add(false)
        }),
        remember(
            "m\u{2212}",
            i18n_id!("calculator-key-subtract-from-memory"),
            |c| c.memory_add(true),
        ),
        remember(
            "mr",
            i18n_id!("calculator-key-recall-memory"),
            Calc::memory_recall,
        ),
        toggle_second,
        function(calc, "x\u{b2}", i18n_id!("calculator-key-square"), "\u{b2}"),
        function(calc, "x\u{b3}", i18n_id!("calculator-key-cube"), "\u{b3}"),
        function(calc, "x^y", i18n_id!("calculator-key-power"), "^"),
        function(calc, "e^x", i18n_id!("calculator-key-exponential"), "e^"),
        pair(
            ("10^x", i18n_id!("calculator-key-ten-to-the-power"), "10^"),
            ("2^x", i18n_id!("calculator-key-two-to-the-power"), "2^"),
        ),
        function(calc, "1/x", i18n_id!("calculator-key-reciprocal"), "1/("),
        function(
            calc,
            "\u{221a}x",
            i18n_id!("calculator-key-square-root"),
            "\u{221a}(",
        ),
        function(
            calc,
            "\u{221b}x",
            i18n_id!("calculator-key-cube-root"),
            "\u{221b}(",
        ),
        function(calc, "y\u{221a}x", i18n_id!("calculator-key-root"), "root("),
        function(
            calc,
            "ln",
            i18n_id!("calculator-key-natural-logarithm"),
            "ln(",
        ),
        pair(
            ("log", i18n_id!("calculator-key-logarithm-base-10"), "log("),
            ("log2", i18n_id!("calculator-key-logarithm-base-2"), "log2("),
        ),
        function(calc, "x!", i18n_id!("calculator-key-factorial"), "!"),
        pair(
            ("sin", i18n_id!("calculator-key-sine"), "sin("),
            (
                "sin\u{207b}\u{b9}",
                i18n_id!("calculator-key-inverse-sine"),
                "asin(",
            ),
        ),
        pair(
            ("cos", i18n_id!("calculator-key-cosine"), "cos("),
            (
                "cos\u{207b}\u{b9}",
                i18n_id!("calculator-key-inverse-cosine"),
                "acos(",
            ),
        ),
        pair(
            ("tan", i18n_id!("calculator-key-tangent"), "tan("),
            (
                "tan\u{207b}\u{b9}",
                i18n_id!("calculator-key-inverse-tangent"),
                "atan(",
            ),
        ),
        function(calc, "e", i18n_id!("calculator-key-euler-number"), "e"),
        function(
            calc,
            "EE",
            i18n_id!("calculator-key-times-ten-to-the-power"),
            "E",
        ),
        angle,
        pair(
            ("sinh", i18n_id!("calculator-key-hyperbolic-sine"), "sinh("),
            (
                "sinh\u{207b}\u{b9}",
                i18n_id!("calculator-key-inverse-hyperbolic-sine"),
                "asinh(",
            ),
        ),
        pair(
            (
                "cosh",
                i18n_id!("calculator-key-hyperbolic-cosine"),
                "cosh(",
            ),
            (
                "cosh\u{207b}\u{b9}",
                i18n_id!("calculator-key-inverse-hyperbolic-cosine"),
                "acosh(",
            ),
        ),
        pair(
            (
                "tanh",
                i18n_id!("calculator-key-hyperbolic-tangent"),
                "tanh(",
            ),
            (
                "tanh\u{207b}\u{b9}",
                i18n_id!("calculator-key-inverse-hyperbolic-tangent"),
                "atanh(",
            ),
        ),
        function(calc, "\u{3c0}", i18n_id!("calculator-key-pi"), "\u{3c0}"),
        random,
    ]
}

/// The bitwise keys of Programmer: four columns, five rows. Letters that are
/// not digits of the base are off.
pub fn programmer_function_keys(calc: &Calc) -> Vec<KeypadKey> {
    let base = calc.with(|s| s.base());
    let hex = |letter: &'static str| {
        let key = typed(calc, letter, letter)
            .tone(KeyTone::Digit)
            .name(tr!("calculator-key-hex-digit", digit = letter));
        key.enabled(base == 16)
    };
    vec![
        function(calc, "<<", i18n_id!("calculator-key-shift-left"), " << "),
        function(calc, ">>", i18n_id!("calculator-key-shift-right"), " >> "),
        function(calc, "RoL", i18n_id!("calculator-key-rotate-left"), " rol "),
        function(
            calc,
            "RoR",
            i18n_id!("calculator-key-rotate-right"),
            " ror ",
        ),
        function(calc, "AND", i18n_id!("calculator-key-and"), " and "),
        function(calc, "OR", i18n_id!("calculator-key-or"), " or "),
        function(
            calc,
            "XOR",
            i18n_id!("calculator-key-exclusive-or"),
            " xor ",
        ),
        function(calc, "NOT", i18n_id!("calculator-key-not"), "not "),
        function(calc, "NAND", i18n_id!("calculator-key-not-and"), " nand "),
        function(calc, "NOR", i18n_id!("calculator-key-not-or"), " nor "),
        function(calc, "(", i18n_id!("calculator-key-open-bracket"), "("),
        function(calc, ")", i18n_id!("calculator-key-close-bracket"), ")"),
        hex("A"),
        hex("B"),
        hex("C"),
        hex("D"),
        hex("E"),
        hex("F"),
        function(calc, "ans", i18n_id!("calculator-key-last-answer"), "ans"),
    ]
}

/// The number keys of Programmer: digits that do not fit the base are off.
pub fn programmer_keys(calc: &Calc) -> Vec<KeypadKey> {
    let base = calc.with(|s| s.base());
    let enabled = |key: KeypadKey, value: u32| key.enabled(value < base);
    let d = |n: &'static str, value: u32| enabled(digit(calc, n), value);
    vec![
        clear_key(calc),
        delete_key(calc),
        operator(calc, "mod", i18n_id!("calculator-key-remainder"), " mod "),
        operator(calc, "\u{f7}", i18n_id!("calculator-key-divide"), "\u{f7}"),
        d("7", 7),
        d("8", 8),
        d("9", 9),
        operator(
            calc,
            "\u{d7}",
            i18n_id!("calculator-key-multiply"),
            "\u{d7}",
        ),
        d("4", 4),
        d("5", 5),
        d("6", 6),
        operator(
            calc,
            "\u{2212}",
            i18n_id!("calculator-key-subtract"),
            "\u{2212}",
        ),
        d("1", 1),
        d("2", 2),
        d("3", 3),
        operator(calc, "+", i18n_id!("calculator-key-add"), "+"),
        d("0", 0).span(2),
        equals_key(calc).span(2),
    ]
}

/// The number keys of the converter.
pub fn convert_keys(calc: &Calc) -> Vec<KeypadKey> {
    let decimal = calc.with(|s| s.locale.decimal);
    vec![
        digit(calc, "7"),
        digit(calc, "8"),
        digit(calc, "9"),
        delete_key(calc),
        digit(calc, "4"),
        digit(calc, "5"),
        digit(calc, "6"),
        clear_key(calc),
        digit(calc, "1"),
        digit(calc, "2"),
        digit(calc, "3"),
        negate_key(calc),
        digit(calc, "0").span(2),
        point_key(calc, decimal),
        equals_key(calc),
    ]
}

/// The bits of a programmer value as a grid of toggles, most significant
/// first, 16 to a row.
pub fn bit_keys(calc: &Calc, bits: &[bool]) -> Vec<KeypadKey> {
    let total = bits.len();
    bits.iter()
        .enumerate()
        .map(|(position, set)| {
            let index = (total - 1 - position) as u32;
            let calc = calc.clone();
            keypad_key(if *set { "1" } else { "0" }, move |_: ()| {
                calc.toggle_bit(index)
            })
            .name(format!("Bit {index}, {}", if *set { "on" } else { "off" }))
            .tone(KeyTone::Plain)
            .active(*set)
        })
        .collect()
}

/// How many columns the bit grid has for a word.
pub fn bit_columns(word: Word) -> u8 {
    word.bits.min(16) as u8
}

/// The mode a pad belongs to, for a screenshot's name.
pub fn pad_mode(mode: Mode) -> &'static str {
    match mode {
        Mode::Basic => "basic",
        Mode::Scientific => "scientific",
        Mode::Programmer => "programmer",
        Mode::Convert => "convert",
    }
}
