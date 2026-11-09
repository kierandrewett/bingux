//! Time zones: what an event's clock is, and what instant a wall-clock time is.
//!
//! The IANA database is bundled (`chrono-tz`), so an event keeps its meaning
//! on a machine with no `tzdata`. A time with no zone is *floating*: it reads
//! the same on any clock, so it is resolved in the zone it is shown in.

use std::str::FromStr;
use std::sync::LazyLock;

use chrono::{DateTime, Duration, LocalResult, NaiveDateTime, Offset, TimeZone, Utc};
use chrono_tz::Tz;

/// The clock a wall-clock time is read on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Zone {
    /// No zone: the same wall-clock time wherever it is read.
    Floating,
    Utc,
    Named(Tz),
}

/// Windows zone names that calendar files from Outlook and Exchange carry,
/// and the IANA zone each stands for (the CLDR "windowsZones" golden zones).
const WINDOWS_ZONES: &[(&str, &str)] = &[
    ("Dateline Standard Time", "Etc/GMT+12"),
    ("UTC-11", "Etc/GMT+11"),
    ("Hawaiian Standard Time", "Pacific/Honolulu"),
    ("Alaskan Standard Time", "America/Anchorage"),
    ("Pacific Standard Time", "America/Los_Angeles"),
    ("Mountain Standard Time", "America/Denver"),
    ("US Mountain Standard Time", "America/Phoenix"),
    ("Central Standard Time", "America/Chicago"),
    ("Central America Standard Time", "America/Guatemala"),
    ("Canada Central Standard Time", "America/Regina"),
    ("Central Standard Time (Mexico)", "America/Mexico_City"),
    ("Eastern Standard Time", "America/New_York"),
    ("US Eastern Standard Time", "America/Indianapolis"),
    ("SA Pacific Standard Time", "America/Bogota"),
    ("Atlantic Standard Time", "America/Halifax"),
    ("Newfoundland Standard Time", "America/St_Johns"),
    ("E. South America Standard Time", "America/Sao_Paulo"),
    ("Argentina Standard Time", "America/Argentina/Buenos_Aires"),
    ("SA Western Standard Time", "America/La_Paz"),
    ("Pacific SA Standard Time", "America/Santiago"),
    ("Greenland Standard Time", "America/Godthab"),
    ("Azores Standard Time", "Atlantic/Azores"),
    ("Cape Verde Standard Time", "Atlantic/Cape_Verde"),
    ("UTC", "UTC"),
    ("GMT Standard Time", "Europe/London"),
    ("Greenwich Standard Time", "Atlantic/Reykjavik"),
    ("W. Europe Standard Time", "Europe/Berlin"),
    ("Central Europe Standard Time", "Europe/Budapest"),
    ("Romance Standard Time", "Europe/Paris"),
    ("Central European Standard Time", "Europe/Warsaw"),
    ("W. Central Africa Standard Time", "Africa/Lagos"),
    ("Namibia Standard Time", "Africa/Windhoek"),
    ("GTB Standard Time", "Europe/Bucharest"),
    ("Middle East Standard Time", "Asia/Beirut"),
    ("Egypt Standard Time", "Africa/Cairo"),
    ("South Africa Standard Time", "Africa/Johannesburg"),
    ("FLE Standard Time", "Europe/Kiev"),
    ("Israel Standard Time", "Asia/Jerusalem"),
    ("Turkey Standard Time", "Europe/Istanbul"),
    ("Arab Standard Time", "Asia/Riyadh"),
    ("Arabic Standard Time", "Asia/Baghdad"),
    ("Russian Standard Time", "Europe/Moscow"),
    ("E. Africa Standard Time", "Africa/Nairobi"),
    ("Iran Standard Time", "Asia/Tehran"),
    ("Arabian Standard Time", "Asia/Dubai"),
    ("Azerbaijan Standard Time", "Asia/Baku"),
    ("Afghanistan Standard Time", "Asia/Kabul"),
    ("West Asia Standard Time", "Asia/Tashkent"),
    ("Pakistan Standard Time", "Asia/Karachi"),
    ("India Standard Time", "Asia/Kolkata"),
    ("Sri Lanka Standard Time", "Asia/Colombo"),
    ("Nepal Standard Time", "Asia/Kathmandu"),
    ("Central Asia Standard Time", "Asia/Almaty"),
    ("Bangladesh Standard Time", "Asia/Dhaka"),
    ("Myanmar Standard Time", "Asia/Rangoon"),
    ("SE Asia Standard Time", "Asia/Bangkok"),
    ("China Standard Time", "Asia/Shanghai"),
    ("Singapore Standard Time", "Asia/Singapore"),
    ("W. Australia Standard Time", "Australia/Perth"),
    ("Taipei Standard Time", "Asia/Taipei"),
    ("Tokyo Standard Time", "Asia/Tokyo"),
    ("Korea Standard Time", "Asia/Seoul"),
    ("Cen. Australia Standard Time", "Australia/Adelaide"),
    ("AUS Central Standard Time", "Australia/Darwin"),
    ("E. Australia Standard Time", "Australia/Brisbane"),
    ("AUS Eastern Standard Time", "Australia/Sydney"),
    ("Tasmania Standard Time", "Australia/Hobart"),
    ("West Pacific Standard Time", "Pacific/Port_Moresby"),
    ("Central Pacific Standard Time", "Pacific/Guadalcanal"),
    ("New Zealand Standard Time", "Pacific/Auckland"),
    ("Tonga Standard Time", "Pacific/Tongatapu"),
];

