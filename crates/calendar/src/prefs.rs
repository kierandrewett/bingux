//! Preferences. The locale's week and clock conventions come from the app
//! (`bingux_ui::locale`), so this crate stays free of the UI stack.

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

    /// The first day of the week: the choice, else `locale_first`, the
    /// locale's first day.
    pub fn first_day(&self, locale_first: Weekday) -> Weekday {
        match self.first_weekday {
            Some(day) => weekday_from_index(day),
            None => locale_first,
        }
    }

    /// Whether times are written on a 12-hour clock: the choice, else
    /// `locale_twelve_hour`, what the locale writes.
    pub fn twelve_hour(&self, locale_twelve_hour: bool) -> bool {
        match self.hour_format {
            HourFormat::H12 => true,
            HourFormat::H24 => false,
            HourFormat::Auto => locale_twelve_hour,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_override_the_locale() {
        let mut prefs = Prefs::default();
        assert_eq!(prefs.first_day(Weekday::Sun), Weekday::Sun);
        assert!(prefs.twelve_hour(true));
        prefs.first_weekday = Some(2);
        prefs.hour_format = HourFormat::H24;
        assert_eq!(prefs.first_day(Weekday::Sun), Weekday::Wed);
        assert!(!prefs.twelve_hour(true));
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
