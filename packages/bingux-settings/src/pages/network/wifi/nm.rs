//! What NetworkManager says about Wi-Fi, and the commands that change it:
//! the networks in range, the saved profiles, joining, forgetting, and the
//! secrets a saved profile holds. Pure parsing sits apart from the commands so
//! the tests can feed it recorded `nmcli` output.

use std::collections::HashSet;
use std::time::Duration;

use crate::pages::network::model::{self, CONNECT, QUICK, nmcli, split_terse};
use crate::system::CommandError;

/// `device wifi list` can take about four seconds on a busy radio.
const SCAN: Duration = Duration::from_secs(10);
/// Taking a connection down releases its DHCP lease, which is not instant.
const DOWN: Duration = Duration::from_secs(20);

/// How a network protects itself, from the `SECURITY` column of `nmcli device
/// wifi list`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Security {
    Open,
    /// Opportunistic Wireless Encryption: encrypted, with no password.
    Owe,
    Wep,
    /// WPA, or WPA and WPA2 together.
    Wpa,
    Wpa2,
    Wpa3,
    /// A WPA2 and WPA3 transition network: either password works.
    Wpa2Wpa3,
    /// 802.1X: a user name and password, not one shared password.
    Enterprise,
}

impl Security {
    pub fn parse(raw: &str) -> Self {
        let raw = raw.trim();
        if raw.is_empty() || raw == "--" {
            return Self::Open;
        }
        if raw.contains("802.1X") {
            return Self::Enterprise;
        }
        if raw.contains("OWE") {
            return Self::Owe;
        }
        let wpa3 = raw.contains("WPA3");
        let wpa2 = raw.contains("WPA2");
        let wpa1 = raw.contains("WPA1");
        match (wpa3, wpa2, wpa1) {
            (true, true, _) => Self::Wpa2Wpa3,
            (true, false, _) => Self::Wpa3,
            (false, true, false) => Self::Wpa2,
            (false, true, true) | (false, false, true) => Self::Wpa,
            _ if raw.contains("WEP") => Self::Wep,
            _ => Self::Wpa,
        }
    }

    /// The security a saved profile's `802-11-wireless-security.key-mgmt` stands for.
    pub fn from_key_mgmt(key_mgmt: &str) -> Self {
        match key_mgmt {
            "" => Self::Open,
            "none" | "ieee8021x" => Self::Wep,
            "sae" => Self::Wpa3,
            "owe" => Self::Owe,
            "wpa-eap" | "wpa-eap-suite-b-192" => Self::Enterprise,
            _ => Self::Wpa2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Owe => "Enhanced Open",
            Self::Wep => "WEP",
            Self::Wpa => "WPA / WPA2",
            Self::Wpa2 => "WPA2",
            Self::Wpa3 => "WPA3",
            Self::Wpa2Wpa3 => "WPA2 / WPA3",
            Self::Enterprise => "Enterprise (802.1X)",
        }
    }

    /// Whether the network asks for one shared password to join.
    pub fn needs_password(self) -> bool {
        matches!(
            self,
            Self::Wep | Self::Wpa | Self::Wpa2 | Self::Wpa3 | Self::Wpa2Wpa3
        )
    }

    /// Whether the connection is protected at all, for the lock icon.
    pub fn secured(self) -> bool {
        self != Self::Open
    }
}

/// One network in range. Access points that share a name are one network.
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Network {
    pub ssid: String,
    /// 0 to 100.
    pub signal: u8,
    pub security: Security,
    pub in_use: bool,
    /// Whether a saved profile joins this network.
    pub saved: bool,
    /// "2.4 GHz", "5 GHz" or "6 GHz", or empty when unknown.
    pub band: String,
    /// "540 Mbit/s", or empty when the access point does not say.
    pub rate: String,
    pub bssid: String,
}

/// One saved Wi-Fi profile, in range or not.
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Saved {
    /// The profile id, which is its name.
    pub name: String,
    pub uuid: String,
    pub ssid: String,
    pub autoconnect: bool,
    pub priority: i32,
    /// An access point profile: a hotspot, not a network to join.
    pub ap: bool,
    pub active: bool,
    /// `802-11-wireless-security.key-mgmt`; empty for an open network.
    pub key_mgmt: String,
    /// `yes`, `no` or `unknown`.
    pub metered: String,
    /// `bg` (2.4 GHz), `a` (5 GHz) or empty for either.
    pub band: String,
}

