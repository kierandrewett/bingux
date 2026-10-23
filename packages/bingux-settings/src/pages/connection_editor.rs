//! The data behind a connection editor: what `nmcli connection show` reports for one profile, which values are
//! valid, and the one `nmcli connection modify` call that writes only the values the user changed. Pure
//! functions, so the argument lists can be unit tested without NetworkManager.

use std::net::{Ipv4Addr, Ipv6Addr};

/// The keys read from `nmcli connection show`, in the order they are asked for.
pub const FIELDS: &[&str] = &[
    "connection.id",
    "connection.autoconnect",
    "connection.permissions",
    "connection.metered",
    "802-11-wireless.bssid",
    "802-11-wireless.cloned-mac-address",
    "802-11-wireless.mtu",
    "802-11-wireless-security.key-mgmt",
    "802-11-wireless-security.psk",
    "802-3-ethernet.mtu",
    "802-3-ethernet.cloned-mac-address",
    "ipv4.method",
    "ipv4.addresses",
    "ipv4.gateway",
    "ipv4.dns",
    "ipv6.method",
    "ipv6.addresses",
    "ipv6.gateway",
    "ipv6.dns",
];

/// The values of one saved connection, as NetworkManager stores them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub id: String,
    pub autoconnect: bool,
    /// True when no user restriction is set, so every user may use it.
    pub everyone: bool,
    /// "yes", "no" or "unknown".
    pub metered: String,
    pub bssid: String,
    pub cloned_mac: String,
    pub mtu: String,
    /// "none", "wpa-psk", "sae" or empty for a profile without security.
    pub key_mgmt: String,
    pub psk: String,
    pub ipv4_method: String,
    pub ipv4_addresses: String,
    pub ipv4_gateway: String,
    pub ipv4_dns: String,
    pub ipv6_method: String,
    pub ipv6_addresses: String,
    pub ipv6_gateway: String,
    pub ipv6_dns: String,
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
    let mut profile = Profile::default();
    for line in output.lines() {
        let Some((key, value)) = split_key_value(line) else {
            continue;
        };
        let value = value.as_str();
        match key {
            "connection.id" => profile.id = value.to_string(),
            "connection.autoconnect" => profile.autoconnect = value == "yes",
            "connection.permissions" => profile.everyone = value.is_empty(),
            "connection.metered" => profile.metered = value.to_string(),
            "802-11-wireless.bssid" => profile.bssid = value.to_string(),
            "802-11-wireless.cloned-mac-address" | "802-3-ethernet.cloned-mac-address" => {
                profile.cloned_mac = value.to_string()
            }
            "802-11-wireless.mtu" | "802-3-ethernet.mtu" => profile.mtu = value.to_string(),
            "802-11-wireless-security.key-mgmt" => profile.key_mgmt = value.to_string(),
            "802-11-wireless-security.psk" => profile.psk = value.to_string(),
            "ipv4.method" => profile.ipv4_method = value.to_string(),
            "ipv4.addresses" => profile.ipv4_addresses = value.to_string(),
            "ipv4.gateway" => profile.ipv4_gateway = value.to_string(),
            "ipv4.dns" => profile.ipv4_dns = value.to_string(),
            "ipv6.method" => profile.ipv6_method = value.to_string(),
            "ipv6.addresses" => profile.ipv6_addresses = value.to_string(),
            "ipv6.gateway" => profile.ipv6_gateway = value.to_string(),
            "ipv6.dns" => profile.ipv6_dns = value.to_string(),
            _ => {}
        }
    }
    profile
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

/// The IPv4 method names NetworkManager uses, with the label shown to the user.
pub const IPV4_METHODS: &[(&str, &str)] = &[
    ("auto", "Automatic (DHCP)"),
    ("manual", "Manual"),
    ("link-local", "Link-local only"),
    ("disabled", "Disabled"),
];

/// The IPv6 method names, with the label shown to the user.
pub const IPV6_METHODS: &[(&str, &str)] = &[
    ("auto", "Automatic"),
    ("manual", "Manual"),
    ("link-local", "Link-local only"),
    ("ignore", "Ignore"),
    ("disabled", "Disabled"),
];

/// The security choices. `None` removes the security section from the profile.
pub const SECURITY_CHOICES: &[(&str, &str)] = &[
    ("none", "None"),
    ("wpa-psk", "WPA & WPA2 Personal"),
    ("sae", "WPA3 Personal"),
];

/// Metered choices, in the order the dropdown shows them.
pub const METERED_CHOICES: &[(&str, &str)] =
    &[("unknown", "Automatic"), ("yes", "Yes"), ("no", "No")];

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
    pub everyone: bool,
    pub metered: String,
    pub cloned_mac: String,
    pub mtu: String,
    pub key_mgmt: String,
    pub psk: String,
    pub ipv4_method: String,
    pub ipv4_addresses: String,
    pub ipv4_gateway: String,
    pub ipv4_dns: String,
    pub ipv6_method: String,
    pub ipv6_addresses: String,
    pub ipv6_gateway: String,
    pub ipv6_dns: String,
}

