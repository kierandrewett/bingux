use std::path::PathBuf;

use crate::config::LaunchConfig;
use crate::error::{Error, Result};

/// A running QEMU virtual machine instance.
pub struct QemuInstance {
    /// Unique identifier for this VM.
    pub vm_id: String,
    /// The child process handle.
    pub process: tokio::process::Child,
    /// Unix socket path for the serial console.
    pub serial_socket: PathBuf,
    /// Unix socket path for QMP (QEMU Machine Protocol).
    pub qmp_socket: PathBuf,
    /// Unix socket path for VNC display.
    pub vnc_socket: PathBuf,
    /// Working directory containing sockets and temporary files.
    pub work_dir: PathBuf,
}

impl QemuInstance {
    /// Launch a new QEMU VM with the given configuration.
    pub async fn launch(config: LaunchConfig) -> Result<Self> {
        let vm_id = format!("bingux-{}", std::process::id());
        let work_dir = std::env::temp_dir().join(format!("bingux-qemu-mcp-{}", &vm_id));
        tokio::fs::create_dir_all(&work_dir).await?;

        let serial_socket = work_dir.join("serial.sock");
        let qmp_socket = work_dir.join("qmp.sock");
        let vnc_socket = work_dir.join("vnc.sock");

        let mut cmd = tokio::process::Command::new("qemu-system-x86_64");

        // Disk image (optional if using kernel+initrd)
        if config.image.as_os_str().len() > 0 {
            let fmt = if config.image.extension().map_or(false, |e| e == "qcow2") {
                "qcow2"
            } else {
                "raw"
            };
            cmd.arg("-drive")
                .arg(format!(
                    "file={},format={fmt},if=virtio",
                    config.image.display()
                ));
        }

        // CD-ROM / ISO image
        if let Some(ref iso) = config.iso {
            cmd.arg("-cdrom").arg(iso);
        }

        // Kernel + initrd direct boot
        if let Some(ref kernel) = config.kernel {
            cmd.arg("-kernel").arg(kernel);
        }
        if let Some(ref initrd) = config.initrd {
            cmd.arg("-initrd").arg(initrd);
        }

        // Memory and CPUs
        cmd.arg("-m").arg(&config.memory);
        cmd.arg("-smp").arg(config.cpus.to_string());

        // KVM acceleration
        if config.kvm {
            cmd.arg("-enable-kvm").arg("-cpu").arg("host");
        }

        // GPU: virtio-gpu + optional VGA for VT support
        if config.virtio_gpu {
            if config.vga {
                cmd.arg("-vga").arg("std");
            }
            cmd.arg("-device").arg("virtio-gpu-pci,id=gpu1");
            cmd.arg("-device").arg("virtio-keyboard-pci");
            cmd.arg("-device").arg("virtio-mouse-pci");
        }

        // QMP socket
        cmd.arg("-qmp")
            .arg(format!("unix:{},server,nowait", qmp_socket.display()));

        // Serial console via unix socket
        cmd.arg("-serial")
            .arg(format!("unix:{},server,nowait", serial_socket.display()));

        // Display: VNC on unix socket
        if config.serial_only && !config.virtio_gpu {
            cmd.arg("-display").arg("none");
        } else {
            cmd.arg("-vnc")
                .arg(format!("unix:{}", vnc_socket.display()));
        }

        // No default NIC noise — use virtio-net
        cmd.arg("-nic").arg("user,model=virtio-net-pci");

        // Kernel command line
        let append = config.append.as_deref().unwrap_or("console=ttyS0 quiet");
        cmd.arg("-append").arg(append);

        // Extra QEMU arguments
        for arg in &config.extra_args {
            cmd.arg(arg);
        }

        // Daemonize: no, we manage the child process ourselves
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::piped());

        tracing::info!(vm_id = %vm_id, image = %config.image.display(), "launching QEMU");

        let process = cmd.spawn().map_err(|e| Error::QemuLaunchFailed(e.to_string()))?;

        Ok(Self {
            vm_id,
            process,
            serial_socket,
            qmp_socket,
            vnc_socket,
            work_dir,
        })
    }

    /// Stop the VM. If `force` is true, kill immediately; otherwise send
    /// a QMP `quit` command first.
    pub async fn stop(&mut self, force: bool) -> Result<()> {
        if force {
            tracing::info!(vm_id = %self.vm_id, "force-killing QEMU");
            self.process.kill().await?;
        } else {
            tracing::info!(vm_id = %self.vm_id, "attempting graceful shutdown");
            // Try to send QMP quit, fall back to kill
            // We use start_kill + wait with timeout as the graceful path
            // since the QMP quit will be handled at a higher level.
            match tokio::time::timeout(
                std::time::Duration::from_secs(10),
                self.process.wait(),
            )
            .await
            {
                Ok(_) => {}
                Err(_) => {
                    tracing::warn!(vm_id = %self.vm_id, "graceful shutdown timed out, killing");
                    self.process.kill().await?;
                }
            }
        }

        // Clean up work directory
        let _ = tokio::fs::remove_dir_all(&self.work_dir).await;

        Ok(())
    }

    /// Check whether the QEMU process is still running.
    pub fn is_running(&mut self) -> bool {
        self.process.try_wait().ok().flatten().is_none()
    }
}
