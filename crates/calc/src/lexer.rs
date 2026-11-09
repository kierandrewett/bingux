//! Turns the text of an expression into tokens.
//!
//! The lexer is generous about what people type and paste: `×`, `*` and a
//! lone `x` all multiply; `−` is a minus; `,` is a decimal mark or a group
//! mark by the locale; `1,234.5`, `1.234,5` and `1 234,5` (with a no-break
//! space) all read as numbers; `0x1F`, `0b101` and `0o17` are numbers in
//! other bases.

use num_bigint::BigInt;

use crate::error::{CalcError, Result};
use crate::locale::{Locale, NARROW_NBSP, NBSP};
use crate::number::Num;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Tok {
    Num(Num),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Percent,
    Bang,
    Degree,
    Sqrt,
    Cbrt,
    /// `²` and `³`.
    Super(u32),
    LParen,
    RParen,
    /// An argument separator: `;`, or `,` where it is not a decimal mark.
    Sep,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Token {
    pub tok: Tok,
    /// The character position in the source.
    pub at: usize,
    /// The position just after the token.
    pub end: usize,
}

fn syntax(at: usize, message: impl Into<String>) -> CalcError {
    CalcError::Syntax {
        at,
        message: message.into(),
    }
}

/// Group marks that are never anything else.
fn is_group_only(c: char) -> bool {
    matches!(c, NBSP | NARROW_NBSP | '\u{2019}' | '\'' | '_' | '\u{2009}')
}

fn is_number_sep(c: char) -> bool {
    c == '.' || c == ',' || is_group_only(c)
}

/// Whether a token ends a value, so that a lone `x` after it multiplies.
fn ends_value(tok: &Tok) -> bool {
    matches!(
        tok,
        Tok::Num(_) | Tok::RParen | Tok::Bang | Tok::Percent | Tok::Degree | Tok::Super(_)
    ) || matches!(tok, Tok::Ident(name) if matches!(name.as_str(), "pi" | "e" | "phi" | "tau" | "ans" | "mem"))
}

pub(crate) fn lex(src: &str, locale: &Locale) -> Result<Vec<Token>> {
    let chars: Vec<char> = src.chars().collect();
    let mut out: Vec<Token> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let at = i;
        let before = out.len();
        let push = |tok: Tok, out: &mut Vec<Token>| out.push(Token { tok, at, end: at });
        match c {
            c if c.is_whitespace() => i += 1,
            '0'..='9' => {
                let (num, next) = scan_number(&chars, i, locale)?;
                push(Tok::Num(num), &mut out);
                i = next;
            }
            '.' if chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()) => {
                let (num, next) = scan_number(&chars, i, locale)?;
                push(Tok::Num(num), &mut out);
                i = next;
            }
            ',' if locale.decimal == ','
                && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()) =>
            {
                let (num, next) = scan_number(&chars, i, locale)?;
                push(Tok::Num(num), &mut out);
                i = next;
            }
            '+' => {
                push(Tok::Plus, &mut out);
                i += 1;
            }
            '-' | '\u{2212}' | '\u{2013}' => {
                push(Tok::Minus, &mut out);
                i += 1;
            }
            '*' => {
                if chars.get(i + 1) == Some(&'*') {
                    push(Tok::Caret, &mut out);
                    i += 2;
                } else {
                    push(Tok::Star, &mut out);
                    i += 1;
                }
            }
            '\u{d7}' | '\u{b7}' | '\u{22c5}' => {
                push(Tok::Star, &mut out);
                i += 1;
            }
            '/' | '\u{f7}' | '\u{2215}' => {
                push(Tok::Slash, &mut out);
                i += 1;
            }
            '^' => {
                push(Tok::Caret, &mut out);
                i += 1;
            }
            '%' => {
                push(Tok::Percent, &mut out);
                i += 1;
            }
            '!' => {
                push(Tok::Bang, &mut out);
                i += 1;
            }
            '\u{b0}' => {
                push(Tok::Degree, &mut out);
                i += 1;
            }
            '\u{221a}' => {
                push(Tok::Sqrt, &mut out);
                i += 1;
            }
            '\u{221b}' => {
                push(Tok::Cbrt, &mut out);
                i += 1;
            }
            '\u{b2}' => {
                push(Tok::Super(2), &mut out);
                i += 1;
            }
            '\u{b3}' => {
                push(Tok::Super(3), &mut out);
                i += 1;
            }
            '(' => {
                push(Tok::LParen, &mut out);
                i += 1;
            }
            ')' => {
                push(Tok::RParen, &mut out);
                i += 1;
            }
            ';' | ',' => {
                push(Tok::Sep, &mut out);
                i += 1;
            }
            '\u{3c0}' => {
                push(Tok::Ident("pi".into()), &mut out);
                i += 1;
            }
            '\u{3c4}' => {
                push(Tok::Ident("tau".into()), &mut out);
                i += 1;
            }
            '\u{3c6}' => {
                push(Tok::Ident("phi".into()), &mut out);
                i += 1;
            }
            '=' if chars[i + 1..]
                .iter()
                .all(|c| c.is_whitespace() || *c == '=') =>
            {
                i = chars.len();
            }
            // A lone x after a value multiplies: `2x3`, `3 x (4)`.
            'x' | 'X'
                if out.last().is_some_and(|last| ends_value(&last.tok))
                    && !chars.get(i + 1).is_some_and(|d| d.is_alphabetic()) =>
            {
                push(Tok::Star, &mut out);
                i += 1;
            }
            c if c.is_alphabetic() => {
                let mut j = i;
                while j < chars.len()
                    && (chars[j].is_alphanumeric() || chars[j] == '_')
                    && !matches!(chars[j], '\u{b2}' | '\u{b3}')
                {
                    j += 1;
                }
                let name: String = chars[i..j].iter().collect::<String>().to_lowercase();
                push(Tok::Ident(name), &mut out);
                i = j;
            }
            other => return Err(syntax(i, format!("{other} is not part of an expression."))),
        }
        for token in out.iter_mut().skip(before) {
            token.end = i;
        }
    }
    Ok(out)
}

