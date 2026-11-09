//! The number type: an exact fraction, flagged when it is an approximation.
//!
//! `0.1 + 0.2` is `3/10`, exactly, because every decimal a person types is a
//! fraction of two integers and the integers have no size limit. Functions
//! with no exact answer (the square root of 2, a sine, a logarithm) return a
//! decimal rounded to [`SIG_DIGITS`] significant digits and flag it as an
//! approximation. An approximation that lands within 1e-60 of a simple
//! fraction (`sin(30°)` is `0.5000…0001` at 70 digits) is taken for that
//! fraction, so the common exact answers stay exact.

use std::cmp::Ordering;

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::Ratio;
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::error::{CalcError, Result};

pub type Rat = Ratio<BigInt>;

/// The most bits a numerator and a denominator may hold between them: about
/// 200,000 digits. Past it an operation fails with [`CalcError::TooLarge`].
pub const MAX_BITS: u64 = 700_000;

/// How many significant digits an approximation keeps.
pub const SIG_DIGITS: usize = 70;

/// The most decimal places an exact fraction shows in full.
const MAX_EXACT_PLACES: u64 = 1000;

pub fn pow10(n: usize) -> BigInt {
    BigInt::from(10u32).pow(n as u32)
}

pub(crate) fn rat(n: impl Into<BigInt>) -> Rat {
    Rat::from_integer(n.into())
}

/// A real number: a fraction of two integers of any size, and whether it was
/// rounded on the way.
#[derive(Clone, Debug)]
pub struct Num {
    r: Rat,
    approx: bool,
}

impl PartialEq for Num {
    fn eq(&self, other: &Num) -> bool {
        self.r == other.r
    }
}

impl Eq for Num {}

impl PartialOrd for Num {
    fn partial_cmp(&self, other: &Num) -> Option<Ordering> {
        Some(self.r.cmp(&other.r))
    }
}

fn too_big(r: &Rat) -> bool {
    r.numer().bits() + r.denom().bits() > MAX_BITS
}

/// floor(log10(|r|)) for a non-zero `r`.
pub(crate) fn magnitude10(r: &Rat) -> i64 {
    let n = r.numer().abs();
    let d = r.denom().clone();
    let bits = n.bits() as i64 - d.bits() as i64;
    let mut e = ((bits as f64) * std::f64::consts::LOG10_2).floor() as i64;
    let compare = |e: i64| -> Ordering {
        if e >= 0 {
            n.cmp(&(&d * pow10(e as usize)))
        } else {
            (&n * pow10((-e) as usize)).cmp(&d)
        }
    };
    loop {
        if compare(e) == Ordering::Less {
            e -= 1;
        } else if compare(e + 1) != Ordering::Less {
            e += 1;
        } else {
            return e;
        }
    }
}

fn round_half_away(r: &Rat) -> BigInt {
    let half = Rat::new(BigInt::one(), BigInt::from(2));
    if r.is_negative() {
        -((-r + half).floor().to_integer())
    } else {
        (r + half).floor().to_integer()
    }
}

/// `r` rounded to `sig` significant digits.
pub(crate) fn round_sig(r: &Rat, sig: usize) -> Rat {
    if r.is_zero() {
        return r.clone();
    }
    let shift = sig as i64 - 1 - magnitude10(r);
    if shift >= 0 {
        let scale = pow10(shift as usize);
        let q = round_half_away(&(r * rat(scale.clone())));
        Rat::new(q, scale)
    } else {
        let scale = pow10((-shift) as usize);
        let q = round_half_away(&(r / rat(scale.clone())));
        rat(q * scale)
    }
}

/// The convergent of `r` with the largest denominator not over `max_den`.
fn best_fraction(r: &Rat, max_den: &BigInt) -> Rat {
    let mut x = r.clone();
    let (mut p0, mut q0) = (BigInt::zero(), BigInt::one());
    let (mut p1, mut q1) = (BigInt::one(), BigInt::zero());
    for _ in 0..64 {
        let a = x.floor().to_integer();
        let p2 = &a * &p1 + &p0;
        let q2 = &a * &q1 + &q0;
        if &q2 > max_den {
            break;
        }
        p0 = p1;
        q0 = q1;
        p1 = p2;
        q1 = q2;
        let frac = &x - rat(a);
        if frac.is_zero() {
            break;
        }
        x = frac.recip();
    }
    if q1.is_zero() {
        return r.clone();
    }
    Rat::new(p1, q1)
}

