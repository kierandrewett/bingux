//! One HTTPS GET, for the lookups of this module.

use std::time::Duration;

const AGENT: &str = "bingux-ui-location/0.1 (https://github.com/kierandrewett/bingux)";

/// Why a request did not give a body.
#[derive(Debug)]
pub(super) enum Failure {
    /// The server could not be reached in time.
    Offline,
    /// The server answered with an error status.
    Status,
}

/// GET `url` as text, giving up after `timeout`.
pub(super) fn get(url: &str, timeout: Duration) -> Result<String, Failure> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into();
    let mut response = agent
        .get(url)
        .header("User-Agent", AGENT)
        .call()
        .map_err(|error| match error {
            ureq::Error::StatusCode(_) => Failure::Status,
            _ => Failure::Offline,
        })?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|_| Failure::Offline)
}
