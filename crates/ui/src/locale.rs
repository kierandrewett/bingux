//! Which language and regional conventions the person uses.
//!
//! A [`Locale`] is a BCP 47 tag (`en-US`, `de-CH`, `sr-Latn-RS`) with the
//! CLDR data behind it. [`Locale::current`] reads the environment the way a
//! POSIX program does, and every formatter in [`crate::format`] uses it unless
//! it is given another with `.locale(..)`.
//!
//! # How the environment is read
//!
//! POSIX has one variable per category of convention, and a program reads the
//! one that matches what it is formatting:
//!
//! | [`Category`]       | Variable        | Used for                                  |
//! | ------------------ | --------------- | ----------------------------------------- |
//! | `Category::Messages` | `LC_MESSAGES` | Language, plural rules, lists             |
//! | `Category::Numeric`  | `LC_NUMERIC`  | Numbers, percentages, byte sizes          |
//! | `Category::Time`     | `LC_TIME`     | Dates, times, relative times              |
//! | `Category::Monetary` | `LC_MONETARY` | Currency                                  |
//!
//! `LC_ALL` overrides all of them, and `LANG` is the fallback for each. For
//! `Messages` a non-empty `LANGUAGE` (a colon separated list, as `gettext`
//! reads it) names the preferred language first. `C`, `POSIX` and `C.UTF-8`
//! name no language, so they fall back to [`Locale::fallback`] (`en-US`, the
//! source language of Bingux). The codeset (`.UTF-8`) is ignored; the
//! modifiers `@latin` and `@cyrillic` set the script, others are dropped.
//!
//! A tag the data does not cover (`xx-YY`) is not an error: formatting falls
//! back to the root locale's conventions, as ICU does.
//!
//! ```
//! use bingux_ui::locale::Locale;
//!
//! let locale = Locale::parse("de_CH.UTF-8").unwrap();
//! assert_eq!(locale.tag(), "de-CH");
//! assert_eq!(locale.language(), "de");
//! assert_eq!(locale.region(), Some("CH"));
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use icu::locale::Locale as IcuLocale;

/// Which family of conventions to read from the environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    /// Language, plural rules and lists (`LC_MESSAGES`, then `LANGUAGE`).
    Messages,
    /// Numbers, percentages and sizes (`LC_NUMERIC`).
    Numeric,
    /// Dates and times (`LC_TIME`).
    Time,
    /// Currency (`LC_MONETARY`).
    Monetary,
}

impl Category {
    /// The environment variable of the category.
    pub const fn variable(self) -> &'static str {
        match self {
            Category::Messages => "LC_MESSAGES",
            Category::Numeric => "LC_NUMERIC",
            Category::Time => "LC_TIME",
            Category::Monetary => "LC_MONETARY",
        }
    }
}

/// A language with its region and script.
///
/// Cheap to clone: the tag is shared. Two locales are equal when their tags
/// are.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Locale(Arc<IcuLocale>);

/// A locale set by [`Locale::set_current`], used before the environment.
static OVERRIDE: RwLock<Option<Locale>> = RwLock::new(None);

/// Counts the calls of [`Locale::set_current`].
static REVISION: AtomicU64 = AtomicU64::new(0);

impl Locale {
    /// The language the Bingux apps are written in: American English. Used
    /// when the environment names no language.
    pub fn fallback() -> Locale {
        Locale(Arc::new(icu::locale::locale!("en-US")))
    }

    /// The regional conventions (dates, times, numbers) used when the
    /// environment names none: British English, the person's own region. The
    /// language of the apps is [`Locale::fallback`]; the two are kept apart so
    /// that a change of source language does not reformat anyone's dates.
    pub fn regional_fallback() -> Locale {
        Locale(Arc::new(icu::locale::locale!("en-GB")))
    }

    /// The locale for `tag`: a BCP 47 tag (`en-US`, `zh-Hant-TW`,
    /// `de-u-nu-latn`) or a POSIX one (`de_DE.UTF-8@euro`, `sr_RS@latin`).
    /// `None` when it is neither, or names no language (`C`, `POSIX`).
    pub fn parse(tag: &str) -> Option<Locale> {
        parse_posix(tag).map(|locale| Locale(Arc::new(locale)))
    }

