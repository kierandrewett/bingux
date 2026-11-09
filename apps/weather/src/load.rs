//! The work that runs off the window thread: reading what was saved, finding
//! where the computer is, fetching a forecast and searching for a city. Each
//! is a plain function of its key, for `bingux_ui::task::use_resource`.

use bingux_ui::location::{Fix, LocateError, Locator};
use bingux_weather::time::unix_now;
use bingux_weather::{Failure, Here, Loaded, Place, Settings, Store, units};

/// What start-up reads: the settings and where the computer was last found.
#[derive(Clone, Debug, PartialEq)]
pub struct Boot {
    pub settings: Settings,
    pub here: Option<Here>,
}

pub fn boot(_: ()) -> Result<Boot, String> {
    let store = Store::open();
    Ok(Boot {
        settings: store.settings(),
        here: store.here(),
    })
}

/// Which forecast to load. `refresh` counts the times the person has asked for
/// one (Ctrl+R, the button): a forecast is then too old to use again after half
/// a minute, not ten, and each count is a new key, so a second request loads
/// again.
#[derive(Clone, Debug, PartialEq)]
pub struct ForecastKey {
    /// None while there is no place to ask about.
    pub place: Option<Place>,
    pub refresh: u64,
}

/// A forecast and the place it is for. A loader keeps showing the last value
/// while it loads a new one, so a window that moved to another place must
/// check that what it holds is the place's own.
#[derive(Clone, Debug, PartialEq)]
pub struct Fetched {
    pub key: String,
    pub loaded: Loaded,
}

pub fn forecast(key: ForecastKey) -> Result<Fetched, String> {
    let Some(place) = key.place else {
        return Err(NONE.to_string());
    };
    let id = place.key();
    bingux_weather::load(&Store::open(), &place, key.refresh > 0)
        .map(|loaded| Fetched {
            key: id.clone(),
            loaded,
        })
        // The place is in the error, as in the value: a loader that moved to
        // another place keeps the last result until the new one lands.
        .map_err(|failure| format!("{}@{id}", failure.code()))
}

/// The error of a load with no place to ask about.
pub const NONE: &str = "none";

/// The failure and the place key in a forecast loader's error text.
pub fn failure_of(error: &str) -> (Failure, &str) {
    match error.split_once('@') {
        Some((code, key)) => (Failure::from_code(code), key),
        None => (Failure::from_code(error), ""),
    }
}

/// Whether two loads show the same thing, so a poll that found nothing new
/// redraws nothing. The time it was fetched is shown ("Updated 5 min ago"), so
/// a new fetch is never the same.
pub fn same(old: &Fetched, new: &Fetched) -> bool {
    old == new
}

/// Where to look for the position.
#[derive(Clone, Debug, PartialEq)]
pub struct LocateKey {
    pub on: bool,
    /// Counts the times the person asked to look again.
    pub attempt: u64,
}

/// Why the position is not known. The loader's error text.
pub const OFF: &str = "off";
pub const DENIED: &str = "denied";
pub const OFFLINE: &str = "offline";
pub const UNAVAILABLE: &str = "unavailable";

pub fn locate(key: LocateKey) -> Result<Here, String> {
    if !key.on {
        return Err(OFF.to_string());
    }
    match Locator::new().locate() {
        Ok(fix) => {
            let here = here_of(&fix);
            // The next start shows this place before the lookup has finished.
            let _ = Store::open().save_here(&here);
            Ok(here)
        }
        Err(LocateError::Denied) => Err(DENIED.to_string()),
        Err(LocateError::Offline) => Err(OFFLINE.to_string()),
        Err(LocateError::Unavailable | LocateError::Disabled) => Err(UNAVAILABLE.to_string()),
    }
}

fn here_of(fix: &Fix) -> Here {
    let detail: Vec<&str> = [fix.region.as_deref(), fix.country.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    let mut place = Place::new(
        fix.name
            .clone()
            .unwrap_or_else(|| "My Location".to_string()),
        fix.latitude,
        fix.longitude,
    );
    place.detail = detail.join(", ");
    place.timezone = fix.timezone.clone();
    Here {
        place,
        source: fix.source.id().to_string(),
        at: unix_now(),
    }
}

/// Cities that match `query`, for the search sheet.
pub fn search(query: String) -> Result<Vec<Place>, String> {
    bingux_weather::search(&query, &units::language()).map_err(|error| {
        if error.is_offline() {
            OFFLINE.to_string()
        } else {
            UNAVAILABLE.to_string()
        }
    })
}

/// Writes the settings to disk, off the window thread.
pub fn save(settings: Settings) {
    bingux_ui::task::spawn_detached("weather-save", move || {
        let _ = Store::open().save_settings(&settings);
    });
}

/// Deletes the forecasts of places that are no longer in the list.
pub fn prune(keep: Vec<String>) {
    bingux_ui::task::spawn_detached("weather-prune", move || {
        Store::open().prune(&keep);
    });
}
