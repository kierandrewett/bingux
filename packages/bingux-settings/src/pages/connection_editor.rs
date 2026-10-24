//! The data behind a connection editor: what `nmcli connection show` reports for one profile, which values are
//! valid, and the one `nmcli connection modify` call that writes only the values the user changed. Pure
//! functions, so the argument lists can be unit tested without NetworkManager.

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};

/// The keys read from `nmcli connection show`. Wired and Wi-Fi keys are both listed; `parse_profile`
/// keeps the pair that matches the profile's connection type.
pub const FIELDS: &[&str] = &[
    "connection.type",
    "connection.id",
    "connection.autoconnect",
    "connection.autoconnect-priority",
    "connection.permissions",
    "connection.metered",
    "802-11-wireless.bssid",
    "802-11-wireless.cloned-mac-address",
    "802-11-wireless.mac-address-randomization",
    "802-11-wireless.mtu",
    "802-11-wireless-security.key-mgmt",
    "802-11-wireless-security.psk",
    "802-3-ethernet.cloned-mac-address",
    "802-3-ethernet.mtu",
    "802-3-ethernet.wake-on-lan",
    "ipv4.method",
    "ipv4.addresses",
    "ipv4.gateway",
    "ipv4.dns",
    "ipv4.dns-search",
    "ipv4.ignore-auto-dns",
    "ipv4.ignore-auto-routes",
    "ipv4.never-default",
    "ipv6.method",
    "ipv6.addresses",
    "ipv6.gateway",
    "ipv6.dns",
    "ipv6.dns-search",
    "ipv6.ignore-auto-dns",
    "ipv6.ignore-auto-routes",
    "ipv6.never-default",
    "ipv6.ip6-privacy",
];

/// The values of one saved connection, as NetworkManager stores them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub id: String,
    /// True for a Wi-Fi profile, false for a wired one.
    pub wireless: bool,
    pub autoconnect: bool,
    /// Higher priorities are tried first when several profiles can connect.
    pub priority: i64,
    /// True when no user restriction is set, so every user may use it.
    pub everyone: bool,
    /// "yes", "no" or "unknown".
    pub metered: String,
    pub bssid: String,
    /// "", "preserve", "permanent", "random", "stable", or "address" for a literal MAC.
    pub cloned_mac: String,
    /// The literal MAC when `cloned_mac` is "address".
    pub cloned_mac_address: String,
    /// Wi-Fi only: "default", "never" or "always".
    pub mac_randomization: String,
    /// Wired only: "default", "none" or "magic".
    pub wake_on_lan: String,
    /// "auto" or the MTU in bytes.
    pub mtu: String,
    /// "none", "wpa-psk", "sae" or empty for a profile without security.
    pub key_mgmt: String,
    pub psk: String,
    /// -1 default, 0 disabled, 1 prefer public addresses, 2 prefer temporary addresses.
    pub ip6_privacy: i64,
    pub ipv4: Ip,
    pub ipv6: Ip,
}

/// The IP settings of one address family.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ip {
    pub method: String,
    pub addresses: String,
    pub gateway: String,
    pub dns: String,
    pub dns_search: String,
    pub ignore_auto_dns: bool,
    pub ignore_auto_routes: bool,
    pub never_default: bool,
}

