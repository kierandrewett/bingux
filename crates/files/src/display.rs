//! The calendar sums a file list needs, and the one size a drive is named
//! by before it has a label. Words for people (sizes, dates) are written in
//! their language by bingux-ui (`bingux_ui::files::display`, from
//! `bingux_ui::format`); this crate cannot depend on it.

/// `1.2 MB`, `340 kB`, `12 bytes`: decimal units, one decimal place under
/// 10. Only for the name of an unlabelled drive (`500 GB Volume`), which is
/// the drive's name rather than a size shown beside it.
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
    fn days() {
        // 2024-03-09 12:34:56 UTC.
        let then = 1_709_987_696;
        let civil = Civil::from_unix(then, 0);
        assert_eq!((civil.year, civil.month, civil.day), (2024, 3, 9));
        assert_eq!((civil.hour, civil.minute), (12, 34));
        // An offset of +2 h moves the clock.
        assert_eq!(Civil::from_unix(then, 7200).hour, 14);
        assert_eq!(
            Civil::from_unix(then + 86_400, 0).day_number,
            civil.day_number + 1
        );
    }
}
