use std::path::PathBuf;

use bingux_common::PackageId;
use bxc_sandbox::SandboxLevel;
use serde::{Deserialize, Serialize};

/// Configuration for launching a sandboxed process.
///
/// Constructed by the CLI/daemon before handing off to the sandbox runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// The fully-qualified package identifier.
    pub package_id: PackageId,
    /// Full store path to the patchelf'd binary to execute.
    pub binary_path: PathBuf,
    /// Command-line arguments passed to the binary.
    pub args: Vec<String>,
    /// The sandbox isolation level.
    pub level: SandboxLevel,
    /// The username running the sandboxed process.
    pub user: String,
    /// UID of the user.
    pub uid: u32,
    /// GID of the user.
    pub gid: u32,
}