/// Undo the `\:` and `\\` escapes of `nmcli -t` output.
pub fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Parse `nmcli --show-secrets -t -f FIELDS connection show id NAME` output: one `key:value` per line.
pub fn parse_profile(output: &str) -> Profile {
    let mut values: HashMap<String, String> = HashMap::new();
    for line in output.lines() {
        if let Some((key, value)) = split_key_value(line) {
            // nmcli prints `--` for a value that is not set.
            let value = if value == "--" { String::new() } else { value };
            values.insert(key.to_string(), value);
        }
    }
    let get = |key: &str| values.get(key).cloned().unwrap_or_default();
    let wireless = get("connection.type") == "802-11-wireless";
    let device = if wireless {
        "802-11-wireless"
    } else {
        "802-3-ethernet"
    };
    let device_key = |suffix: &str| get(&format!("{device}.{suffix}"));
    let (cloned_mac, cloned_mac_address) = split_cloned_mac(&device_key("cloned-mac-address"));
    Profile {
        id: get("connection.id"),
        wireless,
        autoconnect: get("connection.autoconnect") == "yes",
        priority: get("connection.autoconnect-priority").parse().unwrap_or(0),
        everyone: get("connection.permissions").is_empty(),
        metered: or_default(get("connection.metered"), "unknown"),
        bssid: get("802-11-wireless.bssid"),
        cloned_mac,
        cloned_mac_address,
        mac_randomization: or_default(get("802-11-wireless.mac-address-randomization"), "default"),
        wake_on_lan: or_default(get("802-3-ethernet.wake-on-lan"), "none"),
        mtu: or_default(device_key("mtu"), "auto"),
        key_mgmt: get("802-11-wireless-security.key-mgmt"),
        psk: get("802-11-wireless-security.psk"),
        ip6_privacy: get("ipv6.ip6-privacy").parse().unwrap_or(-1),
        ipv4: parse_ip(&get, "ipv4"),
        ipv6: parse_ip(&get, "ipv6"),
    }
}

fn parse_ip(get: &impl Fn(&str) -> String, version: &str) -> Ip {
    let key = |suffix: &str| get(&format!("{version}.{suffix}"));
    Ip {
        method: or_default(key("method"), "auto"),
        addresses: key("addresses"),
        gateway: key("gateway"),
        dns: key("dns"),
        dns_search: key("dns-search"),
        ignore_auto_dns: key("ignore-auto-dns") == "yes",
        ignore_auto_routes: key("ignore-auto-routes") == "yes",
        never_default: key("never-default") == "yes",
    }
}

fn or_default(value: String, default: &str) -> String {
    if value.is_empty() {
        default.to_string()
    } else {
        value
    }
}

/// Split one terse line at its first unescaped colon.
fn split_key_value(line: &str) -> Option<(&str, String)> {
    let mut escaped = false;
    for (index, c) in line.char_indices() {
        match c {
            '\\' if !escaped => escaped = true,
            ':' if !escaped => {
                return Some((&line[..index], unescape(&line[index + 1..])));
            }
            _ => escaped = false,
        }
    }
    None
}

/// The cloned MAC modes NetworkManager accepts as keywords. Any other value is a literal address.
const CLONED_MAC_MODES: &[&str] = &["preserve", "permanent", "random", "stable"];

/// Split the stored `cloned-mac-address` into its mode and, for a literal address, the address.
fn split_cloned_mac(raw: &str) -> (String, String) {
    if raw.is_empty() || CLONED_MAC_MODES.contains(&raw) {
        (raw.to_string(), String::new())
    } else {
        ("address".to_string(), raw.to_string())
    }
}

/// The stored `cloned-mac-address` for a mode and address. Only "address" uses the address.
pub fn join_cloned_mac(mode: &str, address: &str) -> String {
    if mode == "address" {
        address.trim().to_string()
    } else {
        mode.to_string()
    }
}

/// The IPv4 method names NetworkManager uses, with the label shown to the user.
pub const IPV4_METHODS: &[(&str, &str)] = &[
    ("auto", "Automatic (DHCP)"),
    ("link-local", "Link-local only"),
    ("manual", "Manual"),
    ("shared", "Shared to other computers"),
    ("disabled", "Disabled"),
];

/// The IPv6 method names, with the label shown to the user.
pub const IPV6_METHODS: &[(&str, &str)] = &[
    ("auto", "Automatic"),
    ("dhcp", "Automatic, DHCP only"),
    ("link-local", "Link-local only"),
    ("manual", "Manual"),
    ("shared", "Shared to other computers"),
    ("ignore", "Ignore"),
    ("disabled", "Disabled"),
];

