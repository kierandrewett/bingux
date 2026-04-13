import gi
import threading

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk, GLib
from pages.base_page import BasePage
from backend import partitioner, nixos, config_generator
from widgets.log_view import LogView


class InstallPage(BasePage):
    def __init__(self, window):
        super().__init__(window, "Installing", tag="install")

        self.status_label = Gtk.Label()
        self.status_label.add_css_class("title-3")
        self.status_label.set_text("Preparing installation...")
        self.content.append(self.status_label)

        self.progress = Gtk.ProgressBar()
        self.progress.set_show_text(True)
        self.content.append(self.progress)

        self.log_view = LogView()
        self.log_view.set_size_request(-1, 300)
        self.content.append(self.log_view)

    def on_enter(self):
        # Prevent going back during installation
        self.set_can_pop(False)
        threading.Thread(target=self._run_install, daemon=True).start()

    def _set_status(self, text, fraction=None):
        def do():
            self.status_label.set_text(text)
            if fraction is not None:
                self.progress.set_fraction(fraction)
                self.progress.set_text(f"{int(fraction * 100)}%")
        GLib.idle_add(do)

    def _log(self, text):
        GLib.idle_add(self.log_view.append, text)

    def _pulse_progress(self):
        """Slowly advance progress from 50% to 95% during nixos-install."""
        def tick():
            if not self._installing:
                return False
            cur = self.progress.get_fraction()
            if cur < 0.95:
                self.progress.set_fraction(cur + 0.002)
                self.progress.set_text(f"{int((cur + 0.002) * 100)}%")
            return self._installing
        self._installing = True
        GLib.timeout_add(3000, tick)

    def _run_install(self):
        s = self.state
        # Steps 1-7 = 0%–50%, step 8 (nixos-install) = 50%–95%, done = 100%
        prep_steps = 7

        try:
            # Step 1: Generate config (fresh) or use cloned repo
            step = 1
            self._set_status("Preparing configuration...", step / prep_steps * 0.5)
            if s.install_type == "fresh":
                self._log("Generating NixOS configuration...\n")
                config_generator.generate_config(s)
                self._log(f"Config generated for host: {s.hostname}\n")
            else:
                self._log(f"Using repository: {s.repo_url}\n")

            # Step 2: Partition (wipe mode) or use existing partitions
            step = 2
            self._set_status("Partitioning disk...", step / prep_steps * 0.5)
            if s.disk_mode == "wipe":
                self._log(f"Wiping disk: {s.selected_disk}\n")
                ok, _, err = partitioner.wipe_disk(s.selected_disk)
                if not ok:
                    self._log(f"Partitioning failed: {err}\n")
                    self._set_status("Partitioning failed")
                    return
                # wipe_disk sets efi_partition and root_partition
                s.efi_partition = f"{s.selected_disk}1"
                s.root_partition = f"{s.selected_disk}2"
                # Handle NVMe naming (e.g. /dev/nvme0n1p1)
                if "nvme" in s.selected_disk or "mmcblk" in s.selected_disk:
                    s.efi_partition = f"{s.selected_disk}p1"
                    s.root_partition = f"{s.selected_disk}p2"
                self._log(f"EFI: {s.efi_partition}  Root: {s.root_partition}\n")

            # Step 3: Encryption
            step = 3
            self._set_status("Setting up encryption...", step / prep_steps * 0.5)
            root_dev = s.root_partition

            if s.encrypt_root:
                self._log(f"Setting up LUKS on {s.root_partition}\n")
                ok, _, err = partitioner.setup_luks(s.root_partition, s.luks_passphrase, "cryptroot")
                if not ok:
                    self._log(f"LUKS setup failed: {err}\n")
                    self._set_status("Encryption failed")
                    return
                root_dev = "/dev/mapper/cryptroot"
                self._log("LUKS root ready\n")

            # Step 4: Format EFI
            step = 4
            self._set_status("Formatting EFI...", step / prep_steps * 0.5)
            self._log(f"Formatting EFI: {s.efi_partition}\n")
            partitioner.format_fat32(s.efi_partition)
            partitioner.set_efi_type(s.efi_partition)

            # Step 5: Format root
            step = 5
            self._set_status(f"Formatting root ({s.filesystem})...", step / prep_steps * 0.5)
            self._log(f"Formatting root: {root_dev} as {s.filesystem}\n")
            partitioner.format_filesystem(root_dev, s.filesystem)

            if s.home_partition:
                self._log(f"Formatting home: {s.home_partition}\n")
                partitioner.format_filesystem(s.home_partition, s.filesystem, label="home")
            if s.swap_partition:
                self._log(f"Setting up swap: {s.swap_partition}\n")
                partitioner.setup_swap(s.swap_partition)

            # Step 6: Mount
            step = 6
            self._set_status("Mounting filesystems...", step / prep_steps * 0.5)
            if s.filesystem == "btrfs":
                partitioner.setup_btrfs_subvolumes(root_dev, bool(s.home_partition))
            else:
                partitioner.mount_simple(root_dev)
            partitioner.mount_partition(s.efi_partition, "/mnt/boot")
            if s.home_partition:
                partitioner.mount_partition(s.home_partition, "/mnt/home")
            self._log("Filesystems mounted\n")

            # Step 7: Generate hardware config + copy repo
            step = 7
            self._set_status("Generating hardware configuration...", step / prep_steps * 0.5)
            nixos.generate_config()
            nixos.copy_repo(s.selected_host, log_callback=self._log)
            age_key = nixos.generate_ssh_keys()
            if age_key:
                self._log(f"Age key for sops: {age_key}\n")
            self._log("Flake ready\n")

            # Step 8: nixos-install
            self._set_status("Installing Bingux...", 0.5)
            self._log("\n--- nixos-install ---\n\n")
            GLib.idle_add(self._pulse_progress)
            ok = nixos.install(s.selected_host, log_callback=self._log)
            self._installing = False

            if not ok:
                self._set_status("Installation failed")
                self._log("\nInstallation failed.\n")
                GLib.idle_add(self._show_error_actions)
                return

            if s.username and s.password:
                self._log(f"\nSetting password for {s.username}...\n")
                nixos.set_password(s.username, s.password)

            self._set_status("Installation complete!", 1.0)
            self._log("\nInstallation complete!\n")
            GLib.idle_add(self.window.go_next)

        except Exception as e:
            self._set_status("Installation failed")
            self._log(f"\nError: {e}\n")
            GLib.idle_add(self._show_error_actions)

    def _show_error_actions(self):
        """Show action buttons when installation fails."""
        box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        box.set_halign(Gtk.Align.CENTER)
        box.set_margin_top(12)

        copy_btn = Gtk.Button(label="Copy Log")
        copy_btn.add_css_class("pill")
        copy_btn.connect("clicked", self._on_copy_log)
        box.append(copy_btn)

        save_btn = Gtk.Button(label="Save Log")
        save_btn.add_css_class("pill")
        save_btn.connect("clicked", self._on_save_log)
        box.append(save_btn)

        terminal_btn = Gtk.Button(label="Open Terminal")
        terminal_btn.add_css_class("pill")
        terminal_btn.connect("clicked", self._on_terminal)
        box.append(terminal_btn)

        self.content.append(box)

    def _get_log_text(self):
        buf = self.log_view.buffer
        return buf.get_text(buf.get_start_iter(), buf.get_end_iter(), False)

    def _on_copy_log(self, _btn):
        import subprocess
        log = self._get_log_text()
        p = subprocess.Popen(["wl-copy"], stdin=subprocess.PIPE, text=True)
        p.communicate(input=log)

    def _on_save_log(self, _btn):
        log = self._get_log_text()
        path = "/tmp/bingux-install.log"
        with open(path, "w") as f:
            f.write(log)
        self._log(f"\nLog saved to {path}\n")

    def _on_terminal(self, _btn):
        import subprocess
        subprocess.Popen(["gnome-terminal"])