impl Saved {
    pub fn security(&self) -> Security {
        Security::from_key_mgmt(&self.key_mgmt)
    }
}

/// The lists the Wi-Fi screens draw from.
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Model {
    pub networks: Vec<Network>,
    pub saved: Vec<Saved>,
}

/// What an address's name stands for: a saved profile, a network in range,
/// or both.
pub struct Resolved<'a> {
    pub profile: Option<&'a Saved>,
    pub network: Option<&'a Network>,
    pub ssid: String,
}

impl Model {
    /// Saved profiles that join a network, hotspots left out.
    pub fn joinable(&self) -> impl Iterator<Item = &Saved> {
        self.saved.iter().filter(|saved| !saved.ap)
    }

    /// The hotspot profile that is running, if any.
    pub fn hotspot(&self) -> Option<&Saved> {
        self.saved.iter().find(|saved| saved.ap && saved.active)
    }

    /// The network `key` names: a profile id, else a network name. A network
    /// screen is addressed by name, and a saved profile need not be named
    /// like its network.
    pub fn resolve(&self, key: &str) -> Resolved<'_> {
        let profile = self.joinable().find(|saved| saved.name == key).or_else(|| {
            self.joinable()
                .filter(|saved| saved.ssid == key)
                .max_by_key(|saved| saved.active)
        });
        let ssid = profile.map_or_else(|| key.to_string(), |saved| saved.ssid.clone());
        let network = self.networks.iter().find(|network| network.ssid == ssid);
        Resolved {
            profile,
            network,
            ssid,
        }
    }

    /// The joined network and the profile that joined it.
    pub fn current(&self) -> Option<(Option<&Network>, Option<&Saved>)> {
        let profile = self.joinable().find(|saved| saved.active);
        let network = self.networks.iter().find(|network| network.in_use);
        match (network, profile) {
            (None, None) => None,
            (Some(network), None) => Some((
                Some(network),
                self.joinable().find(|saved| saved.ssid == network.ssid),
            )),
            (network, profile) => Some((network, profile)),
        }
    }
}

/// "2.4 GHz", "5 GHz" or "6 GHz" from a frequency such as "5180 MHz". Empty
/// when the frequency is missing or in no known band.
pub fn band_label(freq: &str) -> String {
    let mhz = freq
        .split_whitespace()
        .next()
        .and_then(|value| value.parse::<u32>().ok());
    match mhz {
        Some(2400..=2499) => "2.4 GHz",
        Some(4900..=5899) => "5 GHz",
        Some(5925..=7125) => "6 GHz",
        _ => "",
    }
    .to_string()
}

/// Words for a signal percentage, for the network screen.
pub fn signal_words(signal: u8) -> &'static str {
    match signal {
        80..=u8::MAX => "Excellent",
        60..=79 => "Good",
        40..=59 => "Fair",
        _ => "Weak",
    }
}

/// Filled bars for a row, 0 to 4, from the signal percentage.
pub fn signal_level(signal: u8) -> u8 {
    match signal {
        80..=u8::MAX => 4,
        60..=79 => 3,
        40..=59 => 2,
        1..=39 => 1,
        0 => 0,
    }
}

/// Networks from `device wifi list` output. Access points that share a name
/// are one network, described by the access point in use, else the strongest;
/// it is saved when `is_saved` knows its name.
pub fn parse_networks(output: &str, is_saved: impl Fn(&str) -> bool) -> Vec<Network> {
    let mut networks: Vec<Network> = Vec::new();
    for line in output.lines().filter(|line| !line.is_empty()) {
        let fields = split_terse(line);
        let field = |index: usize| fields.get(index).map(String::as_str).unwrap_or("");
        let ssid = field(0);
        if ssid.is_empty() {
            continue;
        }
        let access_point = Network {
            ssid: ssid.to_string(),
            signal: field(1).parse::<u8>().unwrap_or(0),
            security: Security::parse(field(2)),
            in_use: field(3).trim() == "*",
            saved: is_saved(ssid),
            band: band_label(field(5)),
            rate: field(6).to_string(),
            bssid: field(4).to_string(),
        };
        match networks.iter_mut().find(|network| network.ssid == ssid) {
            Some(network) => {
                if access_point.in_use || (!network.in_use && access_point.signal > network.signal)
                {
                    *network = access_point;
                }
            }
            None => networks.push(access_point),
        }
    }
    // Sorted by name alone. A rescan changes signal and connection, and a sort
    // on either would reorder rows on screen while the list is open.
    networks.sort_by(|a, b| a.ssid.cmp(&b.ssid));
    networks
}

