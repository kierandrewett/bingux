//! The VPN plugins this computer has, and the friendly fields of the ones
//! this app knows.
//!
//! NetworkManager finds a plugin by its `.name` file in
//! `/usr/lib/NetworkManager/VPN` (the package) or `/etc/NetworkManager/VPN`
//! (the administrator). Nothing here is a fixed list: a plugin is offered
//! when its file is there and the program it names is installed.

use std::path::{Path, PathBuf};

use bingux_ui::controls::form::validate;
use bingux_ui::prelude::{i18n_id, tr};

/// The directories NetworkManager reads plugin `.name` files from.
pub const NAME_DIRS: [&str; 2] = ["/usr/lib/NetworkManager/VPN", "/etc/NetworkManager/VPN"];

/// The prefix of every plugin's D-Bus service type.
const SERVICE_PREFIX: &str = "org.freedesktop.NetworkManager.";

/// One installed VPN plugin.
#[derive(Clone, Debug, PartialEq)]
pub struct Plugin {
    /// The name `nmcli` takes as `vpn-type`, such as `openvpn`.
    pub short: String,
    /// The service type stored in a profile (`vpn.service-type`).
    pub service: String,
    /// The name a person reads, such as `OpenVPN`.
    pub name: String,
    /// The program that runs the tunnel.
    pub program: Option<String>,
    /// Whether two profiles of this plugin may be up at once.
    pub supports_multiple: bool,
}

/// What a plugin's `.name` file says, or `None` when it names no plugin.
pub fn parse_name_file(text: &str) -> Option<Plugin> {
    let mut section = "";
    let mut short = None;
    let mut service = None;
    let mut program = None;
    let mut multiple = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            section = name;
            continue;
        }
        if section != "VPN Connection" {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().to_string();
        match key.trim() {
            "name" => short = Some(value),
            "service" => service = Some(value),
            "program" => program = Some(value),
            "supports-multiple-connections" => multiple = value == "true",
            _ => {}
        }
    }
    let short = short.filter(|name| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    })?;
    let service = service.filter(|service| !service.is_empty())?;
    Some(Plugin {
        name: display_name(&short),
        short,
        service,
        program: program.filter(|program| !program.is_empty()),
        supports_multiple: multiple,
    })
}

/// The name a person reads for a plugin's short name. Plugins this app has
/// no name for show their short name with a capital.
pub fn display_name(short: &str) -> String {
    match short {
        "openvpn" => "OpenVPN",
        "openconnect" => "OpenConnect",
        "vpnc" => "VPNC",
        "ssh" => "SSH",
        "pptp" => "PPTP",
        "l2tp" => "L2TP",
        "sstp" => "SSTP",
        "strongswan" => "strongSwan",
        "libreswan" => "Libreswan",
        "openswan" => "Openswan",
        "fortisslvpn" => "Fortinet SSL",
        "iodine" => "Iodine",
        "wireguard" => "WireGuard",
        other => {
            let mut letters = other.chars();
            return match letters.next() {
                Some(first) => first.to_uppercase().chain(letters).collect(),
                None => String::new(),
            };
        }
    }
    .to_string()
}

