//! Programmer mode: whole numbers of a fixed word size.
//!
//! Every result wraps to the word, as it does in hardware: 255 + 1 in an
//! unsigned byte is 0, and 127 + 1 in a signed byte is -128. A step that
//! wrapped sets `overflow`. The operators are C's, with a few words:
//! `+ - * / % & | ^ ~ << >>` and `mod and or xor not nand nor shl shr rol ror`.
//! Division truncates toward zero.

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::error::{CalcError, Result};
use crate::locale::Locale;

/// The width and signedness of a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Word {
    pub bits: u32,
    pub signed: bool,
}

impl Word {
    pub const BYTE: u32 = 8;
    pub const WORD: u32 = 16;
    pub const DWORD: u32 = 32;
    pub const QWORD: u32 = 64;

    pub fn new(bits: u32, signed: bool) -> Word {
        Word {
            bits: bits.clamp(1, 128),
            signed,
        }
    }

    fn modulus(&self) -> BigInt {
        BigInt::one() << self.bits as usize
    }

    pub fn min(&self) -> BigInt {
        if self.signed {
            -(BigInt::one() << (self.bits - 1) as usize)
        } else {
            BigInt::zero()
        }
    }

    pub fn max(&self) -> BigInt {
        if self.signed {
            (BigInt::one() << (self.bits - 1) as usize) - 1
        } else {
            self.modulus() - 1
        }
    }

    pub fn fits(&self, v: &BigInt) -> bool {
        *v >= self.min() && *v <= self.max()
    }

    /// `v` as the word holds it: the low bits, read as signed or unsigned.
    pub fn wrap(&self, v: &BigInt) -> BigInt {
        let modulus = self.modulus();
        let low = v.mod_floor(&modulus);
        if self.signed && low > self.max() {
            low - modulus
        } else {
            low
        }
    }

    /// The bit pattern, as an unsigned number.
    pub fn pattern(&self, v: &BigInt) -> BigInt {
        v.mod_floor(&self.modulus())
    }
}

#[derive(Clone, Debug, PartialEq)]
enum PTok {
    Num(BigInt),
    Plus,
    Minus,
    Star,
    Slash,
    Mod,
    And,
    Or,
    Xor,
    Not,
    Nand,
    Nor,
    Shl,
    Shr,
    Rol,
    Ror,
    LParen,
    RParen,
    Ans,
}

struct PToken {
    tok: PTok,
    at: usize,
}

fn syntax(at: usize, message: impl Into<String>) -> CalcError {
    CalcError::Syntax {
        at,
        message: message.into(),
    }
}

fn base_name(base: u32) -> &'static str {
    match base {
        2 => "binary",
        8 => "octal",
        16 => "hexadecimal",
        _ => "decimal",
    }
}

fn lex(src: &str, base: u32) -> Result<Vec<PToken>> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let at = i;
        let push = |tok: PTok, out: &mut Vec<PToken>, width: usize| {
            out.push(PToken { tok, at });
            width
        };
        i += match c {
            c if c.is_whitespace() => 1,
            '+' => push(PTok::Plus, &mut out, 1),
            '-' | '\u{2212}' => push(PTok::Minus, &mut out, 1),
            '*' | '\u{d7}' | '\u{b7}' => push(PTok::Star, &mut out, 1),
            '/' | '\u{f7}' => push(PTok::Slash, &mut out, 1),
            '%' => push(PTok::Mod, &mut out, 1),
            '&' => push(
                PTok::And,
                &mut out,
                if chars.get(i + 1) == Some(&'&') { 2 } else { 1 },
            ),
            '|' => push(
                PTok::Or,
                &mut out,
                if chars.get(i + 1) == Some(&'|') { 2 } else { 1 },
            ),
            '^' => push(PTok::Xor, &mut out, 1),
            '~' | '\u{ac}' => push(PTok::Not, &mut out, 1),
            '(' => push(PTok::LParen, &mut out, 1),
            ')' => push(PTok::RParen, &mut out, 1),
            '<' if chars.get(i + 1) == Some(&'<') => push(PTok::Shl, &mut out, 2),
            '>' if chars.get(i + 1) == Some(&'>') => push(PTok::Shr, &mut out, 2),
            '=' if chars[i + 1..]
                .iter()
                .all(|c| c.is_whitespace() || *c == '=') =>
            {
                chars.len() - i
            }
            c if c.is_alphanumeric() || c == '_' => {
                let mut j = i;
                while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                let word: String = chars[i..j].iter().collect::<String>().to_lowercase();
                let tok = match word.as_str() {
                    "mod" => PTok::Mod,
                    "and" => PTok::And,
                    "or" => PTok::Or,
                    "xor" => PTok::Xor,
                    "not" => PTok::Not,
                    "nand" => PTok::Nand,
                    "nor" => PTok::Nor,
                    "shl" | "lsh" => PTok::Shl,
                    "shr" | "rsh" => PTok::Shr,
                    "rol" => PTok::Rol,
                    "ror" => PTok::Ror,
                    "ans" => PTok::Ans,
                    _ => PTok::Num(number(&word, base, at)?),
                };
                push(tok, &mut out, j - i)
            }
            other => return Err(syntax(i, format!("{other} is not part of an expression."))),
        };
    }
    Ok(out)
}

