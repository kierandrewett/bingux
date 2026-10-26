//! The data behind a connection's settings pages: every value `nmcli connection show` reports for one profile,
//! the `nmcli connection modify` calls that write only what changed, the choices each setting offers and the
//! checks a typed value must pass. Pure functions, so the argument lists can be unit tested without
//! NetworkManager.

use std::collections::BTreeMap;

use bingux_ui::controls::form::validate::{self, Check};

use crate::pages::network::model::unescape;

/// The `(key, label)` pairs a dropdown shows.
pub type Choices = &'static [(&'static str, &'static str)];

/// Every key the settings pages read, in one `nmcli connection show` call. A key the profile's
/// type does not have (a Wi-Fi key on a wired profile) is left out of the output.
pub const KEYS: &[&str] = &[
    "connection.type",
    "connection.id",
    "connection.uuid",
    "connection.autoconnect",
    "connection.autoconnect-priority",
    "connection.autoconnect-retries",
    "connection.permissions",
    "connection.metered",
    "connection.mdns",
    "connection.llmnr",
    "connection.dns-over-tls",
    "802-11-wireless.ssid",
    "802-11-wireless.bssid",
    "802-11-wireless.hidden",
    "802-11-wireless.band",
    "802-11-wireless.cloned-mac-address",
    "802-11-wireless.mac-address-randomization",
    "802-11-wireless.mtu",
    "802-11-wireless-security.key-mgmt",
    "802-11-wireless-security.psk",
    "802-11-wireless-security.wep-key0",
    "802-11-wireless-security.wep-key-type",
    "802-3-ethernet.cloned-mac-address",
    "802-3-ethernet.mtu",
    "802-3-ethernet.wake-on-lan",
    "802-1x.eap",
    "802-1x.identity",
    "802-1x.anonymous-identity",
    "802-1x.password",
    "802-1x.phase2-auth",
    "802-1x.ca-cert",
    "802-1x.system-ca-certs",
    "802-1x.client-cert",
    "802-1x.private-key",
    "802-1x.private-key-password",
    "ipv4.method",
    "ipv4.addresses",
    "ipv4.gateway",
    "ipv4.routes",
    "ipv4.dns",
    "ipv4.dns-search",
    "ipv4.ignore-auto-dns",
    "ipv4.ignore-auto-routes",
    "ipv4.never-default",
    "ipv4.route-metric",
    "ipv4.may-fail",
    "ipv4.dhcp-hostname",
    "ipv4.dhcp-send-hostname",
    "ipv4.dhcp-client-id",
    "ipv6.method",
    "ipv6.addresses",
    "ipv6.gateway",
    "ipv6.routes",
    "ipv6.dns",
    "ipv6.dns-search",
    "ipv6.ignore-auto-dns",
    "ipv6.ignore-auto-routes",
    "ipv6.never-default",
    "ipv6.route-metric",
    "ipv6.may-fail",
    "ipv6.ip6-privacy",
    "ipv6.addr-gen-mode",
    "proxy.method",
    "proxy.pac-url",
    "proxy.pac-script",
    "proxy.browser-only",
];

/// The kind of profile, from `connection.type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Wifi,
    Wired,
    Vpn,
}

/// The values of one saved connection, as NetworkManager stores them, by key
/// (`ipv4.method`). A value NetworkManager prints as `--` is empty.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProfileData {
    values: BTreeMap<String, String>,
}

impl ProfileData {
    /// Parse `nmcli --show-secrets -t -f KEYS connection show id NAME` output: one `key:value` per line.
    pub fn parse(output: &str) -> Self {
        let mut values: BTreeMap<String, String> = BTreeMap::new();
        let mut last: Option<String> = None;
        for line in output.lines() {
            match split_key_value(line) {
                Some((key, value)) if KEYS.contains(&key) => {
                    let value = if value == "--" { String::new() } else { value };
                    values.insert(key.to_string(), value);
                    last = Some(key.to_string());
                }
                // A value with line breaks (a proxy script) continues on the lines after its key.
                _ => {
                    if let Some(value) = last.as_ref().and_then(|key| values.get_mut(key)) {
                        value.push('\n');
                        value.push_str(line);
                    }
                }
            }
        }
        Self { values }
    }

