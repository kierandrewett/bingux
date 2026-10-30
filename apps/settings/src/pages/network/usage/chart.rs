//! The bars over time: received and sent per hour (today) or per day.

use bingux_data_usage::{Bucket, Granularity, Zone};
use bingux_ui::charts::{Series, Unit, bar_chart};
use freya::prelude::Element;

use super::format;
use super::model::Period;

/// The bars as the chart takes them: one name per band, a longer name for
/// its tooltip, and both series in bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartData {
    pub names: Vec<String>,
    /// What a band is called in the tooltip: the hour as a span, or the day.
    pub details: Vec<String>,
    pub received: Vec<f64>,
    pub sent: Vec<f64>,
}

/// What a band is called, on the axis and in the tooltip.
fn band_name(bucket: &Bucket, granularity: Granularity, period: Period, zone: Zone) -> String {
    match granularity {
        Granularity::Day => {
            let date = zone.date_of(bucket.start);
            if period == Period::Week {
                format::weekday_date(date)
            } else {
                format::short_date(date)
            }
        }
        Granularity::Hour | Granularity::Minute => {
            format!("{:02}:00", format::local_hour(zone, bucket.start))
        }
    }
}

/// What a band is called in the tooltip: an hour as the span it covers, a
/// day as its name.
fn band_detail(bucket: &Bucket, granularity: Granularity, name: &str, zone: Zone) -> String {
    match granularity {
        Granularity::Day => name.to_string(),
        Granularity::Hour | Granularity::Minute => {
            let hour = format::local_hour(zone, bucket.start);
            format!("{hour:02}:00 - {:02}:00", (hour + 1) % 24)
        }
    }
}

/// Turns the source's zero-filled buckets into chart data.
pub fn chart_data(
    buckets: &[Bucket],
    granularity: Granularity,
    period: Period,
    zone: Zone,
) -> ChartData {
    let names: Vec<String> = buckets
        .iter()
        .map(|bucket| band_name(bucket, granularity, period, zone))
        .collect();
    let details = buckets
        .iter()
        .zip(&names)
        .map(|(bucket, name)| band_detail(bucket, granularity, name, zone))
        .collect();
    ChartData {
        names,
        details,
        received: buckets.iter().map(|bucket| bucket.rx as f64).collect(),
        sent: buckets.iter().map(|bucket| bucket.tx as f64).collect(),
    }
}

/// Whether any bucket moved a byte; an all-zero chart is just a baseline.
pub fn has_traffic(buckets: &[Bucket]) -> bool {
    buckets.iter().any(|bucket| bucket.rx > 0 || bucket.tx > 0)
}

/// The chart. Bars stack, so the height of a band is what moved in total
/// and the tooltip names both parts; the label says the same to a screen
/// reader.
pub fn chart(data: &ChartData, label: String) -> Element {
    bar_chart([
        Series::values("Received", data.received.iter().copied()),
        Series::values("Sent", data.sent.iter().copied()),
    ])
    .categories(data.names.iter().cloned())
    .details(data.details.iter().cloned())
    .y_unit(Unit::DecimalBytes)
    .stacked()
    .label(label)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bucket(start: i64, rx: u64, tx: u64) -> Bucket {
        Bucket { start, rx, tx }
    }

    #[test]
    fn hours_are_named_by_the_local_clock() {
        // 2026-10-26 00:00 UTC, three hourly buckets, a zone one hour east.
        let day = 1_792_972_800;
        let buckets = [
            bucket(day, 500_000, 0),
            bucket(day + 3600, 0, 2_000_000),
            bucket(day + 7200, 0, 0),
        ];
        let data = chart_data(
            &buckets,
            Granularity::Hour,
            Period::Today,
            Zone::Fixed(3600),
        );
        assert_eq!(data.names, ["01:00", "02:00", "03:00"]);
        assert_eq!(
            data.details,
            ["01:00 - 02:00", "02:00 - 03:00", "03:00 - 04:00"]
        );
        assert_eq!(data.received, [500_000.0, 0.0, 0.0]);
        assert_eq!(data.sent, [0.0, 2_000_000.0, 0.0]);
    }

    #[test]
    fn the_last_hour_of_the_day_ends_at_midnight() {
        let day = 1_792_972_800;
        let buckets = [bucket(day + 23 * 3600, 1, 1)];
        let data = chart_data(&buckets, Granularity::Hour, Period::Today, Zone::Fixed(0));
        assert_eq!(data.details, ["23:00 - 00:00"]);
    }

    #[test]
    fn a_week_is_named_by_weekday_and_longer_spans_by_date() {
        let day = 1_792_972_800; // Monday 26 October 2026.
        let buckets = [bucket(day, 10, 20), bucket(day + 86_400, 30, 40)];
        let week = chart_data(&buckets, Granularity::Day, Period::Week, Zone::Fixed(0));
        assert_eq!(week.names, ["Mon 26 Oct", "Tue 27 Oct"]);
        let month = chart_data(&buckets, Granularity::Day, Period::Month, Zone::Fixed(0));
        assert_eq!(month.names, ["26 Oct", "27 Oct"]);
        assert_eq!(month.details, month.names);
        assert_eq!(month.received, [10.0, 30.0]);
    }

    #[test]
    fn no_buckets_make_empty_data() {
        let data = chart_data(&[], Granularity::Day, Period::Month, Zone::Fixed(0));
        assert!(data.names.is_empty() && data.received.is_empty());
        assert!(!has_traffic(&[]));
        assert!(!has_traffic(&[bucket(0, 0, 0)]));
        assert!(has_traffic(&[bucket(0, 0, 1)]));
    }
}
