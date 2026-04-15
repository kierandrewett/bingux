use std::fs;
use std::path::{Path, PathBuf};

use bingux_common::error::Result;

use crate::config::SystemConfig;

/// A file that was generated (or should be generated) by the config system.
#[derive(Debug, Clone)]
pub struct GeneratedFile {
    /// Absolute path where this file should live (e.g. `/etc/hostname`).
    pub path: PathBuf,
    /// The file content (empty for symlinks — see `generate_timezone`).
    pub content: String,
}

/// Generates `/etc/` configuration files from a parsed `SystemConfig`.
pub struct EtcGenerator {
    /// Target directory, usually `/etc` but can be a tempdir for testing.
    target: PathBuf,
}

impl EtcGenerator {
    pub fn new(target: PathBuf) -> Self {
        Self { target }
    }

    /// Generate all configuration files from the given system config.
    pub fn generate_all(&self, config: &SystemConfig) -> Result<Vec<GeneratedFile>> {
        let mut files = Vec::new();

        // Core identity files
        files.push(self.generate_passwd()?);
        files.push(self.generate_group()?);
        files.push(self.generate_os_release()?);

        // System config derived
        files.push(self.generate_hostname(&config.system.hostname)?);
        files.push(self.generate_locale_conf(&config.system.locale)?);
        files.push(self.generate_locale_gen(&config.system.locale)?);
        files.push(self.generate_vconsole(&config.system.keymap)?);
        files.push(self.generate_timezone(&config.system.timezone)?);

        if let Some(ref net) = config.network {
            if let Some(ref dns) = net.dns {
                files.push(self.generate_resolv_conf(dns)?);
            }
        }

        if let Some(ref fw) = config.firewall {
            if let Some(ref ports) = fw.allow_ports {
                files.push(self.generate_nftables(ports)?);
            }
        }

        Ok(files)
    }

    /// Generate `/etc/hostname`.
    pub fn generate_hostname(&self, hostname: &str) -> Result<GeneratedFile> {
        let path = self.target.join("hostname");
        let content = format!("{hostname}\n");
        write_file(&path, &content)?;
        Ok(GeneratedFile { path, content })
    }

    /// Generate `/etc/locale.conf`.
    pub fn generate_locale_conf(&self, locale: &str) -> Result<GeneratedFile> {
        let path = self.target.join("locale.conf");
        let content = format!("LANG={locale}\n");
        write_file(&path, &content)?;
        Ok(GeneratedFile { path, content })
    }

    /// Generate `/etc/locale.gen`.
    pub fn generate_locale_gen(&self, locale: &str) -> Result<GeneratedFile> {
        let path = self.target.join("locale.gen");
        // locale.gen expects the locale with its encoding, e.g. "en_GB.UTF-8 UTF-8".
        // If the locale already contains an encoding suffix, split on the dot.
        let content = if let Some(dot_pos) = locale.find('.') {
            let encoding = &locale[dot_pos + 1..];
            format!("{locale} {encoding}\n")
        } else {
            format!("{locale} UTF-8\n")
        };
        write_file(&path, &content)?;
        Ok(GeneratedFile { path, content })
    }

    /// Generate `/etc/vconsole.conf`.
    pub fn generate_vconsole(&self, keymap: &str) -> Result<GeneratedFile> {
        let path = self.target.join("vconsole.conf");
        let content = format!("KEYMAP={keymap}\n");
        write_file(&path, &content)?;
        Ok(GeneratedFile { path, content })
    }

    /// Generate `/etc/localtime` as a symlink to the zoneinfo file.
    ///
    /// Returns a `GeneratedFile` with an empty `content` field — the actual
    /// result is the symlink on disk.
    pub fn generate_timezone(&self, timezone: &str) -> Result<GeneratedFile> {
        let path = self.target.join("localtime");
        let zoneinfo = Path::new("/usr/share/zoneinfo").join(timezone);

        // Remove existing file/symlink if present.
        let _ = fs::remove_file(&path);
        std::os::unix::fs::symlink(&zoneinfo, &path)?;

        Ok(GeneratedFile {
            path,
            content: String::new(),
        })
    }

