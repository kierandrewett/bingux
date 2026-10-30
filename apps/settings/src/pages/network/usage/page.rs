//! The Data usage screen: totals, a chart over time, the breakdowns and the
//! limit, for everything, one interface or one connection.

use std::time::Duration;

use bingux_data_usage::{Kind, Totals, Zone, unix_now};
use freya::prelude::*;

use super::UsageScope;
use super::chart;
use super::format;
use super::limits::{self, Level, LimitsGroup};
use super::model::{self, Breakdown, Period, Request, Snapshot};
use crate::icons::Icon;
use crate::pages::loading::veiled;
use crate::routes;
use bingux_ui::prelude::use_router;
use crate::ui::{
    Group, Row, Status, Tone, form_field, page_column, property_row, row, segmented, status,
};
use bingux_data_usage::Source;
use bingux_ui::style::is_compact;
use bingux_ui::task::use_polled_resource;

/// How often a visible page reads the totals again.
const POLL: Duration = Duration::from_secs(30);

/// Shown when the recorder is not keeping the history up to date.
const NOT_RUNNING: &str = "The data usage recorder is not running.";

#[derive(PartialEq)]
pub struct UsagePage {
    pub scope: UsageScope,
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Wifi => "Wi-Fi",
        Kind::Ethernet => "Ethernet",
        Kind::Mobile => "Mobile",
        Kind::Vpn => "VPN",
    }
}

fn kind_icon(kind: Kind) -> Icon {
    match kind {
        Kind::Wifi => Icon::NetworkWirelessSignalExcellent,
        Kind::Ethernet => Icon::NetworkWired,
        Kind::Mobile => Icon::Network,
        Kind::Vpn => Icon::NetworkVpn,
    }
}

/// The title of the chart group for a period.
fn chart_title(period: Period) -> &'static str {
    match period {
        Period::Today => "Today by hour",
        Period::Week => "This week by day",
        Period::Month => "This month by day",
        Period::Cycle => "This billing cycle by day",
        Period::Custom => "Chosen dates by day",
    }
}

/// What the notes at the top say, if the recorder or the limit needs it.
/// Warnings share one card; an error stands alone in its own colour.
fn notices(snapshot: &Snapshot, scope: &UsageScope) -> Vec<Element> {
    let warning = |row: Row| row.leading(Icon::DialogWarning).leading_tone(Tone::Warning);
    let mut warnings: Vec<Element> = Vec::new();
    let mut notices = Vec::new();
    if !snapshot.live {
        let detail = match snapshot.last_sample {
            Some(last) => format!("Last recorded {}.", format::ago(snapshot.now, last)),
            None => "Nothing has been recorded yet.".to_string(),
        };
        warnings.push(Element::from(warning(row(NOT_RUNNING).subtitle(detail))));
    }
    if let Some(what) = &snapshot.unsupported {
        notices.push(Element::from(status(Status::Info(format!(
            "The history source cannot answer this: {what}."
        )))));
    }
    // An interface is compared with no limit: limits belong to the whole
    // computer or to one connection.
    if !matches!(scope, UsageScope::Interface(_)) {
        let used = snapshot.cycle.total();
        if let Some(level) = limits::level(used, &snapshot.settings) {
            let text = limits::message(level, used, &snapshot.settings);
            match level {
                Level::Over => notices.push(Element::from(status(Status::Error(text)))),
                Level::Warn => warnings.push(Element::from(warning(row(text)))),
            }
        }
    }
    if !warnings.is_empty() {
        notices.insert(0, Element::from(Group::new(None::<String>).rows(warnings)));
    }
    notices
}

