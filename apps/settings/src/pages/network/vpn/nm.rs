//! `nmcli` for VPN profiles, with the whole of its error text.
//!
//! The shared runner in `bingux_ui::runtime` keeps only the last line of a
//! failure. `nmcli connection up` ends its message with a `Hint:` line that
//! points at `journalctl`, so that runner would show the hint and lose the
//! reason. VPN screens show the reason, so they run `nmcli` here.

use std::process::Command;

/// Run `nmcli args`. The error is nmcli's own message, without the `Error:`
/// prefix and the `Hint:` lines.
pub fn run(args: &[&str]) -> Result<String, String> {
    let output = Command::new("nmcli")
        .args(args)
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => {
                "Network settings are not available because nmcli is not installed.".to_string()
            }
            _ => format!("Network settings could not be started: {error}."),
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output.status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = clean_error(&stderr);
    if !message.is_empty() {
        return Err(message);
    }
    let message = clean_error(&stdout);
    Err(if message.is_empty() {
        "Network settings failed without saying why.".to_string()
    } else {
        message
    })
}

/// nmcli's failure text as a sentence: `Error: ` prefixes and the `Hint:` and
/// `Warning:` lines are dropped, and the remaining lines are joined.
pub fn clean_error(text: &str) -> String {
    let lines: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("Hint:") && !line.starts_with("Warning:"))
        .map(|line| {
            line.strip_prefix("Error: ")
                .or_else(|| line.strip_prefix("error: "))
                .unwrap_or(line)
                .to_string()
        })
        .collect();
    let joined = lines.join(" ");
    let mut letters = joined.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => String::new(),
    }
}

/// Why a VPN would not connect, in plain words. `message` is nmcli's cleaned
/// failure text from `connection up`; a reason this does not know is shown as
/// nmcli wrote it, so nothing is lost.
pub fn activation_reason(message: &str) -> String {
    let lower = message.to_lowercase();
    let has = |needle: &str| lower.contains(needle);
    if has("secrets were required")
        || has("(10)")
        || has("no secrets")
        || has("no agents were available")
    {
        "The VPN needs a password or key that is not saved. Add it in the VPN's settings."
            .to_string()
    } else if has("(11)") || has("login failed") || has("authentication failed") {
        "The server refused the user name or password.".to_string()
    } else if has("timeout") || has("timed out") || has("(7)") {
        "The server did not answer in time. Check its address and your connection.".to_string()
    } else if has("(8)") || has("(9)") || has("service did not start") || has("failed to start") {
        "The VPN program failed to start. Check its settings, and that its plugin is installed."
            .to_string()
    } else if has("(6)") {
        "The server did not give the VPN a valid network setup.".to_string()
    } else if has("no suitable device") || has("not available on device") {
        "There is no network connection to carry the VPN. Connect to a network first.".to_string()
    } else if has("no such connection") || has("unknown connection") || has("(12)") {
        "This VPN no longer exists.".to_string()
    } else if has("(3)") || has("(4)") || has("(5)") {
        "The VPN was disconnected.".to_string()
    } else if has("not authorized") || has("insufficient privileges") {
        "You are not allowed to change this connection.".to_string()
    } else if message.is_empty() {
        "The VPN did not connect.".to_string()
    } else {
        message.to_string()
    }
}

/// The name and UUID in nmcli's `Connection 'name' (uuid) successfully added.`
pub fn added(output: &str) -> Option<(String, String)> {
    let line = output
        .lines()
        .find(|line| line.starts_with("Connection '"))?;
    let rest = line.strip_prefix("Connection '")?;
    let (name, rest) = rest.rsplit_once("' (")?;
    let (uuid, _) = rest.split_once(')')?;
    (uuid.len() == 36 && uuid.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
        .then(|| (name.to_string(), uuid.to_string()))
}

/// `nmcli -s` values of `fields` for the profile named `name`, in order, with
/// an empty string for a field the profile does not report. Escaping is off,
/// so a value keeps its colons and backslashes.
pub fn show(name: &str, fields: &[&str]) -> Result<Vec<String>, String> {
    let list = fields.join(",");
    let text = run(&[
        "-e",
        "no",
        "-s",
        "-t",
        "-f",
        &list,
        "connection",
        "show",
        "id",
        name,
    ])?;
    let mut values = vec![String::new(); fields.len()];
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if let Some(index) = fields.iter().position(|field| *field == key) {
            values[index] = value.trim().to_string();
        }
    }
    Ok(values)
}

