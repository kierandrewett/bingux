//! Core daemon — ties the permission database, PID registry, syscall
//! decoder, and prompt backend together into a single event handler.
//!
//! The actual seccomp listener fd event loop is Linux-specific and
//! requires root; this module provides the decision logic that sits
//! behind it.

use std::collections::HashMap;
use std::path::PathBuf;

use tracing::{debug, info};

use crate::decoder::{self, FileAccessFlags, PermissionRequest, SyscallEvent};
use crate::error::{GatedError, Result};
use crate::permissions::{PermissionDb, PermissionGrant};
use crate::prompt::{PromptBackend, PromptRequest, PromptResponse};
use crate::registry::PidRegistry;

// ── Response type ─────────────────────────────────────────────────

/// What to tell the seccomp listener about a trapped syscall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventResponse {
    /// Allow the syscall to proceed.
    Continue,
    /// Deny the syscall, returning the given errno (e.g. `libc::EACCES`).
    Deny(i32),
}

/// Standard errno for "permission denied".
pub const EACCES: i32 = 13;

// ── Daemon ────────────────────────────────────────────────────────

/// The main permission daemon.
pub struct GatedDaemon {
    pub registry: PidRegistry,
    pub prompter: Box<dyn PromptBackend>,
    /// One PermissionDb per user (keyed by username).
    permission_dbs: HashMap<String, PermissionDb>,
    /// Base path for permission TOML files.  In production this is
    /// `~/.config/bingux/permissions/` per user, but for the daemon
    /// (which runs as root) we store all users' DBs in a shared tree.
    permissions_base: PathBuf,
    /// Monotonically increasing prompt ID.
    next_prompt_id: u64,
}

impl GatedDaemon {
    pub fn new(prompter: Box<dyn PromptBackend>) -> Self {
        Self::with_base_path(prompter, PathBuf::from("/var/lib/bingux/permissions"))
    }

    /// Create a daemon with a custom permissions base path (useful for
    /// testing).
    pub fn with_base_path(prompter: Box<dyn PromptBackend>, base: PathBuf) -> Self {
        Self {
            registry: PidRegistry::new(),
            prompter,
            permission_dbs: HashMap::new(),
            permissions_base: base,
            next_prompt_id: 1,
        }
    }

    /// Get or create the PermissionDb for a user.
    fn db_for_user(&mut self, user: &str) -> &mut PermissionDb {
        let base = self.permissions_base.join(user);
        self.permission_dbs
            .entry(user.to_string())
            .or_insert_with(|| PermissionDb::new(user, base))
    }

    /// Handle a single seccomp notification event.
    ///
    /// Flow:
    /// 1. Look up PID in the registry → package + user
    /// 2. Decode the syscall → PermissionRequest
    /// 3. Check the permission DB
    /// 4. Allow → `EventResponse::Continue`
    /// 5. Deny → `EventResponse::Deny(EACCES)`
    /// 6. Prompt → ask user, persist if AlwaysAllow, return response
    pub fn handle_event(&mut self, event: SyscallEvent) -> Result<EventResponse> {
        // 1. PID lookup
        let entry = self
            .registry
            .lookup(event.pid)
            .ok_or(GatedError::PidNotFound { pid: event.pid })?;
        let package = entry.package_name.clone();
        let user = entry.user.clone();

        debug!(pid = event.pid, %package, syscall = event.syscall_nr, "handling event");

        // 2. Decode
        let request = decoder::decode_syscall(&event)?;

        // 3. Check permission
        let grant = self.check_permission(&user, &package, &request);

        match grant {
            PermissionGrant::Allow => {
                debug!(%package, "permission allowed");
                Ok(EventResponse::Continue)
            }
            PermissionGrant::Deny => {
                debug!(%package, "permission denied");
                Ok(EventResponse::Deny(EACCES))
            }
            PermissionGrant::Prompt => {
                info!(%package, "prompting user for permission");
                self.prompt_user(&user, &package, &request)
            }
        }
    }

