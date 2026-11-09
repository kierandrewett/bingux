//! Functions of a number: powers, roots, logarithms, trigonometry,
//! factorials. Exact where the answer is exact, 70 digits where it is not.
//!
//! The transcendental ones run in fixed point on big integers (a value is
//! `v / 10^w`), with guard digits beyond the 70 that are kept.

use std::sync::OnceLock;

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::error::{CalcError, Result};
use crate::number::{MAX_BITS, Num, Rat, SIG_DIGITS, magnitude10, pow10, rat};

/// Working digits of the fixed-point functions: the 70 kept and guard digits.
const WORK: usize = 110;

/// Factorials, permutations and combinations stop here (about 100,000 digits).
const MAX_FACTORIAL: u64 = 25_000;

struct Fx {
    w: usize,
    one: BigInt,
}

impl Fx {
    fn new(w: usize) -> Fx {
        Fx { w, one: pow10(w) }
    }

    fn of(&self, r: &Rat) -> BigInt {
        r.numer() * &self.one / r.denom()
    }

    fn rat(&self, v: BigInt) -> Rat {
        Rat::new(v, self.one.clone())
    }

    fn mul(&self, a: &BigInt, b: &BigInt) -> BigInt {
        a * b / &self.one
    }

    fn div(&self, a: &BigInt, b: &BigInt) -> BigInt {
        a * &self.one / b
    }

    /// sqrt of a fixed-point value.
    fn sqrt(&self, v: &BigInt) -> BigInt {
        (v * &self.one).sqrt()
    }
}

/// atan(1/n) in fixed point.
fn atan_inv(n: u32, fx: &Fx) -> BigInt {
    let n = BigInt::from(n);
    let n2 = &n * &n;
    let mut term = &fx.one / &n;
    let mut sum = term.clone();
    let mut k: u32 = 1;
    loop {
        term /= &n2;
        if term.is_zero() {
            break;
        }
        let part = &term / BigInt::from(2 * k + 1);
        if k % 2 == 1 {
            sum -= part;
        } else {
            sum += part;
        }
        k += 1;
    }
    sum
}

/// pi with `w` digits after the point.
fn pi_fx(fx: &Fx) -> BigInt {
    static CACHE: OnceLock<BigInt> = OnceLock::new();
    const CACHED: usize = 420;
    if fx.w <= CACHED {
        let cached = CACHE.get_or_init(|| {
            let g = Fx::new(CACHED + 8);
            (atan_inv(5, &g) * 16 - atan_inv(239, &g) * 4) / pow10(8)
        });
        return cached / pow10(CACHED - fx.w);
    }
    let g = Fx::new(fx.w + 8);
    (atan_inv(5, &g) * 16 - atan_inv(239, &g) * 4) / pow10(8)
}

/// atanh(t) = t + t^3/3 + t^5/5 + ... for a small `t`.
fn atanh_series(fx: &Fx, t: &BigInt) -> BigInt {
    let t2 = fx.mul(t, t);
    let mut term = t.clone();
    let mut sum = t.clone();
    let mut k: u32 = 1;
    loop {
        term = fx.mul(&term, &t2);
        let part = &term / BigInt::from(2 * k + 1);
        if part.is_zero() {
            break;
        }
        sum += part;
        k += 1;
    }
    sum
}

fn exp_fx(fx: &Fx, x: &BigInt) -> BigInt {
    let half = &fx.one / 2;
    let mut halvings = 0u32;
    let mut r = x.clone();
    while r.abs() > half {
        r /= 2;
        halvings += 1;
    }
    let mut term = fx.one.clone();
    let mut sum = fx.one.clone();
    let mut k = 1u32;
    loop {
        term = fx.mul(&term, &r) / BigInt::from(k);
        if term.is_zero() {
            break;
        }
        sum += &term;
        k += 1;
    }
    for _ in 0..halvings {
        sum = fx.mul(&sum, &sum);
    }
    sum
}

pub fn pi() -> Num {
    let fx = Fx::new(WORK);
    Num::approximate(fx.rat(pi_fx(&fx)), None)
}

pub fn e() -> Num {
    exp(&Num::one()).unwrap_or_else(|_| Num::one())
}