/// One sentence on what an IP method does, shown under the method.
pub fn method_caption(version: u8, method: &str) -> &'static str {
    match (version, method) {
        (4, "auto") => "Gets its address, gateway and DNS servers from the network.",
        (6, "auto") => {
            "Gets its address and DNS servers from the network, through router advertisements or DHCPv6."
        }
        (6, "dhcp") => "Gets its address and DNS servers from a DHCPv6 server only.",
        (_, "link-local") => "Only reaches devices on the same link. Nothing beyond this network.",
        (_, "manual") => "Uses the addresses and gateway you enter.",
        (_, "shared") => {
            "Shares this connection with other devices, which get addresses from this computer."
        }
        (6, "ignore") => "NetworkManager does not configure IPv6 for this connection.",
        (_, "disabled") => "Turns this protocol off for the connection.",
        _ => "",
    }
}

/// The IP fields that mean something for a method. Hidden fields are cleared on Apply, except the
/// switches, which keep their stored value.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Shown {
    pub addresses: bool,
    pub gateway: bool,
    pub dns: bool,
    pub dns_search: bool,
    pub auto_dns: bool,
    pub auto_routes: bool,
    pub never_default: bool,
    pub privacy: bool,
}

/// The fields that apply to an IP method. `version` is 4 or 6.
pub fn shown(version: u8, method: &str) -> Shown {
    match method {
        "manual" => Shown {
            addresses: true,
            gateway: true,
            dns: true,
            dns_search: true,
            never_default: true,
            ..Shown::default()
        },
        "auto" | "dhcp" => Shown {
            dns: true,
            dns_search: true,
            auto_dns: true,
            auto_routes: true,
            never_default: true,
            privacy: version == 6,
            ..Shown::default()
        },
        "link-local" | "shared" => Shown {
            dns: true,
            dns_search: true,
            ..Shown::default()
        },
        _ => Shown::default(),
    }
}

/// The Wi-Fi scan and connect MAC randomisation choices, with their labels.
pub const MAC_RANDOMIZATION_CHOICES: &[(&str, &str)] = &[
    ("default", "Default"),
    ("never", "Never"),
    ("always", "Always"),
];

pub fn mac_randomization_caption(mode: &str) -> &'static str {
    match mode {
        "never" => "Always uses the adapter's own address on this network.",
        "always" => "Uses a random address on this network, so it is harder to track.",
        _ => "Follows the system setting.",
    }
}

/// The cloned MAC choices: the mode keyword, or "address" for a literal address.
pub const CLONED_MAC_CHOICES: &[(&str, &str)] = &[
    ("", "Default"),
    ("preserve", "Preserve"),
    ("permanent", "Permanent"),
    ("random", "Random"),
    ("stable", "Stable"),
    ("address", "Specific address"),
];

/// What a cloned MAC mode does, in plain words.
pub fn cloned_mac_caption(mode: &str) -> &'static str {
    match mode {
        "preserve" => "Keeps the address the adapter already has.",
        "permanent" => "Uses the address built into the adapter.",
        "random" => "Uses a new random address each time it connects.",
        "stable" => "Uses a random address that stays the same on this network.",
        "address" => "Uses the address you type here.",
        _ => "Uses the adapter's own address, as NetworkManager chooses.",
    }
}

/// Wake on LAN choices for a wired connection.
pub const WAKE_ON_LAN_CHOICES: &[(&str, &str)] = &[
    ("default", "Default"),
    ("none", "Off"),
    ("magic", "Magic packet"),
];

/// The IPv6 privacy extension choices. Keys are the `ipv6.ip6-privacy` numbers.
pub const IP6_PRIVACY_CHOICES: &[(&str, &str)] = &[
    ("-1", "Default"),
    ("0", "Disabled"),
    ("1", "Prefer public addresses"),
    ("2", "Prefer temporary addresses"),
];