impl Edit {
    /// The editable values of a profile, as the editor starts with them.
    #[cfg(test)]
    pub fn from_profile(profile: &Profile) -> Self {
        Self {
            id: profile.id.clone(),
            autoconnect: profile.autoconnect,
            everyone: profile.everyone,
            metered: profile.metered.clone(),
            cloned_mac: profile.cloned_mac.clone(),
            mtu: profile.mtu.clone(),
            key_mgmt: profile.key_mgmt.clone(),
            psk: profile.psk.clone(),
            ipv4_method: profile.ipv4_method.clone(),
            ipv4_addresses: profile.ipv4_addresses.clone(),
            ipv4_gateway: profile.ipv4_gateway.clone(),
            ipv4_dns: profile.ipv4_dns.clone(),
            ipv6_method: profile.ipv6_method.clone(),
            ipv6_addresses: profile.ipv6_addresses.clone(),
            ipv6_gateway: profile.ipv6_gateway.clone(),
            ipv6_dns: profile.ipv6_dns.clone(),
        }
    }

    /// Every validation message that blocks Apply. Empty when the edit can be written.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.id.trim().is_empty() {
            problems.push("Enter a name for this network.".to_string());
        }
        if !self.mtu.is_empty()
            && self.mtu != "auto"
            && self.mtu.parse::<u32>().map(|v| v < 576).unwrap_or(true)
        {
            problems.push("MTU must be a number of 576 or more, or auto.".to_string());
        }
        if self.ipv4_method == "manual"
            && (!valid_list(&self.ipv4_addresses, valid_ipv4_cidr)
                || self.ipv4_addresses.trim().is_empty())
        {
            problems
                .push("Enter an IPv4 address with a prefix, such as 192.168.1.20/24.".to_string());
        }
        if !self.ipv4_gateway.trim().is_empty() && !valid_ipv4(&self.ipv4_gateway) {
            problems.push("The IPv4 gateway must be an IPv4 address.".to_string());
        }
        if !valid_list(&self.ipv4_dns, valid_ipv4) {
            problems.push("Each IPv4 DNS server must be an IPv4 address.".to_string());
        }
        if self.ipv6_method == "manual"
            && (!valid_list(&self.ipv6_addresses, valid_ipv6_cidr)
                || self.ipv6_addresses.trim().is_empty())
        {
            problems
                .push("Enter an IPv6 address with a prefix, such as 2001:db8::20/64.".to_string());
        }
        if !self.ipv6_gateway.trim().is_empty() && !valid_ipv6_cidr(&self.ipv6_gateway) {
            problems.push("The IPv6 gateway must be an IPv6 address.".to_string());
        }
        if !valid_list(&self.ipv6_dns, |item| {
            valid_ipv4(item) || valid_ipv6_cidr(item)
        }) {
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
/// still finds it. Changes to addresses, DNS, security or the connection id need a reconnect.
pub fn modify_args(original: &Profile, edit: &Edit) -> Option<Vec<String>> {
    let mut changes: Vec<(&str, String)> = Vec::new();
    if edit.id != original.id {
        changes.push(("connection.id", edit.id.clone()));
    }
    if edit.autoconnect != original.autoconnect {
        changes.push(("connection.autoconnect", yes_no(edit.autoconnect)));
    }
    if edit.everyone != original.everyone {
        // An empty permissions value means every user may use the profile.
        changes.push((
            "connection.permissions",
            if edit.everyone {
                String::new()
            } else {
                format!("user:{}", current_user())
            },
        ));
    }
    if edit.metered != original.metered {
        changes.push(("connection.metered", edit.metered.clone()));
    }
    if edit.cloned_mac != original.cloned_mac {
        changes.push((
            "802-11-wireless.cloned-mac-address",
            edit.cloned_mac.clone(),
        ));
    }
    if edit.mtu != original.mtu {
        changes.push(("802-11-wireless.mtu", edit.mtu.clone()));
    }
    if edit.ipv4_method != original.ipv4_method {
        changes.push(("ipv4.method", edit.ipv4_method.clone()));
    }
    if edit.ipv4_addresses != original.ipv4_addresses {
        changes.push(("ipv4.addresses", edit.ipv4_addresses.clone()));
    }
    if edit.ipv4_gateway != original.ipv4_gateway {
        changes.push(("ipv4.gateway", edit.ipv4_gateway.clone()));
    }
    if edit.ipv4_dns != original.ipv4_dns {
        changes.push(("ipv4.dns", edit.ipv4_dns.clone()));
    }
    if edit.ipv6_method != original.ipv6_method {
        changes.push(("ipv6.method", edit.ipv6_method.clone()));
    }
    if edit.ipv6_addresses != original.ipv6_addresses {
        changes.push(("ipv6.addresses", edit.ipv6_addresses.clone()));
    }
    if edit.ipv6_gateway != original.ipv6_gateway {
        changes.push(("ipv6.gateway", edit.ipv6_gateway.clone()));
    }
    if edit.ipv6_dns != original.ipv6_dns {
        changes.push(("ipv6.dns", edit.ipv6_dns.clone()));
    }
    if changes.is_empty() {
        return None;
    }
    let mut args = vec![
        "connection".to_string(),
        "modify".to_string(),
        original.id.clone(),
    ];
    for (key, value) in changes {
        args.push(key.to_string());
        args.push(value);
    }
    Some(args)
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
        assert_eq!(profile.ipv4_method, "auto");
    }

    #[test]
    fn unescapes_colons_in_values() {
        let profile = parse_profile("ipv6.addresses:2001\\:db8\\:\\:20/64\n");
        assert_eq!(profile.ipv6_addresses, "2001:db8::20/64");
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
        edit.ipv4_method = "manual".into();
        edit.ipv4_addresses = "192.168.1.20/24".into();
        edit.ipv4_gateway = "192.168.1.1".into();
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
        edit.ipv4_method = "manual".into();
        assert!(!edit.problems().is_empty());
        edit.ipv4_addresses = "192.168.1.20/24".into();
        assert!(edit.problems().is_empty());
    }
}
