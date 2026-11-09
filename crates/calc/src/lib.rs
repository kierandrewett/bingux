//! The Bingux calculator engine.
//!
//! No UI. Arithmetic is exact: a number is a fraction of two integers of any
//! size, so `0.1 + 0.2` is `0.3` and `100!` has all its 158 digits. Functions
//! with no exact answer return 70 digits.

pub mod error;
mod eval;
pub mod format;
mod funcs;
mod lexer;
pub mod locale;
pub mod number;
mod parser;
pub mod programmer;
pub mod rates;
pub mod session;
pub mod store;
pub mod units;

pub use error::{CalcError, Result};
pub use eval::{Angle, Env, Preview, evaluate, preview};
pub use locale::{Grouping, Locale};
pub use number::{Decimal, Num};