    /// Generate `/etc/resolv.conf` with the given DNS servers.
    pub fn generate_resolv_conf(&self, dns: &[String]) -> Result<GeneratedFile> {
        let path = self.target.join("resolv.conf");
        let content = dns
            .iter()
            .map(|server| format!("nameserver {server}"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        write_file(&path, &content)?;
        Ok(GeneratedFile { path, content })
    }

    /// Generate `/etc/nftables.conf` with basic rules allowing the given ports.
    pub fn generate_nftables(&self, ports: &[u16]) -> Result<GeneratedFile> {
        let path = self.target.join("nftables.conf");

        let port_rules: String = ports
            .iter()
            .map(|p| format!("        tcp dport {p} accept"))
            .collect::<Vec<_>>()
            .join("\n");

        let content = format!(
            r#"#!/usr/sbin/nft -f

flush ruleset

table inet filter {{
    chain input {{
        type filter hook input priority 0; policy drop;

        # Allow established/related connections
        ct state established,related accept

        # Allow loopback
        iif lo accept

        # Allow ICMP
        ip protocol icmp accept
        ip6 nexthdr icmpv6 accept

        # Allowed ports
{port_rules}
    }}

    chain forward {{
        type filter hook forward priority 0; policy drop;
    }}

    chain output {{
        type filter hook output priority 0; policy accept;
    }}
}}
"#
        );

        write_file(&path, &content)?;
        Ok(GeneratedFile { path, content })
    }

    /// Generate `/etc/passwd` — root + system users.
    /// User home dirs use the Bingux `/users/<name>/home` layout.
    pub fn generate_passwd(&self) -> Result<GeneratedFile> {
        let path = self.target.join("passwd");
        let content = "\
root:x:0:0:root:/users/root:/bin/sh
nobody:x:65534:65534:Nobody:/:/sbin/nologin
systemd-journal:x:190:190:systemd Journal:/:/sbin/nologin
dbus:x:81:81:D-Bus:/:/sbin/nologin
"
        .to_string();
        write_file(&path, &content)?;
        Ok(GeneratedFile { path, content })
    }

    /// Generate `/etc/group`.
    pub fn generate_group(&self) -> Result<GeneratedFile> {
        let path = self.target.join("group");
        let content = "\
root:x:0:
nobody:x:65534:
systemd-journal:x:190:
dbus:x:81:
"
        .to_string();
        write_file(&path, &content)?;
        Ok(GeneratedFile { path, content })
    }

    /// Generate `/etc/os-release`.
    pub fn generate_os_release(&self) -> Result<GeneratedFile> {
        let path = self.target.join("os-release");
        let content = "\
NAME=\"Bingux\"
ID=bingux
VERSION_ID=2
PRETTY_NAME=\"Bingux v2\"
HOME_URL=\"https://github.com/kierandrewett/bingux\"
"
        .to_string();
        write_file(&path, &content)?;
        Ok(GeneratedFile { path, content })
    }
}

/// Helper: write content to a file, creating parent directories as needed.
fn write_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse_system_config_str;
    use tempfile::TempDir;

    fn make_generator() -> (TempDir, EtcGenerator) {
        let tmp = TempDir::new().unwrap();
        let generator = EtcGenerator::new(tmp.path().to_path_buf());
        (tmp, generator)
    }

    #[test]
    fn generate_hostname_file() {
        let (_tmp, generator) = make_generator();
        let result = generator.generate_hostname("bingux-pc").unwrap();
        assert_eq!(result.content, "bingux-pc\n");
        assert_eq!(fs::read_to_string(&result.path).unwrap(), "bingux-pc\n");
    }

    #[test]
    fn generate_locale_conf_file() {
        let (_tmp, generator) = make_generator();
        let result = generator.generate_locale_conf("en_GB.UTF-8").unwrap();
        assert_eq!(result.content, "LANG=en_GB.UTF-8\n");
    }

    #[test]
    fn generate_locale_gen_file() {
        let (_tmp, generator) = make_generator();
        let result = generator.generate_locale_gen("en_GB.UTF-8").unwrap();
        assert_eq!(result.content, "en_GB.UTF-8 UTF-8\n");
    }

    #[test]
    fn generate_vconsole_file() {
        let (_tmp, generator) = make_generator();
        let result = generator.generate_vconsole("uk").unwrap();
        assert_eq!(result.content, "KEYMAP=uk\n");
        assert_eq!(fs::read_to_string(&result.path).unwrap(), "KEYMAP=uk\n");
    }

    #[test]
    fn generate_timezone_creates_symlink() {
        let (_tmp, generator) = make_generator();
        let result = generator.generate_timezone("Europe/London").unwrap();
        assert!(result.path.is_symlink());
        let target = fs::read_link(&result.path).unwrap();
        assert_eq!(
            target,
            PathBuf::from("/usr/share/zoneinfo/Europe/London")
        );
    }

    #[test]
    fn generate_resolv_conf_with_multiple_dns() {
        let (_tmp, generator) = make_generator();
        let dns = vec!["1.1.1.1".to_string(), "1.0.0.1".to_string()];
        let result = generator.generate_resolv_conf(&dns).unwrap();
        assert_eq!(result.content, "nameserver 1.1.1.1\nnameserver 1.0.0.1\n");
        assert_eq!(
            fs::read_to_string(&result.path).unwrap(),
            "nameserver 1.1.1.1\nnameserver 1.0.0.1\n"
        );
    }

    #[test]
    fn generate_nftables_with_ports() {
        let (_tmp, generator) = make_generator();
        let result = generator.generate_nftables(&[22, 80, 443]).unwrap();
        assert!(result.content.contains("tcp dport 22 accept"));
        assert!(result.content.contains("tcp dport 80 accept"));
        assert!(result.content.contains("tcp dport 443 accept"));
        assert!(result.content.contains("policy drop"));
    }

    #[test]
    fn generate_all_produces_expected_files() {
        let config_str = r#"
[system]
hostname = "test-box"
locale = "en_GB.UTF-8"
timezone = "Europe/London"
keymap = "uk"

[packages]
keep = ["bash"]

[services]
enable = []

[network]
dns = ["8.8.8.8"]

[firewall]
allow_ports = [22]
"#;

        let config = parse_system_config_str(config_str).unwrap();
        let (_tmp, generator) = make_generator();
        let files = generator.generate_all(&config).unwrap();

        // passwd, group, os-release, hostname, locale.conf, locale.gen, vconsole.conf, localtime, resolv.conf, nftables.conf
        assert_eq!(files.len(), 10);

        let names: Vec<String> = files
            .iter()
            .map(|f| {
                f.path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();

        assert!(names.contains(&"hostname".to_string()));
        assert!(names.contains(&"locale.conf".to_string()));
        assert!(names.contains(&"locale.gen".to_string()));
        assert!(names.contains(&"vconsole.conf".to_string()));
        assert!(names.contains(&"localtime".to_string()));
        assert!(names.contains(&"resolv.conf".to_string()));
        assert!(names.contains(&"nftables.conf".to_string()));
    }

    #[test]
    fn generate_all_minimal_config() {
        let config_str = r#"
[system]
hostname = "minimal"
locale = "en_US.UTF-8"
timezone = "UTC"
keymap = "us"

[packages]
keep = []

[services]
enable = []
"#;

        let config = parse_system_config_str(config_str).unwrap();
        let (_tmp, generator) = make_generator();
        let files = generator.generate_all(&config).unwrap();

        // passwd, group, os-release + 5 config files (no resolv.conf, no nftables.conf).
        assert_eq!(files.len(), 8);
    }
}
