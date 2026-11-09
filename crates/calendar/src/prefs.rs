//! Preferences, and what the locale says about weeks and clocks.

use std::fs;
use std::path::PathBuf;

use chrono::Weekday;
use serde::{Deserialize, Serialize};

use crate::store::{config_dir, write_atomic};

/// How the hours of the day are written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HourFormat {
    /// As the locale writes them.
    #[default]
    Auto,
    H12,
    H24,
}

/// The view a window opens in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    Day,
    #[default]
    Week,
    Month,
    Year,
    Agenda,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub view: ViewKind,
    pub week_numbers: bool,
    /// 0 is Monday, 6 is Sunday; `None` follows the locale.
    pub first_weekday: Option<u8>,
    pub hour_format: HourFormat,
    /// Where a new event goes; `None` is the first calendar.
    pub default_calendar: Option<String>,
    /// Minutes before the start; `None` adds no reminder.
    pub default_reminder: Option<i64>,
    /// Minutes a new timed event lasts.
    pub default_length: i64,
    /// The hours the week and day views scroll to, as the first and last hour.
    pub working_hours: (u32, u32),
    /// Calendars that are not shown.
    pub hidden_calendars: Vec<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            view: ViewKind::Week,
            week_numbers: false,
            first_weekday: None,
            hour_format: HourFormat::Auto,
            default_calendar: None,
            default_reminder: Some(10),
            default_length: 60,
            working_hours: (9, 17),
            hidden_calendars: Vec::new(),
        }
    }
}

impl Prefs {
    fn path() -> PathBuf {
        config_dir().join("settings.json")
    }

    pub fn load() -> Prefs {
        fs::read_to_string(Self::path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = fs::create_dir_all(config_dir());
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = write_atomic(&Self::path(), text.as_bytes());
        }
    }

    /// The first day of the week: the choice, else the locale's.
    pub fn first_day(&self) -> Weekday {
        match self.first_weekday {
            Some(day) => weekday_from_index(day),
            None => Locale::current().first_weekday,
        }
    }

    /// Whether times are written on a 12-hour clock.
    pub fn twelve_hour(&self) -> bool {
        match self.hour_format {
            HourFormat::H12 => true,
            HourFormat::H24 => false,
            HourFormat::Auto => Locale::current().twelve_hour,
        }
    }
}

pub fn weekday_from_index(index: u8) -> Weekday {
    match index % 7 {
        0 => Weekday::Mon,
        1 => Weekday::Tue,
        2 => Weekday::Wed,
        3 => Weekday::Thu,
        4 => Weekday::Fri,
        5 => Weekday::Sat,
        _ => Weekday::Sun,
    }
}

/// What the locale says about calendars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Locale {
    pub first_weekday: Weekday,
    pub twelve_hour: bool,
    /// Whether a date such as 3/4 is the fourth of March.
    pub month_first: bool,
}

impl Locale {
    /// Read from `LC_ALL`, `LC_TIME`, `LANG` and `LANGUAGE`, in that order.
    pub fn current() -> Locale {
        let name = ["LC_ALL", "LC_TIME", "LANG", "LANGUAGE"]
            .iter()
            .filter_map(|variable| std::env::var(variable).ok())
            .find(|value| !value.is_empty() && value != "C" && value != "POSIX")
            .unwrap_or_default();
        Locale::of(&name)
    }

    /// The locale named `name`, such as `en_US.UTF-8`.
    pub fn of(name: &str) -> Locale {
        let name = name.split(['.', '@']).next().unwrap_or("");
        let (language, region) = match name.split_once(['_', '-']) {
            Some((language, region)) => (language.to_lowercase(), region.to_uppercase()),
            None => (name.to_lowercase(), String::new()),
        };
        // CLDR's week data: the territories whose weeks start on a Sunday,
        // on a Saturday, or (the Maldives) on a Friday. Everyone else
        // starts on Monday.
        const SUNDAY: &[&str] = &[
            "AG", "AS", "BD", "BR", "BS", "BT", "BW", "BZ", "CA", "CO", "DM", "DO", "ET", "GT",
            "GU", "HK", "HN", "ID", "IL", "IN", "JM", "JP", "KE", "KH", "KR", "LA", "MH", "MM",
            "MO", "MT", "MX", "MZ", "NI", "NP", "PA", "PE", "PH", "PK", "PR", "PT", "PY", "SA",
            "SG", "SV", "TH", "TT", "TW", "UM", "US", "VE", "VI", "WS", "YE", "ZA", "ZW",
        ];
        const SATURDAY: &[&str] = &[
            "AE", "AF", "BH", "DJ", "DZ", "EG", "IQ", "IR", "JO", "KW", "LY", "OM", "QA", "SD",
            "SY",
        ];
        let region = if region.is_empty() {
            default_region(&language)
        } else {
            region
        };
        let first_weekday = if SUNDAY.contains(&region.as_str()) {
            Weekday::Sun
        } else if SATURDAY.contains(&region.as_str()) {
            Weekday::Sat
        } else {
            Weekday::Mon
        };
        // Territories that write 12-hour times, and those that write the
        // month before the day.
        const TWELVE: &[&str] = &[
            "US", "CA", "AU", "NZ", "IN", "PH", "EG", "SA", "CO", "MX", "PK", "BD", "MY",
        ];
        let twelve_hour = TWELVE.contains(&region.as_str());
        let month_first = matches!(region.as_str(), "US" | "PH" | "CA" | "FM" | "PW" | "MH");
        Locale {
            first_weekday,
            twelve_hour,
            month_first,
        }
    }
}

/// The region a bare language usually means, for `LANG=en`.
fn default_region(language: &str) -> String {
    match language {
        "en" => "GB",
        "fr" => "FR",
        "de" => "DE",
        "es" => "ES",
        "it" => "IT",
        "pt" => "PT",
        "ja" => "JP",
        "ko" => "KR",
        "zh" => "CN",
        "ar" => "EG",
        _ => "",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weeks_start_where_the_locale_says() {
        assert_eq!(Locale::of("en_US.UTF-8").first_weekday, Weekday::Sun);
        assert_eq!(Locale::of("en_GB.UTF-8").first_weekday, Weekday::Mon);
        assert_eq!(Locale::of("de_DE").first_weekday, Weekday::Mon);
        assert_eq!(Locale::of("ar_EG").first_weekday, Weekday::Sat);
        assert_eq!(Locale::of("").first_weekday, Weekday::Mon);
    }

    #[test]
    fn clocks_and_dates_follow_the_region() {
        assert!(Locale::of("en_US").twelve_hour);
        assert!(!Locale::of("en_GB").twelve_hour);
        assert!(Locale::of("en_US").month_first);
        assert!(!Locale::of("fr_FR").month_first);
    }

    #[test]
    fn preferences_survive_a_round_trip_and_missing_keys() {
        let prefs: Prefs = serde_json::from_str("{\"week_numbers\":true}").unwrap();
        assert!(prefs.week_numbers);
        assert_eq!(prefs.default_length, 60);
        let text = serde_json::to_string(&prefs).unwrap();
        assert_eq!(serde_json::from_str::<Prefs>(&text).unwrap(), prefs);
    }
}
