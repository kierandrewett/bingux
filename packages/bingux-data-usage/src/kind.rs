//! Which network interfaces are worth counting, and of what kind.
//!
//! Classification is a pure function of a directory snapshot: everything it
//! looks at lives under `<sysfs_root>/class/net/<name>/`, so tests build a
//! fake tree in a temp directory and pass it as the root.

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
    str::FromStr,
};

/// What an interface carries traffic for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Wifi,
    Ethernet,
    Mobile,
    Vpn,
}

impl Kind {
    /// The value stored in the database `kind` columns.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Wifi => "wifi",
            Kind::Ethernet => "ethernet",
            Kind::Mobile => "mobile",
            Kind::Vpn => "vpn",
        }
    }

    /// A guess from the interface name alone, for interfaces that no longer
    /// exist in sysfs (used for vnStat history). Follows systemd's predictable
    /// names and the classic kernel prefixes.
    pub fn guess_from_name(name: &str) -> Option<Kind> {
        let starts = |prefixes: &[&str]| prefixes.iter().any(|prefix| name.starts_with(prefix));
        if starts(&["wlan", "wlp", "wlx", "wlo", "wls"]) {
            Some(Kind::Wifi)
        } else if starts(&["ww"]) {
            Some(Kind::Mobile)
        } else if starts(&["wg", "tun", "tap", "ppp"]) {
            Some(Kind::Vpn)
        } else if starts(&["eth", "enp", "eno", "ens", "enx", "end"]) {
            Some(Kind::Ethernet)
        } else {
            None
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Kind {
    type Err = ();

    fn from_str(text: &str) -> Result<Self, ()> {
        match text {
            "wifi" => Ok(Kind::Wifi),
            "ethernet" => Ok(Kind::Ethernet),
            "mobile" => Ok(Kind::Mobile),
            "vpn" => Ok(Kind::Vpn),
            _ => Err(()),
        }
    }
}

/// One counted interface and its raw kernel counters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetInterface {
    pub name: String,
    pub kind: Kind,
    pub mac: Option<String>,
    pub ifindex: i64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

/// The default sysfs mount.
pub const SYSFS: &str = "/sys";

fn net_dir(sysfs_root: &Path, name: &str) -> PathBuf {
    sysfs_root.join("class/net").join(name)
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_owned())
}

/// `DEVTYPE=` from the interface's `uevent` file.
fn devtype(dir: &Path) -> Option<String> {
    fs::read_to_string(dir.join("uevent"))
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("DEVTYPE=").map(str::to_owned))
}

/// Classifies one interface, or `None` when it must not be counted:
/// loopback, bridges, veth pairs, docker/podman networks, VLANs, bonds and
/// every other virtual Ethernet (anything without a backing `device`).
///
/// * Wi-Fi: a `wireless` directory, `phy80211` link, or `DEVTYPE=wlan`
///   (the `p2p-dev-*` companions of a radio are skipped).
/// * Mobile: `DEVTYPE=wwan` or a `ww*` name (`wwan0`, `wwp0s20f0u6c3`).
/// * VPN: `DEVTYPE=wireguard`, `DEVTYPE=ppp`/`ppp*`, or a `tun_flags` file
///   (tun/tap, OpenVPN). A tap that is a bridge or bond port (libvirt's
///   `vnet*`) is a VM's cable, not a VPN, and is skipped.
/// * Ethernet: ARPHRD_ETHER with a backing `device` and nothing virtual
///   about it.
pub fn classify(sysfs_root: &Path, name: &str) -> Option<Kind> {
    if name.is_empty() || name.contains('/') || name == "lo" {
        return None;
    }
    let dir = net_dir(sysfs_root, name);
    if !dir.is_dir() {
        return None;
    }
    let devtype = devtype(&dir);
    let devtype = devtype.as_deref();
    if dir.join("bridge").exists() {
        return None;
    }
    if matches!(
        devtype,
        Some("bridge" | "veth" | "vlan" | "bond" | "macvlan" | "ipvlan" | "dummy" | "vxlan")
    ) {
        return None;
    }

    match devtype {
        Some("wireguard") => return Some(Kind::Vpn),
        Some("ppp") => return Some(Kind::Vpn),
        Some("wwan") => return Some(Kind::Mobile),
        _ => {}
    }
    let wireless =
        dir.join("wireless").exists() || dir.join("phy80211").exists() || devtype == Some("wlan");
    if wireless {
        return if name.starts_with("p2p-dev-") {
            None
        } else {
            Some(Kind::Wifi)
        };
    }
    if name.starts_with("ww") {
        return Some(Kind::Mobile);
    }
    if dir.join("tun_flags").exists() {
        return if dir.join("master").exists() {
            None
        } else {
            Some(Kind::Vpn)
        };
    }
    let arphrd = read_trimmed(&dir.join("type")).and_then(|text| text.parse::<u32>().ok());
    if name.starts_with("ppp") || arphrd == Some(512) {
        return Some(Kind::Vpn);
    }
    // Everything below is Ethernet-framed hardware only.
    if arphrd.is_some_and(|value| value != 1) || !dir.join("device").exists() {
        return None;
    }
    Some(Kind::Ethernet)
}

