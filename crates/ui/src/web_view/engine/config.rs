//! What the host tells the engine: the profile and the folders around it, the
//! folders it may read, how it learns about revoked certificates, who it says
//! it is. All of it is chosen here, once, when the host starts the engine,
//! and shown on the settings page; the engine reads nothing else.
//!
//! Where things live is the host's choice ([`EngineFolders`]): a browser with
//! profiles names one profile's folders. Two more are fixed:
//!
//! | What | Where |
//! | ---- | ----- |
//! | downloads | `~/Downloads` |
//! | a private window | one profile, with everything below it, in `$XDG_RUNTIME_DIR`, deleted with the window |

use std::path::{Path, PathBuf};
use std::time::Duration;

use vivarium::config::{
    Directories, Identity, Limits, Locales, RemoteSettings as EngineRemoteSettings, SandboxFloor,
};
use vivarium::engine::{Engine, EngineBuilder};

use crate::web_view::backend::{EngineSettings, RemoteSettings};

/// What a page's WebRTC connection may reveal about the machine
/// (`BINGUX_BROWSER_WEBRTC_IP_POLICY`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebrtcChoice {
    Default,
    DefaultPublic,
    PublicOnly,
    RelayOnly,
}

impl WebrtcChoice {
    fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "default" => Self::Default,
            "default-public" => Self::DefaultPublic,
            "public-only" => Self::PublicOnly,
            "relay-only" => Self::RelayOnly,
            _ => return None,
        })
    }

    /// What the choice means, for the settings page.
    pub const fn text(self) -> &'static str {
        match self {
            Self::Default => "Gecko's default: local addresses may be shared",
            Self::DefaultPublic => {
                "Share the public address, and local ones only when a call needs them"
            }
            Self::PublicOnly => "Share only the public address",
            Self::RelayOnly => "Share nothing: calls go through a relay",
        }
    }
}

/// The host's choices.
#[derive(Clone, Debug)]
pub struct Config {
    pub profile: PathBuf,
    /// A private window's profile is thrown away with it.
    pub ephemeral: bool,
    /// The engine's cache, when it is not below the profile.
    pub cache: Option<PathBuf>,
    /// Where minidumps go, when it is not below the profile.
    pub crash: Option<PathBuf>,
    /// Where the engine's runtime sockets go, when not below the profile.
    pub runtime: Option<PathBuf>,
    pub downloads: PathBuf,
    pub read_dirs: Vec<PathBuf>,
    pub remote_settings: RemoteSettings,
    pub remote_settings_server: Option<String>,
    pub content_processes: u32,
    pub locales: String,
    /// The WebRTC address policy the host asks for. ABI 2.0 has no call that
    /// sets one (the network settings call is gone), so the engine keeps its
    /// own fixed policy and the settings page says so.
    pub webrtc: Option<WebrtcChoice>,
    /// The SDK folder (`VIVARIUM_SDK_DIR`), when it is not where the library is.
    pub sdk: Option<PathBuf>,
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.is_empty())
}

/// `en_GB.UTF-8` as a BCP 47 tag, `en-US` when the environment says nothing.
pub fn locale_tag(lang: &str) -> String {
    let base = lang.split(['.', '@']).next().unwrap_or("");
    if base.is_empty() || base == "C" || base == "POSIX" {
        return "en-US".to_string();
    }
    base.replace('_', "-")
}

/// Where the engine keeps things. The host chooses: a browser with profiles
/// names one profile's folders, an app with one engine names its own, and
/// bingux-ui reads no variable for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineFolders {
    /// A private window: one throwaway profile in `$XDG_RUNTIME_DIR` (or the
    /// temporary folder), with its cache, dumps and sockets below it, deleted
    /// with the window.
    Private,
    /// A profile the host keeps, and the folders around it.
    Profile {
        profile: PathBuf,
        cache: PathBuf,
        /// Where crash dumps go (never uploaded).
        crash: PathBuf,
        /// Where the engine's sockets go; `None` puts them below the profile.
        runtime: Option<PathBuf>,
    },
}

