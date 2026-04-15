use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Configuration for launching a QEMU virtual machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchConfig {
    /// Path to the disk image (qcow2 or raw).
    pub image: PathBuf,

    /// Memory allocation, e.g. "2G", "4096M".
    #[serde(default = "default_memory")]
    pub memory: String,

    /// Number of virtual CPUs.
    #[serde(default = "default_cpus")]
    pub cpus: u32,

    /// Enable KVM hardware acceleration.
    #[serde(default = "default_kvm")]
    pub kvm: bool,

    /// Run headless with serial-only console (no VNC/display).
    #[serde(default)]
    pub serial_only: bool,

    /// Path to kernel image for direct kernel boot (bypasses disk boot).
    #[serde(default)]
    pub kernel: Option<PathBuf>,

    /// Path to initrd/initramfs image.
    #[serde(default)]
    pub initrd: Option<PathBuf>,

    /// Extra kernel command-line arguments.
    #[serde(default)]
    pub append: Option<String>,

    /// Enable virtio-gpu (for graphical compositor testing).
    #[serde(default)]
    pub virtio_gpu: bool,

    /// Enable VGA (std) alongside virtio-gpu for VT support.
    #[serde(default)]
    pub vga: bool,

    /// Extra QEMU arguments passed verbatim.
    #[serde(default)]
    pub extra_args: Vec<String>,
}

fn default_memory() -> String {
    "2G".to_string()
}

fn default_cpus() -> u32 {
    2
}

fn default_kvm() -> bool {
    true
}

impl Default for LaunchConfig {
    fn default() -> Self {
        Self {
            image: PathBuf::new(),
            memory: default_memory(),
            cpus: default_cpus(),
            kvm: default_kvm(),
            serial_only: false,
            kernel: None,
            initrd: None,
            append: None,
            virtio_gpu: false,
            vga: false,
            extra_args: Vec::new(),
        }
    }
}