/// The raw `(rx_bytes, tx_bytes)` counters of an interface.
pub fn read_counters(sysfs_root: &Path, name: &str) -> Option<(u64, u64)> {
    let statistics = net_dir(sysfs_root, name).join("statistics");
    let rx = read_trimmed(&statistics.join("rx_bytes"))?.parse().ok()?;
    let tx = read_trimmed(&statistics.join("tx_bytes"))?.parse().ok()?;
    Some((rx, tx))
}

/// Every countable interface with its counters, sorted by name. An
/// interface that disappears or has unreadable counters while scanning is
/// skipped: it is gone, not an error.
pub fn scan(sysfs_root: &Path) -> io::Result<Vec<NetInterface>> {
    let mut interfaces = Vec::new();
    for entry in fs::read_dir(sysfs_root.join("class/net"))? {
        let Ok(entry) = entry else { continue };
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let Some(kind) = classify(sysfs_root, &name) else {
            continue;
        };
        let Some((rx_bytes, tx_bytes)) = read_counters(sysfs_root, &name) else {
            continue;
        };
        let dir = net_dir(sysfs_root, &name);
        interfaces.push(NetInterface {
            mac: read_trimmed(&dir.join("address"))
                .filter(|mac| !mac.is_empty() && mac != "00:00:00:00:00:00"),
            ifindex: read_trimmed(&dir.join("ifindex"))
                .and_then(|text| text.parse().ok())
                .unwrap_or(0),
            name,
            kind,
            rx_bytes,
            tx_bytes,
        });
    }
    interfaces.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(interfaces)
}

#[cfg(test)]
pub(crate) mod fake {
    //! A fake `/sys/class/net` for tests.

    use std::{fs, path::Path};

    /// Creates `class/net/<name>` with the given files (`path`, `content`).
    /// A trailing `/` in a path creates an empty directory.
    pub fn interface(root: &Path, name: &str, files: &[(&str, &str)]) {
        let dir = root.join("class/net").join(name);
        fs::create_dir_all(dir.join("statistics")).unwrap();
        for (path, content) in files {
            let target = dir.join(path);
            if path.ends_with('/') {
                fs::create_dir_all(target).unwrap();
            } else {
                fs::create_dir_all(target.parent().unwrap()).unwrap();
                fs::write(target, content).unwrap();
            }
        }
    }

    pub fn counters(root: &Path, name: &str, rx: u64, tx: u64) {
        let statistics = root.join("class/net").join(name).join("statistics");
        fs::create_dir_all(&statistics).unwrap();
        fs::write(statistics.join("rx_bytes"), format!("{rx}\n")).unwrap();
        fs::write(statistics.join("tx_bytes"), format!("{tx}\n")).unwrap();
    }

