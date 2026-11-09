//! Words that change with a count: "1 file", "2 files", "5 файлов".

use std::fmt;

use icu::plurals::{PluralCategory as IcuCategory, PluralRules};

use super::cache::cached;
use super::number::{Number, Quantity};
use crate::locale::{Category, Locale};

/// The class of a count in a language's plural rules. English has `One` (1)
/// and `Other`; Russian `One`, `Few`, `Many` and `Other`; Arabic all six;
/// Japanese only `Other`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl From<IcuCategory> for PluralCategory {
    fn from(category: IcuCategory) -> PluralCategory {
        match category {
            IcuCategory::Zero => PluralCategory::Zero,
            IcuCategory::One => PluralCategory::One,
            IcuCategory::Two => PluralCategory::Two,
            IcuCategory::Few => PluralCategory::Few,
            IcuCategory::Many => PluralCategory::Many,
            _ => PluralCategory::Other,
        }
    }
}

/// Which rules to read a count by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PluralKind {
    /// How many: "1 file", "2 files".
    #[default]
    Cardinal,
    /// Which one in order: "1st", "2nd", "3rd", "4th".
    Ordinal,
}

/// The category of `count` in `locale` (the person's language when `None`).
pub fn plural_category(
    count: impl Into<Quantity>,
    kind: PluralKind,
    locale: Option<&Locale>,
) -> PluralCategory {
    let locale = locale
        .cloned()
        .unwrap_or_else(|| Locale::for_category(Category::Messages));
    let key = format!("plural|{}|{kind:?}", locale.tag());
    let rules = cached(&key, || match kind {
        PluralKind::Cardinal => PluralRules::try_new_cardinal(locale.icu().into()).ok(),
        PluralKind::Ordinal => PluralRules::try_new_ordinal(locale.icu().into()).ok(),
    });
    let count = count.into();
    match (rules, count.decimal()) {
        (Some(rules), Some(decimal)) => rules.category_for(decimal).into(),
        _ => PluralCategory::Other,
    }
}

/// The text for a count, one wording per class of count.
///
/// `{n}` in a wording is the count, written for the locale. A class with no
/// wording uses `other`.
///
/// ```
/// use bingux_ui::format::Plural;
/// use bingux_ui::locale::Locale;
///
/// let en = Locale::parse("en-GB").unwrap();
/// let files = |n: i64| {
///     Plural::new(n, "{n} files").none("No files").one("{n} file").locale(&en).to_string()
/// };
/// assert_eq!(files(0), "No files");
/// assert_eq!(files(1), "1 file");
/// assert_eq!(files(1200), "1,200 files");
/// ```
///
/// This picks a wording in code. Messages that are translated hold their
/// plural forms in the catalogue instead, where each language writes its own.
#[derive(Clone, Debug)]
pub struct Plural {
    count: Quantity,
    kind: PluralKind,
    none: Option<String>,
    forms: [Option<String>; 5],
    other: String,
    locale: Option<Locale>,
}

impl Plural {
    /// The wording for every count that has none of its own: `other`.
    pub fn new(count: impl Into<Quantity>, other: &str) -> Plural {
        Plural {
            count: count.into(),
            kind: PluralKind::Cardinal,
            none: None,
            forms: Default::default(),
            other: other.to_string(),
            locale: None,
        }
    }

    /// For a count of exactly zero, in every language: "No files". (`zero`
    /// is the class of languages that have one.)
    pub fn none(mut self, text: &str) -> Plural {
        self.none = Some(text.to_string());
        self
    }

    pub fn zero(self, text: &str) -> Plural {
        self.form(0, text)
    }

    pub fn one(self, text: &str) -> Plural {
        self.form(1, text)
    }

    pub fn two(self, text: &str) -> Plural {
        self.form(2, text)
    }

    pub fn few(self, text: &str) -> Plural {
        self.form(3, text)
    }

    pub fn many(self, text: &str) -> Plural {
        self.form(4, text)
    }

    fn form(mut self, index: usize, text: &str) -> Plural {
        self.forms[index] = Some(text.to_string());
        self
    }

    /// Read the count as an order (1st, 2nd) rather than an amount.
    pub fn ordinal(mut self) -> Plural {
        self.kind = PluralKind::Ordinal;
        self
    }

    pub fn locale(mut self, locale: &Locale) -> Plural {
        self.locale = Some(locale.clone());
        self
    }

    /// The text, with the count in place.
    pub fn format(&self) -> String {
        let locale = self
            .locale
            .clone()
            .unwrap_or_else(|| Locale::for_category(Category::Messages));
        let is_zero = self
            .count
            .decimal()
            .is_some_and(|decimal| decimal.is_zero());
        let wording = match (&self.none, is_zero) {
            (Some(none), true) => none,
            _ => {
                let category = plural_category(self.count.clone(), self.kind, Some(&locale));
                let chosen = match category {
                    PluralCategory::Zero => &self.forms[0],
                    PluralCategory::One => &self.forms[1],
                    PluralCategory::Two => &self.forms[2],
                    PluralCategory::Few => &self.forms[3],
                    PluralCategory::Many => &self.forms[4],
                    PluralCategory::Other => &None,
                };
                chosen.as_ref().unwrap_or(&self.other)
            }
        };
        if !wording.contains("{n}") {
            return wording.clone();
        }
        let numeric = self
            .locale
            .clone()
            .unwrap_or_else(|| Locale::for_category(Category::Numeric));
        let number = Number::new(self.count.clone()).locale(&numeric).format();
        wording.replace("{n}", &number)
    }
}

impl fmt::Display for Plural {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}