/// `nmcli connection modify id <name> <property> <value> ...`.
pub fn modify(name: &str, changes: &[(&str, &str)]) -> Result<(), String> {
    let mut args = vec!["connection", "modify", "id", name];
    for (property, value) in changes {
        args.push(property);
        args.push(value);
    }
    run(&args).map(|_| ())
}

/// Ask a running connection's device to take its saved settings now. Returns
/// whether it could; the settings are saved either way.
pub fn reapply(device: &str) -> bool {
    !device.is_empty() && run(&["device", "reapply", device]).is_ok()
}

/// The names of every connection profile.
pub fn connection_names() -> Result<Vec<String>, String> {
    let text = run(&["-e", "no", "-g", "NAME", "connection", "show"])?;
    Ok(text.lines().map(str::to_string).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_error_drops_hint_and_prefix() {
        let text = "Error: Connection activation failed: (1) Unknown reason\n\
                    Hint: use 'journalctl -xe NM_CONNECTION=abc + NM_DEVICE=wg0' to get more details.\n";
        assert_eq!(
            clean_error(text),
            "Connection activation failed: (1) Unknown reason"
        );
    }

    #[test]
    fn clean_error_joins_lines_and_capitalises() {
        assert_eq!(
            clean_error("error: first.\nsecond line\n"),
            "First. second line"
        );
        assert_eq!(clean_error(""), "");
        assert_eq!(clean_error("Warning: only a warning\n"), "");
    }

    #[test]
    fn activation_reasons_are_plain_words() {
        let cases = [
            (
                "Connection activation failed: Secrets were required, but not provided",
                "The VPN needs a password or key that is not saved. Add it in the VPN's settings.",
            ),
            (
                "Connection activation failed: No agents were available for this request.",
                "The VPN needs a password or key that is not saved. Add it in the VPN's settings.",
            ),
            (
                "Connection activation failed: (11) Login failed",
                "The server refused the user name or password.",
            ),
            (
                "Connection activation failed: (7) Connection timeout",
                "The server did not answer in time. Check its address and your connection.",
            ),
            (
                "Timeout expired (60 seconds)",
                "The server did not answer in time. Check its address and your connection.",
            ),
            (
                "Connection activation failed: (9) The service failed to start",
                "The VPN program failed to start. Check its settings, and that its plugin is installed.",
            ),
            (
                "Connection activation failed: The VPN service failed to start",
                "The VPN program failed to start. Check its settings, and that its plugin is installed.",
            ),
            (
                "Connection activation failed: (6) IP configuration invalid",
                "The server did not give the VPN a valid network setup.",
            ),
            (
                "No suitable device found for this connection (device lo not available)",
                "There is no network connection to carry the VPN. Connect to a network first.",
            ),
            ("", "The VPN did not connect."),
        ];
        for (input, expected) in cases {
            assert_eq!(activation_reason(input), expected, "{input}");
        }
    }

    #[test]
    fn finds_the_name_and_uuid_nmcli_reports() {
        let line = "Connection 'My 'quoted' VPN' (bb332422-a3bd-4524-80fc-44db91369986) successfully added.";
        assert_eq!(
            added(line),
            Some((
                "My 'quoted' VPN".to_string(),
                "bb332422-a3bd-4524-80fc-44db91369986".to_string()
            ))
        );
        assert_eq!(added("Connection 'x' (nonsense) added."), None);
        assert_eq!(added("nothing"), None);
        let with_warning = "Warning: another connection\nConnection 'x' (bb332422-a3bd-4524-80fc-44db91369986) successfully added.";
        assert!(added(with_warning).is_some());
    }

    #[test]
    fn unknown_reason_is_kept_verbatim() {
        assert_eq!(
            activation_reason("Something new happened"),
            "Something new happened"
        );
    }
}
