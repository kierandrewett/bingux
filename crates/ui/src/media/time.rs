//! Times as a player shows and speaks them.

/// A clock reading for `seconds`: `0:07`, `3:45`, `1:02:03`; a negative time
/// (what is left) gets a minus sign, `-3:45`. Seconds are cut, not rounded, so
/// the reading changes as each second passes, the way a clock does.
pub fn format_time(seconds: f64) -> String {
    if !seconds.is_finite() {
        return "0:00".to_owned();
    }
    let sign = if seconds < 0.0 { "-" } else { "" };
    let whole = seconds.abs().floor() as u64;
    let (hours, minutes, secs) = (whole / 3600, whole / 60 % 60, whole % 60);
    if hours > 0 {
        format!("{sign}{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{sign}{minutes}:{secs:02}")
    }
}

/// The same time in words, for a screen reader: "3 minutes 45 seconds".
pub(crate) fn spoken_time(seconds: f64) -> String {
    let whole = if seconds.is_finite() {
        seconds.abs().floor() as u64
    } else {
        0
    };
    let (hours, minutes, secs) = (whole / 3600, whole / 60 % 60, whole % 60);
    let part =
        |id: &'static str, count: u64| crate::i18n::Message::new(id).arg("count", count).format();
    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(part("ui-media-spoken-hours", hours));
    }
    if minutes > 0 {
        parts.push(part("ui-media-spoken-minutes", minutes));
    }
    if secs > 0 || parts.is_empty() {
        parts.push(part("ui-media-spoken-seconds", secs));
    }
    parts
        .into_iter()
        .reduce(|first, second| {
            crate::i18n::Message::new("ui-media-spoken-join")
                .arg("first", first)
                .arg("second", second)
                .format()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_readings() {
        assert_eq!(format_time(0.0), "0:00");
        assert_eq!(format_time(7.9), "0:07");
        assert_eq!(format_time(225.0), "3:45");
        assert_eq!(format_time(3723.0), "1:02:03");
        assert_eq!(format_time(-225.4), "-3:45");
        assert_eq!(format_time(f64::NAN), "0:00");
    }

    #[test]
    fn spoken() {
        assert_eq!(spoken_time(225.0), "3 minutes 45 seconds");
        assert_eq!(spoken_time(3601.0), "1 hour 1 second");
        assert_eq!(spoken_time(0.0), "0 seconds");
    }
}