    /// A profile of `kind` with every value at NetworkManager's default, drawn while the real one loads.
    pub fn placeholder(kind: Kind) -> Self {
        let kind_name = match kind {
            Kind::Wifi => "802-11-wireless",
            Kind::Wired => "802-3-ethernet",
            Kind::Vpn => "vpn",
        };
        let mut text = format!("connection.type:{kind_name}\nipv4.method:auto\nipv6.method:auto\n");
        text.push_str("connection.metered:unknown\nproxy.method:none\n802-3-ethernet.mtu:auto\n");
        Self::parse(&text)
    }

    pub fn get(&self, key: &str) -> &str {
        self.values.get(key).map(String::as_str).unwrap_or("")
    }

    /// Whether a switch key is `yes`.
    pub fn yes(&self, key: &str) -> bool {
        self.get(key) == "yes"
    }

    pub fn int(&self, key: &str) -> Option<i64> {
        self.get(key).trim().parse().ok()
    }

    /// A comma-separated value as its items, trimmed, without empty ones.
    pub fn list(&self, key: &str) -> Vec<String> {
        split_list(self.get(key))
    }

    pub fn kind(&self) -> Kind {
        match self.get("connection.type") {
            "802-11-wireless" => Kind::Wifi,
            "802-3-ethernet" => Kind::Wired,
            _ => Kind::Vpn,
        }
    }

    /// The setting a device-level key lives in: `802-11-wireless` or `802-3-ethernet`.
    pub fn device_setting(&self) -> &'static str {
        match self.kind() {
            Kind::Wifi => "802-11-wireless",
            _ => "802-3-ethernet",
        }
    }

    /// Whether the profile has any value for `setting` (`802-1x`).
    pub fn has_setting(&self, setting: &str) -> bool {
        let prefix = format!("{setting}.");
        self.values
            .iter()
            .any(|(key, value)| key.starts_with(&prefix) && !value.is_empty())
    }

    /// These values with `changes` applied, as they will read once NetworkManager has them.
    pub fn with(&self, changes: &Changes) -> Self {
        let mut next = self.clone();
        for (key, value) in &changes.set {
            next.values.insert(key.clone(), value.clone());
        }
        for setting in &changes.remove {
            let prefix = format!("{setting}.");
            next.values.retain(|key, _| !key.starts_with(&prefix));
        }
        next
    }
}

/// Split a comma-separated value into trimmed, non-empty items.
pub fn split_list(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

/// Join list items the way `nmcli connection modify` takes them.
pub fn join_list(items: &[String]) -> String {
    items
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
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

/// The arguments that read one profile, secrets included.
pub fn read_args(name: &str) -> Vec<String> {
    vec![
        "--show-secrets".into(),
        "-t".into(),
        "-f".into(),
        KEYS.join(","),
        "connection".into(),
        "show".into(),
        "id".into(),
        name.into(),
    ]
}

/// What to write to a profile: values to set, and whole settings to remove
/// (`802-1x`, `802-11-wireless-security`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Changes {
    pub set: Vec<(String, String)>,
    pub remove: Vec<String>,
}

impl Changes {
    pub fn one(key: &str, value: impl Into<String>) -> Self {
        let mut changes = Self::default();
        changes.set(key, value);
        changes
    }

    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();
        match self.set.iter_mut().find(|(other, _)| other == key) {
            Some(entry) => entry.1 = value,
            None => self.set.push((key.to_string(), value)),
        }
    }

    pub fn remove(&mut self, setting: &str) {
        if !self.remove.iter().any(|other| other == setting) {
            self.remove.push(setting.to_string());
        }
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty() && self.remove.is_empty()
    }

    /// The `nmcli` argument lists that write these changes to `name`: one `modify` for all values, then
    /// one per removed setting. Removing comes last so a value that depends on the removed setting
    /// (a key type) is already changed when it goes.
    pub fn commands(&self, name: &str) -> Vec<Vec<String>> {
        let mut commands = Vec::new();
        if !self.set.is_empty() {
            let mut args = vec!["connection".to_string(), "modify".into(), name.into()];
            for (key, value) in &self.set {
                args.push(key.clone());
                args.push(value.clone());
            }
            commands.push(args);
        }
        for setting in &self.remove {
            commands.push(vec![
                "connection".to_string(),
                "modify".into(),
                name.into(),
                "remove".into(),
                setting.clone(),
            ]);
        }
        commands
    }

    /// Whether the running connection has to take these changes with `nmcli device reapply`. The
    /// autoconnect settings, the permissions and the name only matter the next time NetworkManager decides.
    pub fn affects_link(&self) -> bool {
        self.set
            .iter()
            .map(|(key, _)| key.as_str())
            .chain(self.remove.iter().map(String::as_str))
            .any(|key| {
                !matches!(
                    key,
                    "connection.id"
                        | "connection.autoconnect"
                        | "connection.autoconnect-priority"
                        | "connection.autoconnect-retries"
                        | "connection.permissions"
                )
            })
    }
}

