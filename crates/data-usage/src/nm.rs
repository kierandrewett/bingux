//! Which NetworkManager connection owns an interface right now.
//!
//! Attribution is best effort: any failure here leaves the connection of
//! a sample NULL, it never stops sampling.

use crate::{
    cmd::run_capture,
    error::{Error, Result},
};
use std::time::Duration;

/// How long to wait for `nmcli`; it normally answers in milliseconds.
const NMCLI_TIMEOUT: Duration = Duration::from_secs(3);

/// How long an `nmcli device` answer is reused.
pub const ACTIVE_TTL: i64 = 60;

/// A device with an active NetworkManager connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveConnection {
    pub device: String,
    pub uuid: String,
    pub name: String,
}

/// The attribution source. [`NmcliResolver`] is the real one; tests and
/// hosts without NetworkManager use [`NoConnections`] or a stub.
pub trait ConnectionResolver: Send {
    /// Devices with an active connection at `now` (UTC epoch seconds).
    /// Implementations cache: this is called every sampling round.
    fn active(&mut self, now: i64) -> Result<Vec<ActiveConnection>>;

    /// Whether the connection is metered (`yes`/`guess-yes`); `None` when
    /// unknown.
    fn metered(&mut self, uuid: &str) -> Option<bool>;
}

/// Never attributes traffic to a connection.
pub struct NoConnections;

impl ConnectionResolver for NoConnections {
    fn active(&mut self, _now: i64) -> Result<Vec<ActiveConnection>> {
        Ok(Vec::new())
    }

    fn metered(&mut self, _uuid: &str) -> Option<bool> {
        None
    }
}

/// Asks `nmcli -t -f DEVICE,CONNECTION,CON-UUID device`, at most once per
/// [`ACTIVE_TTL`] seconds. A failed query is cached too, so a machine
/// without NetworkManager costs one failed spawn a minute, not one per
/// round.
#[derive(Default)]
pub struct NmcliResolver {
    cached: Option<(i64, std::result::Result<Vec<ActiveConnection>, String>)>,
}

impl ConnectionResolver for NmcliResolver {
    fn active(&mut self, now: i64) -> Result<Vec<ActiveConnection>> {
        let fresh = matches!(&self.cached, Some((at, _)) if (0..ACTIVE_TTL).contains(&(now - at)));
        if !fresh {
            let result = run_capture(
                "nmcli",
                &["-t", "-f", "DEVICE,CONNECTION,CON-UUID", "device"],
                NMCLI_TIMEOUT,
            )
            .map(|text| parse_devices(&text))
            .map_err(|error| format!("nmcli: {error}"));
            self.cached = Some((now, result));
        }
        match &self.cached {
            Some((_, Ok(list))) => Ok(list.clone()),
            Some((_, Err(message))) => Err(Error::Command(message.clone())),
            None => Ok(Vec::new()),
        }
    }

    fn metered(&mut self, uuid: &str) -> Option<bool> {
        let text = run_capture(
            "nmcli",
            &[
                "-g",
                "connection.metered",
                "connection",
                "show",
                "uuid",
                uuid,
            ],
            NMCLI_TIMEOUT,
        )
        .ok()?;
        parse_metered(&text)
    }
}

/// Splits one line of `nmcli -t` output on unescaped colons and removes the
/// `\:` and `\\` escapes.
pub fn split_terse(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut characters = line.chars();
    while let Some(character) = characters.next() {
        match character {
            '\\' => {
                if let Some(escaped) = characters.next() {
                    fields.last_mut().expect("never empty").push(escaped);
                }
            }
            ':' => fields.push(String::new()),
            other => fields.last_mut().expect("never empty").push(other),
        }
    }
    fields
}

/// Parses `nmcli -t -f DEVICE,CONNECTION,CON-UUID device`. Devices without
/// an active connection (`--` or empty) are left out.
pub fn parse_devices(text: &str) -> Vec<ActiveConnection> {
    text.lines()
        .filter_map(|line| {
            let fields = split_terse(line);
            let [device, name, uuid] = fields.as_slice() else {
                return None;
            };
            if device.is_empty() || uuid.is_empty() || uuid == "--" {
                return None;
            }
            Some(ActiveConnection {
                device: device.clone(),
                uuid: uuid.clone(),
                name: name.clone(),
            })
        })
        .collect()
}

/// `yes` and `guess-yes` mean metered; `no`, `guess-no` and `unknown` do
/// not. Anything else is not an answer.
pub fn parse_metered(text: &str) -> Option<bool> {
    match text.trim() {
        "yes" | "guess-yes" => Some(true),
        "no" | "guess-no" | "unknown" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terse_fields_handle_escaped_colons() {
        assert_eq!(
            split_terse("wlan0:Home\\: 5G:abc"),
            ["wlan0", "Home: 5G", "abc"]
        );
        assert_eq!(split_terse("a\\\\b:c"), ["a\\b", "c"]);
        assert_eq!(split_terse(""), [""]);
    }

    #[test]
    fn parses_device_list() {
        let text = "enp3s0:Wired connection 1:5f0e0a3e-3a02-4ef2-9f0f-0123456789ab\n\
                    wlan0:Cafe\\: Guest:11111111-2222-3333-4444-555555555555\n\
                    p2p-dev-wlan0:--:--\n\
                    wg0:--:\n\
                    lo:lo:\n";
        let devices = parse_devices(text);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].device, "enp3s0");
        assert_eq!(devices[0].name, "Wired connection 1");
        assert_eq!(devices[1].name, "Cafe: Guest");
    }

    #[test]
    fn metered_values() {
        assert_eq!(parse_metered("yes\n"), Some(true));
        assert_eq!(parse_metered("guess-yes"), Some(true));
        assert_eq!(parse_metered("guess-no"), Some(false));
        assert_eq!(parse_metered("unknown"), Some(false));
        assert_eq!(parse_metered("no"), Some(false));
        assert_eq!(parse_metered("Error: nonsense"), None);
    }
}
