//! NetworkManager's connection settings over D-Bus, through `busctl`.
//!
//! `nmcli` 1.54 cannot read or write a WireGuard profile's peers: its
//! `wireguard` setting lists no `peers` property, and `+wireguard.peer` is
//! read as the prefix of `wireguard.peer-routes`. NetworkManager itself has
//! them, as the `peers` property of the `wireguard` setting, reachable with
//! `Settings.Connection.GetSettings`, `GetSecrets` and `Update`.
//!
//! `busctl --json=short` prints values with their D-Bus types, and `busctl
//! call` takes the same types as arguments, so a settings dictionary read
//! with one is written back with the other, with only the peers changed.

use std::process::Command;

use serde_json::Value;

const SERVICE: &str = "org.freedesktop.NetworkManager";
const SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
const CONNECTION: &str = "org.freedesktop.NetworkManager.Settings.Connection";

fn busctl(args: &[&str]) -> Result<String, String> {
    let output = Command::new("busctl")
        .arg("--system")
        .args(args)
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => {
                "Peers cannot be read because busctl is not installed.".to_string()
            }
            _ => format!("busctl could not be started: {error}."),
        })?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }
    let text = String::from_utf8_lossy(&output.stderr);
    let message = text.trim().trim_start_matches("Call failed: ").trim();
    Err(if message.is_empty() {
        "NetworkManager did not answer.".to_string()
    } else {
        message.to_string()
    })
}

/// The D-Bus object path of the connection with this UUID.
pub fn connection_path(uuid: &str) -> Result<String, String> {
    let text = busctl(&[
        "call",
        SERVICE,
        SETTINGS_PATH,
        "org.freedesktop.NetworkManager.Settings",
        "GetConnectionByUuid",
        "s",
        uuid,
    ])?;
    // `o "/org/freedesktop/NetworkManager/Settings/19"`
    text.split('"')
        .nth(1)
        .map(str::to_string)
        .ok_or_else(|| "NetworkManager gave no path for this connection.".to_string())
}

/// A call whose one result is `a{sa{sv}}`, as the dictionary inside.
fn settings_call(path: &str, method: &str, argument: Option<&str>) -> Result<Value, String> {
    let mut args = vec!["--json=short", "call", SERVICE, path, CONNECTION, method];
    if let Some(argument) = argument {
        args.push("s");
        args.push(argument);
    }
    let text = busctl(&args)?;
    parse_reply(&text)
}

/// The dictionary in a `busctl --json=short` reply of one `a{sa{sv}}`.
fn parse_reply(text: &str) -> Result<Value, String> {
    let reply: Value = serde_json::from_str(text)
        .map_err(|_| "NetworkManager gave an answer this app cannot read.".to_string())?;
    reply
        .get("data")
        .and_then(|data| data.get(0))
        .cloned()
        .ok_or_else(|| "NetworkManager gave an answer this app cannot read.".to_string())
}

/// All settings of the connection at `path`, without secrets.
pub fn get_settings(path: &str) -> Result<Value, String> {
    settings_call(path, "GetSettings", None)
}

/// The secrets of one setting (`wireguard`) of the connection at `path`.
pub fn get_secrets(path: &str, setting: &str) -> Result<Value, String> {
    settings_call(path, "GetSecrets", Some(setting))
}

/// Replace the connection at `path` with `settings` and save it. Secrets
/// that `settings` leaves out are kept by NetworkManager.
pub fn update(path: &str, settings: &Value) -> Result<(), String> {
    let mut encoded = Vec::new();
    encode("a{sa{sv}}", settings, &mut encoded)?;
    let mut args = vec!["call", SERVICE, path, CONNECTION, "Update", "a{sa{sv}}"];
    args.extend(encoded.iter().map(String::as_str));
    busctl(&args).map(|_| ())
}

/// The first complete type in `signature` and what follows it.
fn split_type(signature: &str) -> Result<(&str, &str), String> {
    let bad = || format!("The D-Bus type {signature:?} is not supported.");
    let first = signature.chars().next().ok_or_else(bad)?;
    match first {
        'a' => {
            let (_, rest) = split_type(&signature[1..])?;
            Ok(signature.split_at(signature.len() - rest.len()))
        }
        '(' | '{' => {
            let mut depth = 0;
            for (index, c) in signature.char_indices() {
                match c {
                    '(' | '{' => depth += 1,
                    ')' | '}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(signature.split_at(index + 1));
                        }
                    }
                    _ => {}
                }
            }
            Err(bad())
        }
        _ => Ok(signature.split_at(1)),
    }
}