/// The simple fraction `r` is a rounding of, if there is one. `zero_scale`
/// is the size of the numbers the value came from: a result far below it is
/// rounding noise and becomes 0 (`sin(π)`); `None` never snaps to 0.
fn snap(r: &Rat, zero_scale: Option<&Rat>) -> Option<Rat> {
    if r.numer().bits() > 400 && r.abs() > rat(pow10(30)) {
        // Too large to be a rounding of a simple fraction: leave it.
        return None;
    }
    let best = best_fraction(r, &BigInt::from(10_000));
    let tolerance_base = if r.abs() > Rat::one() {
        r.abs()
    } else {
        Rat::one()
    };
    let tolerance = tolerance_base * Rat::new(BigInt::one(), pow10(60));
    if best.is_zero() {
        let scale = zero_scale?;
        let limit = scale.abs() * Rat::new(BigInt::one(), pow10(60));
        return (r.abs() <= limit).then(|| Rat::zero());
    }
    ((r - &best).abs() <= tolerance).then_some(best)
}

impl Num {
    pub fn zero() -> Num {
        Num {
            r: Rat::zero(),
            approx: false,
        }
    }

    pub fn one() -> Num {
        Num {
            r: Rat::one(),
            approx: false,
        }
    }

    pub fn from_i64(n: i64) -> Num {
        Num {
            r: rat(n),
            approx: false,
        }
    }

    pub fn from_big(n: BigInt) -> Num {
        Num {
            r: rat(n),
            approx: false,
        }
    }

    /// `numer / denom`, exactly; `None` when `denom` is zero.
    pub fn from_ratio(numer: BigInt, denom: BigInt) -> Option<Num> {
        if denom.is_zero() {
            return None;
        }
        Some(Num {
            r: Rat::new(numer, denom),
            approx: false,
        })
    }

    pub(crate) fn exact(r: Rat) -> Num {
        Num { r, approx: false }
    }

    /// A value computed with rounding: kept to [`SIG_DIGITS`] digits and
    /// taken for a simple fraction when it is that to 60 digits.
    pub(crate) fn approximate(r: Rat, zero_scale: Option<&Rat>) -> Num {
        let r = round_sig(&r, SIG_DIGITS);
        match snap(&r, zero_scale) {
            Some(exact) => Num {
                r: exact,
                approx: false,
            },
            None => Num { r, approx: true },
        }
    }

    /// The value the digits name: `digits` are the integer and fraction
    /// digits together, `places` how many of them are after the point, and
    /// the result is multiplied by ten to the `exp10`.
    pub fn from_digits(negative: bool, digits: &str, places: usize, exp10: i64) -> Result<Num> {
        let magnitude = BigInt::parse_bytes(digits.as_bytes(), 10).ok_or(CalcError::Syntax {
            at: 0,
            message: "That is not a number. Check the digits.".into(),
        })?;
        let scale = exp10 - places as i64;
        if scale.unsigned_abs() > 200_000 {
            return Err(CalcError::TooLarge);
        }
        let r = if scale >= 0 {
            rat(magnitude * pow10(scale as usize))
        } else {
            Rat::new(magnitude, pow10((-scale) as usize))
        };
        let r = if negative { -r } else { r };
        if too_big(&r) {
            return Err(CalcError::TooLarge);
        }
        Ok(Num { r, approx: false })
    }

    pub fn rat(&self) -> &Rat {
        &self.r
    }

    pub fn is_zero(&self) -> bool {
        self.r.is_zero()
    }

    pub fn is_negative(&self) -> bool {
        self.r.is_negative()
    }

    pub fn is_integer(&self) -> bool {
        self.r.is_integer()
    }

    pub fn is_approx(&self) -> bool {
        self.approx
    }

    /// The integer this number is, if it is one.
    pub fn to_big(&self) -> Option<BigInt> {
        self.r.is_integer().then(|| self.r.to_integer())
    }

    pub fn to_i64(&self) -> Option<i64> {
        self.to_big().and_then(|n| n.to_i64())
    }

    pub fn to_f64(&self) -> f64 {
        let n = self.r.numer();
        let d = self.r.denom();
        match (n.to_f64(), d.to_f64()) {
            (Some(n), Some(d)) if n.is_finite() && d.is_finite() => n / d,
            // Too big for a double: scale both down, keeping the ratio.
            _ => {
                let shift = n.bits().max(d.bits()).saturating_sub(1000);
                let n = (n >> shift as usize).to_f64().unwrap_or(f64::NAN);
                let d = (d >> shift as usize).to_f64().unwrap_or(f64::NAN);
                n / d
            }
        }
    }

    pub fn neg(&self) -> Num {
        Num {
            r: -&self.r,
            approx: self.approx,
        }
    }

    pub fn abs(&self) -> Num {
        Num {
            r: self.r.abs(),
            approx: self.approx,
        }
    }