/// The golden ratio, (1 + sqrt 5) / 2.
pub fn phi() -> Num {
    let five = Num::from_i64(5);
    sqrt(&five)
        .and_then(|root| root.add(&Num::one()))
        .and_then(|sum| sum.div(&Num::from_i64(2)))
        .unwrap_or_else(|_| Num::one())
}

pub fn exp(x: &Num) -> Result<Num> {
    if x.is_zero() {
        return Ok(Num::one());
    }
    if x.rat().abs() > rat(100_000) {
        return Err(CalcError::TooLarge);
    }
    let fx = Fx::new(WORK + 12);
    let v = exp_fx(&fx, &fx.of(&x.rat().abs()));
    let r = if x.is_negative() {
        Rat::new(fx.one.clone(), v)
    } else {
        fx.rat(v)
    };
    Ok(Num::approximate(r, None))
}

pub fn ln(x: &Num) -> Result<Num> {
    if x.is_zero() {
        return Err(CalcError::Undefined("The logarithm of zero"));
    }
    if x.is_negative() {
        return Err(CalcError::NotReal);
    }
    if *x.rat() == Rat::one() {
        return Ok(Num::zero());
    }
    let fx = Fx::new(WORK + 10);
    let r = x.rat();
    let mut k = r.numer().bits() as i64 - r.denom().bits() as i64;
    let scaled = |k: i64| -> Rat {
        if k >= 0 {
            r / rat(BigInt::one() << k as usize)
        } else {
            r * rat(BigInt::one() << (-k) as usize)
        }
    };
    let mut m = scaled(k);
    if m > Rat::new(BigInt::from(3), BigInt::from(2)) {
        k += 1;
        m = scaled(k);
    } else if m < Rat::new(BigInt::from(3), BigInt::from(4)) {
        k -= 1;
        m = scaled(k);
    }
    let t = (&m - Rat::one()) / (&m + Rat::one());
    let two = BigInt::from(2);
    let ln_m = atanh_series(&fx, &fx.of(&t)) * &two;
    let third = &fx.one / BigInt::from(3);
    let ln2 = atanh_series(&fx, &third) * &two;
    let total = ln_m + ln2 * BigInt::from(k);
    Ok(Num::approximate(fx.rat(total), None))
}

/// The base-10 logarithm; exact for powers of ten.
pub fn log10(x: &Num) -> Result<Num> {
    log_base(x, &Num::from_i64(10))
}

pub fn log_base(x: &Num, base: &Num) -> Result<Num> {
    if *base.rat() == Rat::one() || !base.rat().is_positive() {
        return Err(CalcError::Domain(
            "A logarithm needs a base above 0 that is not 1.",
        ));
    }
    if *x.rat() == Rat::one() {
        return Ok(Num::zero());
    }
    // An exact power of the base: log_b(b^k) = k.
    if let (Some(k), false) = (
        exact_log(x.rat(), base.rat()),
        x.is_approx() || base.is_approx(),
    ) {
        return Ok(Num::from_i64(k));
    }
    ln(x)?.div(&ln(base)?)
}

/// `k` when `x = base^k` for an integer `k`.
fn exact_log(x: &Rat, base: &Rat) -> Option<i64> {
    if !x.is_positive() {
        return None;
    }
    let (mut value, mut k) = (Rat::one(), 0i64);
    if x >= &Rat::one() {
        while &value < x && k < 4000 {
            value *= base;
            k += 1;
        }
    } else {
        while &value > x && k > -4000 {
            value /= base;
            k -= 1;
        }
    }
    (&value == x).then_some(k)
}

pub fn sqrt(x: &Num) -> Result<Num> {
    if x.is_negative() {
        return Err(CalcError::NotReal);
    }
    if x.is_zero() {
        return Ok(Num::zero());
    }
    let r = x.rat();
    let (a, b) = (r.numer().sqrt(), r.denom().sqrt());
    if &a * &a == *r.numer() && &b * &b == *r.denom() {
        return Ok(Num::exact(Rat::new(a, b)));
    }
    let w = (SIG_DIGITS as i64 + 12 - magnitude10(r) / 2).clamp(80, 200_000) as usize;
    let scaled = r.numer() * pow10(2 * w) / r.denom();
    Ok(Num::approximate(Rat::new(scaled.sqrt(), pow10(w)), None))
}