/// The plugins whose `.name` file is in `dirs` and whose program `installed`
/// reports present. A file in a later directory replaces one of the same
/// plugin in an earlier one, as the administrator's directory overrides the
/// package's. Sorted by name.
pub fn discover_in(dirs: &[&Path], installed: impl Fn(&Path) -> bool) -> Vec<Plugin> {
    let mut found: Vec<Plugin> = Vec::new();
    for dir in dirs {
        let Ok(entries) = crate::system::read_dir(dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "name"))
            .collect();
        files.sort();
        for path in files {
            let Some(plugin) = crate::system::read_to_string(&path)
                .ok()
                .and_then(|text| parse_name_file(&text))
            else {
                continue;
            };
            if plugin
                .program
                .as_deref()
                .is_some_and(|program| !installed(Path::new(program)))
            {
                continue;
            }
            found.retain(|other| other.short != plugin.short);
            found.push(plugin);
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// The plugins installed on this computer.
pub fn discover() -> Vec<Plugin> {
    let dirs: Vec<&Path> = NAME_DIRS.iter().map(Path::new).collect();
    discover_in(&dirs, Path::exists)
}

/// A plugin's short name from its service type, or the whole service type
/// when it is not one of NetworkManager's.
pub fn short_of_service(service: &str) -> &str {
    service.strip_prefix(SERVICE_PREFIX).unwrap_or(service)
}

/// The file kinds `nmcli connection import` reads for a plugin, as the
/// extensions to look for. A plugin not listed reads its own format from any file.
pub fn import_extensions(short: &str) -> &'static [&'static str] {
    match short {
        "wireguard" => &["conf"],
        "openvpn" => &["ovpn", "conf"],
        "vpnc" => &["pcf", "conf"],
        "ssh" => &["sh"],
        _ => &[],
    }
}

/// How a field's value is chosen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Text,
    /// An absolute file path.
    Path,
    /// A secret kept in `vpn.secrets` under the field's key, with the
    /// `vpn.data` key that holds how NetworkManager treats it.
    Secret {
        flags: &'static str,
        /// The choice label that remembers this secret.
        remember: &'static str,
    },
    /// One of `(value, label)` pairs; the first is the default.
    Choice(&'static [(&'static str, &'static str)]),
}

/// Where a field is stored in the profile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Store {
    /// `vpn.data` (or `vpn.secrets` for a secret), under this key.
    Data(&'static str),
    /// `vpn.user-name`.
    UserName,
}

/// One friendly field of a plugin.
#[derive(Clone, Copy)]
pub struct Field {
    pub label: &'static str,
    pub store: Store,
    pub kind: Kind,
    pub hint: &'static str,
    pub placeholder: &'static str,
    /// Needed to create the profile.
    pub required: bool,
    /// Shown only while the `vpn.data` key has one of these values.
    pub when: Option<(&'static str, &'static [&'static str])>,
    pub check: Option<fn(&str) -> Result<(), String>>,
}

impl Field {
    const fn new(label: &'static str, store: Store, kind: Kind) -> Self {
        Self {
            label,
            store,
            kind,
            hint: "",
            placeholder: "",
            required: false,
            when: None,
            check: None,
        }
    }

    const fn hint(mut self, hint: &'static str) -> Self {
        self.hint = hint;
        self
    }

    const fn placeholder(mut self, text: &'static str) -> Self {
        self.placeholder = text;
        self
    }

    const fn required(mut self) -> Self {
        self.required = true;
        self
    }

    const fn when(mut self, key: &'static str, values: &'static [&'static str]) -> Self {
        self.when = Some((key, values));
        self
    }

    const fn check(mut self, check: fn(&str) -> Result<(), String>) -> Self {
        self.check = Some(check);
        self
    }

    /// The key this field's value is stored under, when it is stored by key.
    pub fn key(&self) -> Option<&'static str> {
        match self.store {
            Store::Data(key) => Some(key),
            Store::UserName => None,
        }
    }
}

/// A server address, with an optional port: `vpn.example.com`, `10.0.0.1:1194`.
fn host_with_port(text: &str) -> Result<(), String> {
    let message = tr!("network-vpn-error-host-port");
    let (host, port) = match text.rsplit_once(':') {
        // `fe80::1` has colons but is no host:port; only one colon splits.
        Some((host, port)) if !host.contains(':') => (host, Some(port)),
        _ => (text, None),
    };
    if validate::hostname_or_ip(host).is_err() {
        return Err(message.to_string());
    }
    match port {
        Some(port) if validate::port(port).is_err() => Err(message.to_string()),
        _ => Ok(()),
    }
}

/// A server host name or address, without a port.
fn host_only(text: &str) -> Result<(), String> {
    validate::hostname_or_ip(text)
}

/// An absolute path to a file.
fn absolute_path(text: &str) -> Result<(), String> {
    if text.starts_with('/') {
        Ok(())
    } else {
        Err(tr!("network-vpn-error-absolute-path"))
    }
}

/// A port number.
fn port_number(text: &str) -> Result<(), String> {
    validate::port(text)
}

/// How NetworkManager treats a stored secret, as the `*-flags` values.
pub const SECRET_FLAGS: &[(&str, &str)] = &[
    ("0", i18n_id!("network-vpn-flags-all-users")),
    ("1", i18n_id!("network-vpn-flags-this-user")),
    ("2", i18n_id!("network-vpn-flags-ask")),
];

const OPENVPN_TYPE: &[(&str, &str)] = &[
    ("password", i18n_id!("network-vpn-field-password")),
    ("tls", i18n_id!("network-vpn-choice-certificates")),
    (
        "password-tls",
        i18n_id!("network-vpn-choice-password-certificates"),
    ),
    ("static-key", i18n_id!("network-vpn-choice-static-key")),
];

const OPENCONNECT_PROTOCOL: &[(&str, &str)] = &[
    ("anyconnect", "Cisco AnyConnect"),
    ("nc", "Juniper Network Connect"),
    ("gp", "Palo Alto GlobalProtect"),
    ("pulse", "Pulse Connect Secure"),
    ("f5", "F5 BIG-IP"),
    ("fortinet", "Fortinet"),
    ("array", "Array Networks"),
];

const SSH_AUTH: &[(&str, &str)] = &[
    ("password", i18n_id!("network-vpn-field-password")),
    ("key", i18n_id!("network-vpn-field-key-file")),
    ("ssh-agent", i18n_id!("network-vpn-choice-ssh-agent")),
];

const OPENVPN: &[Field] = &[
    Field::new(
        i18n_id!("network-vpn-field-server"),
        Store::Data("remote"),
        Kind::Text,
    )
    .hint(i18n_id!("network-vpn-hint-server-port"))
    .placeholder("vpn.example.com:1194")
    .required()
    .check(host_with_port),
    Field::new(
        i18n_id!("network-vpn-field-sign-in-with"),
        Store::Data("connection-type"),
        Kind::Choice(OPENVPN_TYPE),
    ),
    Field::new(
        i18n_id!("network-vpn-field-user-name"),
        Store::Data("username"),
        Kind::Text,
    )
    .when("connection-type", &["password", "password-tls"]),
    Field::new(
        i18n_id!("network-vpn-field-password"),
        Store::Data("password"),
        Kind::Secret {
            flags: "password-flags",
            remember: i18n_id!("network-vpn-remember-password"),
        },
    )
    .when("connection-type", &["password", "password-tls"]),
    Field::new(
        i18n_id!("network-vpn-field-ca"),
        Store::Data("ca"),
        Kind::Path,
    )
    .when("connection-type", &["password", "tls", "password-tls"])
    .check(absolute_path),
    Field::new(
        i18n_id!("network-vpn-field-certificate"),
        Store::Data("cert"),
        Kind::Path,
    )
    .when("connection-type", &["tls", "password-tls"])
    .check(absolute_path),
    Field::new(
        i18n_id!("network-vpn-field-your-private-key"),
        Store::Data("key"),
        Kind::Path,
    )
    .when("connection-type", &["tls", "password-tls"])
    .check(absolute_path),
    Field::new(
        i18n_id!("network-vpn-field-key-password"),
        Store::Data("cert-pass"),
        Kind::Secret {
            flags: "cert-pass-flags",
            remember: i18n_id!("network-vpn-remember-key-password"),
        },
    )
    .when("connection-type", &["tls", "password-tls"]),
    Field::new(
        i18n_id!("network-vpn-field-static-key-file"),
        Store::Data("static-key"),
        Kind::Path,
    )
    .when("connection-type", &["static-key"])
    .check(absolute_path),
];

const OPENCONNECT: &[Field] = &[
    Field::new(
        i18n_id!("network-vpn-field-gateway"),
        Store::Data("gateway"),
        Kind::Text,
    )
    .hint(i18n_id!("network-vpn-hint-gateway"))
    .placeholder("vpn.example.com")
    .required()
    .check(host_with_port),
    Field::new(
        i18n_id!("network-vpn-field-protocol"),
        Store::Data("protocol"),
        Kind::Choice(OPENCONNECT_PROTOCOL),
    ),
    Field::new(
        i18n_id!("network-vpn-field-user-name"),
        Store::UserName,
        Kind::Text,
    ),
    Field::new(
        i18n_id!("network-vpn-field-ca"),
        Store::Data("cacert"),
        Kind::Path,
    )
    .check(absolute_path),
    Field::new(
        i18n_id!("network-vpn-field-certificate"),
        Store::Data("usercert"),
        Kind::Path,
    )
    .check(absolute_path),
    Field::new(
        i18n_id!("network-vpn-field-your-private-key"),
        Store::Data("userkey"),
        Kind::Path,
    )
    .check(absolute_path),
];

const VPNC: &[Field] = &[
    Field::new(
        i18n_id!("network-vpn-field-gateway"),
        Store::Data("IPSec gateway"),
        Kind::Text,
    )
    .placeholder("vpn.example.com")
    .required()
    .check(host_only),
    Field::new(
        i18n_id!("network-vpn-field-group-name"),
        Store::Data("IPSec ID"),
        Kind::Text,
    )
    .required(),
    Field::new(
        i18n_id!("network-vpn-field-group-password"),
        Store::Data("IPSec secret"),
        Kind::Secret {
            flags: "IPSec secret-flags",
            remember: i18n_id!("network-vpn-remember-group-password"),
        },
    ),
    Field::new(
        i18n_id!("network-vpn-field-user-name"),
        Store::Data("Xauth username"),
        Kind::Text,
    ),
    Field::new(
        i18n_id!("network-vpn-field-password"),
        Store::Data("Xauth password"),
        Kind::Secret {
            flags: "Xauth password-flags",
            remember: i18n_id!("network-vpn-remember-xauth-password"),
        },
    ),
    Field::new(
        i18n_id!("network-vpn-field-domain"),
        Store::Data("Domain"),
        Kind::Text,
    ),
];

const SSH: &[Field] = &[
    Field::new(
        i18n_id!("network-vpn-field-server"),
        Store::Data("remote"),
        Kind::Text,
    )
    .hint(i18n_id!("network-vpn-hint-ssh-server"))
    .placeholder("ssh.example.com")
    .required()
    .check(host_only),
    Field::new(
        i18n_id!("network-vpn-field-port"),
        Store::Data("port"),
        Kind::Text,
    )
    .placeholder("22")
    .check(port_number),
    Field::new(
        i18n_id!("network-vpn-field-user-name"),
        Store::Data("remote-username"),
        Kind::Text,
    ),
    Field::new(
        i18n_id!("network-vpn-field-sign-in-with"),
        Store::Data("auth-type"),
        Kind::Choice(SSH_AUTH),
    ),
    Field::new(
        i18n_id!("network-vpn-field-key-file"),
        Store::Data("key-file"),
        Kind::Path,
    )
    .when("auth-type", &["key"])
    .check(absolute_path),
    Field::new(
        i18n_id!("network-vpn-field-password"),
        Store::Data("password"),
        Kind::Secret {
            flags: "password-flags",
            remember: i18n_id!("network-vpn-remember-ssh-password"),
        },
    )
    .when("auth-type", &["password"]),
];

/// What a plugin this app has no table for gets: a user name, kept where
/// NetworkManager has a place for it. Everything else of such a profile is in
/// its key and value list.
const GENERIC: &[Field] = &[Field::new("User name", Store::UserName, Kind::Text)];

/// The friendly fields of a plugin, in the order they are shown.
pub fn fields(short: &str) -> &'static [Field] {
    match short {
        "openvpn" => OPENVPN,
        "openconnect" => OPENCONNECT,
        "vpnc" => VPNC,
        "ssh" => SSH,
        _ => GENERIC,
    }
}

/// Whether this app has a table of friendly fields for `short`.
pub fn is_known(short: &str) -> bool {
    matches!(short, "openvpn" | "openconnect" | "vpnc" | "ssh")
}

/// Whether `field` shows, given the current value of `key` in the profile.
pub fn visible(field: &Field, value_of: impl Fn(&str) -> String) -> bool {
    match field.when {
        None => true,
        Some((key, values)) => {
            let value = value_of(key);
            values.contains(&value.as_str())
        }
    }
}

/// The value a choice field shows while its key is not set: the first option.
pub fn choice_default(field: &Field) -> &'static str {
    match field.kind {
        Kind::Choice(options) => options.first().map_or("", |(value, _)| value),
        _ => "",
    }
}

/// Whether a `vpn.data` key belongs to one of `short`'s friendly fields,
/// including the flags key of a secret.
pub fn is_covered(short: &str, key: &str) -> bool {
    fields(short).iter().any(|field| {
        field.key() == Some(key) || matches!(field.kind, Kind::Secret { flags, .. } if flags == key)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPENVPN_NAME: &str = "[VPN Connection]\n\
        name=openvpn\n\
        service=org.freedesktop.NetworkManager.openvpn\n\
        program=/usr/libexec/nm-openvpn-service\n\
        supports-multiple-connections=true\n\
        supports-safe-private-file-access=true\n\
        \n\
        [libnm]\n\
        plugin=libnm-vpn-plugin-openvpn.so\n\
        \n\
        [GNOME]\n\
        auth-dialog=/usr/libexec/nm-openvpn-auth-dialog\n\
        properties=libnm-openvpn-properties\n";

    #[test]
    fn parses_a_name_file() {
        let plugin = parse_name_file(OPENVPN_NAME).expect("a plugin");
        assert_eq!(plugin.short, "openvpn");
        assert_eq!(plugin.name, "OpenVPN");
        assert_eq!(plugin.service, "org.freedesktop.NetworkManager.openvpn");
        assert_eq!(
            plugin.program.as_deref(),
            Some("/usr/libexec/nm-openvpn-service")
        );
        assert!(plugin.supports_multiple);
    }

    #[test]
    fn keys_outside_the_vpn_connection_section_are_ignored() {
        let text = "[GNOME]\nname=wrong\nservice=wrong\n\n[VPN Connection]\nname=ssh\nservice=org.freedesktop.NetworkManager.ssh\n";
        let plugin = parse_name_file(text).expect("a plugin");
        assert_eq!(plugin.short, "ssh");
        assert!(!plugin.supports_multiple);
        assert_eq!(plugin.program, None);
    }

    #[test]
    fn a_file_without_a_service_or_with_a_bad_name_is_no_plugin() {
        assert_eq!(parse_name_file("[VPN Connection]\nname=ssh\n"), None);
        assert_eq!(
            parse_name_file("[VPN Connection]\nname=a b\nservice=x\n"),
            None
        );
        assert_eq!(parse_name_file(""), None);
    }

    #[test]
    fn unknown_plugins_get_a_capitalised_name() {
        assert_eq!(display_name("wibble"), "Wibble");
        assert_eq!(
            display_name(short_of_service("org.freedesktop.NetworkManager.vpnc")),
            "VPNC"
        );
        assert_eq!(short_of_service("com.example.thing"), "com.example.thing");
    }

    #[test]
    fn discovery_reads_both_directories_and_skips_missing_programs() {
        let root = std::env::temp_dir().join(format!("bingux-vpn-plugins-{}", std::process::id()));
        let package = root.join("package");
        let admin = root.join("admin");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::write(package.join("nm-openvpn-service.name"), OPENVPN_NAME).unwrap();
        std::fs::write(
            package.join("nm-gone-service.name"),
            "[VPN Connection]\nname=gone\nservice=org.freedesktop.NetworkManager.gone\nprogram=/nowhere/gone\n",
        )
        .unwrap();
        std::fs::write(
            admin.join("nm-openvpn-service.name"),
            "[VPN Connection]\nname=openvpn\nservice=org.freedesktop.NetworkManager.openvpn\nsupports-multiple-connections=false\n",
        )
        .unwrap();
        std::fs::write(package.join("notes.txt"), "not a plugin").unwrap();
        let found = discover_in(&[&package, &admin], |program| {
            program == Path::new("/usr/libexec/nm-openvpn-service")
        });
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(found.len(), 1);
        // The administrator's file replaced the package's.
        assert!(!found[0].supports_multiple);
    }

    #[test]
    fn a_missing_directory_is_no_plugins() {
        assert!(discover_in(&[Path::new("/nonexistent/vpn")], |_| true).is_empty());
    }

    #[test]
    fn fields_track_their_conditions_and_flags() {
        let openvpn = fields("openvpn");
        let password = openvpn
            .iter()
            .find(|f| f.label == "network-vpn-field-password")
            .unwrap();
        assert!(visible(password, |_| "password".to_string()));
        assert!(!visible(password, |_| "tls".to_string()));
        assert!(is_covered("openvpn", "password-flags"));
        assert!(is_covered("openvpn", "remote"));
        assert!(!is_covered("openvpn", "cipher"));
        assert!(!is_covered("pptp", "gateway"));
    }

    #[test]
    fn server_addresses_are_checked() {
        assert!(host_with_port("vpn.example.com").is_ok());
        assert!(host_with_port("vpn.example.com:1194").is_ok());
        assert!(host_with_port("10.0.0.1:443").is_ok());
        assert!(host_with_port("fe80::1").is_ok());
        assert!(host_with_port("vpn.example.com:99999").is_err());
        assert!(host_with_port("not a host").is_err());
        assert!(host_only("vpn.example.com").is_ok());
    }
}