/// Set `key` in `changes` when `new` differs from `old`.
pub fn diff(changes: &mut Changes, key: &str, old: &str, new: &str) {
    if old != new {
        changes.set(key, new);
    }
}

/// `connection.permissions` for "available to every user" or only the current user.
pub fn permissions_value(everyone: bool) -> String {
    if everyone {
        String::new()
    } else {
        format!("user:{}", current_user())
    }
}

/// The login name for the permissions value. Reads `USER` and falls back to `root`.
fn current_user() -> String {
    std::env::var("USER").unwrap_or_else(|_| "root".to_string())
}

pub fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

// ---- Choices ----------------------------------------------------------------------------------------

/// Metered choices, in the order the dropdown shows them.
pub const METERED_CHOICES: Choices = &[("unknown", "Automatic"), ("yes", "Yes"), ("no", "No")];

/// The IPv4 method names NetworkManager uses, with the label shown to the user.
pub const IPV4_METHODS: Choices = &[
    ("auto", "Automatic (DHCP)"),
    ("manual", "Manual"),
    ("link-local", "Link-local only"),
    ("shared", "Shared to other computers"),
    ("disabled", "Disabled"),
];

/// The IPv6 method names, with the label shown to the user.
pub const IPV6_METHODS: Choices = &[
    ("auto", "Automatic"),
    ("dhcp", "Automatic, DHCP only"),
    ("manual", "Manual"),
    ("link-local", "Link-local only"),
    ("shared", "Shared to other computers"),
    ("ignore", "Ignore"),
    ("disabled", "Disabled"),
];

/// The methods a VPN offers: it has no link to share or limit to the local network.
pub const VPN_IPV4_METHODS: Choices = &[
    ("auto", "Automatic (DHCP)"),
    ("manual", "Manual"),
    ("disabled", "Disabled"),
];

pub const VPN_IPV6_METHODS: Choices = &[
    ("auto", "Automatic"),
    ("manual", "Manual"),
    ("ignore", "Ignore"),
    ("disabled", "Disabled"),
];

/// The methods offered for IP `version` (4 or 6) on a profile of `kind`.
pub fn ip_methods(version: u8, kind: Kind) -> Choices {
    match (version, kind) {
        (4, Kind::Vpn) => VPN_IPV4_METHODS,
        (4, _) => IPV4_METHODS,
        (_, Kind::Vpn) => VPN_IPV6_METHODS,
        _ => IPV6_METHODS,
    }
}

/// The label of `key` in `choices`, or `key` itself when it is not listed.
pub fn label_of(choices: Choices, key: &str) -> String {
    choices
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, label)| label.to_string())
        .unwrap_or_else(|| key.to_string())
}

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

/// The IP fields that mean something for a method.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Shown {
    pub addresses: bool,
    pub routes: bool,
    pub auto_routes: bool,
    pub never_default: bool,
    pub privacy: bool,
}

/// The fields that apply to an IP method. `version` is 4 or 6.
pub fn shown(version: u8, method: &str) -> Shown {
    match method {
        "manual" => Shown {
            addresses: true,
            routes: true,
            never_default: true,
            ..Shown::default()
        },
        "auto" | "dhcp" => Shown {
            routes: true,
            auto_routes: true,
            never_default: true,
            privacy: version == 6,
            ..Shown::default()
        },
        _ => Shown::default(),
    }
}