    /// Map a decoded PermissionRequest to a PermissionGrant by querying
    /// the database.
    fn check_permission(
        &mut self,
        user: &str,
        package: &str,
        request: &PermissionRequest,
    ) -> PermissionGrant {
        match request {
            PermissionRequest::FileAccess { path, .. } => {
                // Check file-level first, then fall back to mount-level
                let db = self.db_for_user(user);
                let file_grant = db.check_file(package, path);
                if file_grant != PermissionGrant::Prompt {
                    return file_grant;
                }
                // Check if any mount prefix covers this path
                // (simplified — real impl would do proper prefix matching)
                file_grant
            }
            PermissionRequest::NetworkConnect { .. } | PermissionRequest::NetworkBind { .. } => {
                self.db_for_user(user)
                    .check_capability(package, "net_outbound")
            }
            PermissionRequest::NetworkListen { .. } => {
                self.db_for_user(user)
                    .check_capability(package, "net_inbound")
            }
            PermissionRequest::DeviceAccess { category, .. } => {
                self.db_for_user(user).check_capability(package, category)
            }
            PermissionRequest::ProcessExec { .. } => {
                self.db_for_user(user).check_capability(package, "exec")
            }
            PermissionRequest::ProcessPtrace { .. } => {
                self.db_for_user(user).check_capability(package, "ptrace")
            }
            PermissionRequest::Mount { .. } => {
                self.db_for_user(user).check_capability(package, "mount")
            }
        }
    }