/// The `n`th root, real: an odd root of a negative number is negative.
pub fn root(x: &Num, n: &Num) -> Result<Num> {
    let Some(n) = n.to_big().and_then(|n| n.to_u32()).filter(|n| *n >= 1) else {
        return Err(CalcError::NeedsInteger("A root"));
    };
    if n == 1 {
        return Ok(x.clone());
    }
    if x.is_negative() {
        return if n % 2 == 1 {
            Ok(root(&x.neg(), &Num::from_i64(n as i64))?.neg())
        } else {
            Err(CalcError::NotReal)
        };
    }
    if x.is_zero() {
        return Ok(Num::zero());
    }
    if n == 2 {
        return sqrt(x);
    }
    let r = x.rat();
    let (a, b) = (r.numer().nth_root(n), r.denom().nth_root(n));
    if a.pow(n) == *r.numer() && b.pow(n) == *r.denom() {
        return Ok(Num::exact(Rat::new(a, b)));
    }
    exp(&ln(x)?.div(&Num::from_i64(n as i64))?)
}

/// `x` to the power `y`.
pub fn pow(x: &Num, y: &Num) -> Result<Num> {
    if y.is_zero() {
        return Ok(Num::one());
    }
    if x.is_zero() {
        return if y.is_negative() {
            Err(CalcError::DivideByZero)
        } else {
            Ok(Num::zero())
        };
    }
    if *x.rat() == Rat::one() {
        return Ok(Num::one());
    }
    if y.is_integer() {
        let Some(e) = y.rat().to_integer().abs().to_u64() else {
            return Err(CalcError::TooLarge);
        };
        let r = x.rat();
        let base_bits = r.numer().bits().max(r.denom().bits());
        let minus_one = *r == -Rat::one();
        if !minus_one && base_bits.saturating_mul(e) > MAX_BITS {
            return Err(CalcError::TooLarge);
        }
        let e32 = if minus_one { (e % 2) as u32 } else { e as u32 };
        let powered = Rat::new(r.numer().pow(e32), r.denom().pow(e32));
        let powered = if y.is_negative() {
            powered.recip()
        } else {
            powered
        };
        return if x.is_approx() {
            Ok(Num::approximate(powered, None))
        } else {
            Ok(Num::exact(powered))
        };
    }
    // A fractional exponent p/q: exact when the root is.
    if !y.is_approx() {
        let (p, q) = (y.rat().numer().clone(), y.rat().denom().clone());
        if x.is_negative() && q.is_even() {
            return Err(CalcError::NotReal);
        }
        if let Some(q32) = q.to_u32() {
            let rooted = root(x, &Num::from_i64(q32 as i64))?;
            if !rooted.is_approx() {
                return pow(&rooted, &Num::from_big(p));
            }
        }
    }
    if x.is_negative() {
        return Err(CalcError::NotReal);
    }
    exp(&ln(x)?.mul(y)?)
}

fn round_div(a: &BigInt, b: &BigInt) -> BigInt {
    // round(a / b) for a positive b, half up.
    let two = BigInt::from(2);
    (a * &two + b).div_floor(&(b * &two))
}

/// sin and cos of `x` radians, in fixed point at `fx`.
fn sin_cos(fx: &Fx, x: &Rat) -> (BigInt, BigInt) {
    let half_pi = pi_fx(fx) / 2;
    let xv = fx.of(x);
    let n = round_div(&xv, &half_pi);
    let r = &xv - &n * &half_pi;
    let r2 = fx.mul(&r, &r);
    // sin r
    let mut term = r.clone();
    let mut sin = r.clone();
    let mut k = 1u64;
    loop {
        term = -fx.mul(&term, &r2) / BigInt::from((2 * k) * (2 * k + 1));
        if term.is_zero() {
            break;
        }
        sin += &term;
        k += 1;
    }
    // cos r
    let mut term = fx.one.clone();
    let mut cos = fx.one.clone();
    let mut k = 1u64;
    loop {
        term = -fx.mul(&term, &r2) / BigInt::from((2 * k - 1) * (2 * k));
        if term.is_zero() {
            break;
        }
        cos += &term;
        k += 1;
    }
    match n.mod_floor(&BigInt::from(4)).to_u32().unwrap_or(0) {
        0 => (sin, cos),
        1 => (cos, -sin),
        2 => (-sin, -cos),
        _ => (-cos, sin),
    }
}