    /// A tree with one of every interface the classifier must handle.
    pub fn typical_tree(root: &Path) {
        interface(root, "lo", &[("type", "772\n")]);
        interface(
            root,
            "enp3s0",
            &[
                ("type", "1\n"),
                ("device/", ""),
                ("address", "aa:bb:cc:00:00:01\n"),
                ("ifindex", "2\n"),
            ],
        );
        interface(
            root,
            "wlan0",
            &[
                ("type", "1\n"),
                ("device/", ""),
                ("wireless/", ""),
                ("uevent", "DEVTYPE=wlan\n"),
                ("ifindex", "3\n"),
            ],
        );
        interface(root, "p2p-dev-wlan0", &[("type", "1\n"), ("phy80211/", "")]);
        interface(
            root,
            "wwan0",
            &[
                ("type", "1\n"),
                ("device/", ""),
                ("uevent", "DEVTYPE=wwan\n"),
                ("ifindex", "4\n"),
            ],
        );
        interface(
            root,
            "wg0",
            &[
                ("type", "65534\n"),
                ("uevent", "DEVTYPE=wireguard\n"),
                ("ifindex", "5\n"),
            ],
        );
        interface(
            root,
            "tun0",
            &[
                ("type", "65534\n"),
                ("tun_flags", "0x1002\n"),
                ("ifindex", "6\n"),
            ],
        );
        interface(root, "ppp0", &[("type", "512\n"), ("ifindex", "7\n")]);
        interface(
            root,
            "br0",
            &[
                ("type", "1\n"),
                ("bridge/", ""),
                ("uevent", "DEVTYPE=bridge\n"),
            ],
        );
        interface(root, "docker0", &[("type", "1\n"), ("bridge/", "")]);
        interface(
            root,
            "veth1234",
            &[("type", "1\n"), ("uevent", "DEVTYPE=veth\n")],
        );
        interface(
            root,
            "vnet0",
            &[("type", "1\n"), ("tun_flags", "0x1002\n"), ("master/", "")],
        );
        interface(root, "dummy0", &[("type", "1\n")]);
        for (name, rx, tx) in [
            ("lo", 10, 10),
            ("enp3s0", 1000, 200),
            ("wlan0", 5000, 600),
            ("p2p-dev-wlan0", 0, 0),
            ("wwan0", 70, 80),
            ("wg0", 300, 400),
            ("tun0", 3, 4),
            ("ppp0", 5, 6),
            ("br0", 1, 1),
            ("docker0", 1, 1),
            ("veth1234", 1, 1),
            ("vnet0", 1, 1),
            ("dummy0", 1, 1),
        ] {
            counters(root, name, rx, tx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn classifies_a_typical_machine() {
        let root = TempDir::new("kind");
        fake::typical_tree(root.path());
        let kind = |name| classify(root.path(), name);
        assert_eq!(kind("lo"), None);
        assert_eq!(kind("enp3s0"), Some(Kind::Ethernet));
        assert_eq!(kind("wlan0"), Some(Kind::Wifi));
        assert_eq!(kind("p2p-dev-wlan0"), None);
        assert_eq!(kind("wwan0"), Some(Kind::Mobile));
        assert_eq!(kind("wg0"), Some(Kind::Vpn));
        assert_eq!(kind("tun0"), Some(Kind::Vpn));
        assert_eq!(kind("ppp0"), Some(Kind::Vpn));
        assert_eq!(kind("br0"), None);
        assert_eq!(kind("docker0"), None);
        assert_eq!(kind("veth1234"), None);
        assert_eq!(
            kind("vnet0"),
            None,
            "a bridged tap is a VM cable, not a VPN"
        );
        assert_eq!(kind("dummy0"), None, "Ethernet without a device is virtual");
        assert_eq!(kind("missing"), None);
        assert_eq!(kind("../etc"), None);
    }

    #[test]
    fn wifi_by_phy80211_and_mobile_by_name() {
        let root = TempDir::new("kind2");
        fake::interface(root.path(), "wlp2s0", &[("type", "1\n"), ("phy80211/", "")]);
        fake::interface(
            root.path(),
            "wwp0s20f0u6c3",
            &[("type", "1\n"), ("device/", "")],
        );
        assert_eq!(classify(root.path(), "wlp2s0"), Some(Kind::Wifi));
        assert_eq!(classify(root.path(), "wwp0s20f0u6c3"), Some(Kind::Mobile));
    }

    #[test]
    fn scan_returns_only_counted_interfaces_with_counters() {
        let root = TempDir::new("scan");
        fake::typical_tree(root.path());
        let found = scan(root.path()).unwrap();
        let names: Vec<_> = found.iter().map(|item| item.name.as_str()).collect();
        assert_eq!(names, ["enp3s0", "ppp0", "tun0", "wg0", "wlan0", "wwan0"]);
        let ethernet = &found[0];
        assert_eq!(
            (ethernet.rx_bytes, ethernet.tx_bytes, ethernet.ifindex),
            (1000, 200, 2)
        );
        assert_eq!(ethernet.mac.as_deref(), Some("aa:bb:cc:00:00:01"));
        assert_eq!(found[2].mac, None, "no address file");
    }

    #[test]
    fn interface_without_readable_counters_is_skipped() {
        let root = TempDir::new("nocounters");
        fake::typical_tree(root.path());
        fs::remove_file(root.path().join("class/net/wlan0/statistics/rx_bytes")).unwrap();
        let found = scan(root.path()).unwrap();
        assert!(found.iter().all(|item| item.name != "wlan0"));
    }

    #[test]
    fn kind_round_trips_through_text() {
        for kind in [Kind::Wifi, Kind::Ethernet, Kind::Mobile, Kind::Vpn] {
            assert_eq!(kind.as_str().parse::<Kind>(), Ok(kind));
        }
        assert_eq!("bridge".parse::<Kind>(), Err(()));
        assert_eq!(Kind::guess_from_name("wlp3s0"), Some(Kind::Wifi));
        assert_eq!(Kind::guess_from_name("docker0"), None);
    }
}