fn number(word: &str, base: u32, at: usize) -> Result<BigInt> {
    let (radix, digits) = match word.get(..2) {
        Some("0x") => (16, &word[2..]),
        Some("0b") if base != 16 => (2, &word[2..]),
        Some("0o") => (8, &word[2..]),
        _ => (base, word),
    };
    let digits: String = digits.chars().filter(|c| *c != '_').collect();
    if digits.is_empty() {
        return Err(syntax(at, "That is not a number."));
    }
    BigInt::parse_bytes(digits.as_bytes(), radix).ok_or_else(|| {
        let bad = digits
            .chars()
            .find(|c| c.to_digit(radix).is_none())
            .unwrap_or('?');
        syntax(at, format!("{bad} is not a digit in {}.", base_name(radix)))
    })
}

struct Machine<'a> {
    tokens: &'a [PToken],
    pos: usize,
    word: Word,
    ans: &'a BigInt,
    overflow: bool,
    depth: usize,
}

impl Machine<'_> {
    fn peek(&self) -> Option<&PTok> {
        self.tokens.get(self.pos).map(|t| &t.tok)
    }

    fn put(&mut self, v: BigInt) -> BigInt {
        if !self.word.fits(&v) {
            self.overflow = true;
        }
        self.word.wrap(&v)
    }

    /// Binary levels, loosest first: or, xor, and, shifts, sums, products.
    fn level(&mut self, level: usize) -> Result<BigInt> {
        if level == 6 {
            return self.unary();
        }
        let mut left = self.level(level + 1)?;
        loop {
            let Some(tok) = self.peek().cloned() else {
                break;
            };
            let belongs = match level {
                0 => matches!(tok, PTok::Or | PTok::Nor),
                1 => matches!(tok, PTok::Xor),
                2 => matches!(tok, PTok::And | PTok::Nand),
                3 => matches!(tok, PTok::Shl | PTok::Shr | PTok::Rol | PTok::Ror),
                4 => matches!(tok, PTok::Plus | PTok::Minus),
                _ => matches!(tok, PTok::Star | PTok::Slash | PTok::Mod),
            };
            if !belongs {
                break;
            }
            self.pos += 1;
            let right = self.level(level + 1)?;
            left = self.apply(&tok, left, right)?;
        }
        Ok(left)
    }

    fn apply(&mut self, op: &PTok, a: BigInt, b: BigInt) -> Result<BigInt> {
        let bits = self.word.bits;
        let result = match op {
            PTok::Plus => a + b,
            PTok::Minus => a - b,
            PTok::Star => a * b,
            PTok::Slash => {
                if b.is_zero() {
                    return Err(CalcError::DivideByZero);
                }
                a / b
            }
            PTok::Mod => {
                if b.is_zero() {
                    return Err(CalcError::DivideByZero);
                }
                a % b
            }
            PTok::And => a & b,
            PTok::Or => a | b,
            PTok::Xor => a ^ b,
            PTok::Nand => !(a & b),
            PTok::Nor => !(a | b),
            PTok::Shl | PTok::Shr | PTok::Rol | PTok::Ror => {
                if b.is_negative() {
                    return Err(CalcError::Domain("A shift needs a count of 0 or more."));
                }
                let count = b.to_u32().unwrap_or(u32::MAX);
                match op {
                    PTok::Shl => {
                        if count >= bits {
                            BigInt::zero()
                        } else {
                            a << count as usize
                        }
                    }
                    PTok::Shr => {
                        if count >= bits {
                            if a.is_negative() {
                                BigInt::from(-1)
                            } else {
                                BigInt::zero()
                            }
                        } else {
                            a >> count as usize
                        }
                    }
                    _ => {
                        let pattern = self.word.pattern(&a);
                        let n = count % bits;
                        if n == 0 {
                            pattern
                        } else {
                            let (left, right) = if *op == PTok::Rol {
                                (n, bits - n)
                            } else {
                                (bits - n, n)
                            };
                            let modulus = BigInt::one() << bits as usize;
                            ((&pattern << left as usize) | (&pattern >> right as usize))
                                .mod_floor(&modulus)
                        }
                    }
                }
            }
            _ => unreachable!("not a binary operator"),
        };
        // Only arithmetic can overflow; a bitwise result is the low bits by definition.
        if matches!(
            op,
            PTok::Plus | PTok::Minus | PTok::Star | PTok::Slash | PTok::Mod
        ) {
            Ok(self.put(result))
        } else {
            Ok(self.word.wrap(&result))
        }
    }

    fn unary(&mut self) -> Result<BigInt> {
        match self.peek() {
            Some(PTok::Minus) => {
                self.pos += 1;
                self.depth += 1;
                if self.depth > 150 {
                    return Err(syntax(0, "Too many brackets inside one another."));
                }
                let v = self.unary()?;
                self.depth -= 1;
                Ok(self.put(-v))
            }
            Some(PTok::Plus) => {
                self.pos += 1;
                self.unary()
            }
            Some(PTok::Not) => {
                self.pos += 1;
                self.depth += 1;
                if self.depth > 150 {
                    return Err(syntax(0, "Too many brackets inside one another."));
                }
                let v = self.unary()?;
                self.depth -= 1;
                Ok(self.word.wrap(&!v))
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Result<BigInt> {
        let Some(token) = self.tokens.get(self.pos) else {
            return Err(CalcError::Incomplete);
        };
        self.pos += 1;
        match &token.tok {
            PTok::Num(n) => {
                let n = n.clone();
                Ok(self.put(n))
            }
            PTok::Ans => Ok(self.ans.clone()),
            PTok::LParen => {
                self.depth += 1;
                if self.depth > 150 {
                    return Err(syntax(token.at, "Too many brackets inside one another."));
                }
                let inner = self.level(0)?;
                self.depth -= 1;
                match self.tokens.get(self.pos) {
                    Some(PToken {
                        tok: PTok::RParen, ..
                    }) => {
                        self.pos += 1;
                        Ok(inner)
                    }
                    Some(other) => Err(syntax(other.at, "Check the brackets.")),
                    None => Err(CalcError::Incomplete),
                }
            }
            _ => Err(syntax(
                token.at,
                "Unexpected operator. Check the expression.",
            )),
        }
    }
}

/// What a programmer expression came to.
#[derive(Clone, Debug, PartialEq)]
pub struct IntValue {
    pub value: BigInt,
    /// Some step did not fit the word and wrapped.
    pub overflow: bool,
}

fn run(tokens: &[PToken], word: Word, ans: &BigInt) -> Result<IntValue> {
    if tokens.is_empty() {
        return Err(CalcError::Incomplete);
    }
    let mut machine = Machine {
        tokens,
        pos: 0,
        word,
        ans,
        overflow: false,
        depth: 0,
    };
    let value = machine.level(0)?;
    if let Some(extra) = tokens.get(machine.pos) {
        return Err(if extra.tok == PTok::RParen {
            syntax(extra.at, "There is a closing bracket with no opening one.")
        } else {
            syntax(extra.at, "Check the expression.")
        });
    }
    Ok(IntValue {
        value,
        overflow: machine.overflow,
    })
}

/// Evaluates `source` with numbers typed in `base` (2, 8, 10 or 16).
pub fn evaluate_int(source: &str, base: u32, word: Word, ans: &BigInt) -> Result<IntValue> {
    run(&lex(source, base)?, word, ans)
}

/// The value of `source`, or of what is finished of it.
pub fn preview_int(source: &str, base: u32, word: Word, ans: &BigInt) -> Result<Option<IntValue>> {
    let mut tokens = lex(source, base)?;
    for _ in 0..8 {
        if tokens.is_empty() {
            return Ok(None);
        }
        let open = tokens.iter().filter(|t| t.tok == PTok::LParen).count() as i64
            - tokens.iter().filter(|t| t.tok == PTok::RParen).count() as i64;
        let mut closed: Vec<PToken> = tokens
            .iter()
            .map(|t| PToken {
                tok: t.tok.clone(),
                at: t.at,
            })
            .collect();
        let at = closed.last().map_or(0, |t| t.at + 1);
        for _ in 0..open.max(0) {
            closed.push(PToken {
                tok: PTok::RParen,
                at,
            });
        }
        match run(&closed, word, ans) {
            Ok(value) => return Ok(Some(value)),
            Err(CalcError::Incomplete) => {
                tokens.pop();
            }
            Err(error) => return Err(error),
        }
    }
    Ok(None)
}

/// The digits of `v` in `base`. A negative number in base 2, 8 or 16 is
/// written as its bit pattern, as hardware holds it; in base 10 it keeps its
/// sign. `group` puts a space every four digits (every three in octal) or the
/// locale's group mark every three in decimal.
pub fn format_int(v: &BigInt, word: Word, base: u32, group: bool, locale: &Locale) -> String {
    match base {
        10 => {
            let digits = v.abs().to_string();
            let digits = if group {
                locale.group_digits(&digits)
            } else {
                digits
            };
            if v.is_negative() {
                format!("\u{2212}{digits}")
            } else {
                digits
            }
        }
        _ => {
            let pattern = word.pattern(v);
            let mut digits = pattern.to_str_radix(base).to_uppercase();
            let width = match base {
                2 => word.bits as usize,
                8 => (word.bits as usize).div_ceil(3),
                _ => (word.bits as usize).div_ceil(4),
            };
            // Leading zeros show the word's width, so every bit has a place.
            if digits.len() < width {
                digits = format!("{}{}", "0".repeat(width - digits.len()), digits);
            }
            if !group {
                return digits;
            }
            let size = if base == 8 { 3 } else { 4 };
            let first = digits.len() % size;
            let mut out = String::new();
            for (index, ch) in digits.chars().enumerate() {
                if index > 0 && (index + size - first) % size == 0 {
                    out.push(' ');
                }
                out.push(ch);
            }
            out
        }
    }
}

/// The text to put back in the entry for `v`: no marks, no padding.
pub fn plain_int(v: &BigInt, word: Word, base: u32) -> String {
    match base {
        10 => v.to_string(),
        _ => {
            let digits = word.pattern(v).to_str_radix(base).to_uppercase();
            match base {
                16 => format!("0x{digits}"),
                2 => format!("0b{digits}"),
                _ => format!("0o{digits}"),
            }
        }
    }
}

/// The bits of `v`, most significant first.
pub fn bits_of(v: &BigInt, word: Word) -> Vec<bool> {
    let pattern = word.pattern(v);
    (0..word.bits)
        .rev()
        .map(|i| pattern.bit(i as u64))
        .collect()
}

/// `v` with bit `index` (0 is the least significant) flipped.
pub fn toggle_bit(v: &BigInt, word: Word, index: u32) -> BigInt {
    let flipped = word.pattern(v) ^ (BigInt::one() << index as usize);
    word.wrap(&flipped)
}

/// The same bit pattern read in another word: widening a signed negative
/// number keeps its value, narrowing keeps the low bits.
pub fn convert_word(v: &BigInt, from: Word, to: Word) -> BigInt {
    if from.signed == to.signed && to.bits >= from.bits {
        return v.clone();
    }
    to.wrap(v)
}

/// The character `v` names, if it is a printable code point.
pub fn character(v: &BigInt) -> Option<char> {
    let code = v.to_u32()?;
    let c = char::from_u32(code)?;
    (!c.is_control() && !c.is_whitespace() || c == ' ').then_some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(bits: u32, signed: bool) -> Word {
        Word::new(bits, signed)
    }

    fn ev(src: &str, base: u32, word: Word) -> Result<IntValue> {
        evaluate_int(src, base, word, &BigInt::zero())
    }

    fn val(src: &str, base: u32, word: Word) -> String {
        ev(src, base, word)
            .unwrap_or_else(|e| panic!("{src}: {e:?}"))
            .value
            .to_string()
    }

    #[test]
    fn unsigned_arithmetic_wraps() {
        let byte = w(8, false);
        assert_eq!(val("255+1", 10, byte), "0");
        assert!(ev("255+1", 10, byte).unwrap().overflow);
        assert_eq!(val("0-1", 10, byte), "255");
        assert_eq!(val("16*16", 10, byte), "0");
        assert!(!ev("100+100", 10, byte).unwrap().overflow);
    }

    #[test]
    fn signed_arithmetic_wraps() {
        let byte = w(8, true);
        assert_eq!(val("127+1", 10, byte), "-128");
        assert_eq!(val("-128-1", 10, byte), "127");
        assert_eq!(val("-(-128)", 10, byte), "-128");
        assert!(ev("127+1", 10, byte).unwrap().overflow);
        assert_eq!(val("-5", 10, byte), "-5");
    }

    #[test]
    fn division_truncates_toward_zero() {
        let q = w(64, true);
        assert_eq!(val("7/2", 10, q), "3");
        assert_eq!(val("-7/2", 10, q), "-3");
        assert_eq!(val("-7%2", 10, q), "-1");
        assert_eq!(val("7 mod 3", 10, q), "1");
        assert_eq!(ev("1/0", 10, q), Err(CalcError::DivideByZero));
        assert_eq!(ev("1%0", 10, q), Err(CalcError::DivideByZero));
    }

    #[test]
    fn bitwise_operators_and_their_precedence() {
        let d = w(32, false);
        assert_eq!(val("0xF0 & 0x3C", 10, d), "48");
        assert_eq!(val("0xF0 | 0x0F", 10, d), "255");
        assert_eq!(val("0xFF ^ 0x0F", 10, d), "240");
        assert_eq!(val("6 and 3", 10, d), "2");
        assert_eq!(val("6 or 3", 10, d), "7");
        assert_eq!(val("6 xor 3", 10, d), "5");
        // & binds tighter than ^, which binds tighter than |.
        assert_eq!(val("1 | 2 ^ 3 & 1", 10, d), "3");
        // Shifts are looser than +.
        assert_eq!(val("1 << 2 + 1", 10, d), "8");
    }

    #[test]
    fn not_and_the_negated_operators() {
        assert_eq!(val("~0", 10, w(8, false)), "255");
        assert_eq!(val("~0", 10, w(8, true)), "-1");
        assert_eq!(val("not 5", 10, w(8, false)), "250");
        assert_eq!(val("5 nand 3", 10, w(8, false)), "254");
        assert_eq!(val("5 nor 3", 10, w(8, false)), "248");
    }

    #[test]
    fn shifts_and_rotations() {
        let b = w(8, false);
        assert_eq!(val("1 << 7", 10, b), "128");
        assert_eq!(val("1 << 8", 10, b), "0");
        assert_eq!(val("128 >> 7", 10, b), "1");
        assert_eq!(val("0x81 rol 1", 10, b), "3");
        assert_eq!(val("0x81 ror 1", 10, b), "192");
        assert_eq!(val("5 rol 8", 10, b), "5");
        assert_eq!(val("-8 >> 1", 10, w(8, true)), "-4");
        assert_eq!(val("0xF0 >> 4", 10, w(8, true)), "-1");
        assert_eq!(val("-8 >> 1", 10, w(8, false)), "124");
    }

    #[test]
    fn numbers_in_each_base() {
        let d = w(32, false);
        assert_eq!(val("ff", 16, d), "255");
        assert_eq!(val("FF + 1", 16, d), "256");
        assert_eq!(val("101", 2, d), "5");
        assert_eq!(val("17", 8, d), "15");
        assert_eq!(val("0b101 + 0o7 + 0xA", 10, d), "22");
        assert_eq!(val("dead_beef", 16, w(32, false)), "3735928559");
        assert_eq!(val("ff and 0f", 16, d), "15");
    }

    #[test]
    fn a_digit_outside_the_base_is_reported() {
        let err = ev("19", 8, w(8, false)).unwrap_err();
        assert_eq!(err.to_string(), "9 is not a digit in octal.");
        assert!(matches!(
            ev("12", 2, w(8, false)),
            Err(CalcError::Syntax { .. })
        ));
        assert!(matches!(
            ev("1g", 16, w(8, false)),
            Err(CalcError::Syntax { .. })
        ));
    }

    #[test]
    fn a_literal_too_big_for_the_word_wraps_and_says_so() {
        let r = ev("300", 10, w(8, false)).unwrap();
        assert_eq!(r.value, BigInt::from(44));
        assert!(r.overflow);
    }

    #[test]
    fn sixty_four_bit_limits() {
        let u = w(64, false);
        assert_eq!(val("0xFFFFFFFFFFFFFFFF", 10, u), "18446744073709551615");
        assert_eq!(val("0xFFFFFFFFFFFFFFFF + 1", 10, u), "0");
        let s = w(64, true);
        assert_eq!(val("0x7FFFFFFFFFFFFFFF + 1", 10, s), "-9223372036854775808");
    }

    #[test]
    fn ans_and_brackets() {
        let r = evaluate_int("ans * (2 + 3)", 10, w(16, true), &BigInt::from(4)).unwrap();
        assert_eq!(r.value, BigInt::from(20));
        assert!(matches!(
            ev("(1", 10, w(8, true)),
            Err(CalcError::Incomplete)
        ));
        assert!(matches!(
            ev("1)", 10, w(8, true)),
            Err(CalcError::Syntax { .. })
        ));
    }

    #[test]
    fn the_preview_shows_what_is_finished() {
        let b = w(8, false);
        let p = |s: &str| {
            preview_int(s, 10, b, &BigInt::zero())
                .unwrap()
                .map(|v| v.value.to_string())
        };
        assert_eq!(p(""), None);
        assert_eq!(p("5 +"), Some("5".into()));
        assert_eq!(p("(5 + 1"), Some("6".into()));
        assert_eq!(p("5 and"), Some("5".into()));
    }

    #[test]
    fn a_negative_number_shows_its_bit_pattern() {
        let loc = Locale::EN;
        let b = w(8, true);
        assert_eq!(format_int(&BigInt::from(-1), b, 16, false, &loc), "FF");
        assert_eq!(format_int(&BigInt::from(-1), b, 2, false, &loc), "11111111");
        assert_eq!(
            format_int(&BigInt::from(-1), b, 10, false, &loc),
            "\u{2212}1"
        );
        assert_eq!(format_int(&BigInt::from(5), b, 2, true, &loc), "0000 0101");
        assert_eq!(
            format_int(&BigInt::from(255), w(32, false), 16, true, &loc),
            "0000 00FF"
        );
        assert_eq!(
            format_int(&BigInt::from(1_234_567), w(32, false), 10, true, &loc),
            "1,234,567"
        );
        assert_eq!(
            format_int(&BigInt::from(8), w(16, false), 8, true, &loc),
            "000 010"
        );
    }

    #[test]
    fn plain_text_round_trips() {
        let d = w(32, false);
        for v in [0i64, 1, 255, 65535, 4_000_000_000] {
            for base in [2, 8, 10, 16] {
                let text = plain_int(&BigInt::from(v), d, base);
                assert_eq!(val(&text, base, d), v.to_string(), "{text} base {base}");
            }
        }
    }

    #[test]
    fn bit_view() {
        let b = w(8, false);
        let bits = bits_of(&BigInt::from(5), b);
        assert_eq!(bits, [false, false, false, false, false, true, false, true]);
        assert_eq!(toggle_bit(&BigInt::from(5), b, 1), BigInt::from(7));
        assert_eq!(toggle_bit(&BigInt::from(7), b, 1), BigInt::from(5));
        assert_eq!(
            toggle_bit(&BigInt::from(0), w(8, true), 7),
            BigInt::from(-128)
        );
    }

    #[test]
    fn changing_the_word_keeps_or_cuts_the_value() {
        assert_eq!(
            convert_word(&BigInt::from(-1), w(8, true), w(32, true)),
            BigInt::from(-1)
        );
        assert_eq!(
            convert_word(&BigInt::from(300), w(16, false), w(8, false)),
            BigInt::from(44)
        );
        assert_eq!(
            convert_word(&BigInt::from(200), w(8, false), w(8, true)),
            BigInt::from(-56)
        );
    }

    #[test]
    fn characters() {
        assert_eq!(character(&BigInt::from(65)), Some('A'));
        assert_eq!(character(&BigInt::from(10)), None);
        assert_eq!(character(&BigInt::from(0x1F600)), Some('\u{1F600}'));
        assert_eq!(character(&BigInt::from(0xD800)), None);
        assert_eq!(character(&BigInt::from(32)), Some(' '));
    }
}