/// Metered choices, in the order the dropdown shows them.
pub const METERED_CHOICES: &[(&str, &str)] =
    &[("unknown", "Automatic"), ("yes", "Yes"), ("no", "No")];

/// The security choices. `None` removes the security section from the profile.
pub const SECURITY_CHOICES: &[(&str, &str)] = &[
    ("none", "None"),
    ("wpa-psk", "WPA & WPA2 Personal"),
    ("sae", "WPA3 Personal"),
];

/// Valid IPv4 address: four decimal octets.
pub fn valid_ipv4(text: &str) -> bool {
    text.trim().parse::<Ipv4Addr>().is_ok()
}

/// Valid address with a prefix length, such as `192.168.1.20/24`.
pub fn valid_ipv4_cidr(text: &str) -> bool {
    let Some((address, prefix)) = text.trim().split_once('/') else {
        return false;
    };
    valid_ipv4(address) && prefix.parse::<u8>().map(|p| p <= 32).unwrap_or(false)
}

/// Valid IPv6 address, with or without a prefix length.
pub fn valid_ipv6_cidr(text: &str) -> bool {
    let text = text.trim();
    let (address, prefix) = match text.split_once('/') {
        Some((address, prefix)) => (address, Some(prefix)),
        None => (text, None),
    };
    address.parse::<Ipv6Addr>().is_ok()
        && prefix.is_none_or(|p| p.parse::<u8>().map(|p| p <= 128).unwrap_or(false))
}

/// Valid MAC address: six hex pairs separated by colons.
pub fn valid_mac(text: &str) -> bool {
    let parts: Vec<&str> = text.trim().split(':').collect();
    parts.len() == 6
        && parts
            .iter()
            .all(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_hexdigit()))
}

/// A comma-separated list where every item passes `valid`. An empty list is valid.
pub fn valid_list(text: &str, valid: impl Fn(&str) -> bool) -> bool {
    text.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .all(valid)
}

/// The values the user can change, in the form the editor shows them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Edit {
    pub id: String,
    pub autoconnect: bool,
    pub priority: i64,
    pub everyone: bool,
    pub metered: String,
    pub cloned_mac: String,
    pub cloned_mac_address: String,
    pub mac_randomization: String,
    pub wake_on_lan: String,
    pub mtu: String,
    pub key_mgmt: String,
    pub psk: String,
    pub ip6_privacy: i64,
    pub ipv4: Ip,
    pub ipv6: Ip,
}

impl Edit {
    /// The editable values of a profile, as the editor starts with them.
    /// Used by the tests; the editor builds its own `Edit`.
    #[cfg(test)]
    pub fn from_profile(profile: &Profile) -> Self {
        Self {
            id: profile.id.clone(),
            autoconnect: profile.autoconnect,
            priority: profile.priority,
            everyone: profile.everyone,
            metered: profile.metered.clone(),
            cloned_mac: profile.cloned_mac.clone(),
            cloned_mac_address: profile.cloned_mac_address.clone(),
            mac_randomization: profile.mac_randomization.clone(),
            wake_on_lan: profile.wake_on_lan.clone(),
            mtu: profile.mtu.clone(),
            key_mgmt: profile.key_mgmt.clone(),
            psk: profile.psk.clone(),
            ip6_privacy: profile.ip6_privacy,
            ipv4: profile.ipv4.clone(),
            ipv6: profile.ipv6.clone(),
        }
    }

