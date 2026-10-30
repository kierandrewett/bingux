//! Network data usage for the Bingux desktop: a recorder that samples
//! interface counters into a SQLite database, and readers that answer the
//! questions Settings asks (today, this week, this billing cycle, per
//! interface, per connection).
//!
//! See the crate README for the schema, retention and reset rules.
//!
//! * [`Recorder`] runs on a thread inside `bingux-statusd`.
//! * [`Reader`] (own database) and [`VnstatReader`] (vnStat) both implement
//!   [`UsageSource`]; [`open_best`] picks one.
//! * [`Settings`] reads and writes the billing day and the monthly limit.

mod cmd;
mod error;
mod kind;
mod nm;
mod reader;
mod recorder;
mod schema;
mod settings;
mod time;
mod vnstat;

#[cfg(test)]
mod testutil;

pub use error::{Error, Result};
pub use kind::{Kind, NetInterface, SYSFS, classify, read_counters, scan};
pub use nm::{ActiveConnection, ConnectionResolver, NmcliResolver, NoConnections};
pub use reader::{
    Bucket, ConnectionInfo, Granularity, InterfaceInfo, MAX_BUCKETS, Reader, STALE_AFTER, Scope,
    Source, Totals, UsageSource,
};
pub use recorder::{Config, Pruned, Recorder, Retention, SampleSummary, Stop};
pub use schema::{BUSY_TIMEOUT, MIGRATIONS, SCHEMA_V1, SCHEMA_VERSION, default_path};
pub use settings::{KEY_BILLING_DAY, KEY_MONTHLY_LIMIT, KEY_WARN_PERCENT, Settings, UsageSettings};
pub use time::{
    Date, Periods, Zone, billing_cycle, days_in_month, month_start, unix_now, week_start,
};
pub use vnstat::VnstatReader;

/// The best available history: vnStat when it is installed and has data,
/// otherwise this crate's own database at [`default_path`]. Fails with
/// [`Error::NotFound`] when neither exists, which means "no usage history
/// yet, the recorder has not run".
pub fn open_best() -> Result<Box<dyn UsageSource>> {
    open_best_in(Zone::Local)
}

/// [`open_best`] with an explicit zone for local days.
pub fn open_best_in(zone: Zone) -> Result<Box<dyn UsageSource>> {
    if let Some(vnstat) = VnstatReader::detect(zone) {
        return Ok(Box::new(vnstat));
    }
    Ok(Box::new(Reader::open_default()?.with_zone(zone)))
}

#[cfg(test)]
mod readme {
    #[test]
    fn readme_documents_the_schema_verbatim() {
        let readme = include_str!("../README.md");
        assert!(
            readme.contains(crate::SCHEMA_V1),
            "README.md must contain the schema from schema.rs verbatim"
        );
    }
}