/// The digits a trigonometric function needs for an angle of this size.
fn trig_precision(x: &Num) -> Result<usize> {
    let mag = magnitude10(x.rat());
    if mag > 50_000 {
        return Err(CalcError::TooLarge);
    }
    Ok(WORK + 10 + mag.unsigned_abs().min(50_000) as usize)
}

/// `(sin, cos)` of an angle in radians as exact-or-approximate numbers.
fn sin_cos_num(x: &Num) -> Result<(Num, Num, BigInt, Fx)> {
    let fx = Fx::new(trig_precision(x)?);
    let (s, c) = sin_cos(&fx, x.rat());
    let scale = x.rat().abs();
    let sin = Num::approximate(fx.rat(s.clone()), Some(&scale));
    let cos = Num::approximate(fx.rat(c.clone()), Some(&scale));
    Ok((sin, cos, c, fx))
}

pub fn sin(x: &Num) -> Result<Num> {
    if x.is_zero() {
        return Ok(Num::zero());
    }
    Ok(sin_cos_num(x)?.0)
}

pub fn cos(x: &Num) -> Result<Num> {
    if x.is_zero() {
        return Ok(Num::one());
    }
    Ok(sin_cos_num(x)?.1)
}

pub fn tan(x: &Num) -> Result<Num> {
    if x.is_zero() {
        return Ok(Num::zero());
    }
    let (sin, cos, c, fx) = sin_cos_num(x)?;
    // Within 1e-45 of zero the cosine is rounding: the tangent has no value.
    if c.abs() * pow10(45) < fx.one {
        return Err(CalcError::Undefined("The tangent of that angle"));
    }
    sin.div(&cos)
}

pub fn atan(x: &Num) -> Result<Num> {
    if x.is_zero() {
        return Ok(Num::zero());
    }
    let fx = Fx::new(WORK + 10);
    let negative = x.is_negative();
    let mut a = fx.of(&x.rat().abs());
    let invert = a > fx.one;
    if invert {
        a = fx.div(&fx.one, &a);
    }
    for _ in 0..3 {
        let root = fx.sqrt(&(&fx.one + fx.mul(&a, &a)));
        a = fx.div(&a, &(&fx.one + root));
    }
    let a2 = fx.mul(&a, &a);
    let mut term = a.clone();
    let mut sum = a;
    let mut k = 1u64;
    loop {
        term = -fx.mul(&term, &a2);
        let part = &term / BigInt::from(2 * k + 1);
        if part.is_zero() {
            break;
        }
        sum += part;
        k += 1;
    }
    let mut result: BigInt = sum * BigInt::from(8);
    if invert {
        result = pi_fx(&fx) / 2 - result;
    }
    if negative {
        result = -result;
    }
    Ok(Num::approximate(fx.rat(result), None))
}

pub fn asin(x: &Num) -> Result<Num> {
    let one = Num::one();
    if x.rat().abs() > Rat::one() {
        return Err(CalcError::Domain("The arcsine needs a value from -1 to 1."));
    }
    if x.is_zero() {
        return Ok(Num::zero());
    }
    if x.rat().abs() == Rat::one() {
        let quarter = pi().div(&Num::from_i64(2))?;
        return Ok(if x.is_negative() {
            quarter.neg()
        } else {
            quarter
        });
    }
    let side = sqrt(&one.sub(&x.mul(x)?)?)?;
    atan(&x.div(&side)?)
}

pub fn acos(x: &Num) -> Result<Num> {
    if x.rat().abs() > Rat::one() {
        return Err(CalcError::Domain(
            "The arccosine needs a value from -1 to 1.",
        ));
    }
    if *x.rat() == Rat::one() {
        return Ok(Num::zero());
    }
    pi().div(&Num::from_i64(2))?.sub(&asin(x)?)
}

