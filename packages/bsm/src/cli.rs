//! Command line of `bsm`. Hand parsed: the surface is small and a parse error must be a
//! plain sentence, not a framework's usage dump.

use std::path::PathBuf;

use bsm_compositor::Backend;

pub const HELP: &str = "\
bsm - the Bingux Session Manager: one persistent system compositor and session manager

USAGE:
    bsm [OPTIONS]

OPTIONS:
    --backend <drm|winit|headless>   Display backend (default: drm, or `backend` in the config)
    --config <PATH>                  Configuration file (default: /etc/bsm/bsm.toml; must exist if given)
    --runtime-dir <DIR>              Runtime directory (default: /run/bsm)
    --state-dir <DIR>                State directory (default: /var/lib/bsm)
    --helper-socket <PATH>           bsm-helper socket (default: /run/bsm-helper/helper.sock)
    --dbus <system|session|none|ADDR>  Where to publish org.bingux.Session1 (default: system)
    --system-services <logind|none>  polkit, inhibitors and power through the system bus (default: logind;
                                     `none` refuses every power action: development and tests only)
    --outputs <WxH[,WxH...]>         Headless backend: fake outputs side by side (default: 1280x720)
    --frame-dump <DIR>               Headless backend: write every composed frame as a PNG
    --print-config                   Print the resolved settings and exit
    --version                        Print the version and exit
    --help                           Print this text and exit

Development: `bsm --backend winit` runs inside an existing Wayland desktop and
`bsm --backend headless` runs without any display. See docs/bsm/manager.md.
";

#[derive(Debug, Clone, PartialEq)]
pub enum DbusMode {
    System,
    Session,
    None,
    Address(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cli {
    pub backend: Option<String>,
    pub config: Option<PathBuf>,
    pub runtime_dir: Option<PathBuf>,
    pub state_dir: Option<PathBuf>,
    pub helper_socket: Option<PathBuf>,
    pub dbus: Option<String>,
    pub system_services: Option<String>,
    pub outputs: Option<Vec<(u32, u32)>>,
    pub frame_dump: Option<PathBuf>,
    pub print_config: bool,
}

#[derive(Debug, PartialEq)]
pub enum Parsed {
    Run(Cli),
    Help,
    Version,
}

pub fn parse_backend(s: &str) -> Result<Backend, String> {
    match s {
        "drm" => Ok(Backend::Drm),
        "winit" => Ok(Backend::Winit),
        "headless" => Ok(Backend::Headless),
        other => Err(format!("unknown backend {other:?}; use drm, winit or headless")),
    }
}

pub fn parse_dbus(s: &str) -> Result<DbusMode, String> {
    match s {
        "system" => Ok(DbusMode::System),
        "session" => Ok(DbusMode::Session),
        "none" => Ok(DbusMode::None),
        a if a.contains('=') || a.starts_with("unix:") => Ok(DbusMode::Address(a.to_owned())),
        other => Err(format!("unknown bus {other:?}; use system, session, none or a bus address")),
    }
}

pub fn parse_outputs(s: &str) -> Result<Vec<(u32, u32)>, String> {
    let mut v = Vec::new();
    for part in s.split(',') {
        let (w, h) = part.split_once('x').ok_or_else(|| format!("output {part:?} must look like 1280x720"))?;
        let w: u32 = w.parse().map_err(|_| format!("bad width in {part:?}"))?;
        let h: u32 = h.parse().map_err(|_| format!("bad height in {part:?}"))?;
        if !(16..=16384).contains(&w) || !(16..=16384).contains(&h) {
            return Err(format!("output {part:?} is out of range"));
        }
        v.push((w, h));
    }
    if v.is_empty() || v.len() > 8 {
        return Err("between 1 and 8 outputs".into());
    }
    Ok(v)
}

pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Parsed, String> {
    let mut it = args.into_iter();
    let mut cli = Cli::default();
    while let Some(a) = it.next() {
        let (name, inline) = match a.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n.to_owned(), Some(v.to_owned())),
            _ => (a.clone(), None),
        };
        let mut value = |what: &str| -> Result<String, String> {
            inline.clone().or_else(|| it.next()).ok_or_else(|| format!("{name} needs {what}"))
        };
        match name.as_str() {
            "--help" | "-h" => return Ok(Parsed::Help),
            "--version" | "-V" => return Ok(Parsed::Version),
            "--print-config" => cli.print_config = true,
            "--backend" => {
                let v = value("a backend")?;
                parse_backend(&v)?;
                cli.backend = Some(v);
            }
            "--config" => cli.config = Some(value("a path")?.into()),
            "--runtime-dir" => cli.runtime_dir = Some(value("a directory")?.into()),
            "--state-dir" => cli.state_dir = Some(value("a directory")?.into()),
            "--helper-socket" => cli.helper_socket = Some(value("a path")?.into()),
            "--dbus" => {
                let v = value("a bus")?;
                parse_dbus(&v)?;
                cli.dbus = Some(v);
            }
            "--system-services" => {
                let v = value("logind or none")?;
                if v != "logind" && v != "none" {
                    return Err(format!("unknown system services {v:?}; use logind or none"));
                }
                cli.system_services = Some(v);
            }
            "--outputs" => cli.outputs = Some(parse_outputs(&value("a list like 1280x720")?)?),
            "--frame-dump" => cli.frame_dump = Some(value("a directory")?.into()),
            other => return Err(format!("unknown option {other:?}; try --help")),
        }
    }
    Ok(Parsed::Run(cli))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Parsed, String> {
        parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn flags_parse_in_both_spellings() {
        let Parsed::Run(c) = p(&["--backend", "headless", "--outputs=800x600,1024x768", "--dbus", "none"]).unwrap() else { panic!() };
        assert_eq!(c.backend.as_deref(), Some("headless"));
        assert_eq!(c.outputs, Some(vec![(800, 600), (1024, 768)]));
        assert_eq!(c.dbus.as_deref(), Some("none"));
    }

    #[test]
    fn version_and_help_win() {
        assert_eq!(p(&["--version"]).unwrap(), Parsed::Version);
        assert_eq!(p(&["--backend", "drm", "--help"]).unwrap(), Parsed::Help);
    }

    #[test]
    fn mistakes_are_sentences() {
        assert!(p(&["--backend", "x11"]).unwrap_err().contains("unknown backend"));
        assert!(p(&["--frobnicate"]).unwrap_err().contains("unknown option"));
        assert!(p(&["--outputs", "1280"]).unwrap_err().contains("1280x720"));
        assert!(p(&["--outputs", "1x1"]).unwrap_err().contains("out of range"));
        assert!(p(&["--backend"]).unwrap_err().contains("needs"));
    }

    #[test]
    fn dbus_modes() {
        assert_eq!(parse_dbus("system").unwrap(), DbusMode::System);
        assert_eq!(parse_dbus("unix:path=/tmp/x").unwrap(), DbusMode::Address("unix:path=/tmp/x".into()));
        assert!(parse_dbus("maybe").is_err());
    }
}