    /// The conventions for messages (language, plurals, lists) of the running
    /// process: what [`Locale::for_category`] gives for [`Category::Messages`].
    pub fn current() -> Locale {
        Locale::for_category(Category::Messages)
    }

    /// The locale of one category of conventions, from the environment (see
    /// the module documentation), or the one set by [`Locale::set_current`].
    pub fn for_category(category: Category) -> Locale {
        if let Some(locale) = OVERRIDE
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
        {
            return locale;
        }
        Locale::from_environment(category, |name| std::env::var(name).ok())
    }

    /// Makes every `for_category` answer `locale` (`None`: read the
    /// environment again). For the taxonomy's locale switch and for tests;
    /// an app takes its locale from the environment.
    pub fn set_current(locale: Option<Locale>) {
        *OVERRIDE
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = locale;
        REVISION.fetch_add(1, Ordering::AcqRel);
    }

    /// A number that changes whenever [`Locale::set_current`] is called.
    /// Whatever is built from the locale (message bundles, cached text)
    /// compares it with the one it was built under and rebuilds when it
    /// differs. The environment of a running process does not change.
    pub fn revision() -> u64 {
        REVISION.load(Ordering::Acquire)
    }

    /// The languages the person prefers, most wanted first, for matching
    /// against the languages an app has translations for: the one set by
    /// [`Locale::set_current`] alone when there is one; otherwise every
    /// language of `LANGUAGE` in order (when the locale is not `C`), then the
    /// language of `LC_ALL`, `LC_MESSAGES` or `LANG`; at least one, which is
    /// [`Locale::fallback`] when the environment names none. No language
    /// appears twice.
    pub fn preferred() -> Vec<Locale> {
        if let Some(locale) = OVERRIDE
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
        {
            return vec![locale];
        }
        Locale::preferred_from_environment(|name| std::env::var(name).ok())
    }

    /// [`Locale::preferred`] as `variable` (a stand-in for the process
    /// environment) says.
    pub fn preferred_from_environment(variable: impl Fn(&str) -> Option<String>) -> Vec<Locale> {
        let value = |name: &str| variable(name).filter(|text| !text.is_empty());
        let base = value("LC_ALL")
            .or_else(|| value(Category::Messages.variable()))
            .or_else(|| value("LANG"))
            .as_deref()
            .and_then(Locale::parse);
        let mut list: Vec<Locale> = Vec::new();
        if base.is_some() {
            // gettext: LANGUAGE ranks languages, but only when the locale is
            // not C.
            for entry in value("LANGUAGE").iter().flat_map(|list| list.split(':')) {
                if let Some(locale) = Locale::parse(entry)
                    && !list.contains(&locale)
                {
                    list.push(locale);
                }
            }
        }
        if let Some(base) = base
            && !list.contains(&base)
        {
            list.push(base);
        }
        if list.is_empty() {
            list.push(Locale::fallback());
        }
        list
    }

    /// The locale of `category` as `variable` (a stand-in for the process
    /// environment) says.
    pub fn from_environment(
        category: Category,
        variable: impl Fn(&str) -> Option<String>,
    ) -> Locale {
        if category == Category::Messages {
            return Locale::preferred_from_environment(variable).swap_remove(0);
        }
        let value = |name: &str| variable(name).filter(|text| !text.is_empty());
        value("LC_ALL")
            .or_else(|| value(category.variable()))
            .or_else(|| value("LANG"))
            .as_deref()
            .and_then(Locale::parse)
            .unwrap_or_else(Locale::regional_fallback)
    }

    /// The BCP 47 tag: `en-US`.
    pub fn tag(&self) -> String {
        self.0.to_string()
    }

    /// The language subtag: `en`.
    pub fn language(&self) -> &str {
        self.0.id.language.as_str()
    }

    /// The region subtag: `GB`, when the tag has one.
    pub fn region(&self) -> Option<&str> {
        self.0.id.region.as_ref().map(|region| region.as_str())
    }