/// The IPv6 privacy extension choices. Keys are the `ipv6.ip6-privacy` numbers.
pub const IP6_PRIVACY_CHOICES: Choices = &[
    ("-1", "Default"),
    ("0", "Disabled"),
    ("1", "Prefer public addresses"),
    ("2", "Prefer temporary addresses"),
];

/// How an IPv6 address is made from the hardware address.
pub const ADDR_GEN_CHOICES: Choices = &[
    ("default", "Default"),
    ("stable-privacy", "Stable, private"),
    ("eui64", "From the hardware address"),
];

/// The `connection.mdns` and `connection.llmnr` numbers: -1 default, 0 off, 1 resolve only, 2 on.
pub const MULTICAST_DNS_CHOICES: Choices = &[
    ("-1", "Default"),
    ("0", "Off"),
    ("1", "Resolve only"),
    ("2", "On"),
];

/// `connection.dns-over-tls`: -1 default, 0 off, 1 when the server offers it, 2 always.
pub const DNS_OVER_TLS_CHOICES: Choices = &[
    ("-1", "Default"),
    ("0", "Off"),
    ("1", "When available"),
    ("2", "Always"),
];

/// `connection.autoconnect-retries`: how often NetworkManager tries before it gives up.
pub const RETRY_CHOICES: Choices = &[
    ("-1", "Default"),
    ("1", "Once"),
    ("2", "2 times"),
    ("3", "3 times"),
    ("5", "5 times"),
    ("10", "10 times"),
    ("0", "Forever"),
];

/// The 5 GHz and 2.4 GHz choices for a Wi-Fi profile.
pub const BAND_CHOICES: Choices = &[("", "Automatic"), ("bg", "2.4 GHz"), ("a", "5 GHz")];

/// The Wi-Fi scan and connect MAC randomisation choices, with their labels.
pub const MAC_RANDOMIZATION_CHOICES: Choices = &[
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
pub const CLONED_MAC_CHOICES: Choices = &[
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
        _ => "Uses the adapter's own address.",
    }
}

/// The cloned MAC modes NetworkManager accepts as keywords. Any other value is a literal address.
const CLONED_MAC_MODES: &[&str] = &["preserve", "permanent", "random", "stable", "stable-ssid"];

/// Split the stored `cloned-mac-address` into its mode and, for a literal address, the address.
pub fn split_cloned_mac(raw: &str) -> (String, String) {
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

/// Wake on LAN choices for a wired connection.
pub const WAKE_ON_LAN_CHOICES: Choices = &[
    ("default", "Default"),
    ("none", "Off"),
    ("magic", "Magic packet"),
];

/// What Wake on LAN does for each choice.
pub fn wake_on_lan_caption(mode: &str) -> &'static str {
    match mode {
        "magic" => "Wakes when it receives a magic packet from the network.",
        "none" => "Does not wake from the network.",
        _ => "Follows the system setting.",
    }
}

/// Wi-Fi key types. `none` removes the security setting; `wep` is `key-mgmt none` with a WEP key.
pub const WIFI_SECURITY_CHOICES: Choices = &[
    ("none", "None"),
    ("wpa-psk", "WPA & WPA2 Personal"),
    ("sae", "WPA3 Personal"),
    ("wpa-eap", "WPA & WPA2 Enterprise"),
    ("wep", "WEP"),
];

/// The key type of a Wi-Fi profile as the Security page names it.
pub fn wifi_security_kind(data: &ProfileData) -> &'static str {
    match data.get("802-11-wireless-security.key-mgmt") {
        "" => "none",
        "wpa-psk" => "wpa-psk",
        "sae" => "sae",
        "wpa-eap" | "ieee8021x" => "wpa-eap",
        // `none` with a security setting is static WEP.
        _ => "wep",
    }
}

/// The EAP methods 802.1X offers.
pub const EAP_CHOICES: Choices = &[
    ("peap", "PEAP"),
    ("ttls", "Tunnelled TLS"),
    ("tls", "TLS"),
    ("fast", "FAST"),
];

