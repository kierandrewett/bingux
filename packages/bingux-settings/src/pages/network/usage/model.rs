//! What the page reads: the periods, the request for one scope and period,
//! and the snapshot of numbers a read returns. Everything here runs off the UI
//! thread except the small pure helpers the page calls while it draws.

use std::path::Path;

use bingux_data_usage::{
    Bucket, Date, Error, Granularity, Kind, Periods, Reader, SYSFS, Scope, Source, Totals,
    UsageSettings, UsageSource, Zone, open_best, scan, unix_now,
};

use super::UsageScope;
use super::format;

/// The span the chart and the breakdowns cover.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Today,
    Week,
    Month,
    Cycle,
    Custom,
}

impl Period {
    pub const ALL: [Period; 5] = [
        Period::Today,
        Period::Week,
        Period::Month,
        Period::Cycle,
        Period::Custom,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::Week => "week",
            Self::Month => "month",
            Self::Cycle => "cycle",
            Self::Custom => "custom",
        }
    }

    pub fn from_key(key: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|period| period.key() == key)
            .unwrap_or(Self::Today)
    }

    /// The short name on the period control.
    pub fn label(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Week => "Week",
            Self::Month => "Month",
            Self::Cycle => "Cycle",
            Self::Custom => "Custom",
        }
    }

    /// What the groups say the numbers are for.
    pub fn phrase(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Week => "This week",
            Self::Month => "This month",
            Self::Cycle => "This billing cycle",
            Self::Custom => "The chosen dates",
        }
    }

    /// How finely the chart cuts the span: hours for one day, days beyond.
    pub fn granularity(self) -> Granularity {
        match self {
            Self::Today => Granularity::Hour,
            _ => Granularity::Day,
        }
    }

    /// The half-open `[from, to)` span of this period; `custom` is the
    /// span of the chosen dates.
    pub fn span(self, periods: &Periods, custom: Option<(i64, i64)>) -> Option<(i64, i64)> {
        match self {
            Self::Today => Some(periods.today),
            Self::Week => Some(periods.week),
            Self::Month => Some(periods.month),
            Self::Cycle => Some(periods.cycle),
            Self::Custom => custom,
        }
    }
}

/// The longest custom range: a year is enough for any plan, and keeps the
/// chart readable.
const MAX_CUSTOM_DAYS: i64 = 366;

/// The first and last day of a custom range, both included, from what was
/// typed in the two fields.
pub fn custom_dates(from: &str, to: &str) -> Result<(Date, Date), String> {
    let (from, to) = (format::parse_date(from)?, format::parse_date(to)?);
    if to < from {
        return Err("The end date is before the start date.".into());
    }
    if to.to_days() - from.to_days() >= MAX_CUSTOM_DAYS {
        return Err("Choose a range of at most one year.".into());
    }
    Ok((from, to))
}

/// The `[from, to)` seconds of the days `from` to `to`, both included.
pub fn custom_span(zone: Zone, from: Date, to: Date) -> (i64, i64) {
    (zone.date_start(from), zone.date_start(to.add_days(1)))
}

/// What one read is for. The page reads again when it changes.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub scope: UsageScope,
    pub period: Period,
    /// The first and last day of the chosen range, when it is valid.
    pub custom: Option<(Date, Date)>,
}

/// One interface or connection in a breakdown.
#[derive(Clone, Debug, PartialEq)]
pub struct Breakdown {
    pub name: String,
    pub kind: Kind,
    pub metered: bool,
    pub totals: Totals,
}

/// Everything the page shows, read in one go so the numbers agree.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub now: i64,
    pub zone: Zone,
    /// Where the history comes from; `None` when nothing has been recorded.
    pub source: Option<Source>,
    /// When the recorder last finished a round.
    pub last_sample: Option<i64>,
    /// Whether the recorder is keeping the history up to date.
    pub live: bool,
    /// The limits that apply to the scope.
    pub settings: UsageSettings,
    /// The limits for all connections.
    pub global: UsageSettings,
    pub periods: Periods,
    pub today: Totals,
    pub week: Totals,
    pub month: Totals,
    pub cycle: Totals,
    /// The totals of the chosen dates, when the period is Custom.
    pub custom: Option<Totals>,
    /// The bytes moved since the computer started; none for a connection,
    /// which the kernel does not count.
    pub since_boot: Option<Totals>,
    /// The chart's buckets, or why the source cannot give them.
    pub series: Result<Vec<Bucket>, String>,
    pub interfaces: Vec<Breakdown>,
    pub connections: Vec<Breakdown>,
    /// Set when the source cannot answer for this scope (vnStat knows no
    /// connections).
    pub unsupported: Option<String>,
}

