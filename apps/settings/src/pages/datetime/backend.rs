//! The commands behind the Date and Time page: `timedatectl`, `gsettings`,
//! `bingux-config` and `date`.
//!
//! Snapshot builds can stand a fake clock in for them with
//! `BINGUX_SETTINGS_FAKE_TIMEDATE`. `1` keeps NTP, the time zone, the clock
//! format and the time in memory, so the page can be driven and captured
//! without touching the machine's clock. `fail` answers the same reads but
//! refuses every write, to check that a failed change reverts. The zone the
//! fake starts in is `BINGUX_SETTINGS_FAKE_ZONE` (default `Europe/London`).
//! `BINGUX_SETTINGS_FAKE_NOW=<Unix seconds>` freezes the fake clock at that time.

use std::process::Command;
use std::time::Duration;

use crate::system::{self, CommandError, CommandResult};

const TIMEOUT: Duration = Duration::from_secs(5);

/// Runs one of the page's commands, or answers it from the fake clock.
pub fn run(program: &str, args: &[&str]) -> CommandResult {
    #[cfg(feature = "snapshot")]
    if fake::active() {
        if program == "date" {
            return date(None, args.first().copied().unwrap_or("+%c"));
        }
        return fake::run(program, args);
    }
    system::run(program, args, TIMEOUT)
}

/// `gsettings get` for a key, quoted as gsettings prints it.
pub fn gsettings_get(schema: &str, key: &str) -> CommandResult {
    run("gsettings", &["get", schema, key])
}

/// `gsettings set` with a serialised GVariant value.
pub fn gsettings_set(schema: &str, key: &str, value: &str) -> CommandResult {
    run("gsettings", &["set", schema, key, value])
}