    fn combine(&self, other: &Num, r: Rat, cancels: bool) -> Result<Num> {
        if too_big(&r) {
            return Err(CalcError::TooLarge);
        }
        if !(self.approx || other.approx) {
            return Ok(Num { r, approx: false });
        }
        let scale = cancels.then(|| {
            let (a, b) = (self.r.abs(), other.r.abs());
            if a > b { a } else { b }
        });
        Ok(Num::approximate(r, scale.as_ref()))
    }

    pub fn add(&self, other: &Num) -> Result<Num> {
        self.combine(other, &self.r + &other.r, true)
    }

    pub fn sub(&self, other: &Num) -> Result<Num> {
        self.combine(other, &self.r - &other.r, true)
    }

    pub fn mul(&self, other: &Num) -> Result<Num> {
        self.combine(other, &self.r * &other.r, false)
    }

    pub fn div(&self, other: &Num) -> Result<Num> {
        if other.r.is_zero() {
            return Err(CalcError::DivideByZero);
        }
        self.combine(other, &self.r / &other.r, false)
    }

    /// The remainder with the sign of the divisor (`-7 mod 3` is 2).
    pub fn modulo(&self, other: &Num) -> Result<Num> {
        if other.r.is_zero() {
            return Err(CalcError::DivideByZero);
        }
        let quotient = (&self.r / &other.r).floor();
        let r = &self.r - quotient * &other.r;
        self.combine(other, r, false)
    }

    pub fn recip(&self) -> Result<Num> {
        Num::one().div(self)
    }

    pub fn floor(&self) -> Num {
        Num {
            r: self.r.floor(),
            approx: false,
        }
    }

    pub fn ceil(&self) -> Num {
        Num {
            r: self.r.ceil(),
            approx: false,
        }
    }

    pub fn trunc(&self) -> Num {
        Num {
            r: self.r.trunc(),
            approx: false,
        }
    }

    /// Rounded half away from zero to `places` decimal places (negative
    /// `places` round to tens, hundreds, ...).
    pub fn round_to(&self, places: i64) -> Result<Num> {
        if places.unsigned_abs() > 100_000 {
            return Err(CalcError::TooLarge);
        }
        let scale = if places >= 0 {
            rat(pow10(places as usize))
        } else {
            Rat::new(BigInt::one(), pow10((-places) as usize))
        };
        let q = round_half_away(&(&self.r * &scale));
        Ok(Num {
            r: rat(q) / scale,
            approx: self.approx,
        })
    }

    pub fn sign(&self) -> Num {
        Num::from_i64(if self.r.is_zero() {
            0
        } else if self.r.is_negative() {
            -1
        } else {
            1
        })
    }

    /// The digits of this number, rounded to `precision` significant digits
    /// unless it is an exact decimal.
    pub fn to_decimal(&self, precision: usize) -> Decimal {
        let negative = self.r.is_negative();
        let a = self.r.abs();
        if a.is_zero() {
            return Decimal {
                negative: false,
                digits: "0".into(),
                e: 0,
                rounded: false,
                exact: true,
            };
        }
        if !self.approx {
            if let Some(places) = terminating_places(a.denom()) {
                if places <= MAX_EXACT_PLACES {
                    let scaled = a.numer() * pow10(places as usize) / a.denom();
                    let digits = scaled.to_string();
                    let e = digits.len() as i64 - 1 - places as i64;
                    let digits = digits.trim_end_matches('0');
                    return Decimal {
                        negative,
                        digits: digits.to_string(),
                        e,
                        rounded: false,
                        exact: true,
                    };
                }
            }
        }
        let precision = precision.clamp(1, 60);
        let mut e = magnitude10(&a);
        let shift = precision as i64 - 1 - e;
        let mut q = if shift >= 0 {
            round_half_away(&(&a * rat(pow10(shift as usize))))
        } else {
            round_half_away(&(&a / rat(pow10((-shift) as usize))))
        };
        if q >= pow10(precision) {
            q /= BigInt::from(10);
            e += 1;
        }
        let digits = q.to_string();
        let digits = digits.trim_end_matches('0');
        let digits = if digits.is_empty() { "0" } else { digits };
        Decimal {
            negative,
            digits: digits.to_string(),
            e,
            rounded: true,
            exact: false,
        }
    }
}

/// `k` when `1/denom` has exactly `k` decimal places, `None` when it never
/// ends (or is too large to check cheaply).
fn terminating_places(denom: &BigInt) -> Option<u64> {
    if denom.bits() > 6000 {
        return None;
    }
    let mut d = denom.clone();
    let (mut twos, mut fives) = (0u64, 0u64);
    while d.is_even() {
        d >>= 1usize;
        twos += 1;
    }
    let five = BigInt::from(5);
    loop {
        let (q, rem) = d.div_rem(&five);
        if !rem.is_zero() {
            break;
        }
        d = q;
        fives += 1;
    }
    d.is_one().then_some(twos.max(fives))
}