    /// Every validation message that blocks Apply. Empty when the edit can be written.
    /// Fields hidden by the chosen method are not checked: they are cleared on Apply.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.id.trim().is_empty() {
            problems.push("Enter a name for this network.".to_string());
        }
        if self.mtu != "auto" && !self.mtu.parse::<u32>().is_ok_and(|v| v >= 576) {
            problems.push("MTU must be Automatic or a number of 576 or more.".to_string());
        }
        if self.cloned_mac == "address" && !valid_mac(&self.cloned_mac_address) {
            problems.push("Enter a MAC address such as 02:00:00:00:00:01.".to_string());
        }

        let v4 = shown(4, &self.ipv4.method);
        if self.ipv4.method == "manual"
            && (!valid_list(&self.ipv4.addresses, valid_ipv4_cidr)
                || self.ipv4.addresses.trim().is_empty())
        {
            problems
                .push("Enter an IPv4 address with a prefix, such as 192.168.1.20/24.".to_string());
        }
        if v4.gateway && !self.ipv4.gateway.trim().is_empty() && !valid_ipv4(&self.ipv4.gateway) {
            problems.push("The IPv4 gateway must be an IPv4 address.".to_string());
        }
        if v4.dns && !valid_list(&self.ipv4.dns, valid_ipv4) {
            problems.push("Each IPv4 DNS server must be an IPv4 address.".to_string());
        }

        let v6 = shown(6, &self.ipv6.method);
        if self.ipv6.method == "manual"
            && (!valid_list(&self.ipv6.addresses, valid_ipv6_cidr)
                || self.ipv6.addresses.trim().is_empty())
        {
            problems
                .push("Enter an IPv6 address with a prefix, such as 2001:db8::20/64.".to_string());
        }
        if v6.gateway
            && !self.ipv6.gateway.trim().is_empty()
            && !valid_ipv6_cidr(&self.ipv6.gateway)
        {
            problems.push("The IPv6 gateway must be an IPv6 address.".to_string());
        }
        if v6.dns
            && !valid_list(&self.ipv6.dns, |item| {
                valid_ipv4(item) || valid_ipv6_cidr(item)
            })
        {
            problems.push("Each DNS server must be an IP address.".to_string());
        }

        if self.key_mgmt != "none" && !self.key_mgmt.is_empty() && self.psk.len() < 8 {
            problems.push("The Wi-Fi password must be at least 8 characters.".to_string());
        }
        problems
    }
}

/// The `nmcli connection modify` arguments for the values that differ from `original`,
/// or `None` when nothing changed. The profile is named by its original id, so a rename
/// still finds it. Hidden IP text fields are cleared; hidden switches keep their value.
pub fn modify_args(original: &Profile, edit: &Edit) -> Option<Vec<String>> {
    let mut changes: Vec<(String, String)> = Vec::new();
    let device = if original.wireless {
        "802-11-wireless"
    } else {
        "802-3-ethernet"
    };
    if edit.id != original.id {
        push(&mut changes, "connection.id", edit.id.clone());
    }
    if edit.autoconnect != original.autoconnect {
        push(
            &mut changes,
            "connection.autoconnect",
            yes_no(edit.autoconnect),
        );
    }
    if edit.priority != original.priority {
        push(
            &mut changes,
            "connection.autoconnect-priority",
            edit.priority.to_string(),
        );
    }
    if edit.everyone != original.everyone {
        // An empty permissions value means every user may use the profile.
        push(
            &mut changes,
            "connection.permissions",
            if edit.everyone {
                String::new()
            } else {
                format!("user:{}", current_user())
            },
        );
    }
    if edit.metered != original.metered {
        push(&mut changes, "connection.metered", edit.metered.clone());
    }
    let old_mac = join_cloned_mac(&original.cloned_mac, &original.cloned_mac_address);
    let new_mac = join_cloned_mac(&edit.cloned_mac, &edit.cloned_mac_address);
    if old_mac != new_mac {
        push(
            &mut changes,
            format!("{device}.cloned-mac-address"),
            new_mac,
        );
    }
    if original.wireless && edit.mac_randomization != original.mac_randomization {
        push(
            &mut changes,
            "802-11-wireless.mac-address-randomization",
            edit.mac_randomization.clone(),
        );
    }
    if !original.wireless && edit.wake_on_lan != original.wake_on_lan {
        push(
            &mut changes,
            "802-3-ethernet.wake-on-lan",
            edit.wake_on_lan.clone(),
        );
    }
    if edit.mtu != original.mtu {
        push(&mut changes, format!("{device}.mtu"), edit.mtu.clone());
    }
    if shown(6, &edit.ipv6.method).privacy && edit.ip6_privacy != original.ip6_privacy {
        push(
            &mut changes,
            "ipv6.ip6-privacy",
            edit.ip6_privacy.to_string(),
        );
    }
    ip_changes(&mut changes, 4, &original.ipv4, &edit.ipv4);
    ip_changes(&mut changes, 6, &original.ipv6, &edit.ipv6);
    if changes.is_empty() {
        return None;
    }
    let mut args = vec![
        "connection".to_string(),
        "modify".to_string(),
        original.id.clone(),
    ];
    for (key, value) in changes {
        args.push(key);
        args.push(value);
    }
    Some(args)
}

