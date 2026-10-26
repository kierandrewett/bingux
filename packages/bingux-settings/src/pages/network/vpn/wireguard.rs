//! WireGuard profiles: reading and writing the interface through `nmcli` and
//! the peers through D-Bus (see [`super::bus`] for why), composing a wg-quick
//! file for a new tunnel, and importing one.

use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::process::{Command, Stdio};

use std::net::Ipv6Addr;
use std::str::FromStr;

use bingux_ui::controls::form::validate;
use serde_json::{Value, json};

use super::{bus, nm};

/// One peer of a WireGuard interface.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Peer {
    pub public_key: String,
    pub preshared_key: String,
    /// `host:port`; empty for a peer that connects to us.
    pub endpoint: String,
    pub allowed_ips: Vec<String>,
    /// Seconds between keepalive packets; 0 for none.
    pub keepalive: u32,
}

/// The interface half of a WireGuard profile. Empty text means "automatic"
/// for the port, MTU and firewall mark.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Interface {
    pub private_key: String,
    pub listen_port: String,
    pub mtu: String,
    pub fwmark: String,
    /// IPv4 and IPv6 addresses of the tunnel, each with its prefix length.
    pub addresses: Vec<String>,
    pub dns: Vec<String>,
}

/// A WireGuard profile as read from NetworkManager.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct WgProfile {
    pub uuid: String,
    /// The network interface the tunnel runs as, such as `wg0`.
    pub device_name: String,
    pub interface: Interface,
    pub peers: Vec<Peer>,
}

