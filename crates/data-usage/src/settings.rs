//! The `settings` table: billing cycle, monthly limit and warning threshold.
//!
//! One global row per key plus optional per-connection overrides. The
//! reader side never writes anything else: [`Settings`] is the only write
//! path Settings uses, and it takes short transactions under the busy
//! timeout so it never waits long for the recorder.

use crate::{
    error::{Error, Result},
    schema,
};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;

pub const KEY_BILLING_DAY: &str = "billing_day";
pub const KEY_MONTHLY_LIMIT: &str = "monthly_limit_bytes";
pub const KEY_WARN_PERCENT: &str = "warn_percent";

const KEYS: [&str; 3] = [KEY_BILLING_DAY, KEY_MONTHLY_LIMIT, KEY_WARN_PERCENT];

/// The effective settings after applying overrides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsageSettings {
    /// Day of the month a billing cycle starts on, 1 to 31. Default 1.
    pub billing_day: u8,
    /// Bytes per billing cycle; `None` means no limit (the default).
    pub monthly_limit_bytes: Option<u64>,
    /// Percent of the limit at which to warn, 1 to 100. Default 80.
    pub warn_percent: u8,
}

impl Default for UsageSettings {
    fn default() -> Self {
        UsageSettings {
            billing_day: 1,
            monthly_limit_bytes: None,
            warn_percent: 80,
        }
    }
}

impl UsageSettings {
    /// The byte count at which the warning starts, if there is a limit.
    pub fn warn_threshold_bytes(&self) -> Option<u64> {
        self.monthly_limit_bytes
            .map(|limit| (u128::from(limit) * u128::from(self.warn_percent) / 100) as u64)
    }
}

/// Resolves a connection key (uuid, else name) to a database id.
fn connection_id(connection: &Connection, key: &str) -> Result<i64> {
    connection
        .query_row(
            "SELECT id FROM connections WHERE nm_uuid = ?1 OR name = ?1
             ORDER BY nm_uuid = ?1 DESC, id LIMIT 1",
            [key],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| Error::UnknownScope(key.to_owned()))
}

/// The raw text of one row, exactly in the given scope (no fallback).
pub(crate) fn get_row(
    connection: &Connection,
    key: &str,
    scope: Option<&str>,
) -> Result<Option<String>> {
    let id = match scope {
        Some(name) => match connection_id(connection, name) {
            Ok(id) => id,
            Err(Error::UnknownScope(_)) => return Ok(None),
            Err(other) => return Err(other),
        },
        None => 0,
    };
    Ok(connection
        .query_row(
            "SELECT value FROM settings WHERE COALESCE(connection_id, 0) = ?1 AND key = ?2",
            params![id, key],
            |row| row.get(0),
        )
        .optional()?)
}

/// Global settings with the connection's overrides on top. Unparseable or
/// out-of-range stored text falls back to the default rather than failing
/// the page.
pub(crate) fn load(connection: &Connection, scope: Option<&str>) -> Result<UsageSettings> {
    let mut settings = UsageSettings::default();
    let layers: &[Option<&str>] = if scope.is_some() {
        &[None, scope]
    } else {
        &[None]
    };
    for layer in layers {
        if let Some(value) = get_row(connection, KEY_BILLING_DAY, *layer)?
            .and_then(|text| text.parse::<u8>().ok())
            .filter(|day| (1..=31).contains(day))
        {
            settings.billing_day = value;
        }
        if let Some(text) = get_row(connection, KEY_MONTHLY_LIMIT, *layer)? {
            // "0" and "off" both mean no limit, so a connection can switch
            // off a global limit.
            settings.monthly_limit_bytes = text.parse::<u64>().ok().filter(|bytes| *bytes > 0);
        }
        if let Some(value) = get_row(connection, KEY_WARN_PERCENT, *layer)?
            .and_then(|text| text.parse::<u8>().ok())
            .filter(|percent| (1..=100).contains(percent))
        {
            settings.warn_percent = value;
        }
    }
    Ok(settings)
}

/// Read and write access to the settings rows.
pub struct Settings {
    connection: Connection,
}