/// The changes for one IP family. A text field the method hides is written empty, once.
fn ip_changes(changes: &mut Vec<(String, String)>, version: u8, original: &Ip, edit: &Ip) {
    let prefix = if version == 4 { "ipv4" } else { "ipv6" };
    if edit.method != original.method {
        push(changes, format!("{prefix}.method"), edit.method.clone());
    }
    let shown = shown(version, &edit.method);
    let texts = [
        (
            shown.addresses,
            "addresses",
            &original.addresses,
            &edit.addresses,
        ),
        (shown.gateway, "gateway", &original.gateway, &edit.gateway),
        (shown.dns, "dns", &original.dns, &edit.dns),
        (
            shown.dns_search,
            "dns-search",
            &original.dns_search,
            &edit.dns_search,
        ),
    ];
    for (visible, suffix, old, new) in texts {
        let value = if visible { new.as_str() } else { "" };
        if value != old.as_str() {
            push(changes, format!("{prefix}.{suffix}"), value.to_string());
        }
    }
    let switches = [
        (
            shown.auto_dns,
            "ignore-auto-dns",
            original.ignore_auto_dns,
            edit.ignore_auto_dns,
        ),
        (
            shown.auto_routes,
            "ignore-auto-routes",
            original.ignore_auto_routes,
            edit.ignore_auto_routes,
        ),
        (
            shown.never_default,
            "never-default",
            original.never_default,
            edit.never_default,
        ),
    ];
    for (visible, suffix, old, new) in switches {
        if visible && old != new {
            push(changes, format!("{prefix}.{suffix}"), yes_no(new));
        }
    }
}

fn push(changes: &mut Vec<(String, String)>, key: impl Into<String>, value: impl Into<String>) {
    changes.push((key.into(), value.into()));
}

/// The arguments that change the Wi-Fi security of a profile. `None` removes the
/// security section; `wpa-psk` and `sae` set the key type and password.
/// Returns `None` when security is unchanged and the password is unchanged.
pub fn security_args(original: &Profile, edit: &Edit) -> Option<Vec<String>> {
    if edit.key_mgmt == original.key_mgmt && edit.psk == original.psk {
        return None;
    }
    let mut args = vec![
        "connection".to_string(),
        "modify".to_string(),
        original.id.clone(),
    ];
    if edit.key_mgmt == "none" {
        args.push("remove".to_string());
        args.push("802-11-wireless-security".to_string());
    } else {
        args.push("802-11-wireless-security.key-mgmt".to_string());
        args.push(edit.key_mgmt.clone());
        args.push("802-11-wireless-security.psk".to_string());
        args.push(edit.psk.clone());
    }
    Some(args)
}

fn yes_no(value: bool) -> String {
    if value { "yes" } else { "no" }.to_string()
}

