//! What can go wrong in a calculation, worded for the person who typed it.

use std::fmt;

/// A calculation that has no answer. `Display` is the sentence the
/// application shows: it says what failed and, where it helps, what to do.
#[derive(Clone, Debug, PartialEq)]
pub enum CalcError {
    /// A division by zero, or a remainder, or a power of zero below zero.
    DivideByZero,
    /// The answer is not a real number (the square root of -1, the logarithm
    /// of zero).
    NotReal,
    /// The function has no value there (the tangent of 90 degrees).
    Undefined(&'static str),
    /// The input has a mistake at this character position.
    Syntax { at: usize, message: String },
    /// The input stops where more is needed (`5 +`, `sin(`).
    Incomplete,
    /// The result would have too many digits to hold.
    TooLarge,
    /// A name the calculator does not know.
    Unknown(String),
    /// A function that works on whole numbers only.
    NeedsInteger(&'static str),
    /// A function asked about a value outside its domain.
    Domain(&'static str),
    /// Units that cannot be converted into one another, or are not known.
    Unit(String),
    /// A currency rate is needed and none is available.
    NoRate(String),
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CalcError::DivideByZero => f.write_str("Cannot divide by zero. Change the divisor."),
            CalcError::NotReal => f.write_str("The result is not a real number. Check the values."),
            CalcError::Undefined(what) => write!(f, "{what} has no value. Change the value."),
            CalcError::Syntax { message, .. } => f.write_str(message),
            CalcError::Incomplete => {
                f.write_str("The expression is not finished. Complete it, then try again.")
            }
            CalcError::TooLarge => {
                f.write_str("The result has too many digits. Use smaller numbers.")
            }
            CalcError::Unknown(name) => write!(
                f,
                "\u{201c}{name}\u{201d} is not a known name. Check the spelling."
            ),
            CalcError::NeedsInteger(what) => {
                write!(f, "{what} needs a whole number. Change the value.")
            }
            CalcError::Domain(what) => f.write_str(what),
            CalcError::Unit(message) => f.write_str(message),
            CalcError::NoRate(code) => write!(
                f,
                "No exchange rate for {code} yet. Connect to the internet to update the rates."
            ),
        }
    }
}

impl std::error::Error for CalcError {}

pub type Result<T> = std::result::Result<T, CalcError>;
