//! A message with its variables, and what a variable can hold.

use std::borrow::Cow;
use std::fmt;

use fluent_bundle::types::{FluentNumber, FluentNumberOptions};
use fluent_bundle::{FluentArgs, FluentValue};

use crate::format::Moment;

/// A value for one of a message's `$variables`: text, a number (counts pick
/// the plural variant and are written in the catalogue's language) or a
/// moment (written as a date by `DATETIME()` or by default).
#[derive(Debug)]
pub struct Arg(pub(super) FluentValue<'static>);

impl Arg {
    /// Any value as text: `Arg::text(path.display())`.
    pub fn text(value: impl fmt::Display) -> Arg {
        Arg(FluentValue::String(Cow::Owned(value.to_string())))
    }

    /// A number with at least `least` and at most `most` digits after the
    /// point: a size in GB, a temperature.
    pub fn number(value: f64, least: usize, most: usize) -> Arg {
        let options = FluentNumberOptions {
            minimum_fraction_digits: Some(least),
            maximum_fraction_digits: Some(most.max(least)),
            ..FluentNumberOptions::default()
        };
        Arg(FluentValue::Number(FluentNumber::new(value, options)))
    }
}

impl From<&str> for Arg {
    fn from(value: &str) -> Arg {
        Arg(FluentValue::String(Cow::Owned(value.to_string())))
    }
}

impl From<&String> for Arg {
    fn from(value: &String) -> Arg {
        Arg::from(value.as_str())
    }
}

impl From<String> for Arg {
    fn from(value: String) -> Arg {
        Arg(FluentValue::String(Cow::Owned(value)))
    }
}

impl From<Cow<'_, str>> for Arg {
    fn from(value: Cow<'_, str>) -> Arg {
        Arg(FluentValue::String(Cow::Owned(value.into_owned())))
    }
}

macro_rules! number_arg {
    ($($kind:ty),*) => {$(
        impl From<$kind> for Arg {
            fn from(value: $kind) -> Arg {
                // Integers past 2^53 lose their last digits; a count never gets there.
                Arg(FluentValue::Number(FluentNumber::new(value as f64, FluentNumberOptions::default())))
            }
        }
        impl From<&$kind> for Arg {
            fn from(value: &$kind) -> Arg {
                Arg::from(*value)
            }
        }
    )*};
}

number_arg!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, f32, f64);

impl From<Moment> for Arg {
    fn from(value: Moment) -> Arg {
        Arg(FluentValue::Custom(Box::new(
            super::localiser::DateValue::new(value),
        )))
    }
}

/// A message to format: its ID and its variables. [`tr!`](crate::tr) builds
/// one; use it directly when the ID is chosen at run time.
///
/// ```ignore
/// let id = if folder { i18n_id!("files-delete-folder") } else { i18n_id!("files-delete-file") };
/// let text = Message::new(id).arg("name", &name).format();
/// ```
pub struct Message {
    id: Cow<'static, str>,
    args: FluentArgs<'static>,
}

impl Message {
    /// The message `id`, or the attribute `id.attribute`.
    pub fn new(id: impl Into<Cow<'static, str>>) -> Message {
        Message {
            id: id.into(),
            args: FluentArgs::new(),
        }
    }

    /// Sets `$name` to `value`.
    pub fn arg(mut self, name: &'static str, value: impl Into<Arg>) -> Message {
        self.args.set(name, value.into().0);
        self
    }

    /// The ID asked for.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The ID asked for, without the variables.
    pub fn into_id(self) -> Cow<'static, str> {
        self.id
    }

    /// The text in the person's language.
    pub fn format(&self) -> String {
        super::localiser::format(&self.id, Some(&self.args))
    }

    #[cfg(test)]
    pub(super) fn args(&self) -> &FluentArgs<'static> {
        &self.args
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Message")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}
