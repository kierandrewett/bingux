//! The sites the test double knows: a host name picks how a load ends.

use crate::web_view::ids::DownloadId;
use crate::web_view::page::{LoadError, Security, SecurityState};
use crate::web_view::requests::{
    AuthRequest, CertError, CertProblem, DialogKind, DialogRequest, DownloadInfo, DownloadState,
    PermissionKind, PermissionRequest, RequestKind,
};

use super::paint::Look;

/// Something that happens once a page has loaded.
pub(super) enum After {
    Ask(RequestKind),
    Download(DownloadInfo),
    Sound,
    Crash,
}

/// How a load ends.
pub(super) enum Outcome {
    Blank,
    Page {
        title: String,
        security: Security,
        look: Look,
        hue: f32,
        after: Vec<After>,
    },
    Error(LoadError),
    Cert(CertError),
}

/// An address as an address bar fixes it up: a bare host gets `https://`.
pub fn fix_up(input: &str) -> String {
    let text = input.trim();
    if text.is_empty() {
        return String::new();
    }
    if text.contains("://") || text.starts_with("about:") || text.starts_with("bingux:") {
        return text.to_string();
    }
    if text.starts_with('/') {
        return format!("file://{text}");
    }
    format!("https://{text}")
}

/// The host of an address, lower case, without a port.
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    authority.split(':').next().unwrap_or("").to_ascii_lowercase()
}

fn hue_of(host: &str) -> f32 {
    let sum: u32 = host.bytes().map(u32::from).sum();
    (sum * 37 % 360) as f32
}

fn secure() -> Security {
    Security {
        state: SecurityState::Secure,
        ..Security::default()
    }
}

fn cert(host: &str, code: &str, problem: CertProblem, overridable: bool) -> CertError {
    CertError {
        host: host.to_string(),
        code: code.to_string(),
        problem,
        issuer: Some("Example Untrusted CA".to_string()),
        fingerprint: Some("4F:8A:1B:C2:93:0E:77:D5:2A:61:BE:09:F3:CC:18:40".to_string()),
        valid_until: (problem == CertProblem::Expired).then(|| "12 March 2025".to_string()),
        overridable,
    }
}

/// What loading `url` does.
pub(super) fn resolve(url: &str, next_download: u64) -> Outcome {
    if url.is_empty() || url == "about:blank" {
        return Outcome::Blank;
    }
    let host = host_of(url);
    let page = |title: &str, security: Security, look: Look, after: Vec<After>| Outcome::Page {
        title: title.to_string(),
        security,
        look,
        hue: hue_of(&host),
        after,
    };
    if url.starts_with("file://") {
        return Outcome::Page {
            title: url.rsplit('/').next().unwrap_or("File").to_string(),
            security: Security {
                state: SecurityState::Local,
                ..Security::default()
            },
            look: Look::Site,
            hue: 30.0,
            after: Vec::new(),
        };
    }
    match host.as_str() {
        "example.com" => page("Example Domain", secure(), Look::Site, Vec::new()),
        "news.bingux.dev" => page("Bingux News", secure(), Look::Site, Vec::new()),
        "plain.example.net" => page("A plain page", Security::default(), Look::Site, Vec::new()),
        "mixed.example.net" => page(
            "Mixed content",
            Security {
                state: SecurityState::Broken,
                mixed_active: true,
                ..Security::default()
            },
            Look::Site,
            Vec::new(),
        ),
        "expired.example.net" => Outcome::Cert(cert(&host, "SEC_ERROR_EXPIRED_CERTIFICATE", CertProblem::Expired, true)),
        "selfsigned.example.net" => Outcome::Cert(cert(&host, "SEC_ERROR_UNKNOWN_ISSUER", CertProblem::SelfSigned, true)),
        "wronghost.example.net" => Outcome::Cert(cert(&host, "SSL_ERROR_BAD_CERT_DOMAIN", CertProblem::WrongHost, true)),
        "pinned.example.net" => Outcome::Cert(cert(&host, "MOZILLA_PKIX_ERROR_ADDITIONAL_POLICY_CONSTRAINT_FAILED", CertProblem::Other, false)),
        "missing.example.net" => Outcome::Error(LoadError {
            code: 0x804B_001E,
            name: "NS_ERROR_UNKNOWN_HOST".to_string(),
        }),
        "refused.example.net" => Outcome::Error(LoadError {
            code: 0x804B_000E,
            name: "NS_ERROR_CONNECTION_REFUSED".to_string(),
        }),
        "offline.example.net" => Outcome::Error(LoadError {
            code: 0x804B_0014,
            name: "NS_ERROR_OFFLINE".to_string(),
        }),
        "crash.example.net" => page("Crashing page", secure(), Look::Site, vec![After::Crash]),
        "perm.example.net" => page(
            "Where are you?",
            secure(),
            Look::Site,
            vec![After::Ask(RequestKind::Permission(PermissionRequest {
                origin: "https://perm.example.net".to_string(),
                kinds: vec![PermissionKind::Geolocation],
            }))],
        ),
        "dialog.example.net" => page(
            "Asks a question",
            secure(),
            Look::Site,
            vec![After::Ask(RequestKind::Dialog(DialogRequest {
                origin: "https://dialog.example.net".to_string(),
                kind: DialogKind::Confirm,
                message: "Delete this draft?".to_string(),
            }))],
        ),
        "auth.example.net" => page(
            "Private area",
            secure(),
            Look::Site,
            vec![After::Ask(RequestKind::Auth(AuthRequest {
                host: "auth.example.net".to_string(),
                realm: "Staff only".to_string(),
                proxy: false,
                retry: false,
            }))],
        ),
        "files.example.net" => page(
            "Reports",
            secure(),
            Look::Site,
            vec![After::Download(DownloadInfo {
                id: DownloadId(next_download),
                url: "https://files.example.net/report-2026.pdf".to_string(),
                filename: "report-2026.pdf".to_string(),
                path: None,
                mime: "application/pdf".to_string(),
                state: DownloadState::Running,
                received: 3_200_000,
                total: Some(8_400_000),
                speed: 1_100_000,
            })],
        ),
        "video.example.net" => page("Watch", secure(), Look::Video, vec![After::Sound]),
        _ => page(
            &host_title(&host),
            if url.starts_with("http://") { Security::default() } else { secure() },
            Look::Site,
            Vec::new(),
        ),
    }
}

fn host_title(host: &str) -> String {
    let name = host.strip_prefix("www.").unwrap_or(host);
    let mut chars = name.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_fixed_up_like_the_address_bar_does() {
        assert_eq!(fix_up("example.com"), "https://example.com");
        assert_eq!(fix_up("  http://a.b/c "), "http://a.b/c");
        assert_eq!(fix_up("about:blank"), "about:blank");
        assert_eq!(fix_up("/etc/hostname"), "file:///etc/hostname");
        assert_eq!(fix_up(""), "");
    }

    #[test]
    fn the_host_is_found_in_an_address() {
        assert_eq!(host_of("https://User@Example.COM:8080/a?b#c"), "example.com");
        assert_eq!(host_of("example.com/x"), "example.com");
    }

    #[test]
    fn known_hosts_end_the_way_they_are_named() {
        assert!(matches!(resolve("https://expired.example.net/", 1), Outcome::Cert(_)));
        assert!(matches!(resolve("https://missing.example.net/", 1), Outcome::Error(_)));
        assert!(matches!(resolve("about:blank", 1), Outcome::Blank));
        let Outcome::Page { security, .. } = resolve("http://anything.example/", 1) else {
            panic!("a page");
        };
        assert_eq!(security.state, SecurityState::Insecure);
    }
}
