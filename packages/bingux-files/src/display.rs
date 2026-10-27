//! Sizes and times as a file list shows them.

use std::time::{SystemTime, UNIX_EPOCH};

/// `1.2 MB`, `340 kB`, `12 bytes`: decimal units, one decimal place under
/// 10, as GNOME Files writes sizes.
pub fn size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["kB", "MB", "GB", "TB", "PB"];
    if bytes < 1000 {
        return if bytes == 1 {
            "1 byte".into()
        } else {
            format!("{bytes} bytes")
        };
    }
    let mut value = bytes as f64 / 1000.0;
    let mut unit = 0;
    while value >= 1000.0 && unit + 1 < UNITS.len() {
        value /= 1000.0;
        unit += 1;
    }
    if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

/// A broken-down local time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Civil {
    pub year: i64,
    /// 1 to 12.
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    /// Days since 1970-01-01 of this date, for comparing dates.
    pub day_number: i64,
}

impl Civil {
    /// The date and time `seconds` after the epoch plus `offset` seconds.
    pub fn from_unix(seconds: i64, offset: i64) -> Civil {
        let local = seconds + offset;
        let day_number = local.div_euclid(86400);
        let in_day = local.rem_euclid(86400);
        // Howard Hinnant's civil_from_days.
        let z = day_number + 719468;
        let era = z.div_euclid(146097);
        let day_of_era = z.rem_euclid(146097);
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let month_index = (5 * day_of_year + 2) / 153;
        let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
        let month = if month_index < 10 {
            month_index + 3
        } else {
            month_index - 9
        } as u32;
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        Civil {
            year,
            month,
            day,
            hour: (in_day / 3600) as u32,
            minute: (in_day % 3600 / 60) as u32,
            day_number,
        }
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A modified time: `14:03` today, `Yesterday`, `12 Oct` this year,
/// `12 Oct 2024` before. `offset` is the local offset from UTC in seconds.
pub fn modified_at(seconds: i64, now: i64, offset: i64) -> String {
    let then = Civil::from_unix(seconds, offset);
    let today = Civil::from_unix(now, offset);
    match today.day_number - then.day_number {
        0 => format!("{:02}:{:02}", then.hour, then.minute),
        1 => "Yesterday".into(),
        _ if then.year == today.year => format!("{} {}", then.day, MONTHS[then.month as usize - 1]),
        _ => format!(
            "{} {} {}",
            then.day,
            MONTHS[then.month as usize - 1],
            then.year
        ),
    }
}

/// [`modified_at`] for a `SystemTime`, now, in the local time zone.
pub fn modified(time: SystemTime) -> String {
    let seconds = match time.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_secs() as i64,
        Err(before) => -(before.duration().as_secs() as i64),
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |after| after.as_secs() as i64);
    modified_at(seconds, now, local_offset(seconds))
}

/// A time in full, for a Properties dialog: `9 Mar 2024, 12:34`.
pub fn full_at(seconds: i64, offset: i64) -> String {
    let then = Civil::from_unix(seconds, offset);
    format!(
        "{} {} {}, {:02}:{:02}",
        then.day,
        MONTHS[then.month as usize - 1],
        then.year,
        then.hour,
        then.minute
    )
}

/// [`full_at`] for a `SystemTime`, in the local time zone.
pub fn full(time: SystemTime) -> String {
    let seconds = match time.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_secs() as i64,
        Err(before) => -(before.duration().as_secs() as i64),
    };
    full_at(seconds, local_offset(seconds))
}

/// The local zone's offset from UTC at `seconds`, from the C library.
pub fn local_offset(seconds: i64) -> i64 {
    let time = seconds as libc::time_t;
    // SAFETY: `localtime_r` writes only into the `tm` we pass.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&time, &mut tm).is_null() {
            0
        } else {
            tm.tm_gmtoff
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(size(0), "0 bytes");
        assert_eq!(size(1), "1 byte");
        assert_eq!(size(1_500), "1.5 kB");
        assert_eq!(size(340_000), "340 kB");
        assert_eq!(size(2_100_000_000), "2.1 GB");
    }

    #[test]
    fn times() {
        // 2024-03-09 12:34:56 UTC.
        let then = 1_709_987_696;
        assert_eq!(Civil::from_unix(then, 0).month, 3);
        assert_eq!(modified_at(then, then + 60, 0), "12:34");
        assert_eq!(modified_at(then, then + 86_400, 0), "Yesterday");
        assert_eq!(modified_at(then, then + 5 * 86_400, 0), "9 Mar");
        assert_eq!(modified_at(then, then + 400 * 86_400, 0), "9 Mar 2024");
        // An offset of +2 h moves the clock.
        assert_eq!(modified_at(then, then + 60, 7200), "14:34");
        assert_eq!(full_at(then, 0), "9 Mar 2024, 12:34");
        assert_eq!(full_at(then, 7200), "9 Mar 2024, 14:34");
    }
}
