//! Currency exchange rates: the European Central Bank's daily reference
//! rates, which are free, need no key and cover 30 currencies.
//!
//! The rates are euros-to-currency, as the ECB publishes them. A snapshot is
//! built into the program, so conversion works on the first launch with no
//! network; the application replaces it with a fresh download and keeps that
//! in the cache for the next launch.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::number::Num;

/// The feed of the daily reference rates.
pub const ECB_URL: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml";

/// Rates this old are fetched again (the ECB publishes once each working day).
pub const STALE_AFTER_SECS: u64 = 12 * 3600;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rates {
    /// The day the rates are for, `2026-11-06`.
    pub date: String,
    /// Unix time of the download; 0 for the snapshot built into the program.
    pub fetched: u64,
    pub source: String,
    /// Units of each currency for one euro, as decimal text.
    pub per_euro: BTreeMap<String, String>,
}

impl Rates {
    pub fn empty() -> Rates {
        Rates {
            date: String::new(),
            fetched: 0,
            source: String::new(),
            per_euro: BTreeMap::new(),
        }
    }

    /// The rates the program was built with.
    pub fn bundled() -> Rates {
        serde_json::from_str(include_str!("../data/ecb-rates.json"))
            .unwrap_or_else(|_| Rates::empty())
    }

    pub fn set(&mut self, code: &str, rate: &str) {
        self.per_euro.insert(code.to_string(), rate.to_string());
    }

    /// Units of `code` for one euro.
    pub fn per_euro(&self, code: &str) -> Option<Num> {
        if code == "EUR" {
            return Some(Num::one());
        }
        let text = self.per_euro.get(code)?;
        let (int, frac) = text.split_once('.').unwrap_or((text.as_str(), ""));
        let rate = Num::from_digits(false, &format!("{int}{frac}"), frac.len(), 0).ok()?;
        (!rate.is_zero()).then_some(rate)
    }

    pub fn has(&self, code: &str) -> bool {
        code == "EUR" || self.per_euro.contains_key(code)
    }

    /// Whether these rates are older than [`STALE_AFTER_SECS`] at `now`.
    pub fn is_stale(&self, now: u64) -> bool {
        self.fetched == 0 || now.saturating_sub(self.fetched) >= STALE_AFTER_SECS
    }

    /// Reads the ECB feed (`<Cube time='…'>` holding `<Cube currency='…'
    /// rate='…'/>` lines); `None` when it holds no rates.
    pub fn parse_ecb(xml: &str, now: u64) -> Option<Rates> {
        let date = attribute(xml, "time")?;
        let mut rates = Rates {
            date,
            fetched: now,
            source: "European Central Bank".to_string(),
            per_euro: BTreeMap::new(),
        };
        for cube in xml.split("<Cube").skip(1) {
            let tag = cube.split('>').next().unwrap_or("");
            let (Some(code), Some(rate)) = (attribute(tag, "currency"), attribute(tag, "rate"))
            else {
                continue;
            };
            let valid = code.len() == 3
                && code.chars().all(|c| c.is_ascii_uppercase())
                && !rate.is_empty()
                && rate.chars().all(|c| c.is_ascii_digit() || c == '.')
                && rate.matches('.').count() <= 1;
            if valid {
                rates.per_euro.insert(code, rate);
            }
        }
        (!rates.per_euro.is_empty()).then_some(rates)
    }
}

/// The value of `name='value'` or `name="value"` in `text`.
fn attribute(text: &str, name: &str) -> Option<String> {
    let mut from = 0;
    while let Some(found) = text[from..].find(name) {
        let start = from + found;
        from = start + name.len();
        let before_ok = start == 0
            || !text[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric());
        let rest = text[start + name.len()..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(quote) = rest.chars().next().filter(|c| *c == '\'' || *c == '"') else {
            continue;
        };
        if !before_ok {
            continue;
        }
        let inner = &rest[1..];
        let end = inner.find(quote)?;
        return Some(inner[..end].to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = "<?xml version='1.0'?>\n<gesmes:Envelope>\n<Cube>\n<Cube time='2026-11-06'>\n\
<Cube currency='USD' rate='1.1165'/>\n<Cube currency=\"JPY\" rate=\"176.77\"/>\n</Cube>\n</Cube>\n</gesmes:Envelope>";

    #[test]
    fn the_ecb_feed_is_read() {
        let rates = Rates::parse_ecb(FEED, 1000).unwrap();
        assert_eq!(rates.date, "2026-11-06");
        assert_eq!(rates.fetched, 1000);
        assert_eq!(rates.per_euro.len(), 2);
        assert_eq!(rates.per_euro["USD"], "1.1165");
        assert_eq!(rates.per_euro["JPY"], "176.77");
    }

    #[test]
    fn a_feed_without_rates_is_refused() {
        assert!(Rates::parse_ecb("<html>Not found</html>", 5).is_none());
        assert!(Rates::parse_ecb("<Cube time='2026-11-06'></Cube>", 5).is_none());
        assert!(Rates::parse_ecb("", 5).is_none());
    }

    #[test]
    fn rubbish_rates_are_skipped() {
        let feed = "<Cube time='2026-01-01'><Cube currency='USD' rate='abc'/><Cube currency='gbp' rate='0.8'/><Cube currency='CHF' rate='0.9'/></Cube>";
        let rates = Rates::parse_ecb(feed, 1).unwrap();
        assert_eq!(rates.per_euro.keys().collect::<Vec<_>>(), ["CHF"]);
    }

    #[test]
    fn the_bundled_snapshot_covers_every_listed_currency() {
        let rates = Rates::bundled();
        assert_eq!(rates.date.len(), 10);
        assert_eq!(rates.fetched, 0);
        for unit in crate::units::currencies() {
            assert!(rates.has(unit.id), "{} is not in the snapshot", unit.id);
            assert!(rates.per_euro(unit.id).is_some());
        }
    }

    #[test]
    fn staleness() {
        let mut rates = Rates::bundled();
        assert!(rates.is_stale(10));
        rates.fetched = 1000;
        assert!(!rates.is_stale(1000 + STALE_AFTER_SECS - 1));
        assert!(rates.is_stale(1000 + STALE_AFTER_SECS));
    }

    #[test]
    fn the_euro_is_one() {
        assert_eq!(Rates::empty().per_euro("EUR"), Some(Num::one()));
        assert_eq!(Rates::empty().per_euro("USD"), None);
    }
}
