//! bingux-gated — the Bingux permission daemon.
//!
//! In production this daemon runs as root, receives seccomp
//! notifications from sandboxed processes, and consults the per-user
//! permission database to decide whether to allow or deny each
//! trapped syscall.
//!
//! For testing and development, the `--mock` flag replays a small set
//! of synthetic events instead of reading from a real seccomp fd.

use std::path::PathBuf;
use std::time::SystemTime;

use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

use bingux_gated::daemon::GatedDaemon;
use bingux_gated::decoder::{SYS_CONNECT, SYS_EXECVE, SYS_OPENAT};
use bingux_gated::decoder::SyscallEvent;
use bingux_gated::listener::SeccompListener;
use bingux_gated::prompt::{MockPrompter, PromptResponse, TtyPrompter};
use bingux_gated::registry::SandboxEntry;

use bingux_common::package_id::Arch;
use bingux_common::PackageId;

// ── CLI ──────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "bingux-gated", about = "Bingux permission daemon")]
struct Cli {
    /// Run with synthetic mock events instead of a real seccomp fd.
    #[arg(long)]
    mock: bool,

    /// Prompt policy: "tty" for interactive TTY prompts, "auto-allow"
    /// to allow everything, "auto-deny" to deny everything.
    #[arg(long, default_value = "auto-deny")]
    prompt: PromptPolicy,

    /// Base path for permission TOML files.
    #[arg(long, default_value = "/var/lib/bingux/permissions")]
    permissions_dir: PathBuf,
}

#[derive(Clone, Debug)]
enum PromptPolicy {
    Tty,
    AutoAllow,
    AutoDeny,
}

impl std::str::FromStr for PromptPolicy {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "tty" => Ok(Self::Tty),
            "auto-allow" => Ok(Self::AutoAllow),
            "auto-deny" => Ok(Self::AutoDeny),
            other => Err(format!("unknown prompt policy: {other:?} (expected tty, auto-allow, auto-deny)")),
        }
    }
}

// ── Main ─────────────────────────────────────────────────────────

fn main() {
    // Initialise tracing (controlled by RUST_LOG, default info)
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // Build the prompt backend
    let prompter: Box<dyn bingux_gated::prompt::PromptBackend> = match cli.prompt {
        PromptPolicy::Tty => Box::new(TtyPrompter),
        PromptPolicy::AutoAllow => Box::new(MockPrompter::new(PromptResponse::AllowOnce)),
        PromptPolicy::AutoDeny => Box::new(MockPrompter::new(PromptResponse::Deny)),
    };

    // Build the daemon
    let mut daemon = GatedDaemon::with_base_path(prompter, cli.permissions_dir);

    if cli.mock {
        run_mock(&mut daemon);
    } else {
        info!("real seccomp listener not yet implemented — use --mock for testing");
        std::process::exit(1);
    }
}

/// Run the daemon with a mock notification source containing some
/// synthetic events.  Registers a fake sandbox entry so the PID
/// lookups succeed.
fn run_mock(daemon: &mut GatedDaemon) {
    info!("running with mock notification source");

    // Register a fake sandbox process
    let pid = 42;
    daemon.registry.register(
        pid,
        SandboxEntry {
            package_name: "demo-app".to_string(),
            package_id: PackageId::new("demo-app", "1.0", Arch::X86_64Linux)
                .expect("valid package id"),
            user: "testuser".to_string(),
            uid: 1000,
            listener_fd: None,
            started_at: SystemTime::now(),
        },
    );

    // Build a set of synthetic events
    let events = vec![
        // File read
        SyscallEvent {
            pid,
            syscall_nr: SYS_OPENAT,
            args: [0, 0x1000, 0, 0, 0, 0], // O_RDONLY
        },
        // Network connect
        SyscallEvent {
            pid,
            syscall_nr: SYS_CONNECT,
            args: [3, 0x2000, 16, 0, 0, 0],
        },
        // Exec
        SyscallEvent {
            pid,
            syscall_nr: SYS_EXECVE,
            args: [0x3000, 0, 0, 0, 0, 0],
        },
        // Unknown PID (should be handled gracefully)
        SyscallEvent {
            pid: 9999,
            syscall_nr: SYS_OPENAT,
            args: [0, 0x4000, 0, 0, 0, 0],
        },
    ];

    let mut listener = SeccompListener::new_mock(events);
    match listener.run(daemon) {
        Ok(count) => info!(count, "mock run complete"),
        Err(e) => {
            tracing::error!(%e, "mock run failed");
            std::process::exit(1);
        }
    }
}
