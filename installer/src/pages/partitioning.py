import gi
import subprocess

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk
from pages.base_page import BasePage
from backend.disks import list_partitions, format_size, detect_partitions


class PartitioningPage(BasePage):
    def __init__(self, window):
        super().__init__(window, "Partitions", tag="partitions")

        btn_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        btn_box.set_halign(Gtk.Align.CENTER)

        gparted_btn = Gtk.Button(label="Open GParted")
        gparted_btn.add_css_class("pill")
        gparted_btn.connect("clicked", self._on_gparted)
        btn_box.append(gparted_btn)

        refresh_btn = Gtk.Button(label="Refresh")
        refresh_btn.add_css_class("pill")
        refresh_btn.connect("clicked", lambda _: self.on_enter())
        btn_box.append(refresh_btn)

        self.content.append(btn_box)

        self.group = Adw.PreferencesGroup()
        self.group.set_title("Assign Partitions")
        self.group.set_description("Partition your disk with GParted, then assign roles below.")

        self.efi_combo = Adw.ComboRow(title="EFI Partition")
        self.efi_combo.set_subtitle("Required \u2022 FAT32 \u2022 ~1 GB")
        self.group.add(self.efi_combo)

        self.root_combo = Adw.ComboRow(title="Root Partition")
        self.root_combo.set_subtitle("Required \u2022 Main system partition")
        self.group.add(self.root_combo)

        self.home_combo = Adw.ComboRow(title="Home Partition")
        self.home_combo.set_subtitle("Optional \u2022 Separate /home")
        self.group.add(self.home_combo)

        self.swap_combo = Adw.ComboRow(title="Swap Partition")
        self.swap_combo.set_subtitle("Optional")
        self.group.add(self.swap_combo)

        self.content.append(self.group)

        self.error_label = Gtk.Label()
        self.error_label.add_css_class("error")
        self.content.append(self.error_label)

        self.add_nav_buttons()
        self._part_names = []
        self._none_names = []

    def should_show(self):
        return getattr(self.state, "disk_mode", "wipe") == "manual"

    def on_enter(self):
        parts = list_partitions(self.state.selected_disk)
        efi_auto, root_auto, swap_auto = detect_partitions(self.state.selected_disk)

        self._part_names = [p.get("name", "") for p in parts]
        part_labels = []
        for p in parts:
            name = p.get("name", "")
            size = format_size(p.get("size"))
            fstype = p.get("fstype") or ""
            part_labels.append(f"{name}  ({size}  {fstype})".strip())

        none_labels = ["(none)"] + part_labels
        self._none_names = [""] + self._part_names

        for combo, auto_val in [
            (self.efi_combo, efi_auto),
            (self.root_combo, root_auto),
        ]:
            model = Gtk.StringList()
            for label in part_labels:
                model.append(label)
            combo.set_model(model)
            if auto_val in self._part_names:
                combo.set_selected(self._part_names.index(auto_val))

        for combo, auto_val in [
            (self.home_combo, ""),
            (self.swap_combo, swap_auto),
        ]:
            model = Gtk.StringList()
            for label in none_labels:
                model.append(label)
            combo.set_model(model)
            if auto_val in self._none_names:
                combo.set_selected(self._none_names.index(auto_val))

    def _on_gparted(self, _btn):
        try:
            subprocess.Popen(["gparted"])
        except FileNotFoundError:
            pass

    def validate(self):
        self.error_label.set_text("")

        efi_idx = self.efi_combo.get_selected()
        root_idx = self.root_combo.get_selected()

        if efi_idx >= len(self._part_names) or root_idx >= len(self._part_names):
            self.error_label.set_text("EFI and root partitions are required.")
            return False

        self.state.efi_partition = self._part_names[efi_idx]
        self.state.root_partition = self._part_names[root_idx]

        home_idx = self.home_combo.get_selected()
        self.state.home_partition = self._none_names[home_idx] if home_idx < len(self._none_names) else ""

        swap_idx = self.swap_combo.get_selected()
        self.state.swap_partition = self._none_names[swap_idx] if swap_idx < len(self._none_names) else ""

        if self.state.efi_partition == self.state.root_partition:
            self.error_label.set_text("EFI and root must be different partitions.")
            return False

        return True
