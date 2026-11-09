//! Lists of words: "A, B and C".

use std::fmt;

use icu::list::ListFormatter;
use icu::list::options::{ListFormatterOptions, ListLength};

use super::Width;
use super::cache::cached;
use crate::locale::{Category, Locale};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    And,
    Or,
    Unit,
}

/// Items joined as the language joins them, with its own separators and
/// conjunction, and its rule for the comma before the last one.
///
/// ```
/// use bingux_ui::format::List;
/// use bingux_ui::locale::Locale;
///
/// let en = Locale::parse("en-GB").unwrap();
/// assert_eq!(List::and(["Mum", "Dad", "Sam"]).locale(&en).to_string(), "Mum, Dad and Sam");
/// assert_eq!(List::or(["tea", "coffee"]).locale(&en).to_string(), "tea or coffee");
/// ```
#[derive(Clone, Debug)]
pub struct List {
    items: Vec<String>,
    kind: Kind,
    width: Width,
    locale: Option<Locale>,
}

impl List {
    fn new(kind: Kind, items: impl IntoIterator<Item = impl AsRef<str>>) -> List {
        List {
            items: items
                .into_iter()
                .map(|item| item.as_ref().to_string())
                .collect(),
            kind,
            width: Width::Long,
            locale: None,
        }
    }

    /// "A, B and C": all of them.
    pub fn and(items: impl IntoIterator<Item = impl AsRef<str>>) -> List {
        List::new(Kind::And, items)
    }

    /// "A, B or C": one of them.
    pub fn or(items: impl IntoIterator<Item = impl AsRef<str>>) -> List {
        List::new(Kind::Or, items)
    }

    /// "A, B, C": a run of units or values, as in `5 ft, 10 in`.
    pub fn units(items: impl IntoIterator<Item = impl AsRef<str>>) -> List {
        List::new(Kind::Unit, items)
    }

    /// `Long` joins with the word ("A, B and C"), `Short` may use a symbol
    /// ("A, B, & C" in `en-US`), `Narrow` the least ("A, B, C").
    pub fn width(mut self, width: Width) -> List {
        self.width = width;
        self
    }

    /// The locale to write it for, instead of the person's language
    /// (`LC_MESSAGES`).
    pub fn locale(mut self, locale: &Locale) -> List {
        self.locale = Some(locale.clone());
        self
    }

    /// The text. An empty list is empty, one item is itself.
    pub fn format(&self) -> String {
        let locale = self
            .locale
            .clone()
            .unwrap_or_else(|| Locale::for_category(Category::Messages));
        let key = format!("list|{}|{:?}|{:?}", locale.tag(), self.kind, self.width);
        let formatter = cached(&key, || {
            let prefs = locale.icu().into();
            let length = match self.width {
                Width::Long => ListLength::Wide,
                Width::Short => ListLength::Short,
                Width::Narrow => ListLength::Narrow,
            };
            let options = ListFormatterOptions::default().with_length(length);
            match self.kind {
                Kind::And => ListFormatter::try_new_and(prefs, options),
                Kind::Or => ListFormatter::try_new_or(prefs, options),
                Kind::Unit => ListFormatter::try_new_unit(prefs, options),
            }
            .ok()
        });
        match formatter {
            Some(formatter) => formatter.format_to_string(self.items.iter().map(String::as_str)),
            None => self.items.join(", "),
        }
    }
}

impl fmt::Display for List {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}
