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

fn operator(calc: &Calc, label: &str, name: &str, text: &'static str) -> KeypadKey {
    typed(calc, label, text).name(name).tone(KeyTone::Operator)
}

fn function(calc: &Calc, label: &str, name: &str, text: &'static str) -> KeypadKey {
    typed(calc, label, text).name(name).tone(KeyTone::Plain)
}

fn delete_key(calc: &Calc) -> KeypadKey {
    let calc = calc.clone();
    keypad_key("Delete", move |_: ()| calc.backspace())
        .icon(Icon::EditClear)
        .name("Delete the last character")
        .tone(KeyTone::Plain)
}

fn clear_key(calc: &Calc) -> KeypadKey {
    let entry = calc.can_clear_entry();
    let press = calc.clone();
    keypad_key(if entry { "C" } else { "AC" }, move |_: ()| press.clear())
        .name(if entry {
            "Clear the entry"
        } else {
            "Clear everything"
        })
        .tone(KeyTone::Destructive)
}

fn equals_key(calc: &Calc) -> KeypadKey {
    let calc = calc.clone();
    keypad_key("=", move |_: ()| calc.equals())
        .name("Equals")
        .tone(KeyTone::Suggested)
}

fn negate_key(calc: &Calc) -> KeypadKey {
    let calc = calc.clone();
    keypad_key("\u{b1}", move |_: ()| calc.negate())
        .name("Change sign")
        .tone(KeyTone::Plain)
}

fn point_key(calc: &Calc, decimal: char) -> KeypadKey {
    let calc = calc.clone();
    let text: &'static str = if decimal == ',' { "," } else { "." };
    keypad_key(decimal.to_string(), move |_: ()| calc.type_text(text))
        .name("Decimal point")
        .tone(KeyTone::Digit)
}

