import gi
import subprocess

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk
from pages.base_page import BasePage
from backend.disks import list_disks, format_size


class DiskPage(BasePage):
    def __init__(self, window):
        super().__init__(window, "Disk Selection", tag="disk")

        self.disk_group = Adw.PreferencesGroup()
        self.disk_group.set_title("Select a Disk")
        self.disk_group.set_description(
            "Choose the disk to install Bingux on. "
            "You can use GParted to partition it first."
        )
        self.content.append(self.disk_group)

        gparted_btn = Gtk.Button(label="Open GParted")
        gparted_btn.add_css_class("pill")
        gparted_btn.set_halign(Gtk.Align.CENTER)
        gparted_btn.connect("clicked", self._on_gparted)
        self.content.append(gparted_btn)

        self.add_nav_buttons()
        self.disk_rows = []

    def on_enter(self):
        self._refresh_disks()

    def _refresh_disks(self):
        for row in self.disk_rows:
            self.disk_group.remove(row)
        self.disk_rows.clear()

        check_group = None
        disks = list_disks()

        for disk in disks:
            name = disk.get("name", "")
            model = disk.get("model") or "Unknown"
            size = format_size(disk.get("size"))
            children = disk.get("children", [])
            part_count = len(children)

            row = Adw.ActionRow()
            row.set_title(f"{model}")
            row.set_subtitle(f"{name}  \u2022  {size}  \u2022  {part_count} partitions")

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

    def _on_gparted(self, _btn):
        try:
            subprocess.Popen(["gparted"])
        except FileNotFoundError:
            pass

    def validate(self):
        if not self.state.selected_disk:
            return False
        return True
