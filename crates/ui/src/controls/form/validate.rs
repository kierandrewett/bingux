//! Validators for [`FormField::validate`](super::FormField::validate).
//!
//! Each one is a pure `fn(&str) -> Result<(), String>`: the error is a
//! sentence a person can act on, not a parse error. The text is checked as
//! typed, so a validator never trims: stray spaces are part of what the
//! person sees rejected, except in the list validators, whose separators
//! are spaces and commas.
//!
//! An empty string is an error for every validator ("Enter ..."). The field
//! itself never shows an error for an empty value, so a form decides whether
//! a blank field is allowed by pairing [`FormField::optional`](super::FormField::optional)
//! with the validator, and by checking emptiness itself when it applies.
//!
//! Zone identifiers (`fe80::1%eth0`) are rejected by the IPv6 validators:
//! the address fields these serve (NetworkManager's addresses, DNS servers,
//! gateways) take a bare address.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::ops::RangeInclusive;
use std::str::FromStr;

/// What every validator returns.
pub type Check = Result<(), String>;

fn fail(message: &str) -> Check {
    Err(message.to_string())
}

/// A dotted IPv4 address, such as `192.168.1.10`. Leading zeros are refused,
/// as `010` reads as octal to many tools.
pub fn ipv4(text: &str) -> Check {
    Ipv4Addr::from_str(text)
        .map(|_| ())
        .or_else(|_| fail("Enter an IPv4 address such as 192.168.1.10."))
}

/// An IPv6 address, such as `2001:db8::1`, `::1` or `::ffff:1.2.3.4`. A zone
/// (`fe80::1%eth0`) is not accepted.
pub fn ipv6(text: &str) -> Check {
    Ipv6Addr::from_str(text)
        .map(|_| ())
        .or_else(|_| fail("Enter an IPv6 address such as 2001:db8::1."))
}

/// An IPv4 or an IPv6 address.
pub fn ip(text: &str) -> Check {
    IpAddr::from_str(text)
        .map(|_| ())
        .or_else(|_| fail("Enter an IP address such as 192.168.1.10 or 2001:db8::1."))
}

/// A prefix length written as digits only, within `0..=max`.
fn prefix(text: &str, max: u8) -> Option<u8> {
    if text.is_empty() || text.len() > 3 || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse::<u8>().ok().filter(|length| *length <= max)
}

/// An address and prefix length, such as `192.168.1.10/24`.
pub fn ipv4_cidr(text: &str) -> Check {
    let valid = text.split_once('/').is_some_and(|(address, length)| {
        Ipv4Addr::from_str(address).is_ok() && prefix(length, 32).is_some()
    });
    if valid {
        Ok(())
    } else {
        fail("Enter an address and prefix length such as 192.168.1.10/24 (0 to 32).")
    }
}

/// An address and prefix length, such as `2001:db8::1/64`.
pub fn ipv6_cidr(text: &str) -> Check {
    let valid = text.split_once('/').is_some_and(|(address, length)| {
        Ipv6Addr::from_str(address).is_ok() && prefix(length, 128).is_some()
    });
    if valid {
        Ok(())
    } else {
        fail("Enter an address and prefix length such as 2001:db8::1/64 (0 to 128).")
    }
}

/// Several addresses (IPv4 or IPv6) separated by commas or spaces, such as
/// `1.1.1.1, 9.9.9.9`. At least one is required.
pub fn ip_list(text: &str) -> Check {
    let mut count = 0;
    for item in text.split([',', ' ', '\t']).filter(|item| !item.is_empty()) {
        if IpAddr::from_str(item).is_err() {
            return Err(format!("\u{201c}{item}\u{201d} is not an IP address."));
        }
        count += 1;
    }
    if count == 0 {
        return fail("Enter one or more IP addresses, separated by commas.");
    }
    Ok(())
}

/// A netmask such as `255.255.255.0` (ones, then zeros), or a prefix length
/// such as `24` or `/24`.
pub fn ipv4_netmask_or_prefix(text: &str) -> Check {
    let message = "Enter a prefix length such as 24, or a netmask such as 255.255.255.0.";
    let digits = text.strip_prefix('/').unwrap_or(text);
    if prefix(digits, 32).is_some() {
        return Ok(());
    }
    match Ipv4Addr::from_str(text) {
        Ok(mask) => {
            let bits = u32::from(mask);
            // A netmask is a run of ones then a run of zeros: inverted, it is
            // 2^n - 1, which has no bits in common with its successor.
            if bits.leading_ones() + bits.trailing_zeros() == 32 {
                Ok(())
            } else {
                fail("A netmask has ones and then zeros, such as 255.255.255.0.")
            }
        }
        Err(_) => fail(message),
    }
}