/// The items of a list typed with commas, spaces or new lines between them.
pub fn parse_list(text: &str) -> Vec<String> {
    text.split([',', ' ', '\t', '\n', '\r'])
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

/// A list as it is shown in a field: one item per line.
pub fn list_text(items: &[String]) -> String {
    items.join("\n")
}

/// `0` and `0x0` are NetworkManager's "automatic"; the form shows blank.
fn blank_zero(value: &str) -> String {
    match value.trim() {
        "0" | "0x0" => String::new(),
        other => other.to_string(),
    }
}

/// The peers in a profile's `GetSettings` reply, with preshared keys from the
/// matching `GetSecrets` reply (matched by public key).
pub fn peers_from(settings: &Value, secrets: &Value) -> Vec<Peer> {
    let text = |item: &Value, key: &str| {
        item.get(key)
            .and_then(|variant| variant.get("data"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let list = |value: &Value| -> Vec<Value> {
        value
            .pointer("/wireguard/peers/data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let secret_peers = list(secrets);
    list(settings)
        .iter()
        .map(|item| {
            let public_key = text(item, "public-key");
            let preshared_key = secret_peers
                .iter()
                .find(|secret| text(secret, "public-key") == public_key)
                .map(|secret| text(secret, "preshared-key"))
                .unwrap_or_default();
            Peer {
                allowed_ips: item
                    .get("allowed-ips")
                    .and_then(|variant| variant.get("data"))
                    .and_then(Value::as_array)
                    .map(|ips| {
                        ips.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
                keepalive: item
                    .get("persistent-keepalive")
                    .and_then(|variant| variant.get("data"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as u32,
                endpoint: text(item, "endpoint"),
                preshared_key,
                public_key,
            }
        })
        .collect()
}

/// `settings` with the WireGuard peers replaced by `peers`.
pub fn settings_with_peers(settings: &Value, peers: &[Peer]) -> Value {
    let variant = |signature: &str, data: Value| json!({"type": signature, "data": data});
    let items: Vec<Value> = peers
        .iter()
        .map(|peer| {
            let mut item = serde_json::Map::new();
            item.insert("public-key".into(), variant("s", json!(peer.public_key)));
            if !peer.endpoint.is_empty() {
                item.insert("endpoint".into(), variant("s", json!(peer.endpoint)));
            }
            item.insert("allowed-ips".into(), variant("as", json!(peer.allowed_ips)));
            item.insert(
                "persistent-keepalive".into(),
                variant("u", json!(peer.keepalive)),
            );
            if !peer.preshared_key.is_empty() {
                item.insert(
                    "preshared-key".into(),
                    variant("s", json!(peer.preshared_key)),
                );
                // Flag 0: NetworkManager stores the key with the profile.
                item.insert("preshared-key-flags".into(), variant("u", json!(0)));
            }
            Value::Object(item)
        })
        .collect();
    let mut updated = settings.clone();
    if let Some(root) = updated.as_object_mut() {
        let wireguard = root
            .entry("wireguard")
            .or_insert_with(|| Value::Object(Default::default()));
        if let Some(wireguard) = wireguard.as_object_mut() {
            wireguard.insert("peers".into(), variant("aa{sv}", Value::Array(items)));
        }
    }
    updated
}

/// `settings` with the private key from the `GetSecrets` reply `secrets`.
/// NetworkManager drops a secret an update does not carry, so the key is
/// sent back with the peers.
pub fn settings_with_private_key(settings: &Value, secrets: &Value) -> Value {
    let mut updated = settings.clone();
    if let (Some(key), Some(wireguard)) = (
        secrets.pointer("/wireguard/private-key"),
        updated.get_mut("wireguard").and_then(Value::as_object_mut),
    ) {
        wireguard.insert("private-key".into(), key.clone());
    }
    updated
}

/// Read the WireGuard profile named `name`.
pub fn load(name: &str) -> Result<WgProfile, String> {
    let values = nm::show(
        name,
        &[
            "connection.uuid",
            "connection.interface-name",
            "wireguard.private-key",
            "wireguard.listen-port",
            "wireguard.mtu",
            "wireguard.fwmark",
            "ipv4.addresses",
            "ipv6.addresses",
            "ipv4.dns",
            "ipv6.dns",
        ],
    )?;
    let [
        uuid,
        device_name,
        private_key,
        listen_port,
        mtu,
        fwmark,
        addresses4,
        addresses6,
        dns4,
        dns6,
    ] = <[String; 10]>::try_from(values)
        .map_err(|_| "NetworkManager returned an unexpected answer.".to_string())?;
    let path = bus::connection_path(&uuid)?;
    let peers = peers_from(
        &bus::get_settings(&path)?,
        &bus::get_secrets(&path, "wireguard")?,
    );
    let mut addresses = parse_list(&addresses4);
    addresses.extend(parse_list(&addresses6));
    let mut dns = parse_list(&dns4);
    dns.extend(parse_list(&dns6));
    Ok(WgProfile {
        uuid,
        device_name,
        interface: Interface {
            private_key,
            listen_port: blank_zero(&listen_port),
            mtu: blank_zero(&mtu),
            fwmark: blank_zero(&fwmark),
            addresses,
            dns,
        },
        peers,
    })
}

/// `items` split into IPv4 and IPv6 by whether they contain a colon.
fn by_family(items: &[String]) -> (Vec<&str>, Vec<&str>) {
    let (v6, v4): (Vec<&str>, Vec<&str>) = items
        .iter()
        .map(String::as_str)
        .partition(|item| item.contains(':'));
    (v4, v6)
}

/// Write the interface half of the profile named `name`.
pub fn save_interface(name: &str, interface: &Interface) -> Result<(), String> {
    let zero = |text: &str| {
        if text.is_empty() {
            "0".to_string()
        } else {
            text.to_string()
        }
    };
    let (addresses4, addresses6) = by_family(&interface.addresses);
    let (dns4, dns6) = by_family(&interface.dns);
    let mut changes: Vec<(&str, String)> = vec![
        ("wireguard.private-key", interface.private_key.clone()),
        ("wireguard.listen-port", zero(&interface.listen_port)),
        ("wireguard.mtu", zero(&interface.mtu)),
        ("wireguard.fwmark", zero(&interface.fwmark)),
    ];
    for (family, addresses, dns) in [("ipv4", addresses4, dns4), ("ipv6", addresses6, dns6)] {
        let on = !addresses.is_empty();
        changes.push((
            if family == "ipv4" {
                "ipv4.method"
            } else {
                "ipv6.method"
            },
            if on { "manual" } else { "disabled" }.to_string(),
        ));
        changes.push((
            if family == "ipv4" {
                "ipv4.addresses"
            } else {
                "ipv6.addresses"
            },
            addresses.join(", "),
        ));
        changes.push((
            if family == "ipv4" {
                "ipv4.dns"
            } else {
                "ipv6.dns"
            },
            dns.join(","),
        ));
        if !on {
            // NetworkManager refuses a DNS search list on a disabled family.
            changes.push((
                if family == "ipv4" {
                    "ipv4.dns-search"
                } else {
                    "ipv6.dns-search"
                },
                String::new(),
            ));
        }
    }
    let borrowed: Vec<(&str, &str)> = changes
        .iter()
        .map(|(property, value)| (*property, value.as_str()))
        .collect();
    nm::modify(name, &borrowed)
}

/// Replace the peers of the profile with `uuid` by `peers`.
pub fn save_peers(uuid: &str, peers: &[Peer]) -> Result<(), String> {
    let path = bus::connection_path(uuid)?;
    let settings = bus::get_settings(&path)?;
    let secrets = bus::get_secrets(&path, "wireguard")?;
    let updated = settings_with_private_key(&settings_with_peers(&settings, peers), &secrets);
    bus::update(&path, &updated)
}

/// Checks for the fields of a WireGuard form, as `fn(&str) -> Result` for
/// `form_field(..).validate(..)`. A blank optional field is never passed to
/// them by the field, and the form's own check skips blanks too.
pub mod check {
    use super::*;

    pub type Check = Result<(), String>;

    /// One or more addresses with a prefix length, separated by commas, spaces
    /// or new lines.
    pub fn cidr_list(text: &str) -> Check {
        let items = parse_list(text);
        if items.is_empty() {
            return Err(
                "Enter at least one address with its prefix length, such as 10.8.0.2/32."
                    .to_string(),
            );
        }
        for item in items {
            if validate::ipv4_cidr(&item).is_err() && validate::ipv6_cidr(&item).is_err() {
                return Err(format!(
                    "\u{201c}{item}\u{201d} is not an address with a prefix length, such as 10.8.0.2/32."
                ));
            }
        }
        Ok(())
    }

    /// Addresses of DNS servers, separated by commas, spaces or new lines.
    pub fn dns_list(text: &str) -> Check {
        let items = parse_list(text);
        if items.is_empty() {
            return Err("Enter one or more IP addresses, one per line.".to_string());
        }
        for item in items {
            if validate::ip(&item).is_err() {
                return Err(format!("\u{201c}{item}\u{201d} is not an IP address."));
            }
        }
        Ok(())
    }

    /// `host:port`, or `[IPv6 address]:port`.
    pub fn endpoint(text: &str) -> Check {
        let message =
            "Enter the server as host:port, such as vpn.example.com:51820 or [2001:db8::1]:51820.";
        let parsed = match text.strip_prefix('[') {
            Some(rest) => rest
                .split_once("]:")
                .filter(|(host, _)| Ipv6Addr::from_str(host).is_ok()),
            None => text
                .rsplit_once(':')
                .filter(|(host, _)| !host.contains(':') && validate::hostname_or_ip(host).is_ok()),
        };
        match parsed {
            Some((_, port)) if validate::port(port).is_ok() => Ok(()),
            _ => Err(message.to_string()),
        }
    }

    pub fn mtu(text: &str) -> Check {
        validate::integer_in(68..=65535)(text)
            .map_err(|_| "Enter a size from 68 to 65535, or leave it blank.".to_string())
    }

    /// A firewall mark: a number, in decimal or as `0x` and hexadecimal digits.
    pub fn fwmark(text: &str) -> Check {
        let valid = match text.strip_prefix("0x") {
            Some(hex) => u32::from_str_radix(hex, 16).is_ok(),
            None => text.parse::<u32>().is_ok(),
        };
        if valid {
            Ok(())
        } else {
            Err("Enter a number such as 51820 or 0xca6c, or leave it blank.".to_string())
        }
    }

    pub fn keepalive(text: &str) -> Check {
        validate::integer_in(0..=65535)(text).map_err(|_| {
            "Enter a number of seconds from 0 to 65535, or leave it blank.".to_string()
        })
    }

    pub fn key(text: &str) -> Check {
        validate::wireguard_key(text)
    }

    pub fn port(text: &str) -> Check {
        validate::port(text)
    }
}

/// The allowed addresses that send all traffic through a peer.
pub const ALL_TRAFFIC: [&str; 2] = ["0.0.0.0/0", "::/0"];

/// Whether `items` are exactly the routes for all traffic, in either family.
pub fn is_all_traffic(items: &[String]) -> bool {
    !items.is_empty()
        && items
            .iter()
            .all(|item| ALL_TRAFFIC.contains(&item.as_str()))
}

/// Whether the `wg` tool is installed. It generates keys and derives public
/// keys; nothing else here needs it.
pub fn has_wg() -> bool {
    bingux_ui::runtime::which("wg")
}

/// A new private key from `wg genkey`.
pub fn generate_private_key() -> Result<String, String> {
    let output = Command::new("wg")
        .arg("genkey")
        .output()
        .map_err(|error| format!("Keys cannot be generated: {error}."))?;
    if !output.status.success() {
        return Err("wg could not generate a key.".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The public key of `private_key`, from `wg pubkey`; `None` when `wg` is not
/// installed or rejects the key.
pub fn public_key(private_key: &str) -> Option<String> {
    let mut child = Command::new("wg")
        .arg("pubkey")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child
        .stdin
        .take()?
        .write_all(format!("{private_key}\n").as_bytes())
        .ok()?;
    let output = child.wait_with_output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|key| !key.is_empty())
}

/// What a new tunnel is made from.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct NewTunnel {
    pub interface: Interface,
    pub peer: Peer,
}

/// The wg-quick file for a new tunnel, with only the keys NetworkManager's
/// importer reads.
pub fn compose(tunnel: &NewTunnel) -> String {
    let interface = &tunnel.interface;
    let mut text = String::from("[Interface]\n");
    text.push_str(&format!("PrivateKey = {}\n", interface.private_key));
    if !interface.addresses.is_empty() {
        text.push_str(&format!("Address = {}\n", interface.addresses.join(", ")));
    }
    if !interface.dns.is_empty() {
        text.push_str(&format!("DNS = {}\n", interface.dns.join(", ")));
    }
    if !interface.listen_port.is_empty() {
        text.push_str(&format!("ListenPort = {}\n", interface.listen_port));
    }
    if !interface.mtu.is_empty() {
        text.push_str(&format!("MTU = {}\n", interface.mtu));
    }
    if !interface.fwmark.is_empty() {
        text.push_str(&format!("FwMark = {}\n", interface.fwmark));
    }
    let peer = &tunnel.peer;
    text.push_str("\n[Peer]\n");
    text.push_str(&format!("PublicKey = {}\n", peer.public_key));
    if !peer.preshared_key.is_empty() {
        text.push_str(&format!("PresharedKey = {}\n", peer.preshared_key));
    }
    if !peer.endpoint.is_empty() {
        text.push_str(&format!("Endpoint = {}\n", peer.endpoint));
    }
    text.push_str(&format!("AllowedIPs = {}\n", peer.allowed_ips.join(", ")));
    if peer.keepalive > 0 {
        text.push_str(&format!("PersistentKeepalive = {}\n", peer.keepalive));
    }
    text
}

/// The longest name Linux allows for a network interface.
const INTERFACE_NAME_MAX: usize = 15;

/// An interface name for a tunnel called `display`: letters, digits and a few
/// punctuation marks, at most 15 of them, never empty.
pub fn interface_name(display: &str) -> String {
    let name: String = display
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '=' | '+' | '.') {
                c
            } else {
                '-'
            }
        })
        .take(INTERFACE_NAME_MAX)
        .collect();
    let name = name.trim_matches('-').to_string();
    if name.is_empty() || name == "." || name == ".." {
        "wg-vpn".to_string()
    } else {
        name
    }
}

/// The first of `base`, `base 2`, `base 3`... that is not in `taken`.
pub fn unique_name(base: &str, taken: &[String]) -> String {
    if !taken.iter().any(|name| name == base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or_else(|| base.to_string())
}

/// Import wg-quick `text` as a profile called `display`, which is made unique
/// among the existing profiles. The file lives for the one call, in a private
/// directory, and is removed after. Returns the profile's name.
pub fn import_text(text: &str, display: &str) -> Result<String, String> {
    let existing = nm::connection_names()?;
    let name = unique_name(display, &existing);
    let directory = std::env::temp_dir().join(format!(
        "bingux-settings-vpn-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .map_err(|error| format!("A private folder for the import could not be made: {error}."))?;
    let result = (|| {
        let file = directory.join(format!("{}.conf", interface_name(&name)));
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&file)
            .and_then(|mut handle| handle.write_all(text.as_bytes()))
            .map_err(|error| format!("The import file could not be written: {error}."))?;
        let file = file.to_string_lossy().into_owned();
        let added = nm::run(&["connection", "import", "type", "wireguard", "file", &file])?;
        let (_, uuid) = nm::added(&added)
            .ok_or_else(|| "NetworkManager did not say which profile it made.".to_string())?;
        // A new VPN does not connect by itself until the person says so.
        nm::run(&[
            "connection",
            "modify",
            "uuid",
            &uuid,
            "connection.id",
            &name,
            "connection.autoconnect",
            "no",
        ])?;
        Ok(name.clone())
    })();
    let _ = std::fs::remove_dir_all(&directory);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tunnel() -> NewTunnel {
        NewTunnel {
            interface: Interface {
                private_key: "YFAnE0psgIdiAF7XR4abxiwVRnlMfeltxu2s8/4Q43k=".into(),
                addresses: vec!["10.77.0.2/32".into(), "fd77::2/128".into()],
                dns: vec!["10.77.0.1".into()],
                ..Interface::default()
            },
            peer: Peer {
                public_key: "HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=".into(),
                endpoint: "127.0.0.1:51820".into(),
                allowed_ips: vec!["10.77.0.0/24".into()],
                keepalive: 25,
                ..Peer::default()
            },
        }
    }

    #[test]
    fn composes_a_wg_quick_file() {
        let text = compose(&tunnel());
        assert_eq!(
            text,
            "[Interface]\n\
             PrivateKey = YFAnE0psgIdiAF7XR4abxiwVRnlMfeltxu2s8/4Q43k=\n\
             Address = 10.77.0.2/32, fd77::2/128\n\
             DNS = 10.77.0.1\n\
             \n\
             [Peer]\n\
             PublicKey = HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=\n\
             Endpoint = 127.0.0.1:51820\n\
             AllowedIPs = 10.77.0.0/24\n\
             PersistentKeepalive = 25\n"
        );
    }

    #[test]
    fn optional_keys_appear_only_when_set() {
        let mut full = tunnel();
        full.interface.listen_port = "51999".into();
        full.interface.mtu = "1380".into();
        full.interface.fwmark = "0x4d2".into();
        full.peer.preshared_key = "FpCyhws9cxwWoV4xELtfJvjJN+zQVRE+GYI0f/CXe4I=".into();
        let text = compose(&full);
        assert!(text.contains("ListenPort = 51999\n"));
        assert!(text.contains("MTU = 1380\n"));
        assert!(text.contains("FwMark = 0x4d2\n"));
        assert!(text.contains("PresharedKey = FpCyhws9cxwWoV4xELtfJvjJN+zQVRE+GYI0f/CXe4I=\n"));
        let mut bare = tunnel();
        bare.interface.dns.clear();
        bare.peer.keepalive = 0;
        bare.peer.endpoint.clear();
        let text = compose(&bare);
        assert!(!text.contains("DNS"));
        assert!(!text.contains("PersistentKeepalive"));
        assert!(!text.contains("Endpoint"));
    }

    #[test]
    fn interface_names_fit_linux() {
        assert_eq!(interface_name("Work VPN"), "Work-VPN");
        assert_eq!(interface_name("a very long tunnel name indeed").len(), 15);
        assert_eq!(interface_name("  "), "wg-vpn");
        assert_eq!(interface_name("Büro"), "B-ro");
        assert_eq!(interface_name(".."), "wg-vpn");
    }

    #[test]
    fn names_are_made_unique() {
        let taken = vec!["Work".to_string(), "Work 2".to_string()];
        assert_eq!(unique_name("Home", &taken), "Home");
        assert_eq!(unique_name("Work", &taken), "Work 3");
    }

    #[test]
    fn lists_split_on_commas_spaces_and_lines() {
        assert_eq!(
            parse_list("10.0.0.2/32, fd00::2/128\n10.0.0.3/32"),
            ["10.0.0.2/32", "fd00::2/128", "10.0.0.3/32"]
        );
        assert!(parse_list("  \n ").is_empty());
        assert_eq!(blank_zero("0x0"), "");
        assert_eq!(blank_zero("0x4d2"), "0x4d2");
        assert_eq!(blank_zero("51820"), "51820");
    }

    fn reply(peers: Value) -> Value {
        json!({"wireguard": {"peers": {"type": "aa{sv}", "data": peers}}})
    }

    #[test]
    fn reads_peers_and_joins_their_preshared_keys() {
        let settings = reply(json!([{
            "public-key": {"type": "s", "data": "PUB="},
            "endpoint": {"type": "s", "data": "127.0.0.1:51820"},
            "allowed-ips": {"type": "as", "data": ["10.77.0.0/24"]},
            "persistent-keepalive": {"type": "u", "data": 25}
        }, {
            "public-key": {"type": "s", "data": "OTHER="},
            "allowed-ips": {"type": "as", "data": []}
        }]));
        let secrets = reply(json!([{
            "public-key": {"type": "s", "data": "PUB="},
            "preshared-key": {"type": "s", "data": "PSK="}
        }]));
        let peers = peers_from(&settings, &secrets);
        assert_eq!(peers.len(), 2);
        assert_eq!(peers[0].preshared_key, "PSK=");
        assert_eq!(peers[0].keepalive, 25);
        assert_eq!(peers[0].endpoint, "127.0.0.1:51820");
        assert_eq!(peers[1].preshared_key, "");
        assert_eq!(peers[1].endpoint, "");
        assert!(peers_from(&json!({}), &json!({})).is_empty());
    }

    #[test]
    fn writes_peers_back_into_the_settings_and_keeps_the_rest() {
        let settings = json!({
            "connection": {"id": {"type": "s", "data": "x"}},
            "wireguard": {"peers": {"type": "aa{sv}", "data": []}}
        });
        let peer = Peer {
            public_key: "PUB=".into(),
            preshared_key: "PSK=".into(),
            endpoint: "1.2.3.4:51820".into(),
            allowed_ips: vec!["0.0.0.0/0".into()],
            keepalive: 15,
        };
        let updated = settings_with_peers(&settings, &[peer.clone()]);
        assert_eq!(updated["connection"], settings["connection"]);
        let back = peers_from(&updated, &updated);
        assert_eq!(back, vec![peer]);
        let cleared = settings_with_peers(&updated, &[]);
        assert!(peers_from(&cleared, &cleared).is_empty());
        let from_nothing = settings_with_peers(&json!({}), &[]);
        assert_eq!(from_nothing["wireguard"]["peers"]["type"], "aa{sv}");
    }

    #[test]
    fn the_private_key_travels_with_the_peers() {
        let settings = json!({"wireguard": {"peers": {"type": "aa{sv}", "data": []}}});
        let secrets = json!({"wireguard": {"private-key": {"type": "s", "data": "KEY="}}});
        let updated = settings_with_private_key(&settings, &secrets);
        assert_eq!(updated["wireguard"]["private-key"]["data"], "KEY=");
        assert_eq!(
            updated["wireguard"]["peers"],
            settings["wireguard"]["peers"]
        );
        // Nothing to carry: the settings come back as they were.
        assert_eq!(settings_with_private_key(&settings, &json!({})), settings);
    }

    #[test]
    fn splits_addresses_by_family() {
        let items: Vec<String> = ["10.0.0.2/32", "fd00::2/128", "10.0.0.3/32"]
            .map(String::from)
            .to_vec();
        let (v4, v6) = by_family(&items);
        assert_eq!(v4, ["10.0.0.2/32", "10.0.0.3/32"]);
        assert_eq!(v6, ["fd00::2/128"]);
    }

    #[test]
    fn checks_accept_and_refuse_the_right_things() {
        assert!(check::cidr_list("10.8.0.2/32, fd08::2/128\n10.8.0.3/32").is_ok());
        assert!(check::cidr_list("").is_err());
        assert!(check::cidr_list("10.8.0.2").is_err());
        assert!(check::cidr_list("10.8.0.2/33").is_err());
        assert!(check::dns_list("1.1.1.1, 2606:4700::1111").is_ok());
        assert!(check::dns_list("one.one.one.one").is_err());
        assert!(check::dns_list("10.8.0.1\nfd08::1").is_ok());
        assert!(check::endpoint("vpn.example.com:51820").is_ok());
        assert!(check::endpoint("127.0.0.1:51820").is_ok());
        assert!(check::endpoint("[2001:db8::1]:51820").is_ok());
        assert!(check::endpoint("vpn.example.com").is_err());
        assert!(check::endpoint("2001:db8::1:51820").is_err());
        assert!(check::endpoint("[2001:db8::1]").is_err());
        assert!(check::endpoint("vpn.example.com:0").is_err());
        assert!(check::mtu("1420").is_ok());
        assert!(check::mtu("12").is_err());
        assert!(check::fwmark("0x4d2").is_ok());
        assert!(check::fwmark("51820").is_ok());
        assert!(check::fwmark("0xzz").is_err());
        assert!(check::fwmark("-1").is_err());
        assert!(check::keepalive("25").is_ok());
        assert!(check::keepalive("70000").is_err());
        assert!(check::key("HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=").is_ok());
        assert!(check::key("too short").is_err());
    }

    #[test]
    fn all_traffic_is_exactly_the_default_routes() {
        let list = |items: &[&str]| {
            items
                .iter()
                .map(|item| item.to_string())
                .collect::<Vec<_>>()
        };
        assert!(is_all_traffic(&list(&["0.0.0.0/0"])));
        assert!(is_all_traffic(&list(&["0.0.0.0/0", "::/0"])));
        assert!(!is_all_traffic(&list(&["0.0.0.0/0", "10.0.0.0/8"])));
        assert!(!is_all_traffic(&[]));
    }
}
