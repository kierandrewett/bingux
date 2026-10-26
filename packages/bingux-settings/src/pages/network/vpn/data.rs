//! A plugin VPN's `vpn.data` and `vpn.secrets`: lists of `key = value` pairs
//! in the text format nmcli reads and writes, and the profile that holds them.
//!
//! The format, as nmcli 1.54 behaves: pairs are separated by commas, a value
//! may contain `\,` for a comma and `\\` for a backslash, a backslash before
//! anything else is a plain backslash, and nmcli prints a backslash twice only
//! where a single one would be read as an escape (before a comma, another
//! backslash or the end of the value). Keys may contain spaces (`IPSec ID`).

use super::nm;

/// Ordered `(key, value)` pairs.
pub type Entries = Vec<(String, String)>;

/// Split nmcli's text into pairs. An item with no `=` is dropped.
pub fn parse(text: &str) -> Entries {
    let mut items: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if matches!(chars.peek(), Some('\\' | ',')) => {
                current.push(chars.next().unwrap_or('\\'));
            }
            ',' => items.push(std::mem::take(&mut current)),
            other => current.push(other),
        }
    }
    items.push(current);
    items
        .into_iter()
        .filter_map(|item| {
            let (key, value) = item.split_once('=')?;
            let key = key.trim();
            (!key.is_empty()).then(|| (key.to_string(), value.trim().to_string()))
        })
        .collect()
}

/// The text nmcli takes for `entries`.
pub fn serialise(entries: &[(String, String)]) -> String {
    entries
        .iter()
        .map(|(key, value)| format!("{key} = {}", escape(value)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A value with commas escaped, and backslashes doubled where they would
/// otherwise be read as an escape.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ',' => out.push_str("\\,"),
            '\\' => {
                out.push('\\');
                if matches!(chars.peek(), None | Some('\\' | ',')) {
                    out.push('\\');
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// The value of `key`.
pub fn get<'a>(entries: &'a [(String, String)], key: &str) -> Option<&'a str> {
    entries
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

/// Set `key` to `value`, or remove it when `value` is empty, keeping the
/// position of an existing key.
pub fn set(entries: &mut Entries, key: &str, value: &str) {
    match entries.iter().position(|(name, _)| name == key) {
        Some(index) if value.is_empty() => {
            entries.remove(index);
        }
        Some(index) => entries[index].1 = value.to_string(),
        None if value.is_empty() => {}
        None => entries.push((key.to_string(), value.to_string())),
    }
}

/// A plugin VPN profile: the NetworkManager `vpn` setting.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PluginProfile {
    pub uuid: String,
    /// The plugin's service type, such as `org.freedesktop.NetworkManager.openvpn`.
    pub service: String,
    /// `vpn.user-name`.
    pub user_name: String,
    pub data: Entries,
    pub secrets: Entries,
}

/// Read the plugin profile named `name`, with the secrets NetworkManager
/// keeps for it.
pub fn load(name: &str) -> Result<PluginProfile, String> {
    let values = nm::show(
        name,
        &[
            "connection.uuid",
            "vpn.service-type",
            "vpn.user-name",
            "vpn.data",
            "vpn.secrets",
        ],
    )?;
    let [uuid, service, user_name, data, secrets] = <[String; 5]>::try_from(values)
        .map_err(|_| "NetworkManager returned an unexpected answer.".to_string())?;
    Ok(PluginProfile {
        uuid,
        service,
        user_name,
        data: parse(&data),
        secrets: parse(&secrets),
    })
}

/// Write the settings of `profile` to the profile named `name`.
pub fn save(name: &str, profile: &PluginProfile) -> Result<(), String> {
    nm::modify(
        name,
        &[
            ("vpn.data", &serialise(&profile.data)),
            ("vpn.secrets", &serialise(&profile.secrets)),
            ("vpn.user-name", &profile.user_name),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(items: &[(&str, &str)]) -> Entries {
        items
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn parses_what_nmcli_prints_for_an_imported_ovpn() {
        let text = "ca = /home/k/ca.pem, challenge-response-flags = 2, cipher = AES-256-GCM, \
                    connection-type = password, dev = tun, password-flags = 1, \
                    remote = vpn.example.net:1194, remote-cert-tls = server";
        let entries = parse(text);
        assert_eq!(entries.len(), 8);
        assert_eq!(get(&entries, "remote"), Some("vpn.example.net:1194"));
        assert_eq!(get(&entries, "password-flags"), Some("1"));
    }

    #[test]
    fn keys_may_contain_spaces_and_values_may_be_empty() {
        let entries = parse("IPSec ID = grp, IPSec gateway = 1.2.3.4, y = ");
        assert_eq!(
            entries,
            pairs(&[("IPSec ID", "grp"), ("IPSec gateway", "1.2.3.4"), ("y", "")])
        );
    }

    #[test]
    fn a_value_keeps_equals_signs() {
        assert_eq!(parse("a = x=y"), pairs(&[("a", "x=y")]));
    }

    #[test]
    fn commas_and_backslashes_follow_nmcli() {
        // Verified against nmcli 1.54: what it prints is what it reads.
        assert_eq!(parse(r"a = x\,y"), pairs(&[("a", "x,y")]));
        assert_eq!(
            parse(r"a = C:\vpn\ca.pem"),
            pairs(&[("a", r"C:\vpn\ca.pem")])
        );
        assert_eq!(parse(r"a = C:\\vpn"), pairs(&[("a", r"C:\vpn")]));
        assert_eq!(parse(r"a = trailing\\"), pairs(&[("a", r"trailing\")]));
        assert_eq!(
            parse(r"a = x\\\,y, b = 2"),
            pairs(&[("a", r"x\,y"), ("b", "2")])
        );
        assert_eq!(serialise(&pairs(&[("a", "x,y")])), r"a = x\,y");
        assert_eq!(
            serialise(&pairs(&[("a", r"C:\vpn\ca.pem")])),
            r"a = C:\vpn\ca.pem"
        );
        assert_eq!(serialise(&pairs(&[("a", r"trailing\")])), r"a = trailing\\");
        assert_eq!(serialise(&pairs(&[("a", r"x\,y")])), r"a = x\\\,y");
    }

    #[test]
    fn serialise_then_parse_returns_the_same_pairs() {
        let original = pairs(&[
            ("remote", "vpn.example.net:1194"),
            ("path", r"C:\a\b\"),
            ("odd", r"x\,\\y,z"),
            ("IPSec ID", "group one"),
            ("empty", ""),
        ]);
        assert_eq!(parse(&serialise(&original)), original);
    }

    #[test]
    fn set_replaces_appends_and_removes() {
        let mut entries = pairs(&[("a", "1"), ("b", "2")]);
        set(&mut entries, "a", "9");
        set(&mut entries, "c", "3");
        set(&mut entries, "b", "");
        set(&mut entries, "zzz", "");
        assert_eq!(entries, pairs(&[("a", "9"), ("c", "3")]));
    }
}