/// Append the `busctl call` arguments for `value`, a JSON value shaped as
/// `busctl --json=short` prints type `signature`: arrays and structs as
/// arrays, dictionaries as objects, variants as `{"type": .., "data": ..}`.
fn encode(signature: &str, value: &Value, out: &mut Vec<String>) -> Result<(), String> {
    let mismatch = || format!("A value does not match its D-Bus type {signature:?}.");
    match signature.chars().next().ok_or_else(mismatch)? {
        's' | 'o' | 'g' => out.push(value.as_str().ok_or_else(mismatch)?.to_string()),
        'b' => out.push(value.as_bool().ok_or_else(mismatch)?.to_string()),
        'y' | 'n' | 'q' | 'i' | 'u' | 'x' | 't' | 'd' => {
            if !value.is_number() {
                return Err(mismatch());
            }
            out.push(value.to_string());
        }
        'v' => {
            let inner = value
                .get("type")
                .and_then(Value::as_str)
                .ok_or_else(mismatch)?;
            out.push(inner.to_string());
            encode(inner, value.get("data").ok_or_else(mismatch)?, out)?;
        }
        'a' if signature.starts_with("a{") => {
            let body = &signature[2..signature.len() - 1];
            let (key, value_type) = split_type(body)?;
            if key != "s" {
                return Err(format!("A dictionary keyed by {key:?} is not supported."));
            }
            let map = value.as_object().ok_or_else(mismatch)?;
            out.push(map.len().to_string());
            for (name, item) in map {
                out.push(name.clone());
                encode(value_type, item, out)?;
            }
        }
        'a' => {
            let items = value.as_array().ok_or_else(mismatch)?;
            out.push(items.len().to_string());
            for item in items {
                encode(&signature[1..], item, out)?;
            }
        }
        '(' => {
            let items = value.as_array().ok_or_else(mismatch)?;
            let mut rest = &signature[1..signature.len() - 1];
            if items.is_empty() && !rest.is_empty() {
                return Err(mismatch());
            }
            for item in items {
                let (member, tail) = split_type(rest)?;
                encode(member, item, out)?;
                rest = tail;
            }
        }
        _ => return Err(mismatch()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn args(signature: &str, value: &Value) -> Vec<String> {
        let mut out = Vec::new();
        encode(signature, value, &mut out).expect("encodes");
        out
    }

    #[test]
    fn splits_complete_types() {
        assert_eq!(split_type("sa{sv}").unwrap(), ("s", "a{sv}"));
        assert_eq!(split_type("a{sa{sv}}u").unwrap(), ("a{sa{sv}}", "u"));
        assert_eq!(split_type("a(ayuay)").unwrap(), ("a(ayuay)", ""));
        assert_eq!(split_type("(ayuay)b").unwrap(), ("(ayuay)", "b"));
        assert!(split_type("").is_err());
    }

    #[test]
    fn encodes_variants_arrays_and_dictionaries() {
        let settings = json!({
            "wireguard": {
                "peers": {"type": "aa{sv}", "data": [{
                    "allowed-ips": {"type": "as", "data": ["10.77.0.0/24", "fd77::/64"]}
                }]}
            }
        });
        let got = args("a{sa{sv}}", &settings);
        let expected = [
            "1",
            "wireguard",
            "1",
            "peers",
            "aa{sv}",
            "1",
            "1",
            "allowed-ips",
            "as",
            "2",
            "10.77.0.0/24",
            "fd77::/64",
        ];
        assert_eq!(got, expected);
        let number = args("v", &json!({"type": "u", "data": 25}));
        assert_eq!(number, ["u", "25"]);
    }

    #[test]
    fn encodes_structs_and_byte_arrays() {
        // An IPv6 address as NetworkManager keeps it: `(ayuay)`.
        let address = json!({"type": "a(ayuay)", "data": [
            [[253, 119, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2], 128,
             [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]]
        ]});
        let got = args("v", &address);
        assert_eq!(got[0], "a(ayuay)");
        assert_eq!(got[1], "1");
        assert_eq!(got[2], "16");
        assert_eq!(got[3], "253");
        assert_eq!(got[19], "128");
        assert_eq!(got[20], "16");
        assert_eq!(got.len(), 21 + 16);
    }

    #[test]
    fn a_wrong_shape_is_an_error() {
        let mut out = Vec::new();
        assert!(encode("s", &json!(5), &mut out).is_err());
        assert!(encode("a{ia{sv}}", &json!({}), &mut out).is_err());
        assert!(encode("as", &json!("x"), &mut out).is_err());
    }

    #[test]
    fn reads_the_dictionary_of_a_reply() {
        let reply =
            r#"{"type":"a{sa{sv}}","data":[{"connection":{"id":{"type":"s","data":"x"}}}]}"#;
        let settings = parse_reply(reply).unwrap();
        assert_eq!(settings["connection"]["id"]["data"], "x");
        assert!(parse_reply("nonsense").is_err());
    }
}