impl Settings {
    /// Opens (creating and migrating if the recorder has not run yet) the
    /// database at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Settings> {
        Ok(Settings {
            connection: schema::open_read_write(path.as_ref())?,
        })
    }

    /// Opens the database at [`crate::default_path`].
    pub fn open_default() -> Result<Settings> {
        Settings::open(schema::default_path()?)
    }

    /// The raw text of one row. `connection` is a connection uuid or name;
    /// `None` is the global row. There is no fallback: a missing override
    /// is `None`. Use [`Settings::load`] for effective values.
    pub fn get(&self, key: &str, connection: Option<&str>) -> Result<Option<String>> {
        get_row(&self.connection, key, connection)
    }

    /// Writes one row. `key` must be [`KEY_BILLING_DAY`],
    /// [`KEY_MONTHLY_LIMIT`] or [`KEY_WARN_PERCENT`] and `value` valid for
    /// it (`billing_day` 1 to 31, `warn_percent` 1 to 100,
    /// `monthly_limit_bytes` a byte count, 0 meaning "no limit"). An unknown
    /// `connection` is an error: overrides need a recorded connection.
    pub fn set(&self, key: &str, connection: Option<&str>, value: &str) -> Result<()> {
        validate(key, value)?;
        let id = match connection {
            Some(name) => Some(connection_id(&self.connection, name)?),
            None => None,
        };
        self.connection.execute(
            "INSERT INTO settings (connection_id, key, value) VALUES (?1, ?2, ?3)
             ON CONFLICT (COALESCE(connection_id, 0), key) DO UPDATE SET value = excluded.value",
            params![id, key, value],
        )?;
        Ok(())
    }

    /// Removes one row so the next scope up applies again. A no-op if absent.
    pub fn clear(&self, key: &str, connection: Option<&str>) -> Result<()> {
        if !KEYS.contains(&key) {
            return Err(Error::Invalid(format!("unknown setting {key}")));
        }
        let id = match connection {
            Some(name) => match connection_id(&self.connection, name) {
                Ok(id) => id,
                Err(Error::UnknownScope(_)) => return Ok(()),
                Err(other) => return Err(other),
            },
            None => 0,
        };
        self.connection.execute(
            "DELETE FROM settings WHERE COALESCE(connection_id, 0) = ?1 AND key = ?2",
            params![id, key],
        )?;
        Ok(())
    }

    /// The effective settings: global rows, then the connection's overrides.
    pub fn load(&self, connection: Option<&str>) -> Result<UsageSettings> {
        load(&self.connection, connection)
    }

    pub fn set_billing_day(&self, day: u8, connection: Option<&str>) -> Result<()> {
        self.set(KEY_BILLING_DAY, connection, &day.to_string())
    }

    /// `None` switches the limit off.
    pub fn set_monthly_limit(&self, bytes: Option<u64>, connection: Option<&str>) -> Result<()> {
        self.set(
            KEY_MONTHLY_LIMIT,
            connection,
            &bytes.unwrap_or(0).to_string(),
        )
    }

    pub fn set_warn_percent(&self, percent: u8, connection: Option<&str>) -> Result<()> {
        self.set(KEY_WARN_PERCENT, connection, &percent.to_string())
    }
}