/// The four-column keypad of Basic, and the numbers of the other modes.
pub fn basic_keys(calc: &Calc) -> Vec<KeypadKey> {
    let decimal = calc.with(|s| s.locale.decimal);
    vec![
        clear_key(calc),
        delete_key(calc),
        operator(calc, "%", "Percent", "%"),
        operator(calc, "\u{f7}", "Divide", "\u{f7}"),
        digit(calc, "7"),
        digit(calc, "8"),
        digit(calc, "9"),
        operator(calc, "\u{d7}", "Multiply", "\u{d7}"),
        digit(calc, "4"),
        digit(calc, "5"),
        digit(calc, "6"),
        operator(calc, "\u{2212}", "Subtract", "\u{2212}"),
        digit(calc, "1"),
        digit(calc, "2"),
        digit(calc, "3"),
        operator(calc, "+", "Add", "+"),
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
    let remember = |label: &str, name: &str, f: fn(&Calc)| {
        let calc = calc.clone();
        keypad_key(label, move |_: ()| f(&calc))
            .name(name)
            .tone(KeyTone::Plain)
    };
    let toggle_second = {
        let calc = calc.clone();
        keypad_key("2nd", move |_: ()| calc.toggle_second())
            .name("Second functions")
            .tone(KeyTone::Plain)
            .active(second)
    };
    let angle = {
        let calc = calc.clone();
        keypad_key(if radians { "Rad" } else { "Deg" }, move |_: ()| {
            calc.toggle_angle()
        })
        .name(if radians {
            "Angles in radians. Switch to degrees"
        } else {
            "Angles in degrees. Switch to radians"
        })
        .tone(KeyTone::Plain)
    };
    let random = {
        let calc = calc.clone();
        keypad_key("Rand", move |_: ()| {
            let number = calc.random_text();
            calc.type_text(&number);
        })
        .name("Random number")
        .tone(KeyTone::Plain)
    };
    let pair = |plain: (&'static str, &'static str, &'static str),
                inverse: (&'static str, &'static str, &'static str)| {
        let (label, name, text) = if second { inverse } else { plain };
        function(calc, label, name, text)
    };
    vec![
        function(calc, "(", "Open bracket", "("),
        function(calc, ")", "Close bracket", ")"),
        remember("mc", "Clear memory", Calc::memory_clear),
        remember("m+", "Add to memory", |c| c.memory_add(false)),
        remember("m\u{2212}", "Subtract from memory", |c| c.memory_add(true)),
        remember("mr", "Recall memory", Calc::memory_recall),
        toggle_second,
        function(calc, "x\u{b2}", "Square", "\u{b2}"),
        function(calc, "x\u{b3}", "Cube", "\u{b3}"),
        function(calc, "x^y", "Power", "^"),
        function(calc, "e^x", "Exponential", "e^"),
        pair(
            ("10^x", "Ten to the power", "10^"),
            ("2^x", "Two to the power", "2^"),
        ),
        function(calc, "1/x", "Reciprocal", "1/("),
        function(calc, "\u{221a}x", "Square root", "\u{221a}("),
        function(calc, "\u{221b}x", "Cube root", "\u{221b}("),
        function(calc, "y\u{221a}x", "Root", "root("),
        function(calc, "ln", "Natural logarithm", "ln("),
        pair(
            ("log", "Logarithm base 10", "log("),
            ("log2", "Logarithm base 2", "log2("),
        ),
        function(calc, "x!", "Factorial", "!"),
        pair(
            ("sin", "Sine", "sin("),
            ("sin\u{207b}\u{b9}", "Inverse sine", "asin("),
        ),
        pair(
            ("cos", "Cosine", "cos("),
            ("cos\u{207b}\u{b9}", "Inverse cosine", "acos("),
        ),
        pair(
            ("tan", "Tangent", "tan("),
            ("tan\u{207b}\u{b9}", "Inverse tangent", "atan("),
        ),
        function(calc, "e", "Euler's number", "e"),
        function(calc, "EE", "Times ten to the power", "E"),
        angle,
        pair(
            ("sinh", "Hyperbolic sine", "sinh("),
            ("sinh\u{207b}\u{b9}", "Inverse hyperbolic sine", "asinh("),
        ),
        pair(
            ("cosh", "Hyperbolic cosine", "cosh("),
            ("cosh\u{207b}\u{b9}", "Inverse hyperbolic cosine", "acosh("),
        ),
        pair(
            ("tanh", "Hyperbolic tangent", "tanh("),
            ("tanh\u{207b}\u{b9}", "Inverse hyperbolic tangent", "atanh("),
        ),
        function(calc, "\u{3c0}", "Pi", "\u{3c0}"),
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
            .name(format!("Hex digit {letter}"));
        key.enabled(base == 16)
    };
    vec![
        function(calc, "<<", "Shift left", " << "),
        function(calc, ">>", "Shift right", " >> "),
        function(calc, "RoL", "Rotate left", " rol "),
        function(calc, "RoR", "Rotate right", " ror "),
        function(calc, "AND", "And", " and "),
        function(calc, "OR", "Or", " or "),
        function(calc, "XOR", "Exclusive or", " xor "),
        function(calc, "NOT", "Not", "not "),
        function(calc, "NAND", "Not and", " nand "),
        function(calc, "NOR", "Not or", " nor "),
        function(calc, "(", "Open bracket", "("),
        function(calc, ")", "Close bracket", ")"),
        hex("A"),
        hex("B"),
        hex("C"),
        hex("D"),
        hex("E"),
        hex("F"),
        function(calc, "ans", "Last answer", "ans"),
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
        operator(calc, "mod", "Remainder", " mod "),
        operator(calc, "\u{f7}", "Divide", "\u{f7}"),
        d("7", 7),
        d("8", 8),
        d("9", 9),
        operator(calc, "\u{d7}", "Multiply", "\u{d7}"),
        d("4", 4),
        d("5", 5),
        d("6", 6),
        operator(calc, "\u{2212}", "Subtract", "\u{2212}"),
        d("1", 1),
        d("2", 2),
        d("3", 3),
        operator(calc, "+", "Add", "+"),
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