/// The inner authentication methods an EAP method accepts. Empty for TLS, which has none.
pub fn phase2_choices(eap: &str) -> Choices {
    match eap {
        "peap" => &[("mschapv2", "MSCHAPv2"), ("gtc", "GTC"), ("md5", "MD5")],
        "ttls" => &[
            ("mschapv2", "MSCHAPv2"),
            ("mschap", "MSCHAP"),
            ("pap", "PAP"),
            ("chap", "CHAP"),
            ("gtc", "GTC"),
            ("md5", "MD5"),
        ],
        "fast" => &[("mschapv2", "MSCHAPv2"), ("gtc", "GTC")],
        _ => &[],
    }
}

/// Whether an EAP method logs in with a name and password (all but TLS).
pub fn eap_uses_password(eap: &str) -> bool {
    eap != "tls"
}

/// The proxy choices of a profile.
pub const PROXY_CHOICES: Choices = &[("none", "None"), ("auto", "Automatic")];

/// What the Security row of the settings screen shows for a wired profile.
pub fn dot1x_summary(data: &ProfileData) -> &'static str {
    if data.get("802-1x.eap").is_empty() {
        "802.1X off"
    } else {
        "802.1X on"
    }
}

/// The IPv4 or IPv6 row's value on the settings screen.
pub fn ip_summary(data: &ProfileData, version: u8, kind: Kind) -> String {
    let prefix = if version == 4 { "ipv4" } else { "ipv6" };
    let method = data.get(&format!("{prefix}.method"));
    let label = label_of(
        ip_methods(version, kind),
        if method.is_empty() { "auto" } else { method },
    );
    match (method, data.list(&format!("{prefix}.addresses")).first()) {
        ("manual", Some(first)) => format!("Manual · {first}"),
        _ => label,
    }
}

/// The DNS row's value: `Automatic`, or the servers set by hand.
pub fn dns_summary(data: &ProfileData) -> String {
    let servers = dns_servers(data);
    let automatic = !(data.yes("ipv4.ignore-auto-dns") || data.yes("ipv6.ignore-auto-dns"));
    match (automatic, servers.is_empty()) {
        (true, true) => "Automatic".to_string(),
        (true, false) => format!("Automatic + {}", servers.join(", ")),
        (false, true) => "None".to_string(),
        (false, false) => servers.join(", "),
    }
}

/// The DNS servers set on the profile: the IPv4 ones, then the IPv6 ones.
pub fn dns_servers(data: &ProfileData) -> Vec<String> {
    let mut servers = data.list("ipv4.dns");
    servers.extend(data.list("ipv6.dns"));
    servers
}

/// Split DNS servers by family for `ipv4.dns` and `ipv6.dns`. An item that is neither goes to IPv4,
/// where NetworkManager reports it; the form does not let one through.
pub fn split_dns(servers: &[String]) -> (Vec<String>, Vec<String>) {
    let mut v4 = Vec::new();
    let mut v6 = Vec::new();
    for server in servers
        .iter()
        .map(|server| server.trim())
        .filter(|server| !server.is_empty())
    {
        if server.contains(':') {
            v6.push(server.to_string());
        } else {
            v4.push(server.to_string());
        }
    }
    (v4, v6)
}

// ---- Validation -------------------------------------------------------------------------------------

/// A route as nmcli writes it: `destination/prefix`, then an optional next hop and metric.
pub fn route(text: &str) -> Check {
    let parts: Vec<&str> = text.split_whitespace().collect();
    let message =
        "Write a route as network/prefix, next hop and metric, such as 10.0.0.0/8 192.168.1.1 100.";
    let Some(destination) = parts.first() else {
        return Err(message.to_string());
    };
    let family_v6 = destination.contains(':');
    let destination_ok = if family_v6 {
        validate::ipv6_cidr(destination).is_ok()
    } else {
        validate::ipv4_cidr(destination).is_ok()
    };
    let hop_ok = parts.get(1).is_none_or(|hop| {
        if family_v6 {
            validate::ipv6(hop).is_ok()
        } else {
            validate::ipv4(hop).is_ok()
        }
    });
    let metric_ok = parts
        .get(2)
        .is_none_or(|metric| metric.parse::<u32>().is_ok());
    if destination_ok && hop_ok && metric_ok && parts.len() <= 3 {
        Ok(())
    } else {
        Err(message.to_string())
    }
}

