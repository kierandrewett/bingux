//! Loading a forecast: from the cache when it is current, from the network
//! when it is not, and from an old cache when the network fails.
//!
//! Several parts of a window ask for the same place at once (the list and the
//! page). A lock for each place makes the second wait for the first and then
//! find the cache current, so a place is fetched once.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use crate::api::{self, Error};
use crate::model::{Forecast, Place};
use crate::store::{FRESH_FOR, Store};
use crate::time::unix_now;

/// How current a forecast must be after the person asked for a refresh.
const REFRESHED_FOR: i64 = 30;

/// Why there is no new forecast.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// No network.
    Offline,
    /// The service answered, but not with a forecast.
    Service,
}

impl Failure {
    /// A word for carrying the failure through a `Result<_, String>`.
    pub fn code(self) -> &'static str {
        match self {
            Failure::Offline => "offline",
            Failure::Service => "service",
        }
    }

    pub fn from_code(code: &str) -> Failure {
        if code == "offline" {
            Failure::Offline
        } else {
            Failure::Service
        }
    }
}

impl From<&Error> for Failure {
    fn from(error: &Error) -> Self {
        if error.is_offline() {
            Failure::Offline
        } else {
            Failure::Service
        }
    }
}

/// A forecast to show.
#[derive(Clone, Debug, PartialEq)]
pub struct Loaded {
    pub forecast: Forecast,
    /// Set when the forecast is the last one saved, because fetching a new one
    /// failed: the window says so, with the forecast's age.
    pub stale: Option<Failure>,
}

static LOCKS: Mutex<Option<HashMap<String, Arc<Mutex<()>>>>> = Mutex::new(None);

fn lock_for(key: &str) -> Arc<Mutex<()>> {
    let mut locks = LOCKS.lock().unwrap_or_else(PoisonError::into_inner);
    locks
        .get_or_insert_with(HashMap::new)
        .entry(key.to_string())
        .or_default()
        .clone()
}

/// The forecast of `place`. `refreshed` is true once the person has asked to
/// refresh: then a forecast older than half a minute is fetched again,
/// instead of one older than ten minutes. Blocks.
pub fn load(store: &Store, place: &Place, refreshed: bool) -> Result<Loaded, Failure> {
    let key = place.key();
    let lock = lock_for(&key);
    let _turn = lock.lock().unwrap_or_else(PoisonError::into_inner);
    let now = unix_now();
    let cached = store.forecast(&key);
    let limit = if refreshed { REFRESHED_FOR } else { FRESH_FOR };
    if let Some(forecast) = &cached {
        if now - forecast.fetched_at < limit {
            return Ok(Loaded {
                forecast: forecast.clone(),
                stale: None,
            });
        }
    }
    match api::fetch(place) {
        Ok(forecast) => {
            // A cache that cannot be written costs the next start-up its
            // head start, nothing more.
            let _ = store.save_forecast(&key, &forecast);
            Ok(Loaded {
                forecast,
                stale: None,
            })
        }
        Err(error) => {
            let failure = Failure::from(&error);
            match cached {
                Some(forecast) => Ok(Loaded {
                    forecast,
                    stale: Some(failure),
                }),
                None => Err(failure),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_current_cache_is_used_without_the_network() {
        let dir = std::env::temp_dir().join(format!("bingux-weather-load-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::in_dir(&dir);
        let place = Place::new("Nowhere", 1.0, 2.0);
        let forecast = Forecast {
            fetched_at: unix_now() - 60,
            ..Forecast::default()
        };
        store.save_forecast(&place.key(), &forecast).unwrap();
        let loaded = load(&store, &place, false).unwrap();
        assert_eq!(loaded.forecast, forecast);
        assert_eq!(loaded.stale, None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn failures_round_trip_through_their_code() {
        assert_eq!(
            Failure::from_code(Failure::Offline.code()),
            Failure::Offline
        );
        assert_eq!(
            Failure::from_code(Failure::Service.code()),
            Failure::Service
        );
    }
}
