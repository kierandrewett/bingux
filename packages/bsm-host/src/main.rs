//! `bsm-host`: runs as the user inside a hosted session, next to
//! `gnoblin --hosted`, and connects Gnoblin to bsm. Exit status is documented
//! in `docs/bsm/bridge.md`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use bsm_host::bridge::{self, Config, Exit};
use bsm_proto::paths::{ENV_GENERATION, ENV_HOST_SOCKET, ENV_SESSION_ID};

const USAGE: &str = "usage: bsm-host [--background-fps N] [--recovery-window SECS] [--version] [--help]
Runs inside a hosted bsm session. Environment:
  BSM_HOST_SOCKET   path of the brokered bsm socket (required)
  BSM_SESSION_ID    session id assigned by bsm (checked against bsm's identity event)
  BSM_GENERATION    generation assigned by bsm (checked likewise)
  XDG_RUNTIME_DIR   the session's runtime directory (required)
  DBUS_SESSION_BUS_ADDRESS  the session bus of the hosted Gnoblin (required)
  BSM_HOST_RECOVERY_SECS    how long to keep reconnecting after bsm went away (default 120)
  BSM_HOST_READY_PEERS      comma list of executable names allowed on ready.sock";

fn usage_error(msg: &str) -> ExitCode {
    eprintln!("bsm-host: {msg}\n{USAGE}");
    ExitCode::from(Exit::Usage as u8)
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let mut background_fps: f64 = 10.0;
    let mut recovery_secs: f64 = std::env::var("BSM_HOST_RECOVERY_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|v: &f64| *v >= 0.0 && *v <= 3600.0)
        .unwrap_or(120.0);
    #[allow(unused_mut)]
    let mut test: Option<bridge::TestMode> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--help" | "-h" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--version" => {
                println!("bsm-host {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--background-fps" => match args.next().and_then(|v| v.parse::<f64>().ok()).filter(|v| *v > 0.0 && *v <= 60.0) {
                Some(v) => background_fps = v,
                None => return usage_error("--background-fps needs a number in (0, 60]"),
            },
            "--recovery-window" => match args.next().and_then(|v| v.parse::<f64>().ok()).filter(|v| *v >= 0.0 && *v <= 3600.0) {
                Some(v) => recovery_secs = v,
                None => return usage_error("--recovery-window needs seconds in [0, 3600]"),
            },
            #[cfg(feature = "test-pattern")]
            "--test-pattern" => {
                test = Some(bridge::TestMode { auto_ready: None, input_log: None });
            }
            #[cfg(feature = "test-pattern")]
            "--test-auto-ready-ms" => match (args.next().and_then(|v| v.parse::<u64>().ok()), test.as_mut()) {
                (Some(ms), Some(t)) => t.auto_ready = Some(Duration::from_millis(ms)),
                _ => return usage_error("--test-auto-ready-ms N must follow --test-pattern"),
            },
            #[cfg(feature = "test-pattern")]
            "--test-input-log" => match (args.next(), test.as_mut()) {
                (Some(p), Some(t)) => t.input_log = Some(PathBuf::from(p)),
                _ => return usage_error("--test-input-log PATH must follow --test-pattern"),
            },
            other => return usage_error(&format!("unknown argument {other:?}")),
        }
    }

    let Some(host_socket) = std::env::var_os(ENV_HOST_SOCKET).map(PathBuf::from) else {
        return usage_error(&format!("{ENV_HOST_SOCKET} is not set"));
    };
    let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) else {
        return usage_error("XDG_RUNTIME_DIR is not set");
    };
    if test.is_none() && std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
        return usage_error("DBUS_SESSION_BUS_ADDRESS is not set");
    }

    let cfg = Config {
        host_socket,
        runtime_dir,
        env_generation: std::env::var(ENV_GENERATION).ok(),
        env_session_id: std::env::var(ENV_SESSION_ID).ok(),
        background_interval: Duration::from_secs_f64(1.0 / background_fps),
        ready_peers: std::env::var("BSM_HOST_READY_PEERS")
            .map(|v| v.split(',').map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect())
            .unwrap_or_else(|_| bsm_host::ready_sock::DEFAULT_PEERS.iter().map(|s| s.to_string()).collect()),
        ready_idle: std::env::var("BSM_HOST_READY_IDLE_SECS")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| *v > 0.0 && *v <= 600.0)
            .map_or(bsm_host::ready_sock::IDLE_TIMEOUT, Duration::from_secs_f64),
        handshake_timeout: std::env::var("BSM_HOST_HANDSHAKE_SECS")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| *v > 0.0 && *v <= 600.0)
            .map_or(Duration::from_secs(10), Duration::from_secs_f64),
        recovery_window: Duration::from_secs_f64(recovery_secs),
        dock_enabled: bsm_host::config::read_dock_enabled(),
        test,
    };
    match bridge::run(cfg) {
        Ok(exit) => {
            log::info!("exiting with status {}", exit as u8);
            ExitCode::from(exit as u8)
        }
        Err(e) => {
            log::error!("{e:#}");
            ExitCode::from(Exit::Internal as u8)
        }
    }
}