/// The login name for the permissions value. Reads `USER` and falls back to `root`.
fn current_user() -> String {
    std::env::var("USER").unwrap_or_else(|_| "root".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOW: &str = "connection.id:Home\n\
connection.autoconnect:yes\n\
connection.permissions:\n\
connection.metered:unknown\n\
802-11-wireless.bssid:--\n\
802-11-wireless.cloned-mac-address:--\n\
802-11-wireless.mtu:auto\n\
802-11-wireless-security.key-mgmt:wpa-psk\n\
802-11-wireless-security.psk:secret123\n\
ipv4.method:auto\n\
ipv4.addresses:\n\
ipv4.gateway:\n\
ipv4.dns:\n\
ipv6.method:auto\n\
ipv6.addresses:\n\
ipv6.gateway:\n\
ipv6.dns:\n";

    #[test]
    fn parses_profile_fields() {
        let profile = parse_profile(SHOW);
        assert_eq!(profile.id, "Home");
        assert!(profile.autoconnect);
        assert!(profile.everyone);
        assert_eq!(profile.key_mgmt, "wpa-psk");
        assert_eq!(profile.psk, "secret123");
        assert_eq!(profile.ipv4.method, "auto");
    }

    #[test]
    fn unescapes_colons_in_values() {
        let profile = parse_profile("ipv6.addresses:2001\\:db8\\:\\:20/64\n");
        assert_eq!(profile.ipv6.addresses, "2001:db8::20/64");
    }

    #[test]
    fn no_changes_means_no_command() {
        let profile = parse_profile(SHOW);
        let edit = Edit::from_profile(&profile);
        assert_eq!(modify_args(&profile, &edit), None);
        assert_eq!(security_args(&profile, &edit), None);
    }

    #[test]
    fn writes_only_changed_values_in_one_call() {
        let profile = parse_profile(SHOW);
        let mut edit = Edit::from_profile(&profile);
        edit.ipv4.method = "manual".into();
        edit.ipv4.addresses = "192.168.1.20/24".into();
        edit.ipv4.gateway = "192.168.1.1".into();
        let args = modify_args(&profile, &edit).expect("changes");
        assert_eq!(
            args,
            [
                "connection",
                "modify",
                "Home",
                "ipv4.method",
                "manual",
                "ipv4.addresses",
                "192.168.1.20/24",
                "ipv4.gateway",
                "192.168.1.1",
            ]
        );
    }

    #[test]
    fn rename_targets_the_original_name() {
        let profile = parse_profile(SHOW);
        let mut edit = Edit::from_profile(&profile);
        edit.id = "Office".into();
        let args = modify_args(&profile, &edit).expect("changes");
        assert_eq!(
            &args[..4],
            ["connection", "modify", "Home", "connection.id"]
        );
        assert_eq!(args[4], "Office");
    }

    #[test]
    fn none_security_removes_the_section() {
        let profile = parse_profile(SHOW);
        let mut edit = Edit::from_profile(&profile);
        edit.key_mgmt = "none".into();
        let args = security_args(&profile, &edit).expect("changes");
        assert_eq!(
            args,
            [
                "connection",
                "modify",
                "Home",
                "remove",
                "802-11-wireless-security"
            ]
        );
    }

    #[test]
    fn validates_addresses() {
        assert!(valid_ipv4("192.168.1.1"));
        assert!(!valid_ipv4("192.168.1"));
        assert!(valid_ipv4_cidr("10.0.0.5/8"));
        assert!(!valid_ipv4_cidr("10.0.0.5/33"));
        assert!(valid_ipv6_cidr("2001:db8::1/64"));
        assert!(valid_list("1.1.1.1, 8.8.8.8", valid_ipv4));
        assert!(!valid_list("1.1.1.1, nope", valid_ipv4));
    }

    #[test]
    fn manual_ipv4_needs_an_address() {
        let profile = parse_profile(SHOW);
        let mut edit = Edit::from_profile(&profile);
        edit.ipv4.method = "manual".into();
        assert!(!edit.problems().is_empty());
        edit.ipv4.addresses = "192.168.1.20/24".into();
        assert!(edit.problems().is_empty());
    }
}