    /// Send a prompt to the user and handle their response.
    fn prompt_user(
        &mut self,
        user: &str,
        package: &str,
        request: &PermissionRequest,
    ) -> Result<EventResponse> {
        let (resource_type, resource_detail, is_dangerous, capability) =
            classify_request(request);

        let prompt_id = self.next_prompt_id;
        self.next_prompt_id += 1;

        let prompt = PromptRequest {
            id: prompt_id,
            package_name: package.to_string(),
            package_icon: None,
            resource_type: resource_type.to_string(),
            resource_detail,
            is_dangerous,
        };

        let response = self.prompter.prompt(prompt)?;

        match response {
            PromptResponse::Deny => Ok(EventResponse::Deny(EACCES)),
            PromptResponse::AllowOnce => Ok(EventResponse::Continue),
            PromptResponse::AlwaysAllow => {
                // Persist the grant
                if let Some(cap) = capability {
                    self.db_for_user(user).grant_capability(package, &cap)?;
                } else if let PermissionRequest::FileAccess { path, flags } = request {
                    let perm = file_flags_to_string(*flags);
                    self.db_for_user(user).grant_file(package, path, &perm)?;
                }
                Ok(EventResponse::Continue)
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────

/// Classify a PermissionRequest into prompt metadata.
///
/// Returns `(resource_type, resource_detail, is_dangerous, capability_name)`.
/// `capability_name` is `Some` if the grant should be stored as a
/// capability rather than a file/mount entry.
fn classify_request(
    request: &PermissionRequest,
) -> (&'static str, String, bool, Option<String>) {
    match request {
        PermissionRequest::FileAccess { path, flags } => {
            let detail = format!("{path} ({})", file_flags_to_string(*flags));
            ("file", detail, false, None)
        }
        PermissionRequest::NetworkConnect { addr, port } => (
            "network",
            format!("connect to {addr}:{port}"),
            false,
            Some("net_outbound".to_string()),
        ),
        PermissionRequest::NetworkBind { addr, port } => (
            "network",
            format!("bind {addr}:{port}"),
            false,
            Some("net_outbound".to_string()),
        ),
        PermissionRequest::NetworkListen { fd } => (
            "network",
            format!("listen on fd {fd}"),
            false,
            Some("net_inbound".to_string()),
        ),
        PermissionRequest::DeviceAccess { device, category } => (
            "device",
            format!("{device} ({category})"),
            false,
            Some(category.clone()),
        ),
        PermissionRequest::ProcessExec { path } => (
            "exec",
            format!("execute {path}"),
            true,
            Some("exec".to_string()),
        ),
        PermissionRequest::ProcessPtrace { target_pid } => (
            "ptrace",
            format!("ptrace pid {target_pid}"),
            true,
            Some("ptrace".to_string()),
        ),
        PermissionRequest::Mount { source, target } => (
            "mount",
            format!("mount {source} → {target}"),
            true,
            Some("mount".to_string()),
        ),
    }
}

fn file_flags_to_string(flags: FileAccessFlags) -> String {
    let mut s = String::new();
    if flags.contains(FileAccessFlags::READ) {
        s.push('r');
    }
    if flags.contains(FileAccessFlags::WRITE) {
        s.push('w');
    }
    if flags.contains(FileAccessFlags::LIST) {
        if !s.is_empty() {
            s.push(',');
        }
        s.push_str("list");
    }
    if s.is_empty() {
        s.push_str("none");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::{SYS_CONNECT, SYS_OPENAT};
    use crate::prompt::MockPrompter;
    use crate::registry::SandboxEntry;
    use bingux_common::package_id::Arch;
    use bingux_common::PackageId;
    use std::time::SystemTime;

    fn setup_daemon(response: PromptResponse, base: PathBuf) -> GatedDaemon {
        let prompter = Box::new(MockPrompter::new(response));
        let mut daemon = GatedDaemon::with_base_path(prompter, base);
        daemon.registry.register(
            100,
            SandboxEntry {
                package_name: "firefox".to_string(),
                package_id: PackageId::new("firefox", "128.0", Arch::X86_64Linux).unwrap(),
                user: "alice".to_string(),
                uid: 1000,
                listener_fd: None,
                started_at: SystemTime::now(),
            },
        );
        daemon
    }

    #[test]
    fn handle_event_pid_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = setup_daemon(PromptResponse::Deny, dir.path().to_path_buf());
        let event = SyscallEvent {
            pid: 999, // not registered
            syscall_nr: SYS_OPENAT,
            args: [0, 0x1000, 0, 0, 0, 0],
        };
        assert!(daemon.handle_event(event).is_err());
    }

    #[test]
    fn handle_event_capability_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = setup_daemon(PromptResponse::Deny, dir.path().to_path_buf());

        // Pre-grant net_outbound
        daemon
            .db_for_user("alice")
            .grant_capability("firefox", "net_outbound")
            .unwrap();

        let event = SyscallEvent {
            pid: 100,
            syscall_nr: SYS_CONNECT,
            args: [3, 0x2000, 16, 0, 0, 0],
        };
        let resp = daemon.handle_event(event).unwrap();
        assert_eq!(resp, EventResponse::Continue);
    }

    #[test]
    fn handle_event_capability_denied() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = setup_daemon(PromptResponse::Deny, dir.path().to_path_buf());

        // Pre-deny net_outbound
        daemon
            .db_for_user("alice")
            .deny_capability("firefox", "net_outbound")
            .unwrap();

        let event = SyscallEvent {
            pid: 100,
            syscall_nr: SYS_CONNECT,
            args: [3, 0x2000, 16, 0, 0, 0],
        };
        let resp = daemon.handle_event(event).unwrap();
        assert_eq!(resp, EventResponse::Deny(EACCES));
    }

    #[test]
    fn handle_event_prompts_user_deny() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = setup_daemon(PromptResponse::Deny, dir.path().to_path_buf());

        // No pre-existing permission → triggers prompt → MockPrompter returns Deny
        let event = SyscallEvent {
            pid: 100,
            syscall_nr: SYS_CONNECT,
            args: [3, 0x2000, 16, 0, 0, 0],
        };
        let resp = daemon.handle_event(event).unwrap();
        assert_eq!(resp, EventResponse::Deny(EACCES));
    }

    #[test]
    fn handle_event_prompts_user_allow_once() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = setup_daemon(PromptResponse::AllowOnce, dir.path().to_path_buf());

        let event = SyscallEvent {
            pid: 100,
            syscall_nr: SYS_CONNECT,
            args: [3, 0x2000, 16, 0, 0, 0],
        };
        let resp = daemon.handle_event(event).unwrap();
        assert_eq!(resp, EventResponse::Continue);

        // Should still prompt next time (not persisted)
        assert_eq!(
            daemon
                .db_for_user("alice")
                .check_capability("firefox", "net_outbound"),
            PermissionGrant::Prompt,
        );
    }

    #[test]
    fn handle_event_prompts_user_always_allow_persists() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = setup_daemon(PromptResponse::AlwaysAllow, dir.path().to_path_buf());