/// A number as digits: `0.d1d2d3...` shifted so that the first digit sits at
/// ten to the `e`. `digits` has no trailing zeros.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decimal {
    pub negative: bool,
    pub digits: String,
    pub e: i64,
    /// Digits were cut: the value is not exactly what is shown.
    pub rounded: bool,
    /// The digits are the whole value.
    pub exact: bool,
}

impl Decimal {
    /// The integer part and the fraction part as plain digits.
    pub fn positional(&self) -> (String, String) {
        let digits = self.digits.as_str();
        if self.e >= 0 {
            let int_len = self.e as usize + 1;
            if digits.len() <= int_len {
                let mut int = digits.to_string();
                int.extend(std::iter::repeat('0').take(int_len - digits.len()));
                (int, String::new())
            } else {
                (digits[..int_len].to_string(), digits[int_len..].to_string())
            }
        } else {
            let zeros = (-self.e - 1) as usize;
            ("0".to_string(), format!("{}{}", "0".repeat(zeros), digits))
        }
    }

    /// Whether the value is shown as `d.ddd` and an exponent: a small or a
    /// large value that is not an exact whole number.
    pub fn is_scientific(&self) -> bool {
        let whole = self.exact && self.e >= 0 && self.digits.len() as i64 <= self.e + 1;
        if whole {
            return false;
        }
        if self.exact {
            self.e < -20
        } else {
            self.e < -7 || self.e > 20
        }
    }

    pub fn mantissa(&self) -> (String, String) {
        let digits = self.digits.as_str();
        (digits[..1].to_string(), digits[1..].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(text: &str) -> Num {
        let (int, frac) = text.split_once('.').unwrap_or((text, ""));
        Num::from_digits(false, &format!("{int}{frac}"), frac.len(), 0).unwrap()
    }

    fn show(num: &Num) -> String {
        let d = num.to_decimal(16);
        let (i, f) = d.positional();
        let body = if f.is_empty() { i } else { format!("{i}.{f}") };
        if d.negative { format!("-{body}") } else { body }
    }

    #[test]
    fn point_one_plus_point_two_is_point_three() {
        let sum = n("0.1").add(&n("0.2")).unwrap();
        assert_eq!(sum, n("0.3"));
        assert_eq!(show(&sum), "0.3");
        assert!(!sum.is_approx());
    }

    #[test]
    fn one_third_is_cut_at_the_precision() {
        let third = Num::one().div(&Num::from_i64(3)).unwrap();
        assert_eq!(show(&third), "0.3333333333333333");
        assert!(third.to_decimal(16).rounded);
    }

    #[test]
    fn exact_decimals_are_shown_in_full() {
        let tiny = Num::one()
            .div(&Num::from_big(BigInt::from(2).pow(30)))
            .unwrap();
        assert_eq!(show(&tiny), "0.000000000931322574615478515625");
    }

    #[test]
    fn two_to_the_hundred_is_exact() {
        let big = Num::from_big(BigInt::from(2).pow(100));
        assert_eq!(show(&big), "1267650600228229401496703205376");
    }

    #[test]
    fn round_half_away_from_zero() {
        assert_eq!(show(&n("2.5").round_to(0).unwrap()), "3");
        assert_eq!(show(&n("-2.5").round_to(0).unwrap()), "-3");
        assert_eq!(show(&n("1.005").round_to(2).unwrap()), "1.01");
    }

    #[test]
    fn modulo_takes_the_divisor_sign() {
        assert_eq!(
            show(&Num::from_i64(-7).modulo(&Num::from_i64(3)).unwrap()),
            "2"
        );
        assert_eq!(
            show(&Num::from_i64(7).modulo(&Num::from_i64(-3)).unwrap()),
            "-2"
        );
    }

    #[test]
    fn a_value_near_a_simple_fraction_is_that_fraction() {
        let half = Rat::new(BigInt::one(), BigInt::from(2));
        let noisy = &half + Rat::new(BigInt::one(), pow10(68));
        assert_eq!(Num::approximate(noisy, None), Num::exact(half));
    }

    #[test]
    fn a_tiny_result_is_not_zeroed_without_a_scale() {
        let tiny = Rat::new(BigInt::one(), pow10(90));
        let n = Num::approximate(tiny.clone(), None);
        assert!(n.is_approx());
        assert_eq!(*n.rat(), tiny);
    }

    #[test]
    fn division_by_zero_fails() {
        assert_eq!(Num::one().div(&Num::zero()), Err(CalcError::DivideByZero));
    }
}