/// One row of `connection show` with the fields
/// `NAME,UUID,TYPE,AUTOCONNECT,AUTOCONNECT-PRIORITY,ACTIVE`.
#[derive(Debug, PartialEq)]
pub struct ListedProfile {
    pub name: String,
    pub uuid: String,
    pub autoconnect: bool,
    pub priority: i32,
    pub active: bool,
}

/// The Wi-Fi profiles in that listing.
pub fn parse_profile_list(output: &str) -> Vec<ListedProfile> {
    output
        .lines()
        .filter_map(|line| {
            let fields = split_terse(line);
            if fields.get(2).map(String::as_str) != Some("802-11-wireless") {
                return None;
            }
            Some(ListedProfile {
                name: fields[0].clone(),
                uuid: fields.get(1)?.clone(),
                autoconnect: fields.get(3).map(String::as_str) == Some("yes"),
                priority: fields.get(4).and_then(|p| p.parse().ok()).unwrap_or(0),
                active: fields.get(5).map(String::as_str) == Some("yes"),
            })
        })
        .collect()
}

/// What `nmcli -t -f <properties> connection show <profile>` says of one profile.
#[derive(Debug, Default, PartialEq)]
pub struct ProfileFacts {
    pub ssid: String,
    pub ap: bool,
    pub key_mgmt: String,
    pub metered: String,
    pub band: String,
}

/// Properties come as `name:value` lines. The value is raw: a colon in it is
/// not escaped, so the line is cut at the first colon only.
pub fn parse_profile_facts(output: &str) -> ProfileFacts {
    let mut facts = ProfileFacts::default();
    for line in output.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name {
            "802-11-wireless.ssid" => facts.ssid = value.to_string(),
            "802-11-wireless.mode" => facts.ap = value == "ap",
            "802-11-wireless-security.key-mgmt" => facts.key_mgmt = value.to_string(),
            "connection.metered" => facts.metered = value.to_string(),
            "802-11-wireless.band" => facts.band = value.to_string(),
            _ => {}
        }
    }
    facts
}

/// The saved profiles, with the details only a profile of its own can give.
fn read_saved() -> Result<Vec<Saved>, CommandError> {
    let listing = nmcli(
        &[
            "-t",
            "-f",
            "NAME,UUID,TYPE,AUTOCONNECT,AUTOCONNECT-PRIORITY,ACTIVE",
            "connection",
            "show",
        ],
        QUICK,
    )?;
    let listed = parse_profile_list(&listing);
    // One `nmcli` per profile, side by side: a dozen profiles would otherwise
    // hold the poll for seconds.
    let facts: Vec<Option<ProfileFacts>> = std::thread::scope(|scope| {
        let reads: Vec<_> = listed
            .iter()
            .map(|profile| {
                scope.spawn(|| {
                    nmcli(
                        &[
                            "-t",
                            "-f",
                            "802-11-wireless.ssid,802-11-wireless.mode,802-11-wireless-security.key-mgmt,connection.metered,802-11-wireless.band",
                            "connection",
                            "show",
                            "uuid",
                            &profile.uuid,
                        ],
                        QUICK,
                    )
                    .ok()
                    .map(|output| parse_profile_facts(&output))
                })
            })
            .collect();
        reads
            .into_iter()
            .map(|read| read.join().ok().flatten())
            .collect()
    });
    let mut saved: Vec<Saved> = listed
        .into_iter()
        .zip(facts)
        // A profile deleted between the two reads has no facts: skip it.
        .filter_map(|(profile, facts)| {
            let facts = facts?;
            Some(Saved {
                name: profile.name,
                uuid: profile.uuid,
                ssid: facts.ssid,
                autoconnect: profile.autoconnect,
                priority: profile.priority,
                ap: facts.ap,
                active: profile.active,
                key_mgmt: facts.key_mgmt,
                metered: facts.metered,
                band: facts.band,
            })
        })
        .collect();
    saved.sort_by_key(|saved| saved.name.to_lowercase());
    Ok(saved)
}