        let event = SyscallEvent {
            pid: 100,
            syscall_nr: SYS_CONNECT,
            args: [3, 0x2000, 16, 0, 0, 0],
        };
        let resp = daemon.handle_event(event).unwrap();
        assert_eq!(resp, EventResponse::Continue);

        // Should now be persisted
        assert_eq!(
            daemon
                .db_for_user("alice")
                .check_capability("firefox", "net_outbound"),
            PermissionGrant::Allow,
        );
    }

    #[test]
    fn dangerous_request_is_flagged() {
        // ptrace is classified as dangerous
        let (_, _, is_dangerous, _) = classify_request(&PermissionRequest::ProcessPtrace {
            target_pid: 200,
        });
        assert!(is_dangerous);

        // File access is not dangerous
        let (_, _, is_dangerous, _) = classify_request(&PermissionRequest::FileAccess {
            path: "/tmp/foo".to_string(),
            flags: FileAccessFlags::READ,
        });
        assert!(!is_dangerous);
    }

    #[test]
    fn file_access_check_allow() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = setup_daemon(PromptResponse::Deny, dir.path().to_path_buf());

        daemon
            .db_for_user("alice")
            .grant_file("firefox", "~/Downloads/file.pdf", "r")
            .unwrap();

        // The event decodes to FileAccess with a stubbed path, so let's
        // test via check_permission directly.
        let grant = daemon.check_permission(
            "alice",
            "firefox",
            &PermissionRequest::FileAccess {
                path: "~/Downloads/file.pdf".to_string(),
                flags: FileAccessFlags::READ,
            },
        );
        assert_eq!(grant, PermissionGrant::Allow);
    }

    #[test]
    fn file_access_check_deny() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = setup_daemon(PromptResponse::Deny, dir.path().to_path_buf());

        daemon
            .db_for_user("alice")
            .grant_file("firefox", "~/.ssh/id_rsa", "deny(r)")
            .unwrap();

        let grant = daemon.check_permission(
            "alice",
            "firefox",
            &PermissionRequest::FileAccess {
                path: "~/.ssh/id_rsa".to_string(),
                flags: FileAccessFlags::READ,
            },
        );
        assert_eq!(grant, PermissionGrant::Deny);
    }

    #[test]
    fn file_flags_to_string_combinations() {
        assert_eq!(file_flags_to_string(FileAccessFlags::READ), "r");
        assert_eq!(file_flags_to_string(FileAccessFlags::WRITE), "w");
        assert_eq!(
            file_flags_to_string(FileAccessFlags::READ | FileAccessFlags::WRITE),
            "rw"
        );
        assert_eq!(
            file_flags_to_string(FileAccessFlags::READ | FileAccessFlags::LIST),
            "r,list"
        );
        assert_eq!(file_flags_to_string(FileAccessFlags::empty()), "none");
    }

    #[test]
    fn permission_keyed_by_name_not_version() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = setup_daemon(PromptResponse::Deny, dir.path().to_path_buf());

        // Grant for "firefox" (no version in the key)
        daemon
            .db_for_user("alice")
            .grant_capability("firefox", "gpu")
            .unwrap();

        // Register a second PID with a different version but same name
        daemon.registry.register(
            200,
            SandboxEntry {
                package_name: "firefox".to_string(),
                package_id: PackageId::new("firefox", "129.0", Arch::X86_64Linux).unwrap(),
                user: "alice".to_string(),
                uid: 1000,
                listener_fd: None,
                started_at: SystemTime::now(),
            },
        );

        // Both should see the grant (keyed by name, not version)
        assert_eq!(
            daemon
                .db_for_user("alice")
                .check_capability("firefox", "gpu"),
            PermissionGrant::Allow,
        );
    }

    #[test]
    fn mount_permission_check() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = setup_daemon(PromptResponse::Deny, dir.path().to_path_buf());

        daemon
            .db_for_user("alice")
            .grant_mount("firefox", "~/Downloads", "list,w")
            .unwrap();

        let result = daemon
            .db_for_user("alice")
            .check_mount("firefox", "~/Downloads");
        assert_eq!(result, Some("list,w".to_string()));

        let result = daemon
            .db_for_user("alice")
            .check_mount("firefox", "~/Documents");
        assert_eq!(result, None);
    }
}