fn totals_rows(
    snapshot: &Snapshot,
    period: Period,
    custom_span: Option<(i64, i64)>,
) -> Vec<Element> {
    let zone = snapshot.zone;
    let span = |(from, to): (i64, i64)| format::range(zone, from, to);
    let periods = &snapshot.periods;
    let mut rows = vec![
        Element::from(
            row("Today")
                .subtitle(span(periods.today))
                .value(format::pair(snapshot.today)),
        ),
        Element::from(
            row("This week")
                .subtitle(span(periods.week))
                .value(format::pair(snapshot.week)),
        ),
        Element::from(
            row("This month")
                .subtitle(span(periods.month))
                .value(format::pair(snapshot.month)),
        ),
        Element::from(
            row("Billing cycle")
                .subtitle(span(periods.cycle))
                .value(format::pair(snapshot.cycle)),
        ),
    ];
    if let (Period::Custom, Some(custom), Some(chosen)) = (period, snapshot.custom, custom_span) {
        rows.push(Element::from(
            row("Chosen dates")
                .subtitle(span(chosen))
                .value(format::pair(custom)),
        ));
    }
    if let Some(since_boot) = snapshot.since_boot {
        rows.push(Element::from(
            row("Since boot")
                .value(format::pair(since_boot)),
        ));
    }
    rows
}

fn chart_group(snapshot: &Snapshot, period: Period, custom_missing: bool) -> Group {
    let group = Group::titled(chart_title(period));
    if custom_missing {
        return group.row(Element::from(status(Status::Empty(
            "Enter a start and end date to see that range.".into(),
        ))));
    }
    match &snapshot.series {
        Err(why) => group.row(Element::from(status(Status::Info(format!("{why}."))))),
        Ok(buckets) if !chart::has_traffic(buckets) => {
            group.row(Element::from(status(Status::Empty(
                if snapshot.source.is_none() {
                    "Nothing has been recorded yet.".into()
                } else {
                    "Nothing was received or sent in this period.".into()
                },
            ))))
        }
        Ok(buckets) => {
            let data = chart::chart_data(buckets, period.granularity(), period, snapshot.zone);
            let total = buckets.iter().fold(Totals::default(), |sum, bucket| {
                sum + Totals {
                    rx: bucket.rx,
                    tx: bucket.tx,
                }
            });
            let label = format!(
                "Data received and sent, {}. Received {}, sent {}.",
                chart_title(period).to_lowercase(),
                format::bytes(total.rx),
                format::bytes(total.tx)
            );
            group.content(chart::chart(&data, label))
        }
    }
}