    /// The script subtag: `Latn`, when the tag has one.
    pub fn script(&self) -> Option<&str> {
        self.0.id.script.as_ref().map(|script| script.as_str())
    }

    /// The same locale with Latin digits (`0` to `9`) in place of the digits
    /// of its own numbering system: `ar-EG` writes `1234` where it writes
    /// `١٢٣٤`. For numbers that animate digit by digit or line up in
    /// columns, where one digit shape is needed.
    pub fn latin_digits(&self) -> Locale {
        use icu::locale::extensions::unicode::{key, value};
        let mut locale = (*self.0).clone();
        locale
            .extensions
            .unicode
            .keywords
            .set(key!("nu"), value!("latn"));
        Locale(Arc::new(locale))
    }

    pub(crate) fn icu(&self) -> &IcuLocale {
        &self.0
    }

    /// The day a week starts on in this locale (CLDR week data), with the
    /// region's usual first day. `Monday` when the data has none.
    pub fn first_weekday(&self) -> crate::calendar::Weekday {
        use crate::calendar::Weekday;
        use icu::calendar::types::Weekday as IcuWeekday;
        let first = icu::calendar::week::WeekInformation::try_new(self.0.as_ref().clone().into())
            .map(|week| week.first_weekday)
            .unwrap_or(IcuWeekday::Monday);
        match first {
            IcuWeekday::Monday => Weekday::Mon,
            IcuWeekday::Tuesday => Weekday::Tue,
            IcuWeekday::Wednesday => Weekday::Wed,
            IcuWeekday::Thursday => Weekday::Thu,
            IcuWeekday::Friday => Weekday::Fri,
            IcuWeekday::Saturday => Weekday::Sat,
            IcuWeekday::Sunday => Weekday::Sun,
        }
    }

    /// Whether the locale writes times on a 12-hour clock (`2:05 PM`).
    pub fn uses_12_hour(&self) -> bool {
        let text = crate::format::Time::of_day(13, 5, 0)
            .hour_cycle(crate::format::HourCycle::Locale)
            .locale(self)
            .format();
        !text.contains("13")
    }

    /// Whether a short numeric date reads month first (`3/4/26`, the fourth
    /// of March) rather than day first (`04/03/2026`).
    pub fn month_first(&self) -> bool {
        let text = crate::format::Date::civil(2026, 3, 4)
            .short()
            .locale(self)
            .format();
        text.trim_start().starts_with('3')
    }
}