/// Snapshot builds can stand in a Wi-Fi radio that is on, with one joined
/// network, two saved and others in range: `BINGUX_SETTINGS_FAKE_WIFI=1`, or
/// any `BINGUX_SETTINGS_FAKE_NETWORK` scenario (see `model::fake_scenario`).
/// `BINGUX_SETTINGS_FAKE_HOTSPOT=1` also runs a hotspot. Nothing is read from
/// `nmcli`, and no radio is changed.
#[cfg(feature = "snapshot")]
pub fn fake() -> Option<Model> {
    let scenario = model::fake_scenario();
    if scenario.is_none() && std::env::var_os("BINGUX_SETTINGS_FAKE_WIFI").is_none() {
        return None;
    }
    let scenario = scenario.unwrap_or_else(|| "online".to_string());
    let radio_on = !matches!(scenario.as_str(), "wifi-off" | "airplane" | "no-wifi");
    let joined = radio_on && scenario != "offline";
    let network = |ssid: &str, signal: u8, security: Security, in_use: bool, saved: bool| Network {
        ssid: ssid.into(),
        signal,
        security,
        in_use,
        saved,
        band: if signal > 50 { "5 GHz" } else { "2.4 GHz" }.into(),
        rate: if signal > 50 {
            "540 Mbit/s"
        } else {
            "144 Mbit/s"
        }
        .into(),
        bssid: format!("A4:2B:B0:11:22:{signal:02X}"),
    };
    let profile = |name: &str, key_mgmt: &str, priority: i32, active: bool| Saved {
        name: name.into(),
        uuid: format!("fake-wifi-{name}"),
        ssid: name.into(),
        autoconnect: true,
        priority,
        ap: false,
        active,
        key_mgmt: key_mgmt.into(),
        metered: "unknown".into(),
        band: String::new(),
    };
    let mut saved = Vec::new();
    let mut networks = Vec::new();
    if scenario != "no-wifi" {
        saved.push(profile("Home", "wpa-psk", 0, joined));
        saved.push(profile("Office", "wpa-psk", 5, false));
        saved.push(profile("Old flat", "wpa-psk", 0, false));
        if std::env::var_os("BINGUX_SETTINGS_FAKE_HOTSPOT").is_some() {
            saved.push(Saved {
                ap: true,
                ..profile("Hotspot", "wpa-psk", 0, true)
            });
        }
    }
    if radio_on {
        networks = vec![
            network("Home", 82, Security::Wpa2, joined, true),
            network("Office", 64, Security::Wpa2Wpa3, false, true),
            network("Neighbour", 41, Security::Wpa3, false, false),
            network("Cafe Guest", 55, Security::Open, false, false),
            network("Campus", 36, Security::Enterprise, false, false),
        ];
    }
    Some(Model { networks, saved })
}

#[cfg(not(feature = "snapshot"))]
pub fn fake() -> Option<Model> {
    None
}

/// Whether snapshot fakes stand in for the system, so commands change nothing.
#[cfg(feature = "snapshot")]
pub fn faked() -> bool {
    fake().is_some()
}

#[cfg(not(feature = "snapshot"))]
pub fn faked() -> bool {
    false
}

/// Read the lists. Networks in range come from NetworkManager's cached scan
/// (a fresh scan can block for ten seconds); `rescan` and the poll loop refresh
/// it. With the radio off or blocked only the saved profiles are read.
pub fn load() -> Result<Model, CommandError> {
    if let Some(model) = fake() {
        return Ok(model);
    }
    let radio = split_terse(nmcli(&["-t", "-f", "WIFI-HW,WIFI", "radio"], QUICK)?.trim());
    let usable = radio.first().is_some_and(|hw| hw == "enabled")
        && radio.get(1).is_some_and(|on| on == "enabled");
    let saved = read_saved()?;
    let networks = if usable {
        parse_networks(
            &nmcli(
                &[
                    "-t",
                    "-f",
                    "SSID,SIGNAL,SECURITY,IN-USE,BSSID,FREQ,RATE",
                    "device",
                    "wifi",
                    "list",
                    "--rescan",
                    "no",
                ],
                QUICK,
            )?,
            |ssid| {
                saved
                    .iter()
                    .any(|profile| !profile.ap && profile.ssid == ssid)
            },
        )
    } else {
        Vec::new()
    };
    Ok(Model { networks, saved })
}