fn scan_number(chars: &[char], start: usize, locale: &Locale) -> Result<(Num, usize)> {
    if chars[start] == '0' {
        let radix = match chars.get(start + 1) {
            Some('x' | 'X') => 16,
            Some('b' | 'B') => 2,
            Some('o' | 'O') => 8,
            _ => 0,
        };
        if radix != 0 {
            let first = start + 2;
            let mut j = first;
            while j < chars.len() && (chars[j].to_digit(radix).is_some() || chars[j] == '_') {
                j += 1;
            }
            if j > first {
                let digits: String = chars[first..j].iter().filter(|c| **c != '_').collect();
                let value = BigInt::parse_bytes(digits.as_bytes(), radix)
                    .ok_or_else(|| syntax(start, "That is not a number."))?;
                return Ok((Num::from_big(value), j));
            }
        }
    }
    // The longest run of digits and marks that could be one number.
    let mut end = start;
    while end < chars.len() {
        let c = chars[end];
        if c.is_ascii_digit() {
            end += 1;
        } else if is_number_sep(c) {
            let digit_follows = chars.get(end + 1).is_some_and(|d| d.is_ascii_digit());
            let trailing_point = c == locale.decimal || c == '.';
            let previous_is_digit = end > start && chars[end - 1].is_ascii_digit();
            if digit_follows || (trailing_point && previous_is_digit && c != ',') {
                end += 1;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    let mut run: Vec<char> = chars[start..end].to_vec();
    let (int, frac) = loop {
        match interpret(&run, locale) {
            Ok(parts) => break parts,
            Err(cut) => run.truncate(cut.max(1)),
        }
    };
    let mut next = start + run.len();
    // An exponent: 1e5, 2.5E-3. `e` alone is Euler's number.
    let mut exp10: i64 = 0;
    if matches!(chars.get(next), Some('e' | 'E')) {
        let mut p = next + 1;
        let negative = match chars.get(p) {
            Some('-' | '\u{2212}') => {
                p += 1;
                true
            }
            Some('+') => {
                p += 1;
                false
            }
            _ => false,
        };
        let first = p;
        while chars.get(p).is_some_and(|d| d.is_ascii_digit()) {
            p += 1;
        }
        if p > first {
            let text: String = chars[first..p].iter().collect();
            let value: i64 = text.parse().map_err(|_| CalcError::TooLarge)?;
            exp10 = if negative { -value } else { value };
            next = p;
        }
    }
    let digits = format!("{int}{frac}");
    Ok((Num::from_digits(false, &digits, frac.len(), exp10)?, next))
}

/// Reads a run of digits and marks as an integer part and a fraction part.
/// `Err(cut)` says the run is not one number: it ends at `cut`.
fn interpret(run: &[char], locale: &Locale) -> std::result::Result<(String, String), usize> {
    let seps: Vec<(usize, char)> = run
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.is_ascii_digit())
        .map(|(i, c)| (i, *c))
        .collect();
    if seps.is_empty() {
        return Ok((run.iter().collect(), String::new()));
    }
    let points: Vec<(usize, char)> = seps
        .iter()
        .copied()
        .filter(|(_, c)| *c == '.' || *c == ',')
        .collect();
    let kinds_dot = points.iter().any(|(_, c)| *c == '.');
    let kinds_comma = points.iter().any(|(_, c)| *c == ',');
    let mut decimal_at: Option<usize> = None;
    if kinds_dot && kinds_comma {
        // The last of the two marks is the decimal mark, and it occurs once.
        let (idx, ch) = *points.last().expect("two kinds present");
        if points.iter().filter(|(_, c)| *c == ch).count() != 1 {
            return Err(points.iter().find(|(_, c)| *c == ch).map_or(1, |(i, _)| *i));
        }
        decimal_at = Some(idx);
    } else if points.len() == 1 {
        let (idx, ch) = points[0];
        if ch == '.' || locale.decimal == ',' {
            decimal_at = Some(idx);
        } else {
            // A comma in a locale with a decimal point: a group mark only if
            // exactly three digits follow the first group.
            let after = run.len() - idx - 1;
            let before = run[..idx]
                .iter()
                .rev()
                .take_while(|c| c.is_ascii_digit())
                .count();
            if after != 3 || !(1..=3).contains(&before) || idx != before {
                return Err(idx);
            }
        }
    }
    let integer_end = decimal_at.unwrap_or(run.len());
    // The group marks of the integer part cut it into groups.
    let mut groups: Vec<String> = vec![String::new()];
    let mut mark_positions = Vec::new();
    for (i, c) in run[..integer_end].iter().enumerate() {
        if c.is_ascii_digit() {
            groups.last_mut().expect("one group").push(*c);
        } else {
            mark_positions.push(i);
            groups.push(String::new());
        }
    }
    if groups.len() > 1 {
        let last = groups.len() - 1;
        for (index, group) in groups.iter().enumerate() {
            let good = if index == 0 {
                (1..=3).contains(&group.len())
            } else if index == last {
                group.len() == 3
            } else {
                (2..=3).contains(&group.len())
            };
            if !good {
                return Err(mark_positions[index.saturating_sub(1)]);
            }
        }
    }
    let mut fraction = String::new();
    if let Some(idx) = decimal_at {
        for (offset, c) in run[idx + 1..].iter().enumerate() {
            if c.is_ascii_digit() {
                fraction.push(*c);
            } else {
                return Err(idx + 1 + offset);
            }
        }
    }
    Ok((groups.concat(), fraction))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(src: &str, locale: &Locale) -> Num {
        match lex(src, locale).unwrap().as_slice() {
            [
                Token {
                    tok: Tok::Num(n), ..
                },
            ] => n.clone(),
            other => panic!("not one number: {other:?}"),
        }
    }

    fn en(src: &str) -> String {
        let n = value(src, &Locale::EN);
        let d = n.to_decimal(20);
        let (i, f) = d.positional();
        if f.is_empty() { i } else { format!("{i}.{f}") }
    }

    #[test]
    fn plain_and_decimal_numbers() {
        assert_eq!(en("42"), "42");
        assert_eq!(en("0.125"), "0.125");
        assert_eq!(en(".5"), "0.5");
        assert_eq!(en("5."), "5");
    }

    #[test]
    fn grouped_numbers_from_a_paste() {
        assert_eq!(en("1,234"), "1234");
        assert_eq!(en("1,234,567.89"), "1234567.89");
        assert_eq!(en("12,34,567"), "1234567");
        assert_eq!(en("1\u{a0}234\u{a0}567"), "1234567");
        let fr = Locale::from_tag("fr_FR");
        let d = value("1\u{202f}234,5", &fr).to_decimal(20);
        assert_eq!(d.positional(), ("1234".to_string(), "5".to_string()));
    }

    #[test]
    fn european_numbers() {
        let de = Locale::from_tag("de_DE");
        let n = |s: &str| {
            let d = value(s, &de).to_decimal(20);
            let (i, f) = d.positional();
            if f.is_empty() { i } else { format!("{i}.{f}") }
        };
        assert_eq!(n("1,5"), "1.5");
        assert_eq!(n("1.234,5"), "1234.5");
        assert_eq!(n("1.234.567"), "1234567");
        // A point alone is a decimal point even here: the numpad types one.
        assert_eq!(n("3.14"), "3.14");
        assert_eq!(n("1,234.5"), "1234.5");
    }

    #[test]
    fn a_comma_that_is_not_a_group_ends_the_number() {
        let tokens = lex("max(1,2)", &Locale::EN).unwrap();
        assert_eq!(tokens.len(), 6);
        assert_eq!(tokens[3].tok, Tok::Sep);
        let tokens = lex("1,23", &Locale::EN).unwrap();
        assert_eq!(tokens.len(), 3);
    }

    #[test]
    fn exponents_and_the_constant_e() {
        assert_eq!(en("1e3"), "1000");
        assert_eq!(en("2.5E-3"), "0.0025");
        assert_eq!(en("1e+2"), "100");
        let tokens = lex("2e", &Locale::EN).unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[1].tok, Tok::Ident("e".into()));
    }

    #[test]
    fn other_bases() {
        assert_eq!(en("0xFF"), "255");
        assert_eq!(en("0b1010"), "10");
        assert_eq!(en("0o17"), "15");
    }

    #[test]
    fn every_multiplication_sign() {
        for src in ["2*3", "2\u{d7}3", "2x3", "2 X 3", "2\u{b7}3"] {
            let tokens = lex(src, &Locale::EN).unwrap();
            assert_eq!(tokens[1].tok, Tok::Star, "{src}");
        }
    }

    #[test]
    fn an_x_that_starts_a_name_is_a_name() {
        let tokens = lex("2 xyz", &Locale::EN).unwrap();
        assert_eq!(tokens[1].tok, Tok::Ident("xyz".into()));
    }

    #[test]
    fn unicode_minus_and_pi() {
        let tokens = lex("\u{2212}\u{3c0}", &Locale::EN).unwrap();
        assert_eq!(tokens[0].tok, Tok::Minus);
        assert_eq!(tokens[1].tok, Tok::Ident("pi".into()));
    }

    #[test]
    fn a_stray_character_is_reported_with_its_place() {
        match lex("2 + $", &Locale::EN) {
            Err(CalcError::Syntax { at, .. }) => assert_eq!(at, 4),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_trailing_equals_sign_is_ignored() {
        assert_eq!(lex("2+2=", &Locale::EN).unwrap().len(), 3);
    }
}