impl std::fmt::Display for Locale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// An ICU locale for a BCP 47 or POSIX tag, `None` for no language.
fn parse_posix(text: &str) -> Option<IcuLocale> {
    let text = text.trim();
    // `ll_CC.codeset@modifier`: the modifier comes last, the codeset before it.
    let (rest, modifier) = match text.split_once('@') {
        Some((rest, modifier)) => (rest, Some(modifier)),
        None => (text, None),
    };
    // Nothing else in a tag holds a dot, so a dot starts the codeset.
    let rest = rest.split('.').next().unwrap_or("");
    if rest.is_empty() || matches!(rest, "C" | "POSIX") {
        return None;
    }
    let mut locale = IcuLocale::try_from_str(&rest.replace('_', "-")).ok()?;
    if locale.id.language.is_unknown() {
        return None;
    }
    if locale.id.script.is_none() {
        let script = match modifier {
            Some("latin") => Some("Latn"),
            Some("cyrillic") => Some("Cyrl"),
            _ => None,
        };
        if let Some(script) = script {
            locale.id.script = script.parse().ok();
        }
    }
    Some(locale)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_string())
        }
    }

    #[test]
    fn posix_forms_become_bcp47() {
        for (given, tag) in [
            ("en_GB.UTF-8", "en-GB"),
            ("de_DE@euro", "de-DE"),
            ("sr_RS@latin", "sr-Latn-RS"),
            ("sr_RS@cyrillic", "sr-Cyrl-RS"),
            ("fr", "fr"),
            ("zh-Hant-TW", "zh-Hant-TW"),
            ("pt_BR.ISO-8859-1", "pt-BR"),
            ("ca_ES.UTF-8@valencia", "ca-ES"),
        ] {
            assert_eq!(
                Locale::parse(given).map(|l| l.tag()).as_deref(),
                Some(tag),
                "{given}"
            );
        }
    }

    #[test]
    fn c_and_garbage_name_no_language() {
        for given in ["", "C", "POSIX", "C.UTF-8", "  ", "!!", "9_9"] {
            assert!(Locale::parse(given).is_none(), "{given:?}");
        }
    }

    #[test]
    fn nothing_set_falls_back_to_american_english_and_british_regions() {
        // The apps are written in American English; the regional conventions
        // (dates, times, numbers) stay British when the environment names none.
        let language = Locale::from_environment(Category::Messages, env(&[]));
        assert_eq!(language, Locale::fallback());
        let time = Locale::from_environment(Category::Time, env(&[]));
        assert_eq!(time, Locale::regional_fallback());
        let c = Locale::from_environment(Category::Numeric, env(&[("LANG", "C.UTF-8")]));
        assert_eq!(c.tag(), "en-GB");
    }

    #[test]
    fn lc_all_beats_the_category_and_lang() {
        let vars = [
            ("LC_ALL", "fr_FR.UTF-8"),
            ("LC_TIME", "de_DE"),
            ("LANG", "en_US"),
        ];
        assert_eq!(
            Locale::from_environment(Category::Time, env(&vars)).tag(),
            "fr-FR"
        );
    }

    #[test]
    fn category_beats_lang_and_each_category_is_its_own() {
        let vars = [
            ("LC_TIME", "de_DE"),
            ("LC_NUMERIC", "fr_FR"),
            ("LANG", "en_US"),
        ];
        assert_eq!(
            Locale::from_environment(Category::Time, env(&vars)).tag(),
            "de-DE"
        );
        assert_eq!(
            Locale::from_environment(Category::Numeric, env(&vars)).tag(),
            "fr-FR"
        );
        assert_eq!(
            Locale::from_environment(Category::Monetary, env(&vars)).tag(),
            "en-US"
        );
    }

    #[test]
    fn an_empty_variable_is_not_set() {
        let vars = [("LC_ALL", ""), ("LANG", "es_ES")];
        assert_eq!(
            Locale::from_environment(Category::Time, env(&vars)).tag(),
            "es-ES"
        );
    }

    #[test]
    fn language_ranks_messages_only_and_not_under_c() {
        let vars = [("LANG", "en_GB.UTF-8"), ("LANGUAGE", "xx-bad-:de:fr")];
        assert_eq!(
            Locale::from_environment(Category::Messages, env(&vars)).tag(),
            "de"
        );
        // Dates still follow LANG.
        assert_eq!(
            Locale::from_environment(Category::Time, env(&vars)).tag(),
            "en-GB"
        );
        let c = [("LANG", "C"), ("LANGUAGE", "de")];
        assert_eq!(
            Locale::from_environment(Category::Messages, env(&c)).tag(),
            "en-US"
        );
    }

    fn preferred(pairs: &[(&str, &str)]) -> Vec<String> {
        Locale::preferred_from_environment(env(pairs))
            .iter()
            .map(Locale::tag)
            .collect()
    }

    #[test]
    fn preferred_lists_every_language_then_the_base_once() {
        let vars = [
            ("LANG", "en_GB.UTF-8"),
            ("LANGUAGE", "fr:de:xx-bad-:fr:en_GB"),
        ];
        assert_eq!(preferred(&vars), ["fr", "de", "en-GB"]);
        assert_eq!(preferred(&[("LC_ALL", "de_DE"), ("LANG", "fr")]), ["de-DE"]);
        assert_eq!(
            preferred(&[("LC_MESSAGES", "es_ES"), ("LANG", "fr")]),
            ["es-ES"]
        );
    }

    #[test]
    fn preferred_is_never_empty_and_ignores_language_under_c() {
        assert_eq!(preferred(&[]), ["en-US"]);
        assert_eq!(
            preferred(&[("LANG", "C"), ("LANGUAGE", "de:fr")]),
            ["en-US"]
        );
        assert_eq!(preferred(&[("LANGUAGE", "de")]), ["en-US"]);
    }
}
