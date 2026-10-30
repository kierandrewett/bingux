//! The monthly limit: when a cycle's traffic is near or over it, and the
//! Limit group that sets it.

use bingux_data_usage::{
    KEY_BILLING_DAY, KEY_MONTHLY_LIMIT, KEY_WARN_PERCENT, Settings, UsageSettings,
};
use freya::prelude::*;

use super::format::{self, LimitUnit};
use crate::system::{self, background_then};
use crate::ui::{
    Group, choice_row, form_field, page_column, pending_bar, row, stepper, switch_row, use_overlays,
};

/// How far through its limit a billing cycle is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Level {
    /// Past the warning threshold, below the limit.
    Warn,
    /// At or past the limit.
    Over,
}

/// Whether `used` bytes in the cycle call for a warning. No limit, no level.
pub fn level(used: u64, settings: &UsageSettings) -> Option<Level> {
    let limit = settings.monthly_limit_bytes?;
    if used >= limit {
        return Some(Level::Over);
    }
    (used >= settings.warn_threshold_bytes()?).then_some(Level::Warn)
}

/// The sentence for a level, with the numbers that caused it.
pub fn message(level: Level, used: u64, settings: &UsageSettings) -> String {
    let limit = settings.monthly_limit_bytes.unwrap_or(0);
    match level {
        Level::Over => format!(
            "Over the monthly limit: {} used of {} this billing cycle.",
            format::bytes(used),
            format::bytes(limit)
        ),
        Level::Warn => {
            let percent = if limit == 0 {
                0
            } else {
                (u128::from(used) * 100 / u128::from(limit)) as u64
            };
            format!(
                "{percent}% of the monthly limit used: {} of {} this billing cycle.",
                format::bytes(used),
                format::bytes(limit)
            )
        }
    }
}

/// One change to the stored limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    BillingDay(u8),
    /// `None` switches the limit off.
    Limit(Option<u64>),
    WarnPercent(u8),
    /// Drop the connection's own limits so the ones for all apply again.
    UseGlobal,
}

/// Writes a change. `connection` is the connection whose own limits change,
/// or `None` for the limits for all connections. Blocks: not for the UI thread.
pub fn store(change: Change, connection: Option<&str>) -> Result<(), String> {
    let settings = Settings::open_default().map_err(|error| error.to_string())?;
    let done = match change {
        Change::BillingDay(day) => settings.set_billing_day(day, connection),
        Change::Limit(bytes) => settings.set_monthly_limit(bytes, connection),
        Change::WarnPercent(percent) => settings.set_warn_percent(percent, connection),
        Change::UseGlobal => [KEY_BILLING_DAY, KEY_MONTHLY_LIMIT, KEY_WARN_PERCENT]
            .into_iter()
            .try_for_each(|key| settings.clear(key, connection)),
    };
    done.map_err(|error| error.to_string())
}

/// The Limit group: the day the cycle starts, the monthly limit and the
/// warning threshold. Changes to the cycle day and the threshold apply at
/// once; the size of the limit needs Apply, because half a number is not a
/// limit.
#[derive(PartialEq)]
pub struct LimitsGroup {
    /// The stored limits that apply to the scope.
    pub settings: UsageSettings,
    /// The connection whose own limits these are; `None` for all.
    pub connection: Option<String>,
    /// When the current cycle runs, such as `15 Oct to 14 Nov`.
    pub cycle: String,
    /// Whether the connection's limits differ from those for all.
    pub own: bool,
    /// Called after a change has been stored.
    pub changed: EventHandler<()>,
}

