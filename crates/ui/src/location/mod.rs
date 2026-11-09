//! Where the computer is, for any application that needs it.
//!
//! A [`Locator`] finds a [`Fix`] the way a person would want it found:
//!
//! 1. **The portal**: `org.freedesktop.portal.Location`, which GeoClue serves
//!    where it is installed. The person is asked once by the desktop. A "no"
//!    ends the search ([`LocateError::Denied`]): an application never goes round
//!    a refusal.
//! 2. **The network address**: a coarse position, to about the city, from a
//!    keyless lookup. It is used only when there is no portal, and a [`Fix`]
//!    says so ([`Source::Network`]), so the application can show where its
//!    position came from.
//! 3. **Nothing**: [`LocateError::Unavailable`] or [`LocateError::Offline`].
//!    The application asks for a city. It never stops working for want of a
//!    position.
//!
//! The position is only a position. Applications keep their own list of
//! chosen places and offer "use my location" as one entry; [`Source`] and
//! [`Source::describe`] are the words for saying how it was found.
//!
//! ```ignore
//! use bingux_ui::location::Locator;
//!
//! // On a worker, or awaited from a `spawn` task:
//! let fix = Locator::new().locate_async().await;
//! ```
//!
//! This module needs the `location` feature (it carries an HTTPS client and a
//! D-Bus client that most applications do not).

mod http;
mod ip;
mod portal;
mod reverse;

use std::time::Duration;

pub use reverse::Named;

/// How a position was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// The desktop's location service, with the person's consent.
    Portal,
    /// The network address, looked up by a service on the internet.
    Network,
}

impl Source {
    /// A short name for storing: `portal` or `network`.
    pub fn id(self) -> &'static str {
        match self {
            Source::Portal => "portal",
            Source::Network => "network",
        }
    }

    /// Words for showing the person how their position was found.
    pub fn describe(self) -> &'static str {
        match self {
            Source::Portal => "From the location service",
            Source::Network => "Approximate, from the network address",
        }
    }
}

/// A position.
#[derive(Clone, Debug, PartialEq)]
pub struct Fix {
    pub latitude: f64,
    pub longitude: f64,
    /// How far off it may be, in metres, when the source says.
    pub accuracy: Option<f64>,
    /// The nearest town, when the source or a lookup named it.
    pub name: Option<String>,
    pub region: Option<String>,
    pub country: Option<String>,
    /// The IANA time zone, when the source said.
    pub timezone: Option<String>,
    pub source: Source,
}

/// Why no position was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocateError {
    /// The person, or the system's policy, refused to share the position.
    Denied,
    /// There is no network, and no location service.
    Offline,
    /// No source gave a position (no service, or the lookup failed).
    Unavailable,
    /// The caller turned every source off.
    Disabled,
}

/// What to try, and for how long.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Locator {
    portal: bool,
    network: bool,
    timeout: Duration,
    name: bool,
}

impl Default for Locator {
    fn default() -> Self {
        Self::new()
    }
}

impl Locator {
    /// The portal, then the network address, naming the town if the source
    /// did not.
    pub fn new() -> Self {
        Self {
            portal: true,
            network: true,
            timeout: Duration::from_secs(10),
            name: true,
        }
    }

    /// Whether to ask the portal.
    pub fn portal(mut self, on: bool) -> Self {
        self.portal = on;
        self
    }

    /// Whether to fall back to the network address.
    pub fn network(mut self, on: bool) -> Self {
        self.network = on;
        self
    }

    /// Whether to look the town up from the coordinates when the source gave
    /// none (one request to OpenStreetMap's Nominatim).
    pub fn name_places(mut self, on: bool) -> Self {
        self.name = on;
        self
    }

    /// How long each source gets.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Finds the position. **Blocks**: call it on a worker, or use
    /// [`Locator::locate_async`].
    pub fn locate(&self) -> Result<Fix, LocateError> {
        crate::runtime::assert_off_ui_thread("a location lookup");
        let mut offline = false;
        if self.portal {
            match portal::locate(self.timeout) {
                Ok(fix) => return Ok(self.named(fix)),
                // A refusal is final: do not go round it with the address.
                Err(portal::Failure::Denied) => return Err(LocateError::Denied),
                Err(portal::Failure::Unavailable) => {}
            }
        }
        if self.network {
            match ip::locate(self.timeout) {
                Ok(fix) => return Ok(self.named(fix)),
                Err(was_offline) => offline = was_offline,
            }
        }
        Err(if !self.portal && !self.network {
            LocateError::Disabled
        } else if offline {
            LocateError::Offline
        } else {
            LocateError::Unavailable
        })
    }

    /// [`Locator::locate`] on the background pool, for a `spawn` task.
    pub async fn locate_async(self) -> Result<Fix, LocateError> {
        crate::task::spawn_blocking(move || self.locate()).await
    }

    fn named(&self, mut fix: Fix) -> Fix {
        if fix.name.is_none() && self.name {
            if let Some(named) = reverse::lookup(fix.latitude, fix.longitude, "en") {
                fix.name = Some(named.name);
                fix.region = fix.region.or(named.region);
                fix.country = fix.country.or(named.country);
            }
        }
        fix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_every_source_off_there_is_nothing_to_try() {
        let locator = Locator::new().portal(false).network(false);
        assert_eq!(locator.locate(), Err(LocateError::Disabled));
    }

    #[test]
    fn sources_describe_themselves() {
        assert_eq!(Source::Portal.id(), "portal");
        assert!(Source::Network.describe().contains("network"));
    }
}
