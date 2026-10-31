//! The Data usage rows of the Network pages: today's traffic and a limit badge. The Network page
//! has one for everything; a connection's page has one for that connection.

use std::time::Duration;

use bingux_data_usage::{Totals, open_best, unix_now};
use freya::prelude::*;

use super::UsageScope;
use super::format;
use super::limits::{self, Level};
use super::model;
use crate::pages::network::model::NetCtx;
use crate::routes;
use crate::system;
use crate::ui::{Row, row};
use bingux_ui::prelude::Target;
use bingux_ui::style::is_compact;

/// How often the row reads the totals again while it is on screen.
const POLL: Duration = Duration::from_secs(30);

/// The cache key of the last summary, so the row has numbers on the first
/// frame after Settings starts.
const CACHE: &str = "data-usage-row";

/// What the row shows.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Summary {
    /// Bytes received and sent today; `None` when nothing has been recorded.
    today: Option<(u64, u64)>,
    /// Whether this billing cycle is near or over the limit.
    level: Option<Level>,
}

/// Reads today's totals and the limit state of `scope`. Blocks: not for the UI thread.
/// A failure reads as "nothing recorded", because the row has no room for an
/// error; the page behind it shows the real message.
fn load_for(scope: &UsageScope) -> Summary {
    let none = Summary {
        today: None,
        level: None,
    };
    let Ok(source) = open_best() else {
        return none;
    };
    let Ok((settings, _)) = model::read_settings(scope.connection()) else {
        return none;
    };
    let periods = bingux_data_usage::Periods::at(source.zone(), unix_now(), settings.billing_day);
    let query = scope.query();
    let totals = |span: (i64, i64)| source.totals(&query, span.0, span.1).unwrap_or_default();
    let today = totals(periods.today);
    Summary {
        today: Some((today.rx, today.tx)),
        level: limits::level(totals(periods.cycle).total(), &settings),
    }
}

/// The summary of everything, for the Network page.
fn load() -> Summary {
    load_for(&UsageScope::All)
}

/// The row's value for a summary.
fn value(summary: Option<&Summary>) -> String {
    match summary {
        None => "—".to_string(),
        Some(Summary { today: None, .. }) => "No data yet".to_string(),
        Some(Summary {
            today: Some((rx, tx)),
            ..
        }) => format!("{} today", format::pair(Totals { rx: *rx, tx: *tx })),
    }
}

/// The row for `summary`, which opens `open`.
fn entry(ctx: NetCtx, summary: Option<&Summary>, open: Target) -> Row {
    let badge = match summary.and_then(|summary| summary.level) {
        Some(Level::Warn) => "Near limit",
        Some(Level::Over) => "Over limit",
        None => "",
    };
    // A phone has no room for the numbers beside the title, so they go
    // under it; a wider row keeps them on the right.
    let text = value(summary);
    let entry = row("Data usage").badge(badge);
    let entry = if is_compact() {
        entry.subtitle(text)
    } else {
        entry.value(text)
    };
    entry
        .navigates()
        .on_press(move |_| ctx.router.open(open.clone()))
}

/// The Data usage row of the Network page: everything, today.
pub fn usage_row(ctx: NetCtx) -> Element {
    Element::from(UsageRow { ctx })
}

/// The Data usage row of a connection's page: that connection, today. It opens `open`, the
/// connection's own usage page, so Back returns to the connection.
pub fn connection_usage_row(ctx: NetCtx, connection: String, open: Target) -> Element {
    scope_usage_row(ctx, UsageScope::Connection(connection), open)
}

/// A Data usage row for any `scope`, today. It opens `open`.
pub fn scope_usage_row(ctx: NetCtx, scope: UsageScope, open: Target) -> Element {
    Element::from(ScopedUsageRow { ctx, scope, open })
}

#[derive(PartialEq)]
struct UsageRow {
    ctx: NetCtx,
}

impl Component for UsageRow {
    fn render(&self) -> impl IntoElement {
        let summary = system::use_cached_saved(CACHE, load);
        use_hook(move || {
            let focus = system::window_focus();
            spawn(async move {
                loop {
                    timer(POLL).await;
                    if *focus.peek() {
                        system::refresh(summary, || system::load_saved(CACHE, load));
                    }
                }
            })
        });
        let summary = summary.read().clone();
        entry(self.ctx, summary.as_ref(), routes::usage(&UsageScope::All))
    }
}

#[derive(PartialEq)]
struct ScopedUsageRow {
    ctx: NetCtx,
    scope: UsageScope,
    open: Target,
}

impl Component for ScopedUsageRow {
    fn render(&self) -> impl IntoElement {
        let summary = use_state(|| None::<Summary>);
        let scope = self.scope.clone();
        use_hook(move || {
            system::background(summary, {
                let scope = scope.clone();
                move || load_for(&scope)
            });
            let focus = system::window_focus();
            spawn(async move {
                loop {
                    timer(POLL).await;
                    if *focus.peek() {
                        let scope = scope.clone();
                        system::refresh(summary, move || load_for(&scope));
                    }
                }
            })
        });
        let summary = summary.read().clone();
        entry(self.ctx, summary.as_ref(), self.open.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_value_reads_like_the_page() {
        assert_eq!(value(None), "—");
        let none = Summary {
            today: None,
            level: None,
        };
        assert_eq!(value(Some(&none)), "No data yet");
        let some = Summary {
            today: Some((1_200_000_000, 80_000_000)),
            level: None,
        };
        assert_eq!(value(Some(&some)), "↓ 1.2 GB · ↑ 80 MB today");
    }
}