/// Ask the radio for a fresh scan. A scan that cannot start (a busy radio)
/// is not an error: the list simply stays as it was.
pub fn rescan() {
    if !faked() {
        let _ = nmcli(&["device", "wifi", "rescan"], SCAN);
    }
}

/// The sentence for a failed join: what probably went wrong, then what
/// `nmcli` said, verbatim.
pub fn join_error(error: &CommandError) -> String {
    let said = error.0.trim();
    let lower = said.to_lowercase();
    let reason = if lower.contains("secrets were required") || lower.contains("(7)") {
        "The password was not accepted."
    } else if lower.contains("timeout expired") {
        "The network did not answer in time."
    } else if lower.contains("no network with ssid") || lower.contains("could not be found") {
        "The network is out of range."
    } else {
        return said.to_string();
    };
    format!("{reason} ({said})")
}

/// The UUIDs of every Wi-Fi profile, to tell the one a failed join created.
fn wifi_uuids() -> HashSet<String> {
    nmcli(
        &[
            "-t",
            "-f",
            "NAME,UUID,TYPE,AUTOCONNECT,AUTOCONNECT-PRIORITY,ACTIVE",
            "connection",
            "show",
        ],
        QUICK,
    )
    .map(|output| {
        parse_profile_list(&output)
            .into_iter()
            .map(|profile| profile.uuid)
            .collect()
    })
    .unwrap_or_default()
}

/// Run a join that may create a profile, and when it fails remove what it
/// created: `nmcli` keeps the profile of a failed join, with the wrong
/// password in it, and the network would then read as saved.
fn join_cleaning_up(
    join: impl FnOnce() -> Result<String, CommandError>,
) -> Result<(), CommandError> {
    let before = wifi_uuids();
    match join() {
        Ok(_) => Ok(()),
        Err(error) => {
            for uuid in wifi_uuids().difference(&before) {
                let _ = nmcli(&["connection", "delete", "uuid", uuid], QUICK);
            }
            Err(error)
        }
    }
}

/// A password every fake join rejects, so a snapshot can show the failure.
#[cfg(feature = "snapshot")]
pub const FAKE_WRONG_PASSWORD: &str = "wrong-password";

/// Bring up a saved profile.
pub fn connect_saved(uuid: &str) -> Result<(), CommandError> {
    if faked() {
        return Ok(());
    }
    nmcli(&["connection", "up", "uuid", uuid], CONNECT).map(|_| ())
}

/// Join a network that is not saved yet; `password` is `None` for an open one.
pub fn connect_new(ssid: &str, password: Option<&str>) -> Result<(), CommandError> {
    #[cfg(feature = "snapshot")]
    if faked() {
        return if password == Some(FAKE_WRONG_PASSWORD) {
            Err(CommandError(
                "Connection activation failed: (7) Secrets were required, but not provided.".into(),
            ))
        } else {
            Ok(())
        };
    }
    join_cleaning_up(|| match password {
        Some(password) => nmcli(
            &["device", "wifi", "connect", ssid, "password", password],
            CONNECT,
        ),
        None => nmcli(&["device", "wifi", "connect", ssid], CONNECT),
    })
}

/// How a hidden network protects itself, as the hidden-network form offers it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HiddenSecurity {
    Open,
    Wpa2,
    Wpa3,
}

impl HiddenSecurity {
    fn key_mgmt(self) -> Option<&'static str> {
        match self {
            Self::Open => None,
            Self::Wpa2 => Some("wpa-psk"),
            Self::Wpa3 => Some("sae"),
        }
    }
}

/// Join a hidden network. It cannot be scanned, so the profile is written
/// with the security the person chose, then brought up.
pub fn connect_hidden(
    ssid: &str,
    security: HiddenSecurity,
    password: &str,
) -> Result<(), CommandError> {
    #[cfg(feature = "snapshot")]
    if faked() {
        return if password == FAKE_WRONG_PASSWORD {
            Err(CommandError(
                "Connection activation failed: (7) Secrets were required, but not provided.".into(),
            ))
        } else {
            Ok(())
        };
    }
    join_cleaning_up(|| {
        let mut args = vec![
            "connection",
            "add",
            "type",
            "wifi",
            "ifname",
            "*",
            "con-name",
            ssid,
            "ssid",
            ssid,
            "802-11-wireless.hidden",
            "yes",
        ];
        if let Some(key_mgmt) = security.key_mgmt() {
            args.extend(["wifi-sec.key-mgmt", key_mgmt, "wifi-sec.psk", password]);
        }
        nmcli(&args, QUICK)?;
        nmcli(&["connection", "up", "id", ssid], CONNECT)
    })
}