impl Component for LimitsGroup {
    fn render(&self) -> impl IntoElement {
        let overlays = use_overlays();
        let saved = self.settings;
        let connection = self.connection.clone();
        let changed = self.changed.clone();

        // What is on screen leads the stored value while a change saves.
        let mut shown = use_state(|| saved);
        use_side_effect_with_deps(&saved, move |saved: &UsageSettings| {
            shown.set_if_modified(*saved);
        });

        let saved_fields = saved.monthly_limit_bytes.map(format::size_fields);
        let initial = saved_fields
            .clone()
            .unwrap_or((String::new(), LimitUnit::Gb));
        let mut limit_on = use_state(|| saved.monthly_limit_bytes.is_some());
        let mut size = use_state(|| initial.0.clone());
        let mut unit = use_state(|| initial.1);
        use_side_effect_with_deps(&saved.monthly_limit_bytes, move |bytes: &Option<u64>| {
            let (text, saved_unit) = bytes
                .map(format::size_fields)
                .unwrap_or((String::new(), LimitUnit::Gb));
            limit_on.set_if_modified(bytes.is_some());
            size.set_if_modified(text);
            unit.set_if_modified(saved_unit);
        });

        let current = *shown.read();
        let typed = size.read().clone();
        let chosen = *unit.read();
        let on = *limit_on.read();

        let dirty = on
            && match &saved_fields {
                Some((text, saved_unit)) => typed.trim() != text || chosen != *saved_unit,
                None => !typed.trim().is_empty(),
            };
        let parsed = format::parse_size(&typed, chosen);

        // Every change goes through here: show it, store it in the
        // background, and put the old value back with a toast if it fails.
        let save = {
            let connection = connection.clone();
            let changed = changed.clone();
            move |change: Change, next: UsageSettings| {
                let connection = connection.clone();
                let changed = changed.clone();
                system::optimistic_then(
                    shown,
                    Some(overlays),
                    saved,
                    next,
                    move || store(change, connection.as_deref()),
                    move || changed.call(()),
                );
            }
        };

        let on_day = {
            let save = save.clone();
            move |day: i64| {
                let day = day.clamp(1, 31) as u8;
                save(
                    Change::BillingDay(day),
                    UsageSettings {
                        billing_day: day,
                        ..current
                    },
                );
            }
        };
        let on_warn = {
            let save = save.clone();
            move |percent: i64| {
                let percent = percent.clamp(1, 100) as u8;
                save(
                    Change::WarnPercent(percent),
                    UsageSettings {
                        warn_percent: percent,
                        ..current
                    },
                );
            }
        };
        let on_toggle = {
            let connection = connection.clone();
            let changed = changed.clone();
            move |next: bool| {
                if next || saved.monthly_limit_bytes.is_none() {
                    // Turning it on stores nothing until a size is applied.
                    limit_on.set(next);
                    return;
                }
                let connection = connection.clone();
                let changed = changed.clone();
                system::optimistic_then(
                    limit_on,
                    Some(overlays),
                    true,
                    false,
                    move || store(Change::Limit(None), connection.as_deref()),
                    move || changed.call(()),
                );
            }
        };
        let apply = {
            let save = save.clone();
            move || {
                if let Ok(bytes) = format::parse_size(&size.read(), *unit.read()) {
                    save(
                        Change::Limit(Some(bytes)),
                        UsageSettings {
                            monthly_limit_bytes: Some(bytes),
                            ..current
                        },
                    );
                }
            }
        };
        let mut revert = move || {
            let (text, saved_unit) = saved
                .monthly_limit_bytes
                .map(format::size_fields)
                .unwrap_or((String::new(), LimitUnit::Gb));
            size.set(text);
            unit.set(saved_unit);
            limit_on.set(saved.monthly_limit_bytes.is_some());
        };

        let mut group = Group::titled("Limit").row(
            row("Billing cycle starts on day")
                .subtitle(format!("This cycle: {}", self.cycle))
                .trailing(stepper(i64::from(current.billing_day), on_day).range(1, 31)),
        );
        group = group.row(
            switch_row("Monthly limit", on, on_toggle),
        );
        if on {
            group = group
                .row(
                    form_field("Limit size", size)
                        .suffix(chosen.key())
                        .placeholder("10")
                        .on_submit({
                            let apply = apply.clone();
                            move |()| {
                                if parsed_ok(size) {
                                    apply();
                                }
                            }
                        })
                        .validate(format::check_size),
                )
                .row(choice_row(
                    "Unit",
                    vec![("GB".into(), "GB".into()), ("MB".into(), "MB".into())],
                    chosen.key(),
                    move |key: String| unit.set(LimitUnit::from_key(&key)),
                ));
        }
        group = group.row(
            row("Warn at")
                .trailing(
                    stepper(i64::from(current.warn_percent), on_warn)
                        .range(1, 100)
                        .step(5)
                        .suffix("%")
                        .enabled(saved.monthly_limit_bytes.is_some()),
                ),
        );
        if self.connection.is_some() && self.own {
            let connection = connection.clone();
            let changed = changed.clone();
            group = group.row(
                row("Use the limit for all connections")
                    .navigates()
                    .on_press(move |_| {
                        let connection = connection.clone();
                        let changed = changed.clone();
                        background_then(
                            move || store(Change::UseGlobal, connection.as_deref()),
                            move |result| match result {
                                Ok(()) => changed.call(()),
                                Err(error) => overlays.toast(error),
                            },
                        );
                    }),
            );
        }

        let bar = pending_bar(dirty, parsed.is_ok(), move |_| apply(), move |_| revert());
        page_column().child(bar).child(group)
    }
}

/// Whether the size field holds a valid size, for Enter in the field.
fn parsed_ok(size: State<String>) -> bool {
    format::check_size(&size.read()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limited(limit: u64, warn: u8) -> UsageSettings {
        UsageSettings {
            billing_day: 1,
            monthly_limit_bytes: Some(limit),
            warn_percent: warn,
        }
    }

    #[test]
    fn no_limit_never_warns() {
        assert_eq!(level(u64::MAX, &UsageSettings::default()), None);
    }

    #[test]
    fn warns_at_the_threshold_and_is_over_at_the_limit() {
        let settings = limited(10_000_000_000, 80);
        assert_eq!(level(7_999_999_999, &settings), None);
        assert_eq!(level(8_000_000_000, &settings), Some(Level::Warn));
        assert_eq!(level(9_999_999_999, &settings), Some(Level::Warn));
        assert_eq!(level(10_000_000_000, &settings), Some(Level::Over));
        assert_eq!(level(50_000_000_000, &settings), Some(Level::Over));
    }

    #[test]
    fn a_hundred_percent_threshold_goes_straight_to_over() {
        let settings = limited(1_000_000, 100);
        assert_eq!(level(999_999, &settings), None);
        assert_eq!(level(1_000_000, &settings), Some(Level::Over));
    }

    #[test]
    fn messages_carry_the_numbers() {
        let settings = limited(10_000_000_000, 80);
        assert_eq!(
            message(Level::Warn, 8_300_000_000, &settings),
            "83% of the monthly limit used: 8.3 GB of 10 GB this billing cycle."
        );
        assert_eq!(
            message(Level::Over, 12_000_000_000, &settings),
            "Over the monthly limit: 12 GB used of 10 GB this billing cycle."
        );
    }
}
