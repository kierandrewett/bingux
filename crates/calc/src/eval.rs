//! Evaluates a parsed expression.
//!
//! Percent works as Apple's Calculator does: `50%` is `0.5`; `200 + 10%` is
//! `200 + 10% of 200`, so `220`; `200 - 10%` is `180`; `200 * 10%` is `20`.

use serde::{Deserialize, Serialize};

use crate::error::{CalcError, Result};
use crate::funcs;
use crate::lexer::{Tok, Token, lex};
use crate::locale::Locale;
use crate::number::Num;
use crate::parser::{Ast, Const, Func, Op, parse};

/// How angles are measured by `sin`, `cos`, `tan` and their inverses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Angle {
    #[default]
    Degrees,
    Radians,
}

/// What an expression can read: the angle unit, the last answer, the stored
/// memory and a random source.
#[derive(Clone, Debug)]
pub struct Env {
    pub angle: Angle,
    pub ans: Num,
    pub memory: Num,
    rng: u64,
}

impl Env {
    pub fn new() -> Env {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e37_79b9_7f4a_7c15);
        Env::seeded(seed)
    }

    /// An environment whose `rand` gives the same numbers for the same seed.
    pub fn seeded(seed: u64) -> Env {
        Env {
            angle: Angle::Degrees,
            ans: Num::zero(),
            memory: Num::zero(),
            rng: seed | 1,
        }
    }

    fn next_random(&mut self) -> Num {
        // xorshift64*; 15 decimal digits.
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        let mixed = x.wrapping_mul(0x2545_f491_4f6c_dd1d);
        let digits = mixed % 1_000_000_000_000_000;
        Num::from_digits(false, &format!("{digits:015}"), 15, 0).unwrap_or_else(|_| Num::zero())
    }
}

impl Default for Env {
    fn default() -> Env {
        Env::new()
    }
}

fn hundred() -> Num {
    Num::from_i64(100)
}

fn degrees_to_radians(x: &Num) -> Result<Num> {
    // The angle modulo a turn first, so a huge angle keeps its digits.
    let turn = Num::from_i64(360);
    let reduced = x.modulo(&turn)?;
    if reduced.is_zero() {
        return Ok(Num::zero());
    }
    funcs::pi().mul(&reduced)?.div(&Num::from_i64(180))
}

fn radians_to_degrees(x: &Num) -> Result<Num> {
    if x.is_zero() {
        return Ok(Num::zero());
    }
    x.mul(&Num::from_i64(180))?.div(&funcs::pi())
}

impl Env {
    fn in_radians(&self, angle: &Num) -> Result<Num> {
        match self.angle {
            Angle::Radians => Ok(angle.clone()),
            Angle::Degrees => degrees_to_radians(angle),
        }
    }

    fn from_radians(&self, radians: &Num) -> Result<Num> {
        match self.angle {
            Angle::Radians => Ok(radians.clone()),
            Angle::Degrees => radians_to_degrees(radians),
        }
    }