/// Join an 802.1X network with a user name and password (PEAP with MSCHAPv2,
/// the method most schools and companies use), checking the server against
/// the system's certificate authorities.
pub fn connect_enterprise(ssid: &str, user: &str, password: &str) -> Result<(), CommandError> {
    #[cfg(feature = "snapshot")]
    if faked() {
        return if password == FAKE_WRONG_PASSWORD {
            Err(CommandError(
                "Connection activation failed: (7) Secrets were required, but not provided.".into(),
            ))
        } else {
            Ok(())
        };
    }
    join_cleaning_up(|| {
        nmcli(
            &[
                "connection",
                "add",
                "type",
                "wifi",
                "ifname",
                "*",
                "con-name",
                ssid,
                "ssid",
                ssid,
                "wifi-sec.key-mgmt",
                "wpa-eap",
                "802-1x.eap",
                "peap",
                "802-1x.phase2-auth",
                "mschapv2",
                "802-1x.identity",
                user,
                "802-1x.password",
                password,
                "802-1x.system-ca-certs",
                "yes",
            ],
            QUICK,
        )?;
        nmcli(&["connection", "up", "id", ssid], CONNECT)
    })
}

/// Take a connection down.
pub fn disconnect(uuid: &str) -> Result<(), CommandError> {
    if faked() {
        return Ok(());
    }
    nmcli(&["connection", "down", "uuid", uuid], DOWN).map(|_| ())
}

/// Delete a saved profile, and with it the password.
pub fn forget(uuid: &str) -> Result<(), CommandError> {
    if faked() {
        return Ok(());
    }
    nmcli(&["connection", "delete", "uuid", uuid], QUICK).map(|_| ())
}