pub fn sinh(x: &Num) -> Result<Num> {
    if x.is_zero() {
        return Ok(Num::zero());
    }
    exp(x)?.sub(&exp(&x.neg())?)?.div(&Num::from_i64(2))
}

pub fn cosh(x: &Num) -> Result<Num> {
    exp(x)?.add(&exp(&x.neg())?)?.div(&Num::from_i64(2))
}

pub fn tanh(x: &Num) -> Result<Num> {
    if x.is_zero() {
        return Ok(Num::zero());
    }
    let (up, down) = (exp(x)?, exp(&x.neg())?);
    up.sub(&down)?.div(&up.add(&down)?)
}

pub fn asinh(x: &Num) -> Result<Num> {
    if x.is_zero() {
        return Ok(Num::zero());
    }
    // Odd function: the formula below loses digits for a negative argument.
    let a = x.abs();
    let value = ln(&a.add(&sqrt(&a.mul(&a)?.add(&Num::one())?)?)?)?;
    Ok(if x.is_negative() { value.neg() } else { value })
}

pub fn acosh(x: &Num) -> Result<Num> {
    if x.rat() < &Rat::one() {
        return Err(CalcError::Domain(
            "The inverse hyperbolic cosine needs a value of 1 or more.",
        ));
    }
    ln(&x.add(&sqrt(&x.mul(x)?.sub(&Num::one())?)?)?)
}

pub fn atanh(x: &Num) -> Result<Num> {
    if x.rat().abs() >= Rat::one() {
        return Err(CalcError::Domain(
            "The inverse hyperbolic tangent needs a value between -1 and 1.",
        ));
    }
    let one = Num::one();
    ln(&one.add(x)?.div(&one.sub(x)?)?)?.div(&Num::from_i64(2))
}

fn product(lo: u64, hi: u64) -> BigInt {
    match hi.checked_sub(lo) {
        None => BigInt::one(),
        Some(0) => BigInt::from(lo),
        Some(1) => BigInt::from(lo) * BigInt::from(hi),
        Some(_) => {
            let mid = lo + (hi - lo) / 2;
            product(lo, mid) * product(mid + 1, hi)
        }
    }
}

fn whole(x: &Num, what: &'static str) -> Result<u64> {
    let n = x.to_big().ok_or(CalcError::NeedsInteger(what))?;
    if n.is_negative() {
        return Err(CalcError::Domain("That needs a number of 0 or more."));
    }
    n.to_u64()
        .filter(|n| *n <= MAX_FACTORIAL)
        .ok_or(CalcError::TooLarge)
}

pub fn factorial(x: &Num) -> Result<Num> {
    let n = whole(x, "A factorial")?;
    Ok(Num::from_big(product(2, n)))
}

/// Ordered selections: `n! / (n - k)!`.
pub fn permutations(n: &Num, k: &Num) -> Result<Num> {
    let (n, k) = (whole(n, "nPr")?, whole(k, "nPr")?);
    if k > n {
        return Err(CalcError::Domain("nPr needs k not above n."));
    }
    Ok(Num::from_big(product(n - k + 1, n)))
}

/// Unordered selections: `n! / (k! (n - k)!)`.
pub fn combinations(n: &Num, k: &Num) -> Result<Num> {
    let (n, k) = (whole(n, "nCr")?, whole(k, "nCr")?);
    if k > n {
        return Err(CalcError::Domain("nCr needs k not above n."));
    }
    let k = k.min(n - k);
    Ok(Num::from_big(product(n - k + 1, n) / product(2, k)))
}

pub fn gcd(a: &Num, b: &Num) -> Result<Num> {
    let (a, b) = (
        a.to_big().ok_or(CalcError::NeedsInteger("gcd"))?,
        b.to_big().ok_or(CalcError::NeedsInteger("gcd"))?,
    );
    Ok(Num::from_big(a.gcd(&b)))
}

