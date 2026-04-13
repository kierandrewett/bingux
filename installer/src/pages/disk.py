import gi
import subprocess

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk
from pages.base_page import BasePage
from backend.disks import list_disks, format_size
from widgets.disk_map import DiskMap


class DiskPage(BasePage):
    def __init__(self, window):
        super().__init__(window, "Disk Setup", tag="disk")

        # Partition map preview
        self.disk_map = DiskMap()
        self.content.append(self.disk_map)

        # Disk selection
        self.disk_group = Adw.PreferencesGroup()
        self.disk_group.set_title("Select a Disk")
        self.content.append(self.disk_group)

        # Partitioning mode
        mode_group = Adw.PreferencesGroup()
        mode_group.set_title("Partitioning")

        self.wipe_row = Adw.ActionRow()
        self.wipe_row.set_title("Erase entire disk")
        self.wipe_row.set_subtitle("Automatically create EFI + root partitions")
        self.wipe_check = Gtk.CheckButton()
        self.wipe_check.set_active(True)
        self.wipe_check.connect("toggled", self._on_mode_changed)
        self.wipe_row.add_prefix(self.wipe_check)
        self.wipe_row.set_activatable_widget(self.wipe_check)
        mode_group.add(self.wipe_row)

        self.manual_row = Adw.ActionRow()
        self.manual_row.set_title("Manual partitioning")
        self.manual_row.set_subtitle("Use GParted, then assign partitions on the next page")
        self.manual_check = Gtk.CheckButton()
        self.manual_check.set_group(self.wipe_check)
        self.manual_row.add_prefix(self.manual_check)
        self.manual_row.set_activatable_widget(self.manual_check)
        mode_group.add(self.manual_row)

        self.content.append(mode_group)

        # Filesystem (shown for wipe mode)
        self.fs_group = Adw.PreferencesGroup()
        self.fs_group.set_title("Filesystem")

        fs_options = [
            ("btrfs", "Btrfs", "Snapshots, compression, subvolumes (recommended)"),
            ("ext4", "ext4", "Traditional, stable, well-supported"),
            ("xfs", "XFS", "High-performance journaling"),
        ]
        fs_check_group = None
        self.fs_checks = {}
        for key, name, desc in fs_options:
            row = Adw.ActionRow()
            row.set_title(name)
            row.set_subtitle(desc)
            check = Gtk.CheckButton()
            if fs_check_group is None:
                fs_check_group = check
                check.set_active(True)
            else:
                check.set_group(fs_check_group)
            check.connect("toggled", self._on_fs_toggled, key)
            row.add_prefix(check)
            row.set_activatable_widget(check)
            self.fs_group.add(row)
            self.fs_checks[key] = check

        self.content.append(self.fs_group)

        # Encryption
        self.enc_group = Adw.PreferencesGroup()
        self.enc_group.set_title("Encryption")

        self.encrypt_switch = Adw.SwitchRow(title="Encrypt with LUKS2")
        self.encrypt_switch.set_subtitle("Full disk encryption with a passphrase at boot")
        self.encrypt_switch.connect("notify::active", self._on_encrypt_toggled)
        self.enc_group.add(self.encrypt_switch)

        self.pass_row = Adw.PasswordEntryRow(title="Passphrase")
        self.pass_row.set_visible(False)
        self.enc_group.add(self.pass_row)

        self.pass_confirm_row = Adw.PasswordEntryRow(title="Confirm Passphrase")
        self.pass_confirm_row.set_visible(False)
        self.enc_group.add(self.pass_confirm_row)

        self.content.append(self.enc_group)

        self.error_label = Gtk.Label()
        self.error_label.add_css_class("error")
        self.content.append(self.error_label)

        self.add_nav_buttons()
        self.disk_rows = []

    def on_enter(self):
        self._refresh_disks()

    def _refresh_disks(self):
        for row in self.disk_rows:
            self.disk_group.remove(row)
        self.disk_rows.clear()

        check_group = None
        for disk in list_disks():
            name = disk.get("name", "")
            model = disk.get("model") or "Unknown"
            size = format_size(disk.get("size"))
            children = disk.get("children", [])

            row = Adw.ActionRow()
            row.set_title(model)
            row.set_subtitle(f"{name}  \u2022  {size}  \u2022  {len(children)} partitions")

            check = Gtk.CheckButton()
            if check_group is None:
                check_group = check
            else:
                check.set_group(check_group)

            if self.state.selected_disk == name:
                check.set_active(True)

            check.connect("toggled", self._on_disk_toggled, name)
            row.add_prefix(check)
            row.set_activatable_widget(check)

            self.disk_group.add(row)
            self.disk_rows.append(row)

    def _on_disk_toggled(self, check, disk_name):
        if check.get_active():
            self.state.selected_disk = disk_name
            self._update_map()

    def _on_mode_changed(self, check):
        is_wipe = self.wipe_check.get_active()
        self.fs_group.set_visible(is_wipe)
        self.enc_group.set_visible(is_wipe)
        self._update_map()

    def _update_map(self):
        if not self.state.selected_disk:
            return
        if self.wipe_check.get_active():
            # Show wipe preview
            for d in list_disks():
                if d.get("name") == self.state.selected_disk:
                    self.disk_map.set_wipe_preview(d.get("size", 0))
                    break
        else:
            # Show current partitions
            from backend.disks import list_partitions
            parts = list_partitions(self.state.selected_disk)
            self.disk_map.set_from_lsblk(parts)

    def _on_fs_toggled(self, check, key):
        if check.get_active():
            self.state.filesystem = key

    def _on_encrypt_toggled(self, switch, _pspec):
        active = switch.get_active()
        self.pass_row.set_visible(active)
        self.pass_confirm_row.set_visible(active)

    def validate(self):
        self.error_label.set_text("")

        if not self.state.selected_disk:
            self.error_label.set_text("Please select a disk.")
            return False

        is_wipe = self.wipe_check.get_active()
        self.state.disk_mode = "wipe" if is_wipe else "manual"

        if is_wipe:
            self.state.encrypt_root = self.encrypt_switch.get_active()
            if self.state.encrypt_root:
                p1 = self.pass_row.get_text()
                p2 = self.pass_confirm_row.get_text()
                if not p1:
                    self.error_label.set_text("Passphrase is required for encryption.")
                    return False
                if p1 != p2:
                    self.error_label.set_text("Passphrases do not match.")
                    return False
                self.state.luks_passphrase = p1

        return True