/// The secret a saved profile holds: its WEP key or its WPA password, read
/// from NetworkManager. `Ok(None)` when the profile stores none (an open
/// network, or a secret kept by a keyring agent).
pub fn saved_secret(uuid: &str, key_mgmt: &str) -> Result<Option<String>, CommandError> {
    #[cfg(feature = "snapshot")]
    if uuid.starts_with("fake-wifi-") {
        return Ok(Some("correct horse battery staple".to_string()));
    }
    let property = match key_mgmt {
        "none" => "802-11-wireless-security.wep-key0",
        "" | "wpa-eap" | "wpa-eap-suite-b-192" | "owe" => return Ok(None),
        // Both WPA2 and WPA3 keep the password in `psk`.
        _ => "802-11-wireless-security.psk",
    };
    let output = nmcli(
        &["-s", "-g", property, "connection", "show", "uuid", uuid],
        QUICK,
    )?;
    // `-g` escapes colons and backslashes.
    let secret = model::unescape(output.trim_end_matches(['\r', '\n']));
    Ok((!secret.is_empty()).then_some(secret))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = "\
Home:82:WPA2:*:A4\\:2B\\:B0\\:11\\:22\\:33:5180 MHz:540 Mbit/s
Home:61:WPA2: :A4\\:2B\\:B0\\:99\\:99\\:99:2437 MHz:130 Mbit/s
Office:64:WPA2 WPA3: :00\\:11\\:22\\:33\\:44\\:55:5745 MHz:270 Mbit/s
Cafe\\: Guest:55:--: :AA\\:BB\\:CC\\:DD\\:EE\\:FF:2412 MHz:54 Mbit/s
::WPA2: :01\\:01\\:01\\:01\\:01\\:01:2412 MHz:54 Mbit/s
Weak one:12:WEP: :02\\:02\\:02\\:02\\:02\\:02:2412 MHz:11 Mbit/s
";

    #[test]
    fn networks_merge_access_points_and_keep_the_joined_one() {
        let networks = parse_networks(LIST, |ssid| ssid == "Home");
        let names: Vec<_> = networks.iter().map(|n| n.ssid.as_str()).collect();
        // Sorted by name; the hidden (blank) name is dropped; a colon in a name survives.
        assert_eq!(names, ["Cafe: Guest", "Home", "Office", "Weak one"]);
        let home = &networks[1];
        assert!(home.in_use && home.saved);
        // The joined access point describes the connection, not the louder neighbour.
        assert_eq!(home.band, "5 GHz");
        assert_eq!(home.rate, "540 Mbit/s");
        assert_eq!(home.bssid, "A4:2B:B0:11:22:33");
        assert_eq!(home.signal, 82);
    }

    #[test]
    fn strongest_access_point_wins_when_none_is_joined() {
        let output = "Mesh:40:WPA2: :AA\\:00:2412 MHz:54 Mbit/s\nMesh:75:WPA2: :BB\\:00:5180 MHz:400 Mbit/s\n";
        let networks = parse_networks(output, |_| false);
        assert_eq!(networks.len(), 1);
        assert_eq!(networks[0].signal, 75);
        assert_eq!(networks[0].band, "5 GHz");
        assert!(!networks[0].saved);
    }

    #[test]
    fn security_names() {
        use Security::*;
        let cases = [
            ("", Open),
            ("--", Open),
            ("WEP", Wep),
            ("WPA1", Wpa),
            ("WPA1 WPA2", Wpa),
            ("WPA2", Wpa2),
            ("WPA3", Wpa3),
            ("WPA2 WPA3", Wpa2Wpa3),
            ("WPA2 802.1X", Enterprise),
            ("WPA3 802.1X", Enterprise),
            ("OWE", Owe),
        ];
        for (raw, expected) in cases {
            assert_eq!(Security::parse(raw), expected, "{raw:?}");
        }
        assert!(Wpa3.needs_password() && !Open.needs_password() && !Enterprise.needs_password());
        assert_eq!(Security::from_key_mgmt("sae"), Wpa3);
        assert_eq!(Security::from_key_mgmt(""), Open);
    }

    #[test]
    fn bands_and_signal() {
        assert_eq!(band_label("2437 MHz"), "2.4 GHz");
        assert_eq!(band_label("5180 MHz"), "5 GHz");
        assert_eq!(band_label("5955 MHz"), "6 GHz");
        assert_eq!(band_label(""), "");
        assert_eq!(signal_level(0), 0);
        assert_eq!(signal_level(39), 1);
        assert_eq!(signal_level(100), 4);
        assert_eq!(signal_words(65), "Good");
    }

    #[test]
    fn profile_list_keeps_wifi_only() {
        let output = "\
Wired connection 1:53e2ef18-ad77:802-3-ethernet:yes:-999:yes
EE WiFi:d3bb8163-d8b9:802-11-wireless:yes:0:no
Cafe\\: 5G:aaaa-bbbb:802-11-wireless:no:10:yes
";
        let profiles = parse_profile_list(output);
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0].name, "EE WiFi");
        assert!(profiles[0].autoconnect && !profiles[0].active);
        assert_eq!(profiles[1].name, "Cafe: 5G");
        assert_eq!((profiles[1].priority, profiles[1].active), (10, true));
        assert!(!profiles[1].autoconnect);
    }

    #[test]
    fn profile_facts_keep_colons_in_values() {
        let output = "\
802-11-wireless.ssid:Cafe: \"Un;it,y\"\\x
802-11-wireless.mode:infrastructure
connection.metered:unknown
802-11-wireless-security.key-mgmt:wpa-psk
";
        let facts = parse_profile_facts(output);
        assert_eq!(facts.ssid, "Cafe: \"Un;it,y\"\\x");
        assert!(!facts.ap);
        assert_eq!(facts.key_mgmt, "wpa-psk");
        assert_eq!(facts.metered, "unknown");
        let ap = parse_profile_facts("802-11-wireless.mode:ap\n");
        assert!(ap.ap && ap.key_mgmt.is_empty());
    }

    fn saved(name: &str, ssid: &str, active: bool) -> Saved {
        Saved {
            name: name.into(),
            uuid: format!("uuid-{name}"),
            ssid: ssid.into(),
            autoconnect: true,
            priority: 0,
            ap: false,
            active,
            key_mgmt: "wpa-psk".into(),
            metered: "unknown".into(),
            band: String::new(),
        }
    }

    #[test]
    fn a_network_screen_finds_its_profile_by_id_or_by_name() {
        let model = Model {
            networks: parse_networks("Flat:70:WPA2:*:AA\\:AA:5180 MHz:300 Mbit/s\n", |_| true),
            saved: vec![
                saved("Flat 5G", "Flat", true),
                saved("Work", "Work", false),
                Saved {
                    ap: true,
                    ..saved("Hotspot", "Hotspot", true)
                },
            ],
        };
        let by_id = model.resolve("Flat 5G");
        assert_eq!(by_id.ssid, "Flat");
        assert!(by_id.network.is_some());
        let by_name = model.resolve("Flat");
        assert_eq!(by_name.profile.map(|p| p.name.as_str()), Some("Flat 5G"));
        let unknown = model.resolve("Stranger");
        assert!(unknown.profile.is_none() && unknown.network.is_none());
        assert_eq!(unknown.ssid, "Stranger");
        // A hotspot is not a network to join.
        assert!(model.resolve("Hotspot").profile.is_none());
        assert_eq!(model.hotspot().map(|h| h.name.as_str()), Some("Hotspot"));
        assert_eq!(model.joinable().count(), 2);
    }

    #[test]
    fn nmcli_errors_read_plainly() {
        let wrong = CommandError(
            "Connection activation failed: (7) Secrets were required, but not provided.".into(),
        );
        let text = join_error(&wrong);
        assert!(text.starts_with("The password was not accepted."));
        assert!(text.contains("Secrets were required"));
        assert!(
            join_error(&CommandError("Timeout expired (90 seconds)".into()))
                .starts_with("The network did not answer in time.")
        );
        assert_eq!(
            join_error(&CommandError("Something else broke.".into())),
            "Something else broke."
        );
    }
    /// Deletes the throwaway profile even when an assertion fails.
    struct Throwaway(&'static str);

    impl Drop for Throwaway {
        fn drop(&mut self) {
            let _ = nmcli(&["connection", "delete", "id", self.0], QUICK);
        }
    }

    /// Talks to the real NetworkManager with a throwaway profile; the Wi-Fi
    /// radio is never touched, so it is safe with the radio off. Run with
    /// `cargo test -- --ignored real_profile`.
    #[test]
    #[ignore = "talks to NetworkManager"]
    fn real_profile_round_trip() {
        const NAME: &str = "nm-test-networkmerge-wifi";
        const SSID: &str = "Cafe: \"Un;it,y\"\\x";
        const PASSWORD: &str = "p:a\\ss;w,o\"rd1";
        let _cleanup = Throwaway(NAME);
        let _ = nmcli(&["connection", "delete", "id", NAME], QUICK);
        nmcli(
            &[
                "connection",
                "add",
                "type",
                "wifi",
                "ifname",
                "*",
                "con-name",
                NAME,
                "ssid",
                SSID,
                "wifi-sec.key-mgmt",
                "wpa-psk",
                "wifi-sec.psk",
                PASSWORD,
                "autoconnect",
                "no",
            ],
            QUICK,
        )
        .expect("the profile is added");

        let model = load().expect("the lists load");
        let saved = model
            .saved
            .iter()
            .find(|saved| saved.name == NAME)
            .expect("the profile is listed");
        assert_eq!(saved.ssid, SSID);
        assert_eq!(saved.key_mgmt, "wpa-psk");
        assert!(!saved.autoconnect && !saved.ap && !saved.active);
        assert_eq!(saved.metered, "unknown");

        let secret = saved_secret(&saved.uuid, &saved.key_mgmt).expect("the secret reads");
        assert_eq!(secret.as_deref(), Some(PASSWORD));
        let text = super::super::share::payload(
            &saved.ssid,
            super::super::share::Kind::of(&saved.key_mgmt).unwrap(),
            secret.as_deref(),
        );
        assert_eq!(
            text,
            "WIFI:T:WPA;S:Cafe\\: \\\"Un\\;it\\,y\\\"\\\\x;P:p\\:a\\\\ss\\;w\\,o\\\"rd1;;"
        );

        forget(&saved.uuid).expect("the profile is forgotten");
        assert!(
            !load()
                .expect("the lists load again")
                .saved
                .iter()
                .any(|saved| saved.name == NAME)
        );
    }
}