/// A MAC address: six pairs of hexadecimal digits separated by colons or
/// dashes, such as `00:1A:2B:3C:4D:5E`.
pub fn mac(text: &str) -> Check {
    let separator = if text.contains(':') { ':' } else { '-' };
    let pairs: Vec<&str> = text.split(separator).collect();
    let valid = pairs.len() == 6
        && pairs
            .iter()
            .all(|pair| pair.len() == 2 && pair.bytes().all(|byte| byte.is_ascii_hexdigit()));
    if valid {
        Ok(())
    } else {
        fail("Enter a MAC address such as 00:1A:2B:3C:4D:5E.")
    }
}

/// A TCP or UDP port, 1 to 65535.
pub fn port(text: &str) -> Check {
    integer_in(1..=65535)(text).or_else(|_| fail("Enter a port number from 1 to 65535."))
}

/// A whole number within `range`, written in digits with an optional minus.
/// Returns a closure, for [`FormField::validate_with`](super::FormField::validate_with).
pub fn integer_in(range: RangeInclusive<i64>) -> impl Fn(&str) -> Check + 'static {
    move |text: &str| {
        let digits = text.strip_prefix('-').unwrap_or(text);
        let shaped = !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit());
        match text.parse::<i64>() {
            Ok(number) if shaped && range.contains(&number) => Ok(()),
            _ => Err(format!(
                "Enter a whole number from {} to {}.",
                range.start(),
                range.end()
            )),
        }
    }
}

/// A hostname such as `vpn.example.com` (letters, digits and hyphens in
/// dot-separated labels of up to 63 characters), or an IP address.
pub fn hostname_or_ip(text: &str) -> Check {
    if ip(text).is_ok() {
        return Ok(());
    }
    let message = "Enter a host name such as vpn.example.com, or an IP address.";
    let name = text.strip_suffix('.').unwrap_or(text);
    if name.is_empty() || name.len() > 253 {
        return fail(message);
    }
    let labels: Vec<&str> = name.split('.').collect();
    let labels_ok = labels.iter().all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    });
    // `300.1.1.1` is a mistyped address, not a host name: a name's last
    // label is never all digits.
    let numeric_tail = labels.len() > 1
        && labels
            .last()
            .is_some_and(|label| label.bytes().all(|byte| byte.is_ascii_digit()));
    if labels_ok && !numeric_tail {
        Ok(())
    } else {
        fail(message)
    }
}

/// An absolute URL: `scheme://host[:port][/path]`, such as
/// `http://wpad.example.com/proxy.pac`. A `file:` URL may have no host.
pub fn url(text: &str) -> Check {
    let message = "Enter a full address such as http://example.com/proxy.pac.";
    let Some((scheme, rest)) = text.split_once("://") else {
        return fail(message);
    };
    let scheme_ok = scheme
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !scheme_ok || text.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return fail(message);
    }
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() {
        return if scheme.eq_ignore_ascii_case("file") {
            Ok(())
        } else {
            fail(message)
        };
    }
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let (host, port_text) = if let Some(inner) = host_port.strip_prefix('[') {
        match inner.split_once(']') {
            Some((address, tail)) => (
                format!("[{address}]"),
                tail.strip_prefix(':').or(tail.is_empty().then_some("")),
            ),
            None => return fail(message),
        }
    } else {
        match host_port.rsplit_once(':') {
            Some((host, port)) => (host.to_string(), Some(port)),
            None => (host_port.to_string(), Some("")),
        }
    };
    let host_ok = match host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        Some(address) => Ipv6Addr::from_str(address).is_ok(),
        None => hostname_or_ip(&host).is_ok(),
    };
    let port_ok = match port_text {
        Some("") => true,
        Some(port_text) => port(port_text).is_ok(),
        None => false,
    };
    if host_ok && port_ok {
        Ok(())
    } else {
        fail(message)
    }
}

/// A WireGuard key: 32 bytes in base64, 44 characters ending in `=`.
pub fn wireguard_key(text: &str) -> Check {
    const MESSAGE: &str =
        "A WireGuard key is 44 characters of letters, digits, + and /, ending in =.";
    let bytes = text.as_bytes();
    if bytes.len() != 44 || bytes[43] != b'=' {
        return fail(MESSAGE);
    }
    let mut last = 0;
    for byte in &bytes[..43] {
        last = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return fail(MESSAGE),
        };
    }
    // 43 symbols are 258 bits for 256 of data: the last two must be zero,
    // or the key does not decode to exactly 32 bytes.
    if last & 0b11 != 0 {
        return fail(MESSAGE);
    }
    Ok(())
}