impl Snapshot {
    /// A snapshot with no history: the shape of a loaded page for the first
    /// frame, and the real answer when nothing has been recorded.
    pub fn bare(now: i64) -> Self {
        let settings = UsageSettings::default();
        Self {
            now,
            zone: Zone::Local,
            source: None,
            last_sample: None,
            live: false,
            settings,
            global: settings,
            periods: Periods::at(Zone::Local, now, settings.billing_day),
            today: Totals::default(),
            week: Totals::default(),
            month: Totals::default(),
            cycle: Totals::default(),
            custom: None,
            since_boot: None,
            series: Ok(Vec::new()),
            interfaces: Vec::new(),
            connections: Vec::new(),
            unsupported: None,
        }
    }
}

impl UsageScope {
    pub(super) fn query(&self) -> Scope {
        match self {
            Self::All => Scope::All,
            Self::Interface(name) => Scope::Interface(name.clone()),
            Self::Connection(name) => Scope::Connection(name.clone()),
        }
    }

    /// The connection whose limits this scope uses, if it has its own.
    pub fn connection(&self) -> Option<&str> {
        match self {
            Self::Connection(name) => Some(name),
            Self::All | Self::Interface(_) => None,
        }
    }
}

/// "No history" is not a failure: the recorder has not run, or has not yet
/// migrated the database.
pub fn no_history(error: &Error) -> bool {
    matches!(error, Error::NotFound(_) | Error::Outdated { .. })
}

/// The limits of `connection` and the limits for all, from the recorder's
/// database, or the defaults when it does not exist yet.
pub fn read_settings(connection: Option<&str>) -> Result<(UsageSettings, UsageSettings), String> {
    let reader = match Reader::open_default() {
        Ok(reader) => reader,
        Err(error) if no_history(&error) => {
            let defaults = UsageSettings::default();
            return Ok((defaults, defaults));
        }
        Err(error) => return Err(error.to_string()),
    };
    let global = reader.settings(None).map_err(|error| error.to_string())?;
    let own = reader
        .settings(connection)
        .map_err(|error| error.to_string())?;
    Ok((own, global))
}

/// The kernel counters of every counted interface, for a computer whose
/// recorder has never run.
fn counters_since_boot(scope: &UsageScope) -> Option<Totals> {
    let interfaces = scan(Path::new(SYSFS)).ok()?;
    match scope {
        UsageScope::All => Some(interfaces.iter().fold(Totals::default(), |sum, interface| {
            sum + Totals {
                rx: interface.rx_bytes,
                tx: interface.tx_bytes,
            }
        })),
        UsageScope::Interface(name) => interfaces
            .iter()
            .find(|interface| &interface.name == name)
            .map(|interface| Totals {
                rx: interface.rx_bytes,
                tx: interface.tx_bytes,
            }),
        UsageScope::Connection(_) => None,
    }
}

/// A scope the data does not know has used nothing; a source that cannot
/// answer is remembered in `unsupported`.
fn quiet<T: Default>(
    answer: Result<T, Error>,
    unsupported: &mut Option<String>,
) -> Result<T, String> {
    match answer {
        Ok(value) => Ok(value),
        Err(Error::UnknownScope(_)) => Ok(T::default()),
        Err(Error::Unsupported(what)) => {
            *unsupported = Some(what.to_string());
            Ok(T::default())
        }
        Err(error) => Err(error.to_string()),
    }
}

/// Reads everything the page shows. Blocks: run it off the UI thread.
pub fn load(request: Request) -> Result<Snapshot, String> {
    let now = unix_now();
    let connection = request.scope.connection();
    let (settings, global) = read_settings(connection)?;
    let source = match open_best() {
        Ok(source) => source,
        Err(error) if no_history(&error) => {
            let mut bare = Snapshot::bare(now);
            bare.settings = settings;
            bare.global = global;
            bare.periods = Periods::at(Zone::Local, now, settings.billing_day);
            bare.since_boot = counters_since_boot(&request.scope);
            return Ok(bare);
        }
        Err(error) => return Err(error.to_string()),
    };
    read(&*source, &request, now, settings, global)
}