    pub(crate) fn eval(&mut self, ast: &Ast) -> Result<Num> {
        match ast {
            Ast::Num(n) => Ok(n.clone()),
            Ast::Const(c) => Ok(match c {
                Const::Pi => funcs::pi(),
                Const::E => funcs::e(),
                Const::Phi => funcs::phi(),
                Const::Tau => funcs::pi().mul(&Num::from_i64(2))?,
                Const::Ans => self.ans.clone(),
                Const::Mem => self.memory.clone(),
                Const::Rand => self.next_random(),
            }),
            Ast::Neg(inner) => Ok(self.eval(inner)?.neg()),
            Ast::Percent(inner) => self.eval(inner)?.div(&hundred()),
            Ast::Degrees(inner) => {
                let value = self.eval(inner)?;
                match self.angle {
                    Angle::Degrees => Ok(value),
                    Angle::Radians => degrees_to_radians(&value),
                }
            }
            Ast::Factorial(inner) => funcs::factorial(&self.eval(inner)?),
            Ast::Bin(op, left, right) => {
                let l = self.eval(left)?;
                // A percentage after + or - is a share of the left side.
                if let (Op::Add | Op::Sub, Ast::Percent(inner)) = (op, right.as_ref()) {
                    let share = l.mul(&self.eval(inner)?)?.div(&hundred())?;
                    return if *op == Op::Add {
                        l.add(&share)
                    } else {
                        l.sub(&share)
                    };
                }
                let r = self.eval(right)?;
                match op {
                    Op::Add => l.add(&r),
                    Op::Sub => l.sub(&r),
                    Op::Mul => l.mul(&r),
                    Op::Div => l.div(&r),
                    Op::Mod => l.modulo(&r),
                    Op::Pow => funcs::pow(&l, &r),
                }
            }
            Ast::Call(func, args) => {
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval(arg)?);
                }
                self.call(*func, &values)
            }
        }
    }

    fn call(&mut self, func: Func, v: &[Num]) -> Result<Num> {
        let x = &v[0];
        match func {
            Func::Sin => funcs::sin(&self.in_radians(x)?),
            Func::Cos => funcs::cos(&self.in_radians(x)?),
            Func::Tan => {
                // At 90 degrees and every half turn after it, the angle is
                // exact, so say so instead of reporting rounding noise.
                if self.angle == Angle::Degrees {
                    let half = x.modulo(&Num::from_i64(180))?;
                    if half == Num::from_i64(90) {
                        return Err(CalcError::Undefined("The tangent of that angle"));
                    }
                }
                funcs::tan(&self.in_radians(x)?)
            }
            Func::Asin => self.from_radians(&funcs::asin(x)?),
            Func::Acos => self.from_radians(&funcs::acos(x)?),
            Func::Atan => self.from_radians(&funcs::atan(x)?),
            Func::Atan2 => {
                let (y, x) = (&v[0], &v[1]);
                let radians = if x.is_zero() {
                    if y.is_zero() {
                        return Err(CalcError::Undefined("The angle of the origin"));
                    }
                    let quarter = funcs::pi().div(&Num::from_i64(2))?;
                    if y.is_negative() {
                        quarter.neg()
                    } else {
                        quarter
                    }
                } else {
                    let base = funcs::atan(&y.div(x)?)?;
                    if x.is_negative() {
                        if y.is_negative() {
                            base.sub(&funcs::pi())?
                        } else {
                            base.add(&funcs::pi())?
                        }
                    } else {
                        base
                    }
                };
                self.from_radians(&radians)
            }
            Func::Sinh => funcs::sinh(x),
            Func::Cosh => funcs::cosh(x),
            Func::Tanh => funcs::tanh(x),
            Func::Asinh => funcs::asinh(x),
            Func::Acosh => funcs::acosh(x),
            Func::Atanh => funcs::atanh(x),
            Func::Ln => funcs::ln(x),
            Func::Log => match v.get(1) {
                Some(base) => funcs::log_base(x, base),
                None => funcs::log10(x),
            },
            Func::Log2 => funcs::log_base(x, &Num::from_i64(2)),
            Func::Exp => funcs::exp(x),
            Func::Sqrt => funcs::sqrt(x),
            Func::Cbrt => funcs::root(x, &Num::from_i64(3)),
            Func::Root => funcs::root(&v[0], &v[1]),
            Func::Abs => Ok(x.abs()),
            Func::Floor => Ok(x.floor()),
            Func::Ceil => Ok(x.ceil()),
            Func::Trunc => Ok(x.trunc()),
            Func::Round => {
                let places = match v.get(1) {
                    Some(places) => places.to_i64().ok_or(CalcError::NeedsInteger("round"))?,
                    None => 0,
                };
                x.round_to(places)
            }
            Func::Sign => Ok(x.sign()),
            Func::Fact => funcs::factorial(x),
            Func::Gcd => funcs::gcd(&v[0], &v[1]),
            Func::Lcm => funcs::lcm(&v[0], &v[1]),
            Func::NCr => funcs::combinations(&v[0], &v[1]),
            Func::NPr => funcs::permutations(&v[0], &v[1]),
            Func::Mod => v[0].modulo(&v[1]),
            Func::Min => {
                Ok(v.iter().skip(1).fold(
                    v[0].clone(),
                    |best, n| if *n < best { n.clone() } else { best },
                ))
            }
            Func::Max => {
                Ok(v.iter().skip(1).fold(
                    v[0].clone(),
                    |best, n| if *n > best { n.clone() } else { best },
                ))
            }
        }
    }
}

