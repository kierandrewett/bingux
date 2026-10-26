//! The Data usage row of the Network page: today's traffic and a limit badge.

use std::time::Duration;

use bingux_data_usage::{Scope, Totals, open_best, unix_now};
use freya::prelude::*;

use super::UsageScope;
use super::format;
use super::limits::{self, Level};
use super::model;
use crate::pages::network::model::NetCtx;
use crate::routes::Route;
use crate::system;
use crate::ui::row;
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

/// Reads today's totals and the limit state. Blocks: not for the UI thread.
/// A failure reads as "nothing recorded", because the row has no room for an
/// error; the page behind it shows the real message.
fn load() -> Summary {
    let none = Summary {
        today: None,
        level: None,
    };
    let Ok(source) = open_best() else {
        return none;
    };
    let Ok((settings, _)) = model::read_settings(None) else {
        return none;
    };
    let periods = bingux_data_usage::Periods::at(source.zone(), unix_now(), settings.billing_day);
    let totals = |span: (i64, i64)| {
        source
            .totals(&Scope::All, span.0, span.1)
            .unwrap_or_default()
    };
    let today = totals(periods.today);
    Summary {
        today: Some((today.rx, today.tx)),
        level: limits::level(totals(periods.cycle).total(), &settings),
    }
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

/// The Data usage row of the summary group.
pub fn usage_row(ctx: NetCtx) -> Element {
    Element::from(UsageRow { ctx })
}

#[derive(PartialEq)]
struct UsageRow {
    ctx: NetCtx,
}

impl Component for UsageRow {
    fn render(&self) -> impl IntoElement {
        let ctx = self.ctx;
        let summary = system::use_cached_saved(CACHE, load);
        use_hook(move || {
            spawn(async move {
                loop {
                    timer(POLL).await;
                    system::background(summary, || system::load_saved(CACHE, load));
                }
            })
        });
        let summary = summary.read().clone();
        let badge = match summary.as_ref().and_then(|summary| summary.level) {
            Some(Level::Warn) => "Near limit",
            Some(Level::Over) => "Over limit",
            None => "",
        };
        // A phone has no room for the numbers beside the title, so they go
        // under it; a wider row keeps them on the right.
        let text = value(summary.as_ref());
        let entry = row("Data usage").badge(badge);
        let entry = if is_compact() {
            entry.subtitle(text)
        } else {
            entry.value(text)
        };
        entry.navigates().on_press(move |_| {
            ctx.navigator.push(Route::Usage {
                scope: UsageScope::All,
            })
        })
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
