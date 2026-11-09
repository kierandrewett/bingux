//! Downloads the European Central Bank's daily reference rates.
//!
//! Runs on a worker (`Calc::refresh_rates`). Nothing is reported when the
//! download fails: the rates already held stay in use, and the converter
//! says so.

use std::time::Duration;

use bingux_calc::rates::{ECB_URL, Rates};

pub fn fetch(now: u64) -> Option<Rates> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let mut response = agent.get(ECB_URL).call().ok()?;
    let body = response.body_mut().read_to_string().ok()?;
    Rates::parse_ecb(&body, now)
}