/// A Wi-Fi network name: 1 to 32 bytes.
pub fn ssid(text: &str) -> Check {
    if text.is_empty() {
        fail("Enter the network name.")
    } else if text.len() > 32 {
        Err(format!(
            "A network name is at most 32 bytes; this one is {}.",
            text.len()
        ))
    } else {
        Ok(())
    }
}

/// A WPA password: 8 to 63 printable ASCII characters, or a 64-digit
/// hexadecimal key.
pub fn wpa_passphrase(text: &str) -> Check {
    let length = text.chars().count();
    if length == 64 {
        return if text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            Ok(())
        } else {
            fail("A 64-character key may only use the digits 0 to 9 and the letters A to F.")
        };
    }
    if length < 8 {
        return fail("The password needs at least 8 characters.");
    }
    if length > 63 {
        return fail("The password is at most 63 characters, or 64 for a hexadecimal key.");
    }
    if !text.bytes().all(|byte| (0x20..=0x7e).contains(&byte)) {
        return fail("Use only letters, digits, spaces and punctuation without accents.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_ok(check: fn(&str) -> Check, good: &[&str]) {
        for text in good {
            assert_eq!(check(text), Ok(()), "{text:?} should be accepted");
        }
    }

    fn all_err(check: fn(&str) -> Check, bad: &[&str]) {
        for text in bad {
            assert!(check(text).is_err(), "{text:?} should be rejected");
        }
    }

    #[test]
    fn ipv4_forms() {
        all_ok(ipv4, &["0.0.0.0", "192.168.1.10", "255.255.255.255"]);
        all_err(
            ipv4,
            &[
                "",
                "256.1.1.1",
                "1.2.3",
                "1.2.3.4.5",
                "01.2.3.4",
                " 1.2.3.4",
                "1.2.3.4 ",
                "a.b.c.d",
                "::1",
            ],
        );
    }

    #[test]
    fn ipv6_forms() {
        all_ok(
            ipv6,
            &[
                "::",
                "::1",
                "2001:db8::1",
                "::ffff:1.2.3.4",
                "fe80::1",
                "1:2:3:4:5:6:7:8",
            ],
        );
        all_err(
            ipv6,
            &[
                "",
                "fe80::1%eth0",
                "1.2.3.4",
                "2001:db8:::1",
                "12345::",
                "1:2:3:4:5:6:7:8:9",
                "[::1]",
            ],
        );
    }

    #[test]
    fn any_ip() {
        all_ok(ip, &["10.0.0.1", "::1"]);
        all_err(ip, &["", "10.0.0", "localhost"]);
    }

    #[test]
    fn cidr_boundaries() {
        all_ok(ipv4_cidr, &["0.0.0.0/0", "192.168.1.10/24", "10.0.0.1/32"]);
        all_err(
            ipv4_cidr,
            &[
                "",
                "10.0.0.1",
                "10.0.0.1/33",
                "10.0.0.1/",
                "10.0.0.1/-1",
                "10.0.0.1/ 8",
                "10.0.0/8",
                "10.0.0.1/24/1",
            ],
        );
        all_ok(ipv6_cidr, &["::/0", "2001:db8::1/64", "::1/128"]);
        all_err(
            ipv6_cidr,
            &[
                "",
                "::1",
                "::1/129",
                "::1/",
                "1.2.3.4/24",
                "fe80::1%eth0/64",
            ],
        );
    }

    #[test]
    fn list_of_addresses() {
        all_ok(
            ip_list,
            &[
                "1.1.1.1",
                "1.1.1.1, 9.9.9.9",
                "1.1.1.1 ::1",
                "1.1.1.1,2606:4700::1111",
                " 8.8.8.8 ,",
            ],
        );
        all_err(ip_list, &["", " ", ",", "1.1.1.1, nope", "1.1.1.1;8.8.8.8"]);
        let message = ip_list("1.1.1.1, nope").unwrap_err();
        assert!(message.contains("nope"), "the message names the bad entry");
    }

    #[test]
    fn netmask_or_prefix() {
        all_ok(
            ipv4_netmask_or_prefix,
            &[
                "0",
                "24",
                "/24",
                "32",
                "255.255.255.0",
                "255.255.254.0",
                "0.0.0.0",
                "255.255.255.255",
                "128.0.0.0",
            ],
        );
        all_err(
            ipv4_netmask_or_prefix,
            &[
                "",
                "33",
                "/33",
                "-1",
                "255.0.255.0",
                "0.255.255.255",
                "255.255.255",
                "abc",
                "/",
            ],
        );
    }

    #[test]
    fn mac_forms() {
        all_ok(
            mac,
            &[
                "00:1A:2B:3C:4D:5E",
                "00-1a-2b-3c-4d-5e",
                "ff:ff:ff:ff:ff:ff",
            ],
        );
        all_err(
            mac,
            &[
                "",
                "00:1A:2B:3C:4D",
                "00:1A:2B:3C:4D:5E:6F",
                "00:1A:2B:3C:4D:5G",
                "001A:2B3C:4D5E",
                "0:1A:2B:3C:4D:5E",
                "00:1A-2B:3C:4D:5E",
            ],
        );
    }

    #[test]
    fn port_boundaries() {
        all_ok(port, &["1", "80", "65535"]);
        all_err(
            port,
            &["", "0", "65536", "-1", "80.5", "port", " 80", "+80"],
        );
    }

    #[test]
    fn integers_in_range() {
        let check = integer_in(-5..=10);
        assert_eq!(check("-5"), Ok(()));
        assert_eq!(check("10"), Ok(()));
        assert_eq!(check("0"), Ok(()));
        for bad in ["", "-6", "11", "1.5", "-", "ten", "+3", " 3"] {
            assert!(check(bad).is_err(), "{bad:?} should be rejected");
        }
        assert!(check("11").unwrap_err().contains("-5 to 10"));
    }

    #[test]
    fn hostnames() {
        all_ok(
            hostname_or_ip,
            &[
                "router",
                "vpn.example.com",
                "a-b.c1.example.",
                "10.0.0.1",
                "::1",
                "xn--bcher-kva.example",
                "3com.com",
            ],
        );
        all_err(
            hostname_or_ip,
            &[
                "",
                ".",
                "-a.com",
                "a-.com",
                "a..com",
                "a b.com",
                "under_score.com",
                "300.1.1.1",
                "1.2.3",
                "host:80",
                &"a".repeat(64),
            ],
        );
    }

    #[test]
    fn urls() {
        all_ok(
            url,
            &[
                "http://example.com",
                "https://wpad.example.com/proxy.pac",
                "http://10.0.0.1:8080/x?y=1#z",
                "http://user:pw@host.example:3128/",
                "http://[2001:db8::1]:8080/",
                "http://[::1]/",
                "file:///etc/proxy.pac",
                "socks5://127.0.0.1:1080",
            ],
        );
        all_err(
            url,
            &[
                "",
                "example.com",
                "http://",
                "http:/example.com",
                "://example.com",
                "http://exa mple.com",
                "http://host:99999/",
                "http://host:abc/",
                "1http://x.com",
                "http://[::1/",
                "http://300.1.1.1/",
            ],
        );
    }

    #[test]
    fn wireguard_keys() {
        let good = "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=";
        assert_eq!(wireguard_key(good), Ok(()));
        assert_eq!(
            wireguard_key("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="),
            Ok(())
        );
        all_err(
            wireguard_key,
            &[
                "",
                "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk",
                "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmkA",
                "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBm-=",
                // 43 symbols whose last one sets a bit past 256: not 32 bytes.
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB=",
                "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk==",
            ],
        );
    }

    #[test]
    fn ssid_byte_limits() {
        all_ok(ssid, &["a", "Home", &"x".repeat(32)]);
        all_err(ssid, &["", &"x".repeat(33)]);
        // Eleven three-byte characters are 33 bytes, though only 11 long.
        assert!(ssid(&"\u{20ac}".repeat(11)).is_err());
        assert!(ssid(&"\u{20ac}".repeat(10)).is_ok());
    }

    #[test]
    fn wpa_passphrases() {
        all_ok(
            wpa_passphrase,
            &[
                "12345678",
                "correct horse battery",
                &"a".repeat(63),
                &"AbCdEf0123456789".repeat(4),
            ],
        );
        all_err(
            wpa_passphrase,
            &[
                "",
                "1234567",
                &"a".repeat(65),
                &"z".repeat(64),
                "pässword123",
            ],
        );
        // 64 characters are a raw key, so they must be hexadecimal.
        assert!(wpa_passphrase(&"f".repeat(64)).is_ok());
        assert!(wpa_passphrase(&"a".repeat(64)).is_ok());
    }
}