fn read(
    source: &dyn UsageSource,
    request: &Request,
    now: i64,
    settings: UsageSettings,
    global: UsageSettings,
) -> Result<Snapshot, String> {
    let zone = source.zone();
    let scope = request.scope.query();
    let periods = Periods::at(zone, now, settings.billing_day);
    let mut unsupported = None;
    let mut totals =
        |(from, to): (i64, i64)| quiet(source.totals(&scope, from, to), &mut unsupported);

    let today = totals(periods.today)?;
    let week = totals(periods.week)?;
    let month = totals(periods.month)?;
    let cycle = totals(periods.cycle)?;
    let custom_span = request.custom.map(|(from, to)| custom_span(zone, from, to));
    let custom = match (request.period, custom_span) {
        (Period::Custom, Some(span)) => Some(totals(span)?),
        _ => None,
    };

    let span = request.period.span(&periods, custom_span);
    let series = match span {
        Some((from, to)) => match source.series(&scope, request.period.granularity(), from, to) {
            Ok(buckets) => Ok(buckets),
            Err(Error::UnknownScope(_)) => Ok(Vec::new()),
            Err(Error::Unsupported(what)) => Err(what.to_string()),
            Err(error) => return Err(error.to_string()),
        },
        None => Ok(Vec::new()),
    };

    let (mut interfaces, mut connections) = (Vec::new(), Vec::new());
    if let (UsageScope::All, Some((from, to))) = (&request.scope, span) {
        interfaces = source
            .interface_totals(from, to)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|(info, totals)| Breakdown {
                name: info.name,
                kind: info.kind,
                metered: false,
                totals,
            })
            .collect();
        connections = source
            .connection_totals(from, to)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|(info, totals)| Breakdown {
                name: info.name,
                kind: info.kind,
                metered: info.metered,
                totals,
            })
            .collect();
    }

    let since_boot = match &request.scope {
        UsageScope::All => Some(
            source
                .since_boot_total()
                .map_err(|error| error.to_string())?,
        ),
        UsageScope::Interface(name) => {
            source.since_boot(name).map_err(|error| error.to_string())?
        }
        UsageScope::Connection(_) => None,
    };
    let last_sample = source
        .last_sample_time()
        .map_err(|error| error.to_string())?;
    let live = source.is_live(now).map_err(|error| error.to_string())?;

    Ok(Snapshot {
        now,
        zone,
        source: Some(source.source()),
        last_sample,
        live,
        settings,
        global,
        periods,
        today,
        week,
        month,
        cycle,
        custom,
        since_boot,
        series,
        interfaces,
        connections,
        unsupported,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u8, day: u8) -> Date {
        Date::new(year, month, day).expect("valid date")
    }

    /// Monday 2026-10-26 12:00 UTC, billing day 15.
    fn periods() -> Periods {
        Periods::at(Zone::Fixed(0), 1_793_016_000, 15)
    }

    #[test]
    fn today_is_one_day_of_hours() {
        let periods = periods();
        assert_eq!(
            Period::Today.span(&periods, None),
            Some((1_792_972_800, 1_793_059_200))
        );
        assert_eq!(Period::Today.granularity(), Granularity::Hour);
    }

    #[test]
    fn longer_periods_are_days() {
        let periods = periods();
        for period in [Period::Week, Period::Month, Period::Cycle, Period::Custom] {
            assert_eq!(period.granularity(), Granularity::Day);
        }
        // The week starts on Monday, which is today.
        assert_eq!(
            Period::Week.span(&periods, None),
            Some((1_792_972_800, 1_793_577_600))
        );
        // The month is 31 days of October.
        let (from, to) = Period::Month.span(&periods, None).expect("a month");
        assert_eq!((to - from) / 86_400, 31);
        // The cycle began on 15 October and ends on 15 November.
        let (from, to) = Period::Cycle.span(&periods, None).expect("a cycle");
        assert_eq!(Zone::Fixed(0).date_of(from), date(2026, 10, 15));
        assert_eq!(Zone::Fixed(0).date_of(to), date(2026, 11, 15));
    }

    #[test]
    fn custom_has_no_span_until_it_is_chosen() {
        let periods = periods();
        assert_eq!(Period::Custom.span(&periods, None), None);
        assert_eq!(Period::Custom.span(&periods, Some((1, 2))), Some((1, 2)));
    }

    #[test]
    fn custom_range_includes_both_ends() {
        let (from, to) = custom_dates("2026-10-01", "2026-10-03").expect("valid range");
        let (start, end) = custom_span(Zone::Fixed(0), from, to);
        assert_eq!((end - start) / 86_400, 3);
        assert_eq!(Zone::Fixed(0).date_of(start), date(2026, 10, 1));
        assert_eq!(Zone::Fixed(0).date_of(end), date(2026, 10, 4));
        // One day is a valid range.
        assert!(custom_dates("2026-10-01", "2026-10-01").is_ok());
    }

    #[test]
    fn custom_range_refuses_nonsense() {
        assert!(custom_dates("2026-10-03", "2026-10-01").is_err());
        assert!(custom_dates("2026-10-01", "soon").is_err());
        assert!(custom_dates("2025-10-01", "2026-10-02").is_err());
        assert!(custom_dates("2025-10-27", "2026-10-26").is_ok());
    }

    #[test]
    fn period_keys_round_trip() {
        for period in Period::ALL {
            assert_eq!(Period::from_key(period.key()), period);
        }
        assert_eq!(Period::from_key("nonsense"), Period::Today);
    }
}