/// An IPv4 route.
pub fn route4(text: &str) -> Check {
    route(text)?;
    if text.contains(':') {
        return Err("Enter an IPv4 route such as 10.0.0.0/8 192.168.1.1.".to_string());
    }
    Ok(())
}

/// An IPv6 route.
pub fn route6(text: &str) -> Check {
    route(text)?;
    if !text.contains(':') {
        return Err("Enter an IPv6 route such as 2001:db8::/32 fe80::1.".to_string());
    }
    Ok(())
}

/// A DNS server: an IPv4 or IPv6 address.
pub fn dns_server(text: &str) -> Check {
    validate::ip(text)
}

/// An MTU in bytes.
pub fn mtu(text: &str) -> Check {
    validate::integer_in(576..=65535)(text)
        .map_err(|_| "Enter a size in bytes from 576 to 65535.".to_string())
}

/// A route metric: -1 for automatic, or a whole number.
pub fn metric(text: &str) -> Check {
    validate::integer_in(-1..=i64::from(u32::MAX))(text)
        .map_err(|_| "Enter -1 for automatic, or a whole number.".to_string())
}

/// A DNS search domain such as `example.com`.
pub fn search_domain(text: &str) -> Check {
    let name = text.strip_prefix('~').unwrap_or(text);
    validate::hostname_or_ip(name).map_err(|_| "Enter a domain such as example.com.".to_string())
}

/// The name a DHCP server sees for this computer.
pub fn dhcp_hostname(text: &str) -> Check {
    if text.is_empty() || text.len() > 253 || text.chars().any(char::is_whitespace) {
        Err("Enter a host name without spaces.".to_string())
    } else {
        Ok(())
    }
}

/// The path of a certificate or key file.
pub fn file_path(text: &str) -> Check {
    let path = text.strip_prefix("file://").unwrap_or(text);
    if path.starts_with('/') && !path.ends_with('/') {
        Ok(())
    } else {
        Err("Enter the full path of a file, such as /etc/ssl/certs/ca.pem.".to_string())
    }
}

/// A static WEP key: 5 or 13 characters, or 10 or 26 hexadecimal digits.
pub fn wep_key(text: &str) -> Check {
    let hex = matches!(text.len(), 10 | 26) && text.bytes().all(|byte| byte.is_ascii_hexdigit());
    let ascii =
        matches!(text.len(), 5 | 13) && text.bytes().all(|byte| (0x20..=0x7e).contains(&byte));
    if hex || ascii {
        Ok(())
    } else {
        Err("A WEP key is 5 or 13 characters, or 10 or 26 hexadecimal digits.".to_string())
    }
}