impl Config {
    /// The choices of this process for the engine's `folders`.
    /// `BINGUX_BROWSER_REMOTE_SETTINGS` is `off`, `security` or `mozilla` and
    /// `BINGUX_BROWSER_REMOTE_SETTINGS_SERVER` the mirror `security` reads from
    /// (without one, or without a choice, the security data comes from
    /// Mozilla). `BINGUX_BROWSER_FILE_DIRS` (a path list) adds folders `file://`
    /// pages may read, `BINGUX_BROWSER_WEBRTC_IP_POLICY` (`default`,
    /// `default-public`, `public-only`, `relay-only`) asks for a WebRTC address
    /// policy and `VIVARIUM_SDK_DIR` names the engine.
    pub fn from_environment(folders: EngineFolders) -> Self {
        Self::from_lookup(folders, &|name| std::env::var(name).ok(), &|name| {
            std::env::var_os(name)
        })
    }

    /// [`Config::from_environment`] over any source of variables.
    fn from_lookup(
        folders: EngineFolders,
        var: &dyn Fn(&str) -> Option<String>,
        var_os: &dyn Fn(&str) -> Option<std::ffi::OsString>,
    ) -> Self {
        let path = |name: &str| {
            var_os(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
        };
        let home = path("HOME").unwrap_or_else(|| PathBuf::from("/"));
        let private = folders == EngineFolders::Private;
        let (profile, cache, crash, runtime) = match folders {
            // A private window keeps its cache, dumps and sockets with its
            // profile, so they go with it.
            EngineFolders::Private => {
                let run = path("XDG_RUNTIME_DIR").unwrap_or_else(std::env::temp_dir);
                let profile = run.join(format!("bingux-browser-private-{}", std::process::id()));
                (profile, None, None, None)
            }
            EngineFolders::Profile {
                profile,
                cache,
                crash,
                runtime,
            } => (profile, Some(cache), Some(crash), runtime),
        };
        let server = nonempty(var("BINGUX_BROWSER_REMOTE_SETTINGS_SERVER"));
        let remote_settings = match var("BINGUX_BROWSER_REMOTE_SETTINGS").as_deref() {
            Some("off") => RemoteSettings::Off,
            Some("mozilla") => RemoteSettings::MozillaDirect,
            _ if server.is_some() => RemoteSettings::SecurityOnly,
            _ => RemoteSettings::MozillaDirect,
        };
        let downloads = home.join("Downloads");
        // Folders `file://` pages may read: the usual ones of the home folder
        // that exist, and the ones the host names. The download folder is not
        // one of them: it is a folder of the engine's own, like the profile,
        // cache, dumps and sockets, and the engine refuses to start when a
        // folder pages may read overlaps one of those (one inside the other).
        let mut read_dirs: Vec<PathBuf> = ["Documents", "Desktop", "Pictures", "Videos", "Music"]
            .iter()
            .map(|name| home.join(name))
            .filter(|dir| dir.is_dir())
            .collect();
        if let Some(extra) = var_os("BINGUX_BROWSER_FILE_DIRS") {
            read_dirs.extend(std::env::split_paths(&extra).filter(|dir| dir.is_absolute()));
        }
        let engines_own: Vec<&PathBuf> = [
            Some(&profile),
            cache.as_ref(),
            crash.as_ref(),
            runtime.as_ref(),
            Some(&downloads),
        ]
        .into_iter()
        .flatten()
        .collect();
        read_dirs.retain(|dir| {
            !engines_own
                .iter()
                .any(|own| dir.starts_with(own) || own.starts_with(dir))
        });
        Self {
            profile,
            ephemeral: private,
            cache,
            crash,
            runtime,
            downloads,
            read_dirs,
            remote_settings,
            remote_settings_server: server,
            content_processes: 0,
            locales: locale_tag(&var("LANG").unwrap_or_default()),
            webrtc: var("BINGUX_BROWSER_WEBRTC_IP_POLICY")
                .as_deref()
                .and_then(WebrtcChoice::parse),
            sdk: path("VIVARIUM_SDK_DIR"),
        }
    }

    /// Who the engine says it is. The engine builds the user agent itself,
    /// `Mozilla/5.0 (X11; Linux x86_64; rv:V.0) Gecko/20100101 <ua_name>/V.0`
    /// with its own Gecko version `V`, so naming the product `Firefox` is all
    /// it takes for sites to serve what they serve Firefox; the host reads no
    /// file for it and appends no suffix. The single-instance name stays
    /// `bingux-browser` so the browser never answers to a real Firefox's
    /// remote commands.
    fn identity(&self) -> Identity {
        Identity {
            // The Mozilla application ID the earlier Dot browser used, kept as a nod to it.
            app_id: Some("{818c990f-687c-498c-bc9a-a99d9729702a}".to_string()),
            app_name: Some("bingux-browser".to_string()),
            remoting_name: Some("bingux-browser".to_string()),
            vendor: Some("Bingux".to_string()),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
            ua_name: Some("Firefox".to_string()),
            ..Identity::default()
        }
    }

    fn directories(&self) -> Directories {
        let mut directories = Directories::new(self.profile.clone());
        directories.download = Some(self.downloads.clone());
        directories.cache = self.cache.clone();
        directories.crash = self.crash.clone();
        directories.runtime = self.runtime.clone();
        directories
    }

    fn remote_settings_choice(&self) -> EngineRemoteSettings {
        match (&self.remote_settings, &self.remote_settings_server) {
            (RemoteSettings::Off, _) => EngineRemoteSettings::Off,
            (RemoteSettings::MozillaDirect, _) | (RemoteSettings::SecurityOnly, None) => {
                EngineRemoteSettings::MozillaDirect
            }
            (RemoteSettings::SecurityOnly, Some(server)) => EngineRemoteSettings::SecurityOnly {
                server: server.clone(),
            },
        }
    }

    /// The builder for this configuration. The engine must reach the standard
    /// sandbox or it does not start.
    pub(super) fn builder(&self) -> EngineBuilder {
        let mut builder = Engine::builder(self.profile.clone())
            .directories(self.directories())
            .identity(self.identity())
            .locales(Locales {
                locales: vec![self.locales.clone()],
            })
            .remote_settings(self.remote_settings_choice())
            .sandbox_floor(SandboxFloor::Standard)
            .limits(Limits {
                max_content_processes: self.content_processes,
                ..Limits::default()
            })
            // The UI thread drops the engine at quit: a stuck engine is killed
            // after this, not after the library's ten seconds.
            .shutdown_timeout(Duration::from_secs(3));
        for dir in &self.read_dirs {
            builder = builder.file_read_dir(dir.clone());
        }
        if let Some(sdk) = &self.sdk {
            builder = builder.resource_dir(sdk.clone());
        }
        builder
    }

    fn below(&self, name: &str) -> PathBuf {
        self.profile.join("vivarium").join(name)
    }

    /// What the settings page says.
    pub(super) fn settings(&self) -> EngineSettings {
        let webrtc_ip_policy = match self.webrtc {
            Some(choice) => format!(
                "Asked for \u{201c}{}\u{201d}, but this engine keeps its own fixed policy",
                choice.text()
            ),
            None => {
                "The engine's own fixed policy: no local or TCP addresses are shared".to_string()
            }
        };
        EngineSettings {
            profile_dir: self.profile.clone(),
            cache_dir: self.cache.clone().unwrap_or_else(|| self.below("cache")),
            download_dir: self.downloads.clone(),
            crash_dir: self
                .crash
                .clone()
                .unwrap_or_else(|| self.profile.join("crashes")),
            remote_settings: self.remote_settings,
            remote_settings_server: self.remote_settings_server.clone(),
            content_process_limit: if self.content_processes == 0 {
                8
            } else {
                self.content_processes
            },
            locales: self.locales.clone(),
            webrtc_ip_policy,
            sandbox_floor:
                "Standard: web content level 6, network level 2, seccomp on every process"
                    .to_string(),
        }
    }

    /// A private window's profile goes with it.
    pub(super) fn forget(&self) {
        if self.ephemeral {
            let _ = std::fs::remove_dir_all(&self.profile);
        }
    }

    /// The profile folder.
    pub fn profile(&self) -> &Path {
        &self.profile
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::ffi::OsString;

    use super::*;

    fn config(folders: EngineFolders, vars: &[(&str, &str)]) -> Config {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        let text = vars.clone();
        let os = vars;
        Config::from_lookup(
            folders,
            &move |name| text.get(name).cloned(),
            &move |name| os.get(name).map(OsString::from),
        )
    }

    /// The folders a host with profiles names for one of them.
    fn host() -> EngineFolders {
        EngineFolders::Profile {
            profile: PathBuf::from("/h/.local/share/bingux-browser/profiles/k3p7x2qa/engine"),
            cache: PathBuf::from("/h/.cache/bingux-browser/profiles/k3p7x2qa"),
            crash: PathBuf::from("/h/.local/state/bingux-browser/profiles/k3p7x2qa/crashes"),
            runtime: Some(PathBuf::from("/run/u/bingux-browser/k3p7x2qa")),
        }
    }

    #[test]
    fn a_locale_becomes_a_bcp47_tag() {
        assert_eq!(locale_tag("en_GB.UTF-8"), "en-GB");
        assert_eq!(locale_tag("de_DE@euro"), "de-DE");
        assert_eq!(locale_tag("C"), "en-US");
        assert_eq!(locale_tag(""), "en-US");
    }

    #[test]
    fn the_folders_the_host_names_are_the_engines() {
        let c = config(host(), &[("HOME", "/h"), ("XDG_DATA_HOME", "/elsewhere")]);
        assert_eq!(
            c.profile,
            Path::new("/h/.local/share/bingux-browser/profiles/k3p7x2qa/engine"),
            "the host's profile, not one made up from the XDG folders"
        );
        assert_eq!(
            c.cache.as_deref(),
            Some(Path::new("/h/.cache/bingux-browser/profiles/k3p7x2qa"))
        );
        assert_eq!(
            c.crash.as_deref(),
            Some(Path::new(
                "/h/.local/state/bingux-browser/profiles/k3p7x2qa/crashes"
            ))
        );
        assert_eq!(
            c.runtime.as_deref(),
            Some(Path::new("/run/u/bingux-browser/k3p7x2qa"))
        );
        assert!(!c.ephemeral);
        assert_eq!(c.downloads, Path::new("/h/Downloads"));
        let settings = c.settings();
        assert_eq!(settings.profile_dir, c.profile);
        assert_eq!(
            settings.cache_dir,
            Path::new("/h/.cache/bingux-browser/profiles/k3p7x2qa")
        );
    }

    #[test]
    fn the_web_view_reads_no_profile_variable_of_its_own() {
        let c = config(host(), &[("BINGUX_BROWSER_PROFILE", "/tmp/p")]);
        assert!(
            c.profile.starts_with("/h/.local/share"),
            "the host decides what BINGUX_BROWSER_PROFILE means"
        );
    }

    #[test]
    fn without_a_runtime_folder_the_sockets_stay_below_the_profile() {
        let folders = EngineFolders::Profile {
            profile: PathBuf::from("/tmp/p/engine"),
            cache: PathBuf::from("/tmp/p/cache"),
            crash: PathBuf::from("/tmp/p/crashes"),
            runtime: None,
        };
        let c = config(folders, &[("HOME", "/h")]);
        assert_eq!(c.runtime, None);
        assert_eq!(c.directories().runtime, None);
    }

    #[test]
    fn a_private_window_has_a_profile_of_its_own_that_goes_with_it() {
        let c = config(
            EngineFolders::Private,
            &[("HOME", "/home/a"), ("XDG_RUNTIME_DIR", "/run/user/1")],
        );
        assert!(c.ephemeral);
        assert!(
            c.profile.starts_with("/run/user/1")
                && c.profile
                    .to_string_lossy()
                    .contains("bingux-browser-private-")
        );
        assert_eq!(
            (c.cache, c.crash, c.runtime),
            (None, None, None),
            "its dumps and cache go with it"
        );
    }

    #[test]
    fn remote_settings_follow_the_choice_and_the_mirror() {
        assert_eq!(
            config(host(), &[]).remote_settings,
            RemoteSettings::MozillaDirect
        );
        assert_eq!(
            config(host(), &[("BINGUX_BROWSER_REMOTE_SETTINGS", "off")]).remote_settings,
            RemoteSettings::Off
        );
        let mirrored = config(
            host(),
            &[(
                "BINGUX_BROWSER_REMOTE_SETTINGS_SERVER",
                "https://rs.example/v1",
            )],
        );
        assert_eq!(mirrored.remote_settings, RemoteSettings::SecurityOnly);
        assert_eq!(
            mirrored.remote_settings_choice(),
            EngineRemoteSettings::SecurityOnly {
                server: "https://rs.example/v1".to_string()
            }
        );
        let asked_without_mirror = Config {
            remote_settings: RemoteSettings::SecurityOnly,
            remote_settings_server: None,
            ..config(host(), &[])
        };
        assert_eq!(
            asked_without_mirror.remote_settings_choice(),
            EngineRemoteSettings::MozillaDirect
        );
    }

    #[test]
    fn the_engine_names_itself_firefox_and_the_browser_for_everything_else() {
        let c = config(host(), &[("LANG", "de_DE.UTF-8")]);
        let identity = c.identity();
        assert_eq!(
            identity.ua_name.as_deref(),
            Some("Firefox"),
            "sites see the user agent of Firefox"
        );
        assert_eq!(identity.user_agent_suffix, None, "and nothing appended");
        assert_eq!(identity.remoting_name.as_deref(), Some("bingux-browser"));
        assert_eq!(c.builder().config().locales.locales, ["de-DE"]);
    }

    #[test]
    fn the_directories_reach_the_engine_configuration() {
        let c = config(host(), &[("HOME", "/h"), ("XDG_RUNTIME_DIR", "/run/u")]);
        let directories = c.directories();
        assert_eq!(
            directories.crash.as_deref(),
            Some(Path::new(
                "/h/.local/state/bingux-browser/profiles/k3p7x2qa/crashes"
            ))
        );
        assert_eq!(
            directories.cache.as_deref(),
            Some(Path::new("/h/.cache/bingux-browser/profiles/k3p7x2qa"))
        );
        assert_eq!(
            directories.runtime.as_deref(),
            Some(Path::new("/run/u/bingux-browser/k3p7x2qa"))
        );
        assert_eq!(
            directories.download.as_deref(),
            Some(Path::new("/h/Downloads"))
        );
    }

    #[test]
    fn a_folder_pages_may_read_never_overlaps_a_folder_of_the_engines_own() {
        // The engine refuses to start on such an overlap (it did, once the
        // first run had made ~/Downloads): the folder is left out instead.
        let c = config(
            host(),
            &[
                ("HOME", "/h"),
                (
                    "BINGUX_BROWSER_FILE_DIRS",
                    "/h/pages:/h/Downloads:/h/Downloads/sub:/h:/h/.cache/bingux-browser",
                ),
            ],
        );
        assert_eq!(
            c.read_dirs,
            [Path::new("/h/pages")],
            "the download folder, the home holding the profile and the cache are not pages' to read"
        );
    }

    #[test]
    fn the_webrtc_policy_is_the_hosts_choice_and_the_page_says_the_engine_keeps_its_own() {
        assert_eq!(config(host(), &[]).webrtc, None);
        let c = config(host(), &[("BINGUX_BROWSER_WEBRTC_IP_POLICY", "relay-only")]);
        assert_eq!(c.webrtc, Some(WebrtcChoice::RelayOnly));
        let said = c.settings().webrtc_ip_policy;
        assert!(
            said.contains("relay") && said.contains("keeps its own fixed policy"),
            "{said}"
        );
        assert!(
            config(host(), &[])
                .settings()
                .webrtc_ip_policy
                .contains("fixed policy")
        );
        assert_eq!(
            config(host(), &[("BINGUX_BROWSER_WEBRTC_IP_POLICY", "anything")]).webrtc,
            None,
            "an unknown word is no choice"
        );
    }
}