/// Evaluates `source`: the whole text must be one finished expression.
pub fn evaluate(source: &str, env: &mut Env, locale: &Locale) -> Result<Num> {
    let ast = parse(lex(source, locale)?)?;
    env.eval(&ast)
}

/// What a live preview shows for text that may be unfinished.
#[derive(Clone, Debug, PartialEq)]
pub enum Preview {
    /// Nothing typed yet.
    Empty,
    /// The value of the expression, or of what is finished of it: `5 +`
    /// shows 5 and `2 * (3 + 4` shows 14.
    Value(Num),
    /// The text has a mistake or no answer.
    Error(CalcError),
}

pub fn preview(source: &str, env: &mut Env, locale: &Locale) -> Preview {
    let mut tokens = match lex(source, locale) {
        Ok(tokens) => tokens,
        Err(error) => return Preview::Error(error),
    };
    if tokens.is_empty() {
        return Preview::Empty;
    }
    // What an operator or an open bracket is still waiting for is not part
    // of the answer so far.
    while tokens.last().is_some_and(|t| {
        matches!(
            t.tok,
            Tok::Plus
                | Tok::Minus
                | Tok::Star
                | Tok::Slash
                | Tok::Caret
                | Tok::LParen
                | Tok::Sep
                | Tok::Sqrt
                | Tok::Cbrt
        )
    }) {
        tokens.pop();
    }
    if tokens.is_empty() {
        return Preview::Empty;
    }
    // Close open brackets, and drop what an operator or a function is still
    // waiting for, until what is left is a whole expression.
    for _ in 0..8 {
        let open = tokens.iter().filter(|t| t.tok == Tok::LParen).count() as i64
            - tokens.iter().filter(|t| t.tok == Tok::RParen).count() as i64;
        let mut closed: Vec<Token> = tokens.clone();
        let at = closed.last().map_or(0, |t| t.at + 1);
        for _ in 0..open.max(0) {
            closed.push(Token {
                tok: Tok::RParen,
                at,
                end: at,
            });
        }
        match parse(closed) {
            Ok(ast) => {
                return match env.eval(&ast) {
                    Ok(value) => Preview::Value(value),
                    Err(error) => Preview::Error(error),
                };
            }
            // The text stops where more is needed, or a bracket we closed
            // meets an operator that still waits for its right side: that
            // operator is what is unfinished.
            Err(CalcError::Incomplete) => {
                tokens.pop();
                if tokens.is_empty() {
                    return Preview::Empty;
                }
            }
            Err(CalcError::Syntax { at: failed, .. }) if open > 0 && failed >= at => {
                tokens.pop();
                if tokens.is_empty() {
                    return Preview::Empty;
                }
            }
            Err(error) => return Preview::Error(error),
        }
    }
    Preview::Error(CalcError::Incomplete)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_in(src: &str, angle: Angle) -> Result<String> {
        let mut env = Env::seeded(7);
        env.angle = angle;
        let value = evaluate(src, &mut env, &Locale::EN)?;
        Ok(show(&value, 16))
    }

    fn show(value: &Num, digits: usize) -> String {
        let d = value.to_decimal(digits);
        let (i, f) = d.positional();
        let body = if f.is_empty() { i } else { format!("{i}.{f}") };
        if d.negative { format!("-{body}") } else { body }
    }

    fn run(src: &str) -> Result<String> {
        run_in(src, Angle::Degrees)
    }

    fn ok(src: &str) -> String {
        run(src).unwrap_or_else(|e| panic!("{src}: {e:?}"))
    }

    // ---- exact arithmetic -------------------------------------------------

    #[test]
    fn decimals_add_exactly() {
        assert_eq!(ok("0.1+0.2"), "0.3");
        assert_eq!(ok("0.1+0.2-0.3"), "0");
        assert_eq!(ok("1.1*1.1"), "1.21");
        assert_eq!(ok("0.3/0.1"), "3");
        assert_eq!(ok("1-0.9"), "0.1");
    }

    #[test]
    fn division_is_exact_then_cut_at_the_precision() {
        assert_eq!(ok("1/3"), "0.3333333333333333");
        assert_eq!(ok("2/3"), "0.6666666666666667");
        assert_eq!(ok("1/3*3"), "1");
        assert_eq!(ok("10/4"), "2.5");
        assert_eq!(ok("1/7"), "0.1428571428571429");
    }

    #[test]
    fn precedence_and_brackets() {
        assert_eq!(ok("2+3*4"), "14");
        assert_eq!(ok("(2+3)*4"), "20");
        assert_eq!(ok("-2^2"), "-4");
        assert_eq!(ok("(-2)^2"), "4");
        assert_eq!(ok("2^3^2"), "512");
        assert_eq!(ok("2^-2"), "0.25");
        assert_eq!(ok("10-2-3"), "5");
        assert_eq!(ok("100/10/5"), "2");
        assert_eq!(ok("8/2(2+2)"), "1");
        assert_eq!(ok("2 * -3"), "-6");
        assert_eq!(ok("--4"), "4");
    }

    #[test]
    fn big_numbers_keep_every_digit() {
        assert_eq!(ok("2^100"), "1267650600228229401496703205376");
        assert_eq!(ok("999999999999*999999999999"), "999999999998000000000001");
        assert_eq!(ok("100!").len(), 158);
        assert_eq!(ok("12345678901234567890+1"), "12345678901234567891");
    }

    #[test]
    fn mod_and_whole_number_functions() {
        assert_eq!(ok("17 mod 5"), "2");
        assert_eq!(ok("-7 mod 3"), "2");
        assert_eq!(ok("7.5 mod 2"), "1.5");
        assert_eq!(ok("gcd(12;18)"), "6");
        assert_eq!(ok("lcm(4;6)"), "12");
        assert_eq!(ok("5!"), "120");
        assert_eq!(ok("ncr(5;2)"), "10");
        assert_eq!(ok("npr(5;2)"), "20");
        assert_eq!(ok("round(2.567;2)"), "2.57");
        assert_eq!(ok("floor(-1.5)"), "-2");
        assert_eq!(ok("ceil(-1.5)"), "-1");
        assert_eq!(ok("trunc(-1.5)"), "-1");
        assert_eq!(ok("abs(-3)"), "3");
        assert_eq!(ok("max(1;5;3)"), "5");
        assert_eq!(ok("min(4;2;9)"), "2");
    }

    // ---- percent ----------------------------------------------------------

    #[test]
    fn percent_follows_apple_calculator() {
        assert_eq!(ok("50%"), "0.5");
        assert_eq!(ok("200+10%"), "220");
        assert_eq!(ok("200-10%"), "180");
        assert_eq!(ok("200*10%"), "20");
        assert_eq!(ok("200/50%"), "400");
        assert_eq!(ok("200+10%+10%"), "242");
        assert_eq!(ok("20% of 50"), "10");
        assert_eq!(ok("100+50*10%"), "105");
        assert_eq!(ok("50%*200"), "100");
        assert_eq!(ok("8%"), "0.08");
        assert_eq!(ok("1000-15%-5%"), "807.5");
    }

    // ---- scientific -------------------------------------------------------

    #[test]
    fn degrees_give_exact_answers_at_the_common_angles() {
        assert_eq!(ok("sin(30)"), "0.5");
        assert_eq!(ok("cos(60)"), "0.5");
        assert_eq!(ok("sin(90)"), "1");
        assert_eq!(ok("cos(90)"), "0");
        assert_eq!(ok("sin(180)"), "0");
        assert_eq!(ok("cos(180)"), "-1");
        assert_eq!(ok("tan(45)"), "1");
        assert_eq!(ok("sin(360)"), "0");
        assert_eq!(ok("sin(-30)"), "-0.5");
        assert_eq!(ok("cos(3600000000)"), "1");
        assert_eq!(ok("asin(0.5)"), "30");
        assert_eq!(ok("acos(0.5)"), "60");
        assert_eq!(ok("atan(1)"), "45");
        assert_eq!(ok("sin(45)^2"), "0.5");
    }

    #[test]
    fn radians_and_the_degree_sign() {
        assert_eq!(run_in("sin(pi)", Angle::Radians).unwrap(), "0");
        assert_eq!(run_in("cos(pi)", Angle::Radians).unwrap(), "-1");
        assert_eq!(run_in("sin(pi/6)", Angle::Radians).unwrap(), "0.5");
        assert_eq!(run_in("sin(30°)", Angle::Radians).unwrap(), "0.5");
        assert_eq!(run_in("sin(30°)", Angle::Degrees).unwrap(), "0.5");
        assert_eq!(
            run_in("sin(1)", Angle::Radians).unwrap(),
            "0.8414709848078965"
        );
        assert_eq!(
            run_in("atan(1)", Angle::Radians).unwrap(),
            "0.7853981633974483"
        );
    }

    #[test]
    fn the_tangent_is_undefined_at_a_right_angle() {
        assert!(matches!(run("tan(90)"), Err(CalcError::Undefined(_))));
        assert!(matches!(run("tan(270)"), Err(CalcError::Undefined(_))));
        assert!(matches!(
            run_in("tan(pi/2)", Angle::Radians),
            Err(CalcError::Undefined(_))
        ));
        assert_eq!(ok("tan(0)"), "0");
    }

    #[test]
    fn roots_logs_and_exponentials() {
        assert_eq!(ok("sqrt(16)"), "4");
        assert_eq!(ok("√16"), "4");
        assert_eq!(ok("√2"), "1.414213562373095");
        assert_eq!(ok("√2*√2"), "2");
        assert_eq!(ok("cbrt(27)"), "3");
        assert_eq!(ok("∛-8"), "-2");
        assert_eq!(ok("root(81;4)"), "3");
        assert_eq!(ok("log(1000)"), "3");
        assert_eq!(ok("log(8;2)"), "3");
        assert_eq!(ok("log2(1024)"), "10");
        assert_eq!(ok("ln(e)"), "1");
        assert_eq!(ok("ln(1)"), "0");
        assert_eq!(ok("exp(0)"), "1");
        assert_eq!(ok("exp(1)"), "2.718281828459045");
        assert_eq!(ok("e^2"), "7.38905609893065");
        assert_eq!(ok("10^-3"), "0.001");
        assert_eq!(ok("8^(1/3)"), "2");
        assert_eq!(ok("2^0.5"), "1.414213562373095");
        assert_eq!(ok("pi"), "3.141592653589793");
        assert_eq!(ok("2pi"), "6.283185307179586");
        assert_eq!(ok("tau"), "6.283185307179586");
        assert_eq!(ok("phi"), "1.618033988749895");
        assert_eq!(ok("1/2pi"), "0.1591549430918953");
        assert_eq!(ok("sinh(0)"), "0");
        assert_eq!(ok("cosh(0)"), "1");
        assert_eq!(ok("atan2(1;1)"), "45");
        assert_eq!(ok("atan2(-1;-1)"), "-135");
    }

    #[test]
    fn implicit_multiplication() {
        assert_eq!(ok("2(3+4)"), "14");
        assert_eq!(ok("(1+1)(2+2)"), "8");
        assert_eq!(ok("3sin(30)"), "1.5");
        assert_eq!(ok("2x3"), "6");
        assert_eq!(ok("2×3"), "6");
        assert_eq!(ok("6÷3"), "2");
        assert_eq!(ok("sin 30 + 1"), "1.5");
        assert_eq!(ok("3²"), "9");
    }

    // ---- errors -----------------------------------------------------------

    #[test]
    fn errors_say_what_failed() {
        assert_eq!(run("1/0"), Err(CalcError::DivideByZero));
        assert_eq!(run("5 mod 0"), Err(CalcError::DivideByZero));
        assert_eq!(run("0^-1"), Err(CalcError::DivideByZero));
        assert_eq!(run("√-1"), Err(CalcError::NotReal));
        assert_eq!(run("(-8)^0.5"), Err(CalcError::NotReal));
        assert!(matches!(run("ln(0)"), Err(CalcError::Undefined(_))));
        assert_eq!(run("ln(-1)"), Err(CalcError::NotReal));
        assert!(matches!(run("asin(2)"), Err(CalcError::Domain(_))));
        assert!(matches!(run("3.5!"), Err(CalcError::NeedsInteger(_))));
        assert!(matches!(run("(-3)!"), Err(CalcError::Domain(_))));
        assert_eq!(run("10^1000000"), Err(CalcError::TooLarge));
        assert_eq!(run("1e999999999"), Err(CalcError::TooLarge));
        assert_eq!(run("5+"), Err(CalcError::Incomplete));
        assert!(matches!(run("5+*2"), Err(CalcError::Syntax { .. })));
        assert!(matches!(run("hello"), Err(CalcError::Unknown(_))));
        assert_eq!(run(""), Err(CalcError::Incomplete));
    }

    #[test]
    fn ans_and_memory() {
        let mut env = Env::seeded(1);
        env.ans = Num::from_i64(7);
        env.memory = Num::from_i64(3);
        let v = evaluate("ans*2+mem", &mut env, &Locale::EN).unwrap();
        assert_eq!(show(&v, 16), "17");
    }

    #[test]
    fn rand_is_between_zero_and_one() {
        let mut env = Env::seeded(99);
        for _ in 0..50 {
            let v = evaluate("rand", &mut env, &Locale::EN).unwrap();
            assert!(v >= Num::zero() && v < Num::one());
        }
        let a = evaluate("rand", &mut Env::seeded(5), &Locale::EN).unwrap();
        let b = evaluate("rand", &mut Env::seeded(5), &Locale::EN).unwrap();
        assert_eq!(a, b);
    }

    // ---- locale and paste -------------------------------------------------

    #[test]
    fn a_pasted_grouped_expression_reads() {
        assert_eq!(ok("1,234 + 5,678.5"), "6912.5");
        assert_eq!(ok("1\u{a0}000\u{a0}000 / 4"), "250000");
    }

    #[test]
    fn german_input() {
        let de = Locale::from_tag("de_DE");
        let mut env = Env::seeded(3);
        let v = evaluate("1.234,5 + 0,5", &mut env, &de).unwrap();
        assert_eq!(show(&v, 16), "1235");
        // Arguments are separated by semicolons where the comma is a mark.
        let v = evaluate("max(1,5;2)", &mut env, &de).unwrap();
        assert_eq!(show(&v, 16), "2");
    }

    // ---- preview ----------------------------------------------------------

    fn preview_text(src: &str) -> String {
        let mut env = Env::seeded(2);
        match preview(src, &mut env, &Locale::EN) {
            Preview::Empty => "empty".into(),
            Preview::Value(v) => show(&v, 16),
            Preview::Error(e) => format!("error: {e}"),
        }
    }

    #[test]
    fn the_preview_shows_what_is_finished() {
        assert_eq!(preview_text(""), "empty");
        assert_eq!(preview_text("  "), "empty");
        assert_eq!(preview_text("5"), "5");
        assert_eq!(preview_text("5+"), "5");
        assert_eq!(preview_text("5+3*"), "8");
        assert_eq!(preview_text("2*(3+4"), "14");
        assert_eq!(preview_text("2*(3+"), "6");
        assert_eq!(preview_text("sin(30"), "0.5");
        assert_eq!(preview_text("sin("), "empty");
        assert_eq!(preview_text("+"), "empty");
    }

    #[test]
    fn the_preview_reports_real_mistakes() {
        assert!(preview_text("5/0").starts_with("error: Cannot divide"));
        assert!(preview_text("5)").starts_with("error"));
        assert!(preview_text("foo").starts_with("error"));
    }
}