pub fn lcm(a: &Num, b: &Num) -> Result<Num> {
    let (a, b) = (
        a.to_big().ok_or(CalcError::NeedsInteger("lcm"))?,
        b.to_big().ok_or(CalcError::NeedsInteger("lcm"))?,
    );
    Ok(Num::from_big(a.lcm(&b)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(n: i64) -> Num {
        Num::from_i64(n)
    }

    fn text(n: &Num, digits: usize) -> String {
        let d = n.to_decimal(digits);
        let (i, f) = d.positional();
        let body = if f.is_empty() { i } else { format!("{i}.{f}") };
        if d.negative { format!("-{body}") } else { body }
    }

    #[test]
    fn pi_to_fifty_digits() {
        assert_eq!(
            text(&pi(), 50),
            "3.1415926535897932384626433832795028841971693993751"
        );
    }

    #[test]
    fn e_to_forty_digits() {
        assert_eq!(text(&e(), 40), "2.718281828459045235360287471352662497757");
    }

    #[test]
    fn sqrt_of_two() {
        assert_eq!(
            text(&sqrt(&int(2)).unwrap(), 40),
            "1.41421356237309504880168872420969807857"
        );
    }

    #[test]
    fn exact_roots_stay_exact() {
        assert_eq!(sqrt(&int(144)).unwrap(), int(12));
        assert!(!sqrt(&int(144)).unwrap().is_approx());
        let quarter = Num::from_ratio(1.into(), 4.into()).unwrap();
        assert_eq!(
            sqrt(&quarter).unwrap(),
            Num::from_ratio(1.into(), 2.into()).unwrap()
        );
        assert_eq!(root(&int(-27), &int(3)).unwrap(), int(-3));
        assert_eq!(root(&int(81), &int(4)).unwrap(), int(3));
    }

    #[test]
    fn the_root_of_a_negative_is_not_real() {
        assert_eq!(sqrt(&int(-1)), Err(CalcError::NotReal));
        assert_eq!(root(&int(-16), &int(4)), Err(CalcError::NotReal));
    }

    #[test]
    fn sqrt_two_squared_is_two() {
        let two = sqrt(&int(2)).unwrap().mul(&sqrt(&int(2)).unwrap()).unwrap();
        assert_eq!(two, int(2));
        assert!(!two.is_approx());
    }

    #[test]
    fn natural_log() {
        assert_eq!(
            text(&ln(&int(2)).unwrap(), 40),
            "0.6931471805599453094172321214581765680755"
        );
        assert_eq!(ln(&int(1)).unwrap(), int(0));
        assert_eq!(ln(&e()).unwrap(), int(1));
    }

    #[test]
    fn log_of_a_power_of_ten_is_exact() {
        assert_eq!(log10(&int(1000)).unwrap(), int(3));
        assert!(!log10(&int(1000)).unwrap().is_approx());
        let milli = Num::from_ratio(1.into(), 1000.into()).unwrap();
        assert_eq!(log10(&milli).unwrap(), int(-3));
        assert_eq!(log_base(&int(8), &int(2)).unwrap(), int(3));
    }

    #[test]
    fn log_of_zero_and_negatives() {
        assert!(matches!(ln(&int(0)), Err(CalcError::Undefined(_))));
        assert_eq!(ln(&int(-1)), Err(CalcError::NotReal));
    }

    #[test]
    fn sine_and_cosine_of_pi() {
        assert_eq!(sin(&pi()).unwrap(), int(0));
        assert_eq!(cos(&pi()).unwrap(), int(-1));
    }

    #[test]
    fn sine_of_one_radian() {
        assert_eq!(
            text(&sin(&int(1)).unwrap(), 40),
            "0.8414709848078965066525023216302989996226"
        );
    }

    #[test]
    fn a_tiny_sine_is_not_zeroed() {
        let tiny = Num::from_ratio(1.into(), pow10(65)).unwrap();
        let s = sin(&tiny).unwrap();
        assert!(!s.is_zero());
    }

    #[test]
    fn tangent_at_a_right_angle_is_undefined() {
        let half = pi().div(&int(2)).unwrap();
        assert!(matches!(tan(&half), Err(CalcError::Undefined(_))));
    }

    #[test]
    fn arctangent_of_one_is_a_quarter_pi() {
        let quarter = pi().div(&int(4)).unwrap();
        let a = atan(&int(1)).unwrap();
        assert_eq!(text(&a, 40), text(&quarter, 40));
    }

    #[test]
    fn arcsine_domain() {
        assert!(matches!(asin(&int(2)), Err(CalcError::Domain(_))));
        assert_eq!(asin(&int(0)).unwrap(), int(0));
        assert_eq!(acos(&int(1)).unwrap(), int(0));
    }

    #[test]
    fn hyperbolic_round_trips() {
        let x = Num::from_ratio(3.into(), 4.into()).unwrap();
        assert_eq!(text(&sinh(&asinh(&x).unwrap()).unwrap(), 30), "0.75");
        assert_eq!(text(&tanh(&atanh(&x).unwrap()).unwrap(), 30), "0.75");
    }

    #[test]
    fn integer_powers_are_exact() {
        assert_eq!(
            text(&pow(&int(2), &int(100)).unwrap(), 1),
            "1267650600228229401496703205376"
        );
        assert_eq!(
            pow(&int(2), &int(-2)).unwrap(),
            Num::from_ratio(1.into(), 4.into()).unwrap()
        );
        assert_eq!(pow(&int(-2), &int(3)).unwrap(), int(-8));
        assert_eq!(pow(&int(0), &int(0)).unwrap(), int(1));
    }

    #[test]
    fn fractional_powers() {
        assert_eq!(
            pow(&int(8), &Num::from_ratio(1.into(), 3.into()).unwrap()).unwrap(),
            int(2)
        );
        assert_eq!(
            pow(&int(4), &Num::from_ratio(3.into(), 2.into()).unwrap()).unwrap(),
            int(8)
        );
        assert_eq!(
            pow(&int(-8), &Num::from_ratio(1.into(), 3.into()).unwrap()).unwrap(),
            int(-2)
        );
        assert_eq!(
            pow(&int(-4), &Num::from_ratio(1.into(), 2.into()).unwrap()),
            Err(CalcError::NotReal)
        );
        let two_root_two = pow(&int(2), &sqrt(&int(2)).unwrap()).unwrap();
        assert_eq!(text(&two_root_two, 30), "2.66514414269022518865029724987");
    }

    #[test]
    fn zero_to_a_negative_power_divides_by_zero() {
        assert_eq!(pow(&int(0), &int(-1)), Err(CalcError::DivideByZero));
    }

    #[test]
    fn huge_powers_are_refused() {
        assert_eq!(pow(&int(10), &int(10_000_000)), Err(CalcError::TooLarge));
    }

    #[test]
    fn factorials() {
        assert_eq!(factorial(&int(0)).unwrap(), int(1));
        assert_eq!(factorial(&int(5)).unwrap(), int(120));
        assert_eq!(
            text(&factorial(&int(25)).unwrap(), 1),
            "15511210043330985984000000"
        );
        assert_eq!(
            factorial(&int(-1)).unwrap_err(),
            CalcError::Domain("That needs a number of 0 or more.")
        );
        assert!(matches!(
            factorial(&Num::from_ratio(1.into(), 2.into()).unwrap()),
            Err(CalcError::NeedsInteger(_))
        ));
        assert_eq!(factorial(&int(1_000_000)), Err(CalcError::TooLarge));
    }

    #[test]
    fn a_thousand_factorial_has_2568_digits() {
        let f = factorial(&int(1000)).unwrap();
        assert_eq!(f.to_big().unwrap().to_string().len(), 2568);
    }

    #[test]
    fn combinations_and_permutations() {
        assert_eq!(combinations(&int(52), &int(5)).unwrap(), int(2_598_960));
        assert_eq!(permutations(&int(10), &int(3)).unwrap(), int(720));
        assert_eq!(combinations(&int(5), &int(0)).unwrap(), int(1));
        assert!(matches!(
            combinations(&int(3), &int(5)),
            Err(CalcError::Domain(_))
        ));
    }

    #[test]
    fn gcd_and_lcm() {
        assert_eq!(gcd(&int(12), &int(18)).unwrap(), int(6));
        assert_eq!(lcm(&int(4), &int(6)).unwrap(), int(12));
    }
}