impl Zone {
    /// The zone a `TZID` parameter names: an IANA name (with the prefix some
    /// producers add, `/mozilla.org/20050126_1/Europe/Paris`), a Windows zone
    /// name, or UTC. `None` when nothing matches, so the caller can say so
    /// and read the time as floating.
    pub fn from_tzid(tzid: &str) -> Option<Zone> {
        let tzid = tzid.trim().trim_matches('"');
        if tzid.is_empty() {
            return None;
        }
        if matches!(
            tzid,
            "UTC" | "Z" | "GMT" | "Etc/UTC" | "Etc/GMT" | "UCT" | "Zulu"
        ) {
            return Some(Zone::Utc);
        }
        if let Ok(tz) = Tz::from_str(tzid) {
            return Some(Zone::Named(tz));
        }
        if let Some((_, iana)) = WINDOWS_ZONES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(tzid))
        {
            return Tz::from_str(iana).ok().map(Zone::Named);
        }
        // A path before the name: try each suffix that starts after a slash.
        let mut rest = tzid;
        while let Some(at) = rest.find('/') {
            rest = &rest[at + 1..];
            if let Ok(tz) = Tz::from_str(rest) {
                return Some(Zone::Named(tz));
            }
        }
        None
    }

    /// The name written to a `TZID` parameter, empty for a floating time.
    pub fn tzid(self) -> String {
        match self {
            Zone::Floating => String::new(),
            Zone::Utc => "UTC".to_string(),
            Zone::Named(tz) => tz.name().to_string(),
        }
    }

    /// The name to show a person.
    pub fn label(self) -> String {
        match self {
            Zone::Floating => "Floating".to_string(),
            Zone::Utc => "UTC".to_string(),
            Zone::Named(tz) => tz.name().replace('_', " "),
        }
    }

    /// The zone this computer is set to: `TZ`, then `/etc/localtime`, then
    /// `/etc/timezone`, else UTC.
    pub fn local() -> Zone {
        static LOCAL: LazyLock<Zone> = LazyLock::new(Zone::detect_local);
        *LOCAL
    }

    /// Reads the computer's zone again, for a clock set while the app ran.
    pub fn detect_local() -> Zone {
        if let Ok(value) = std::env::var("TZ") {
            let value = value.trim_start_matches(':');
            if let Some(zone) = Zone::from_tzid(value) {
                return zone;
            }
        }
        if let Ok(target) = std::fs::read_link("/etc/localtime") {
            let text = target.to_string_lossy();
            if let Some(at) = text.find("zoneinfo/") {
                if let Some(zone) = Zone::from_tzid(&text[at + "zoneinfo/".len()..]) {
                    return zone;
                }
            }
        }
        if let Ok(text) = std::fs::read_to_string("/etc/timezone") {
            if let Some(zone) = Zone::from_tzid(text.trim()) {
                return zone;
            }
        }
        Zone::Utc
    }

    /// The instant a wall-clock time in this zone is. A floating time is read
    /// in `view`. A time that happens twice (the clock went back) is the
    /// first one; a time that never happens (the clock went forward) is read
    /// with the offset from before the gap, as RFC 5545 says, so 02:30 in a
    /// gap that skips 02:00 to 03:00 is 03:30.
    pub fn instant(self, wall: NaiveDateTime, view: Zone) -> DateTime<Utc> {
        match self {
            Zone::Floating => match view {
                Zone::Floating => wall.and_utc(),
                other => other.instant(wall, Zone::Utc),
            },
            Zone::Utc => wall.and_utc(),
            Zone::Named(tz) => match tz.from_local_datetime(&wall) {
                LocalResult::Single(at) => at.with_timezone(&Utc),
                LocalResult::Ambiguous(first, _) => first.with_timezone(&Utc),
                LocalResult::None => {
                    // Walk back until the clock exists, and use that offset.
                    for hours in 1..=26 {
                        let earlier = wall - Duration::hours(hours);
                        match tz.from_local_datetime(&earlier) {
                            LocalResult::Single(at) | LocalResult::Ambiguous(at, _) => {
                                let offset = i64::from(at.offset().fix().local_minus_utc());
                                return (wall - Duration::seconds(offset)).and_utc();
                            }
                            LocalResult::None => {}
                        }
                    }
                    wall.and_utc()
                }
            },
        }
    }

    /// The wall-clock time of an instant on this clock. A floating zone has no
    /// clock of its own and reads as UTC.
    pub fn wall(self, instant: DateTime<Utc>) -> NaiveDateTime {
        match self {
            Zone::Floating | Zone::Utc => instant.naive_utc(),
            Zone::Named(tz) => instant.with_timezone(&tz).naive_local(),
        }
    }

    /// The offset from UTC, in seconds, at an instant.
    pub fn offset_at(self, instant: DateTime<Utc>) -> i32 {
        match self {
            Zone::Floating | Zone::Utc => 0,
            Zone::Named(tz) => instant.with_timezone(&tz).offset().fix().local_minus_utc(),
        }
    }

    /// The wall-clock time of `wall` (read on this clock) on `other`'s clock.
    pub fn convert(self, wall: NaiveDateTime, other: Zone) -> NaiveDateTime {
        if self == other {
            return wall;
        }
        other.wall(self.instant(wall, other))
    }
}

