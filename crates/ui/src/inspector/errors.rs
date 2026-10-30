//! The Errors card of the Performance tab: the failures the runtime caught,
//! newest first, from [`crate::resilience`]'s ring buffer. A panic in a
//! component, handler, effect, task or worker thread never closes the window, so
//! this is where a developer sees that one happened, what it said and where.
//! The card is absent while nothing has failed.

use std::time::{SystemTime, UNIX_EPOCH};

use freya::prelude::*;
use crate::component::ComponentName;

use crate::controls::button::action_button;
use crate::icons::{self, Icon};
use crate::resilience::{ErrorRecord, clear_errors, recent_errors, report_text};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, CAPTION, EMPHASIS, Ink};

/// How many failures the card lists; the ring buffer keeps more for the report.
const SHOWN: usize = 8;

/// The card. `count` is how many failures have been recorded: the tab passes
/// it so the card draws again when a new one arrives.
#[derive(PartialEq)]
pub struct ErrorsCard {
    pub count: u64,
}

fn age(at: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(at);
    match now.saturating_sub(at) {
        0..=1 => "just now".to_string(),
        seconds @ 2..=59 => format!("{seconds} s ago"),
        seconds @ 60..=3599 => format!("{} min ago", seconds / 60),
        seconds => format!("{} h ago", seconds / 3600),
    }
}

fn row(tokens: &crate::theme::Tokens, record: &ErrorRecord) -> Element {
    let place = match (&record.location, record.path.is_empty()) {
        (Some(location), false) => format!("{} · {location}", record.path.join(" › ")),
        (Some(location), true) => location.clone(),
        (None, _) => record.path.join(" › "),
    };
    let repeats = if record.repeats > 1 {
        format!(" · ×{}", record.repeats)
    } else {
        String::new()
    };
    rect()
        .component("ErrorRecord")
        .tw(tokens, &[w_full(), flex_col(), px(GUTTER), py(S3), gap(S1)])
        .child(
            EMPHASIS.in_ink(Ink::Danger).label(
                tokens,
                format!(
                    "{} · {}{} · {}",
                    record.kind.label(),
                    record.component,
                    repeats,
                    age(record.at)
                ),
            ),
        )
        .child(
            BODY.in_ink(Ink::Text)
                .paragraph(tokens, record.message.lines().next().unwrap_or("").to_string())
                .max_lines(3),
        )
        .maybe_child((!place.is_empty()).then(|| CAPTION.in_ink(Ink::Muted).paragraph(tokens, place)))
        .into()
}

impl Component for ErrorsCard {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut refresh = use_state(|| 0u64);
        let _ = *refresh.read();
        let records = recent_errors();
        if records.is_empty() {
            return rect().into_element();
        }
        let total = records.len();
        let header = rect()
            .tw(
                &tokens,
                &[w_full(), flex_row(), items(Align::Center), gap(S3), px(GUTTER), py(S3)],
            )
            .child(icons::icon(
                Icon::DialogError,
                IconSize::Row,
                Ink::Danger.color(&tokens),
            ))
            .child(
                rect().tw(&tokens, &[w_flex()]).child(
                    EMPHASIS
                        .in_ink(Ink::Text)
                        .label(&tokens, format!("Errors ({total})")),
                ),
            )
            .child(
                action_button("Copy report", move |()| {
                    let _ = freya::clipboard::Clipboard::set(report_text());
                })
                .flat()
                .compact(),
            )
            .child(
                action_button("Clear", move |()| {
                    clear_errors();
                    let next = *refresh.peek() + 1;
                    refresh.set(next);
                })
                .flat()
                .compact(),
            );
        let rows = records
            .iter()
            .take(SHOWN)
            .flat_map(|record| {
                [
                    row(&tokens, record),
                    rect().tw(&tokens, SEPARATOR).into_element(),
                ]
            })
            .collect::<Vec<_>>();
        rect()
            .component("ErrorsCard")
            .tw(&tokens, BOX)
            .child(header)
            .child(rect().tw(&tokens, SEPARATOR))
            .children(rows)
            .into_element()
    }
}
