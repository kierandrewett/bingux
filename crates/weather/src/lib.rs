//! Weather data for Bingux, with no UI.
//!
//! [`api`] fetches a forecast, the air quality and a city search from
//! [Open-Meteo](https://open-meteo.com) (no key; the data is CC BY 4.0, so the
//! application that shows it names the source). [`model`] holds what comes
//! back, in metric units, with every time as *wall-clock seconds at the
//! place* (no time zone arithmetic after the fetch). [`units`] writes values in
//! the person's units, [`codes`] turns WMO weather codes into words,
//! [`astro`] works out the moon phase, [`notices`] reads the forecast for
//! conditions worth a warning, and [`store`] keeps the saved places, the
//! settings and the last forecast of each place on disk.
//!
//! Everything here may block (the network, the disk): call it from a worker.

pub mod api;
pub mod astro;
pub mod codes;
pub mod load;
pub mod model;
pub mod notices;
pub mod store;
pub mod text;
pub mod time;
pub mod units;

pub use api::{Error, fetch, search};
pub use load::{Failure, Loaded, load};
pub use model::{AirQuality, Current, Day, Forecast, Hour, Minute, Place};
pub use store::{Here, Settings, Store};
pub use units::{Speed, Temperature, Units};