fn validate(key: &str, value: &str) -> Result<()> {
    let invalid = || Error::Invalid(format!("{value:?} is not a valid {key}"));
    match key {
        KEY_BILLING_DAY => {
            let day: u8 = value.parse().map_err(|_| invalid())?;
            (1..=31).contains(&day).then_some(()).ok_or_else(invalid)
        }
        KEY_WARN_PERCENT => {
            let percent: u8 = value.parse().map_err(|_| invalid())?;
            (1..=100)
                .contains(&percent)
                .then_some(())
                .ok_or_else(invalid)
        }
        KEY_MONTHLY_LIMIT => value.parse::<u64>().map(|_| ()).map_err(|_| invalid()),
        _ => Err(Error::Invalid(format!("unknown setting {key}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn with_connections(settings: &Settings) {
        settings
            .connection
            .execute_batch(
                "INSERT INTO connections (nm_uuid, name, kind) VALUES
                     ('uuid-home', 'Home', 'wifi'), ('uuid-phone', 'Phone', 'wifi');",
            )
            .unwrap();
    }

    #[test]
    fn defaults_when_nothing_is_stored() {
        let dir = TempDir::new("settings0");
        let settings = Settings::open(dir.db()).unwrap();
        assert_eq!(settings.load(None).unwrap(), UsageSettings::default());
        assert_eq!(settings.get(KEY_BILLING_DAY, None).unwrap(), None);
    }

    #[test]
    fn global_values_and_connection_overrides() {
        let dir = TempDir::new("settings1");
        let settings = Settings::open(dir.db()).unwrap();
        with_connections(&settings);
        settings.set_billing_day(15, None).unwrap();
        settings
            .set_monthly_limit(Some(10_000_000_000), None)
            .unwrap();
        settings.set_warn_percent(90, None).unwrap();
        settings
            .set_monthly_limit(Some(2_000_000_000), Some("uuid-phone"))
            .unwrap();
        settings.set_billing_day(3, Some("Phone")).unwrap(); // by name

        let global = settings.load(None).unwrap();
        assert_eq!(global.billing_day, 15);
        assert_eq!(global.monthly_limit_bytes, Some(10_000_000_000));
        assert_eq!(global.warn_threshold_bytes(), Some(9_000_000_000));

        let phone = settings.load(Some("uuid-phone")).unwrap();
        assert_eq!(phone.billing_day, 3);
        assert_eq!(phone.monthly_limit_bytes, Some(2_000_000_000));
        assert_eq!(phone.warn_percent, 90, "inherited from the global row");

        let home = settings.load(Some("Home")).unwrap();
        assert_eq!(home, global, "no override: same as global");

        // Clearing an override reverts to the global value.
        settings
            .clear(KEY_MONTHLY_LIMIT, Some("uuid-phone"))
            .unwrap();
        assert_eq!(
            settings
                .load(Some("uuid-phone"))
                .unwrap()
                .monthly_limit_bytes,
            Some(10_000_000_000)
        );
    }

    #[test]
    fn a_connection_can_switch_off_a_global_limit() {
        let dir = TempDir::new("settings2");
        let settings = Settings::open(dir.db()).unwrap();
        with_connections(&settings);
        settings.set_monthly_limit(Some(5), None).unwrap();
        settings.set_monthly_limit(None, Some("uuid-home")).unwrap();
        assert_eq!(
            settings
                .load(Some("uuid-home"))
                .unwrap()
                .monthly_limit_bytes,
            None
        );
        assert_eq!(settings.load(None).unwrap().monthly_limit_bytes, Some(5));
    }

    #[test]
    fn set_replaces_and_validates() {
        let dir = TempDir::new("settings3");
        let settings = Settings::open(dir.db()).unwrap();
        settings.set_billing_day(5, None).unwrap();
        settings.set_billing_day(6, None).unwrap();
        assert_eq!(
            settings.get(KEY_BILLING_DAY, None).unwrap().as_deref(),
            Some("6")
        );
        let rows: i64 = settings
            .connection
            .query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 1);

        assert!(settings.set(KEY_BILLING_DAY, None, "0").is_err());
        assert!(settings.set(KEY_BILLING_DAY, None, "32").is_err());
        assert!(settings.set(KEY_WARN_PERCENT, None, "101").is_err());
        assert!(settings.set(KEY_MONTHLY_LIMIT, None, "-1").is_err());
        assert!(settings.set("colour", None, "red").is_err());
        assert!(matches!(
            settings.set_billing_day(1, Some("no such connection")),
            Err(Error::UnknownScope(_))
        ));
    }

    #[test]
    fn settings_written_by_one_handle_are_seen_by_the_reader() {
        let dir = TempDir::new("settings4");
        let settings = Settings::open(dir.db()).unwrap();
        settings.set_billing_day(20, None).unwrap();
        let reader = crate::Reader::open(dir.db()).unwrap();
        assert_eq!(reader.settings(None).unwrap().billing_day, 20);
    }
}