/// `date` with `format`, in `zone` when one is given and the machine's zone
/// otherwise. The fake clock supplies both when it is switched on.
pub fn date(zone: Option<&str>, format: &str) -> CommandResult {
    let (zone, epoch) = clock_for(zone);
    let mut command = Command::new("date");
    if let Some(zone) = zone {
        command.env("TZ", zone);
    }
    if let Some(epoch) = epoch {
        command.arg("-d").arg(format!("@{epoch}"));
    }
    let output = command
        .arg(format)
        .output()
        .map_err(|error| CommandError(format!("date could not run: {error}")))?;
    if !output.status.success() {
        return Err(CommandError("The time could not be read.".into()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The zone and Unix time `date` should use: the fake clock's when it is on,
/// otherwise the zone asked for and the real clock.
#[cfg(feature = "snapshot")]
fn clock_for(zone: Option<&str>) -> (Option<String>, Option<i64>) {
    if fake::active() {
        let (fake_zone, epoch) = fake::zone_and_epoch();
        (Some(zone.map_or(fake_zone, str::to_string)), Some(epoch))
    } else {
        (zone.map(str::to_string), None)
    }
}

#[cfg(not(feature = "snapshot"))]
fn clock_for(zone: Option<&str>) -> (Option<String>, Option<i64>) {
    (zone.map(str::to_string), None)
}

#[cfg(feature = "snapshot")]
mod fake {
    use std::process::Command;
    use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{CommandError, CommandResult, TIMEOUT};
    use crate::system;

    const INTERFACE: &str = "org.gnome.desktop.interface";

    struct Clock {
        /// Every write is refused, for checking the revert path.
        fail: bool,
        ntp: bool,
        zone: String,
        /// Seconds the fake clock runs ahead of the real one.
        offset: i64,
        clock_format: String,
        seconds: bool,
        weekday: bool,
        show_date: bool,
    }

    static CLOCK: LazyLock<Mutex<Clock>> = LazyLock::new(|| {
        Mutex::new(Clock {
            fail: std::env::var("BINGUX_SETTINGS_FAKE_TIMEDATE").is_ok_and(|mode| mode == "fail"),
            ntp: true,
            zone: std::env::var("BINGUX_SETTINGS_FAKE_ZONE")
                .unwrap_or_else(|_| "Europe/London".to_string()),
            offset: 0,
            clock_format: "24h".to_string(),
            seconds: false,
            weekday: false,
            show_date: true,
        })
    });

    pub fn active() -> bool {
        std::env::var_os("BINGUX_SETTINGS_FAKE_TIMEDATE").is_some()
    }

    fn lock() -> MutexGuard<'static, Clock> {
        CLOCK.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn real_now() -> i64 {
        // `BINGUX_SETTINGS_FAKE_NOW=<Unix seconds>` stops the clock there, so two captures show
        // the same time.
        if let Some(fixed) = std::env::var("BINGUX_SETTINGS_FAKE_NOW")
            .ok()
            .and_then(|text| text.parse().ok())
        {
            return fixed;
        }
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs() as i64)
    }

    /// The fake zone and the fake Unix time.
    pub fn zone_and_epoch() -> (String, i64) {
        let clock = lock();
        (clock.zone.clone(), real_now() + clock.offset)
    }

    fn flag(on: bool) -> String {
        if on { "true" } else { "false" }.to_string()
    }

    fn refuse() -> CommandResult {
        Err(CommandError("The change could not be saved.".into()))
    }

    /// The Unix time of the wall-clock `text` (`YYYY-MM-DD HH:MM:SS`) in `zone`.
    fn wall_time_epoch(text: &str, zone: &str) -> Result<i64, CommandError> {
        let output = Command::new("date")
            .env("TZ", zone)
            .args(["-d", text, "+%s"])
            .output()
            .map_err(|error| CommandError(format!("date could not run: {error}")))?;
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .map_err(|_| CommandError("The time could not be set.".into()))
    }

    pub fn run(program: &str, args: &[&str]) -> CommandResult {
        let mut clock = lock();
        let bingux_config = program.ends_with("bingux-config");
        match (program, args) {
            ("timedatectl", ["list-timezones"]) => system::run(program, args, TIMEOUT),
            ("timedatectl", ["show"]) => Ok(format!(
                "Timezone={}\nNTP={}\n",
                clock.zone,
                if clock.ntp { "yes" } else { "no" }
            )),
            ("timedatectl", ["set-ntp", value]) => {
                if clock.fail {
                    return refuse();
                }
                clock.ntp = *value == "true";
                Ok(String::new())
            }
            ("timedatectl", ["set-timezone", zone]) => {
                if clock.fail {
                    return refuse();
                }
                clock.zone = zone.to_string();
                Ok(String::new())
            }
            ("timedatectl", ["set-time", text]) => {
                if clock.fail {
                    return refuse();
                }
                let target = wall_time_epoch(text, &clock.zone)?;
                clock.offset = target - real_now();
                Ok(String::new())
            }
            ("gsettings", ["get", INTERFACE, "clock-format"]) => {
                Ok(format!("'{}'", clock.clock_format))
            }
            ("gsettings", ["get", INTERFACE, "clock-show-weekday"]) => Ok(flag(clock.weekday)),
            ("gsettings", ["get", INTERFACE, "clock-show-date"]) => Ok(flag(clock.show_date)),
            ("gsettings", ["set", INTERFACE, key, value]) => {
                if clock.fail {
                    return refuse();
                }
                match *key {
                    "clock-format" => clock.clock_format = value.trim_matches('\'').to_string(),
                    "clock-show-weekday" => clock.weekday = *value == "true",
                    "clock-show-date" => clock.show_date = *value == "true",
                    _ => return Err(CommandError(format!("No fake key {key}."))),
                }
                Ok(String::new())
            }
            (_, ["get", "desktop.clock_seconds"]) if bingux_config => Ok(flag(clock.seconds)),
            (_, ["set", "desktop.clock_seconds", value]) if bingux_config => {
                if clock.fail {
                    return refuse();
                }
                clock.seconds = *value == "true";
                Ok(String::new())
            }
            _ => Err(CommandError(format!(
                "The fake clock does not handle {program}."
            ))),
        }
    }
}