/// The zones worth offering first: the main zone of each offset, and the
/// cities people name. The editor lists these above the rest of the database.
pub fn common_zones() -> Vec<Zone> {
    const NAMES: &[&str] = &[
        "UTC",
        "Europe/London",
        "Europe/Dublin",
        "Europe/Lisbon",
        "Europe/Paris",
        "Europe/Berlin",
        "Europe/Madrid",
        "Europe/Rome",
        "Europe/Amsterdam",
        "Europe/Stockholm",
        "Europe/Athens",
        "Europe/Helsinki",
        "Europe/Istanbul",
        "Europe/Moscow",
        "Africa/Cairo",
        "Africa/Johannesburg",
        "Africa/Lagos",
        "Africa/Nairobi",
        "Asia/Dubai",
        "Asia/Karachi",
        "Asia/Kolkata",
        "Asia/Dhaka",
        "Asia/Bangkok",
        "Asia/Singapore",
        "Asia/Shanghai",
        "Asia/Hong_Kong",
        "Asia/Tokyo",
        "Asia/Seoul",
        "Australia/Perth",
        "Australia/Sydney",
        "Pacific/Auckland",
        "Pacific/Honolulu",
        "America/Anchorage",
        "America/Los_Angeles",
        "America/Denver",
        "America/Phoenix",
        "America/Chicago",
        "America/Mexico_City",
        "America/New_York",
        "America/Toronto",
        "America/Halifax",
        "America/Sao_Paulo",
        "America/Argentina/Buenos_Aires",
    ];
    NAMES
        .iter()
        .filter_map(|name| Zone::from_tzid(name))
        .collect()
}

/// Every zone of the database, by name.
pub fn all_zones() -> Vec<Zone> {
    chrono_tz::TZ_VARIANTS
        .iter()
        .map(|tz| Zone::Named(*tz))
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    fn wall(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, min, 0)
            .unwrap()
    }

    fn london() -> Zone {
        Zone::from_tzid("Europe/London").unwrap()
    }

    #[test]
    fn names_resolve_with_prefixes_and_windows_names() {
        assert_eq!(
            Zone::from_tzid("Europe/Paris"),
            Zone::from_tzid("/mozilla.org/20050126_1/Europe/Paris")
        );
        assert_eq!(Zone::from_tzid("GMT Standard Time"), Some(london()));
        assert_eq!(Zone::from_tzid("Customized Time Zone"), None);
        assert_eq!(Zone::from_tzid("UTC"), Some(Zone::Utc));
    }

    #[test]
    fn a_time_in_the_gap_uses_the_offset_from_before_it() {
        // London springs forward on 2025-03-30: 01:00 becomes 02:00.
        let at = london().instant(wall(2025, 3, 30, 1, 30), Zone::Utc);
        assert_eq!(at, wall(2025, 3, 30, 1, 30).and_utc());
        assert_eq!(london().wall(at), wall(2025, 3, 30, 2, 30));
    }

    #[test]
    fn a_repeated_time_is_the_first_one() {
        // London falls back on 2025-10-26: 02:00 BST becomes 01:00 GMT.
        let at = london().instant(wall(2025, 10, 26, 1, 30), Zone::Utc);
        assert_eq!(at, wall(2025, 10, 26, 0, 30).and_utc());
    }

    #[test]
    fn floating_reads_in_the_view_zone() {
        let paris = Zone::from_tzid("Europe/Paris").unwrap();
        let at = Zone::Floating.instant(wall(2025, 7, 1, 9, 0), paris);
        assert_eq!(at, wall(2025, 7, 1, 7, 0).and_utc());
    }

    #[test]
    fn conversion_crosses_the_date_line() {
        let tokyo = Zone::from_tzid("Asia/Tokyo").unwrap();
        let la = Zone::from_tzid("America/Los_Angeles").unwrap();
        assert_eq!(
            tokyo.convert(wall(2025, 1, 2, 8, 0), la),
            wall(2025, 1, 1, 15, 0)
        );
    }
}
