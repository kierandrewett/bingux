import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk
from pages.base_page import BasePage


class FilesystemPage(BasePage):
    def __init__(self, window):
        super().__init__(window, "Filesystem", tag="filesystem")

        group = Adw.PreferencesGroup()
        group.set_title("Root Filesystem")
        group.set_description("Choose the filesystem for your root partition.")

        self.fs_options = ["btrfs", "ext4", "xfs"]
        self.fs_descriptions = {
            "btrfs": "Modern COW filesystem with snapshots, compression, and subvolumes (recommended)",
            "ext4": "Traditional Linux filesystem \u2014 stable and well-supported",
            "xfs": "High-performance journaling filesystem",
        }

        check_group = None
        self.checks = {}
        for fs in self.fs_options:
            row = Adw.ActionRow()
            row.set_title(fs)
            row.set_subtitle(self.fs_descriptions[fs])

            check = Gtk.CheckButton()
            if check_group is None:
                check_group = check
                check.set_active(True)
            else:
                check.set_group(check_group)

            check.connect("toggled", self._on_toggled, fs)
            row.add_prefix(check)
            row.set_activatable_widget(check)
            group.add(row)
            self.checks[fs] = check

        self.content.append(group)
        self.add_nav_buttons()

    def on_enter(self):
        fs = self.state.filesystem or "btrfs"
        if fs in self.checks:
            self.checks[fs].set_active(True)

    def _on_toggled(self, check, fs):
        if check.get_active():
            self.state.filesystem = fs

    def validate(self):
        return bool(self.state.filesystem)