/// A connection name.
pub fn connection_name(text: &str) -> Check {
    if text.trim().is_empty() {
        Err("Enter a name.".to_string())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOW: &str = "connection.type:802-3-ethernet\n\
connection.id:Home\n\
connection.autoconnect:yes\n\
connection.permissions:\n\
connection.metered:unknown\n\
802-3-ethernet.cloned-mac-address:--\n\
802-3-ethernet.mtu:auto\n\
ipv4.method:manual\n\
ipv4.addresses:192.168.77.5/24, 10.9.0.2/16\n\
ipv4.gateway:192.168.77.1\n\
ipv4.dns:1.1.1.1,9.9.9.9\n\
ipv4.routes:10.50.0.0/16 192.168.77.254 200, 10.60.0.0/16 192.168.77.254\n\
ipv6.method:auto\n\
ipv6.dns:\n";

    #[test]
    fn parses_profile_fields() {
        let data = ProfileData::parse(SHOW);
        assert_eq!(data.get("connection.id"), "Home");
        assert!(data.yes("connection.autoconnect"));
        assert_eq!(data.kind(), Kind::Wired);
        assert_eq!(data.get("802-3-ethernet.cloned-mac-address"), "");
        assert_eq!(
            data.list("ipv4.addresses"),
            ["192.168.77.5/24", "10.9.0.2/16"]
        );
        assert_eq!(data.list("ipv4.dns"), ["1.1.1.1", "9.9.9.9"]);
        assert_eq!(data.list("ipv4.routes").len(), 2);
    }

    #[test]
    fn a_value_with_line_breaks_stays_whole() {
        let data = ProfileData::parse(
            "proxy.method:auto\nproxy.pac-script:function F(u, h) {\n  return \"DIRECT\"; // a: b\n}\nproxy.browser-only:no\n",
        );
        assert_eq!(
            data.get("proxy.pac-script"),
            "function F(u, h) {\n  return \"DIRECT\"; // a: b\n}"
        );
        assert_eq!(data.get("proxy.browser-only"), "no");
        assert_eq!(data.get("proxy.method"), "auto");
    }

    #[test]
    fn unescapes_colons_in_values() {
        let data = ProfileData::parse("ipv6.addresses:2001\\:db8\\:\\:20/64\n");
        assert_eq!(data.get("ipv6.addresses"), "2001:db8::20/64");
    }

    #[test]
    fn commands_write_values_in_one_call_and_remove_after() {
        let mut changes = Changes::default();
        changes.set("ipv4.method", "manual");
        changes.set("ipv4.addresses", "192.168.1.20/24");
        changes.remove("802-1x");
        let commands = changes.commands("Home");
        assert_eq!(
            commands[0],
            [
                "connection",
                "modify",
                "Home",
                "ipv4.method",
                "manual",
                "ipv4.addresses",
                "192.168.1.20/24",
            ]
        );
        assert_eq!(
            commands[1],
            ["connection", "modify", "Home", "remove", "802-1x"]
        );
        assert_eq!(commands.len(), 2);
    }

    #[test]
    fn setting_a_key_twice_keeps_the_last_value() {
        let mut changes = Changes::one("ipv4.dns", "1.1.1.1");
        changes.set("ipv4.dns", "9.9.9.9");
        assert_eq!(
            changes.set,
            [("ipv4.dns".to_string(), "9.9.9.9".to_string())]
        );
    }

    #[test]
    fn no_changes_means_no_command() {
        assert!(Changes::default().commands("Home").is_empty());
        let mut changes = Changes::default();
        diff(&mut changes, "ipv4.method", "auto", "auto");
        assert!(changes.is_empty());
    }

    #[test]
    fn rename_targets_the_original_name() {
        let changes = Changes::one("connection.id", "Office");
        let commands = changes.commands("Home");
        assert_eq!(
            &commands[0][..5],
            ["connection", "modify", "Home", "connection.id", "Office"]
        );
    }

    #[test]
    fn with_applies_sets_and_removals() {
        let data = ProfileData::parse("802-1x.eap:peap\n802-1x.identity:bob\nipv4.method:auto\n");
        let mut changes = Changes::one("ipv4.method", "manual");
        changes.remove("802-1x");
        let next = data.with(&changes);
        assert_eq!(next.get("ipv4.method"), "manual");
        assert!(!next.has_setting("802-1x"));
    }

    #[test]
    fn reapply_skips_autoconnect_and_names() {
        assert!(!Changes::one("connection.autoconnect", "no").affects_link());
        assert!(!Changes::one("connection.id", "x").affects_link());
        assert!(Changes::one("connection.metered", "yes").affects_link());
        assert!(Changes::one("ipv4.dns", "1.1.1.1").affects_link());
    }

    #[test]
    fn permissions_value_is_empty_for_everyone() {
        assert_eq!(permissions_value(true), "");
        assert!(permissions_value(false).starts_with("user:"));
    }

    #[test]
    fn splits_cloned_mac() {
        assert_eq!(split_cloned_mac("random"), ("random".into(), String::new()));
        assert_eq!(
            split_cloned_mac("02:00:00:00:00:01"),
            ("address".into(), "02:00:00:00:00:01".into())
        );
        assert_eq!(
            join_cloned_mac("address", " 02:00:00:00:00:01 "),
            "02:00:00:00:00:01"
        );
        assert_eq!(join_cloned_mac("stable", "ignored"), "stable");
    }

    #[test]
    fn splits_dns_by_family() {
        let servers = ["1.1.1.1".to_string(), "2001:4860:4860::8888".to_string()];
        let (v4, v6) = split_dns(&servers);
        assert_eq!(v4, ["1.1.1.1"]);
        assert_eq!(v6, ["2001:4860:4860::8888"]);
    }

    #[test]
    fn summaries_follow_the_stored_values() {
        let data = ProfileData::parse(SHOW);
        assert_eq!(
            ip_summary(&data, 4, Kind::Wired),
            "Manual · 192.168.77.5/24"
        );
        assert_eq!(ip_summary(&data, 6, Kind::Wired), "Automatic");
        assert_eq!(dns_summary(&data), "Automatic + 1.1.1.1, 9.9.9.9");
        assert_eq!(dot1x_summary(&data), "802.1X off");
    }

    #[test]
    fn wifi_security_kind_reads_key_management() {
        let kind = |text: &str| wifi_security_kind(&ProfileData::parse(text));
        assert_eq!(kind("connection.type:802-11-wireless\n"), "none");
        assert_eq!(
            kind("802-11-wireless-security.key-mgmt:wpa-psk\n"),
            "wpa-psk"
        );
        assert_eq!(kind("802-11-wireless-security.key-mgmt:sae\n"), "sae");
        assert_eq!(
            kind("802-11-wireless-security.key-mgmt:wpa-eap\n"),
            "wpa-eap"
        );
        assert_eq!(kind("802-11-wireless-security.key-mgmt:none\n"), "wep");
    }

    #[test]
    fn validates_routes() {
        assert!(route4("10.0.0.0/8").is_ok());
        assert!(route4("10.0.0.0/8 192.168.1.1 100").is_ok());
        assert!(route4("10.0.0.0 192.168.1.1").is_err());
        assert!(route4("10.0.0.0/8 nope").is_err());
        assert!(route4("10.0.0.0/8 192.168.1.1 100 extra").is_err());
        assert!(route4("2001:db8::/32").is_err());
        assert!(route6("2001:db8::/32 fe80::1").is_ok());
        assert!(route6("10.0.0.0/8").is_err());
    }

    #[test]
    fn validates_numbers_and_paths() {
        assert!(mtu("1500").is_ok());
        assert!(mtu("100").is_err());
        assert!(mtu("big").is_err());
        assert!(metric("-1").is_ok());
        assert!(metric("100").is_ok());
        assert!(metric("x").is_err());
        assert!(file_path("/etc/ssl/ca.pem").is_ok());
        assert!(file_path("file:///etc/ssl/ca.pem").is_ok());
        assert!(file_path("ca.pem").is_err());
        assert!(wep_key("12345").is_ok());
        assert!(wep_key("1234567890").is_ok());
        assert!(wep_key("1234").is_err());
        assert!(dhcp_hostname("laptop").is_ok());
        assert!(dhcp_hostname("my laptop").is_err());
        assert!(search_domain("lan.example").is_ok());
        assert!(search_domain("~corp.example").is_ok());
        assert!(search_domain("bad domain").is_err());
        assert!(dns_server("2606:4700:4700::1111").is_ok());
        assert!(dns_server("dns.example").is_err());
    }

    #[test]
    fn phase2_depends_on_the_eap_method() {
        assert!(phase2_choices("tls").is_empty());
        assert!(
            phase2_choices("peap")
                .iter()
                .any(|(key, _)| *key == "mschapv2")
        );
        assert!(phase2_choices("ttls").iter().any(|(key, _)| *key == "pap"));
        assert!(!eap_uses_password("tls"));
        assert!(eap_uses_password("peap"));
    }

    #[test]
    fn manual_shows_addresses_and_automatic_shows_auto_routes() {
        assert!(shown(4, "manual").addresses);
        assert!(!shown(4, "manual").auto_routes);
        assert!(shown(4, "auto").auto_routes);
        assert!(!shown(4, "auto").privacy);
        assert!(shown(6, "auto").privacy);
        assert_eq!(shown(4, "disabled"), Shown::default());
    }
}
