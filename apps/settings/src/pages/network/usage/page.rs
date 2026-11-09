//! The Data usage screen: totals, a chart over time, the breakdowns and the
//! limit, for everything, one interface or one connection.

use bingux_ui::prelude::tr;
use std::time::Duration;

use bingux_data_usage::{Kind, Totals, Zone, unix_now};
use freya::prelude::*;

use super::UsageScope;
use super::breakdown::breakdown_group;
use super::chart;
use super::format;
use super::limits::{self, Level, LimitsGroup};
use super::model::{self, Period, Request, Snapshot};
use crate::icons::Icon;
use crate::pages::loading::veiled;
use crate::routes;
use crate::ui::{
    Group, Row, Status, Tone, form_field, page_column, property_row, row, segmented, status,
};
use bingux_data_usage::Source;
use bingux_ui::prelude::use_router;
use bingux_ui::task::use_polled_resource;

/// How often a visible page reads the totals again.
const POLL: Duration = Duration::from_secs(30);

/// Shown when the recorder is not keeping the history up to date.
fn not_running() -> String {
    tr!("data-usage-not-running")
}

#[derive(PartialEq)]
pub struct UsagePage {
    pub scope: UsageScope,
}

pub(super) fn kind_name(kind: Kind) -> String {
    match kind {
        Kind::Wifi => tr!("data-usage-kind-wifi"),
        Kind::Ethernet => tr!("data-usage-kind-ethernet"),
        Kind::Mobile => tr!("data-usage-kind-mobile"),
        Kind::Vpn => tr!("data-usage-kind-vpn"),
    }
}

pub(super) fn kind_icon(kind: Kind) -> Icon {
    match kind {
        Kind::Wifi => Icon::NetworkWirelessSignalExcellent,
        Kind::Ethernet => Icon::NetworkWired,
        Kind::Mobile => Icon::Network,
        Kind::Vpn => Icon::NetworkVpn,
    }
}

/// The title of the chart group for a period.
fn chart_title(period: Period) -> String {
    match period {
        Period::Today => tr!("data-usage-chart-today"),
        Period::Week => tr!("data-usage-chart-week"),
        Period::Month => tr!("data-usage-chart-month"),
        Period::Cycle => tr!("data-usage-chart-cycle"),
        Period::Custom => tr!("data-usage-chart-custom"),
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
            Some(last) => tr!(
                "data-usage-last-recorded",
                when = format::ago(snapshot.now, last).as_str()
            ),
            None => tr!("data-usage-nothing-recorded"),
        };
        warnings.push(Element::from(warning(row(&not_running()).subtitle(detail))));
    }
    if snapshot.unsupported.is_some() {
        notices.push(Element::from(status(Status::Info(tr!(
            "data-usage-unavailable"
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
            row(&tr!("data-usage-today"))
                .subtitle(span(periods.today))
                .value(format::pair(snapshot.today)),
        ),
        Element::from(
            row(&tr!("data-usage-this-week"))
                .subtitle(span(periods.week))
                .value(format::pair(snapshot.week)),
        ),
        Element::from(
            row(&tr!("data-usage-this-month"))
                .subtitle(span(periods.month))
                .value(format::pair(snapshot.month)),
        ),
        Element::from(
            row(&tr!("data-usage-billing-cycle"))
                .subtitle(span(periods.cycle))
                .value(format::pair(snapshot.cycle)),
        ),
    ];
    if let (Period::Custom, Some(custom), Some(chosen)) = (period, snapshot.custom, custom_span) {
        rows.push(Element::from(
            row(&tr!("data-usage-chosen-dates"))
                .subtitle(span(chosen))
                .value(format::pair(custom)),
        ));
    }
    if let Some(since_boot) = snapshot.since_boot {
        rows.push(Element::from(
            row(&tr!("data-usage-since-boot")).value(format::pair(since_boot)),
        ));
    }
    rows
}

fn chart_group(snapshot: &Snapshot, period: Period, custom_missing: bool) -> Group {
    let group = Group::titled(&chart_title(period));
    if custom_missing {
        return group.row(Element::from(status(Status::Empty(tr!(
            "data-usage-enter-dates"
        )))));
    }
    match &snapshot.series {
        Err(why) => group.row(Element::from(status(Status::Info(why.clone())))),
        Ok(buckets) if !chart::has_traffic(buckets) => group.row(Element::from(status(
            Status::Empty(if snapshot.source.is_none() {
                tr!("data-usage-nothing-recorded")
            } else {
                tr!("data-usage-nothing-in-period")
            }),
        ))),
        Ok(buckets) => {
            let data = chart::chart_data(buckets, period.granularity(), period, snapshot.zone);
            let total = buckets.iter().fold(Totals::default(), |sum, bucket| {
                sum + Totals {
                    rx: bucket.rx,
                    tx: bucket.tx,
                }
            });
            let label = tr!(
                "data-usage-chart-label",
                period = chart_title(period).to_lowercase(),
                received = format::bytes(total.rx),
                sent = format::bytes(total.tx)
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
        let resource =
            use_polled_resource(request.clone(), model::load, POLL, Snapshot::draws_same);

        if let Some(failure) = resource.with(|state| state.error().map(str::to_string)) {
            return Element::from(
                page_column().child(
                    status(Status::Error(failure))
                        .action(&tr!("data-usage-try-again"), move |_| resource.refresh()),
                ),
            );
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
        let mut selector = Group::titled(&tr!("data-usage-period")).row(
            row(&tr!("data-usage-show"))
                .maybe_subtitle(shown_span)
                .trailing(segmented(options, current.key(), move |key: String| {
                    period.set(Period::from_key(&key))
                })),
        );
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
                    form_field(&tr!("data-usage-from"), from)
                        .placeholder("2026-10-01")
                        .validate(format::check_date),
                )
                .row(
                    form_field(&tr!("data-usage-to"), to)
                        .placeholder("2026-10-07")
                        .validate(format::check_date)
                        .error(range_error),
                );
        }
        page = page.child(selector);

        // Totals.
        page = page.child(Group::titled(&tr!("data-usage-totals")).rows(totals_rows(
            &snapshot,
            current,
            custom_span,
        )));

        // Chart.
        let custom_missing = current == Period::Custom && request.custom.is_none();
        page = page.child(chart_group(&snapshot, current, custom_missing));

        // Breakdowns, for the whole computer only.
        if scope == UsageScope::All {
            page = page.child(breakdown_group(
                tr!("data-usage-by-interface"),
                current.phrase(),
                "usage.interfaces",
                tr!("data-usage-interface"),
                &snapshot.interfaces,
                tr!("data-usage-no-interface"),
                move |name: String| router.open(routes::usage(&UsageScope::Interface(name))),
            ));
            page = page.child(breakdown_group(
                tr!("data-usage-by-connection"),
                current.phrase(),
                "usage.connections",
                tr!("data-usage-connection"),
                &snapshot.connections,
                tr!("data-usage-no-connection"),
                move |name: String| router.open(routes::usage(&UsageScope::Connection(name))),
            ));
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
            Some(Source::Own) => tr!("data-usage-source-own"),
            Some(Source::Vnstat) => "vnStat".to_string(),
            None => tr!("data-usage-nothing-recorded-yet"),
        };
        page = page.child(
            Group::titled(&tr!("data-usage-about"))
                .row(property_row(&tr!("data-usage-source"), source)),
        );

        Element::from(veiled(page, loading))
    }
}