impl Component for UsagePage {
    fn render(&self) -> impl IntoElement {
        let router = use_router();
        let mut period = use_state(|| Period::Today);
        let today = Zone::Local.today(unix_now());
        let from = use_state(|| format::iso_date(today.add_days(-6)));
        let to = use_state(|| format::iso_date(today));

        let custom = (*period.read() == Period::Custom)
            .then(|| model::custom_dates(&from.read(), &to.read()));
        let request = Request {
            scope: self.scope.clone(),
            period: *period.read(),
            custom: custom
                .as_ref()
                .and_then(|dates| dates.as_ref().ok().copied()),
        };
        let resource = use_polled_resource(
            request.clone(),
            model::load,
            POLL,
            Snapshot::draws_same,
        );

        if let Some(failure) = resource.with(|state| state.error().map(str::to_string)) {
            return Element::from(page_column().child(
                status(Status::Error(failure)).action("Try again", move |_| resource.refresh()),
            ));
        }
        let (snapshot, loading) = resource.with(|state| match state.value() {
            Some(snapshot) => (snapshot.clone(), false),
            None => (Snapshot::bare(unix_now()), true),
        });
        let current = *period.read();
        let scope = self.scope.clone();

        let mut page = page_column();
        if !loading {
            page = page.children(notices(&snapshot, &scope));
        }

        // Period.
        let options = Period::ALL
            .iter()
            .map(|period| (period.key().to_string(), period.label().to_string()))
            .collect();
        let custom_span = request
            .custom
            .map(|(first, last)| model::custom_span(snapshot.zone, first, last));
        let shown_span = current
            .span(&snapshot.periods, custom_span)
            .map(|(first, last)| format::range(snapshot.zone, first, last));
        let mut selector =
            Group::titled("Period").row(row("Show").maybe_subtitle(shown_span).trailing(
                segmented(options, current.key(), move |key: String| {
                    period.set(Period::from_key(&key))
                }),
            ));
        if current == Period::Custom {
            let range_error = match (
                format::parse_date(&from.read()),
                format::parse_date(&to.read()),
            ) {
                (Ok(_), Ok(_)) => custom
                    .as_ref()
                    .and_then(|dates| dates.as_ref().err().cloned()),
                _ => None,
            };
            selector = selector
                .row(
                    form_field("From", from)
                        .placeholder("2026-10-01")
                        .validate(format::check_date),
                )
                .row(
                    form_field("To", to)
                        .placeholder("2026-10-07")
                        .validate(format::check_date)
                        .error(range_error),
                );
        }
        page = page.child(selector);

        // Totals.
        page = page.child(
            Group::titled("Totals")
                .rows(totals_rows(&snapshot, current, custom_span)),
        );

        // Chart.
        let custom_missing = current == Period::Custom && request.custom.is_none();
        page = page.child(chart_group(&snapshot, current, custom_missing));

        // Breakdowns, for the whole computer only.
        if scope == UsageScope::All {
            let interfaces = interface_rows(&snapshot.interfaces, move |name: String| {
                router.open(routes::usage(&UsageScope::Interface(name)))
            });
            page = page.child(
                Group::titled("By interface")
                    .description(current.phrase())
                    .rows(or_empty(interfaces, "No interface has been recorded yet.")),
            );
            let connections = connection_rows(&snapshot.connections, move |name: String| {
                router.open(routes::usage(&UsageScope::Connection(name)))
            });
            page = page.child(
                Group::titled("By connection")
                    .description(current.phrase())
                    .rows(or_empty(
                        connections,
                        "No connection has been recorded yet.",
                    )),
            );
        }

        // Limit.
        if !matches!(scope, UsageScope::Interface(_)) {
            let (first, last) = snapshot.periods.cycle;
            page = page.child(LimitsGroup {
                settings: snapshot.settings,
                connection: scope.connection().map(str::to_string),
                cycle: format::range(snapshot.zone, first, last),
                own: snapshot.settings != snapshot.global,
                changed: (move |()| resource.refresh()).into(),
            });
        }

        // Where the numbers come from.
        let source = match snapshot.source {
            Some(Source::Own) => "Recorded by Bingux",
            Some(Source::Vnstat) => "vnStat",
            None => "Nothing recorded yet",
        };
        page = page.child(
            Group::titled("About this data")
                .row(property_row("Source", source)),
        );

        Element::from(veiled(page, loading))
    }
}

/// Rows, or a single empty status when there are none.
fn or_empty(rows: Vec<Element>, empty: &str) -> Vec<Element> {
    if rows.is_empty() {
        vec![Element::from(status(Status::Empty(empty.to_string())))]
    } else {
        rows
    }
}

/// A breakdown row: `detail` and the traffic. A phone has no room for both
/// beside the title, so there the traffic goes under it; a wider row keeps
/// it on the right.
fn totals_row(row: Row, detail: &str, totals: Totals) -> Row {
    if is_compact() {
        row.subtitle(format!("{detail} · {}", format::pair(totals)))
    } else {
        row.subtitle(detail).value(format::pair(totals))
    }
}

fn interface_rows(entries: &[Breakdown], open: impl Fn(String) + Clone + 'static) -> Vec<Element> {
    entries
        .iter()
        .map(|entry| {
            let (name, open) = (entry.name.clone(), open.clone());
            Element::from(
                totals_row(row(kind_name(entry.kind)), &entry.name, entry.totals)
                    .leading(kind_icon(entry.kind))
                    .navigates()
                    .on_press(move |_| open(name.clone())),
            )
        })
        .collect()
}

fn connection_rows(entries: &[Breakdown], open: impl Fn(String) + Clone + 'static) -> Vec<Element> {
    entries
        .iter()
        .map(|entry| {
            let (name, open) = (entry.name.clone(), open.clone());
            Element::from(
                totals_row(row(entry.name.clone()), kind_name(entry.kind), entry.totals)
                    .leading(kind_icon(entry.kind))
                    .badge(if entry.metered { "Metered" } else { "" })
                    .navigates()
                    .on_press(move |_| open(name.clone())),
            )
        })
        .collect()
}
