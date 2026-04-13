import os
import subprocess
import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk
from pages.base_page import BasePage


def _detect_installed_systems():
    """Scan for mountable Linux root partitions with /etc/os-release."""
    systems = []
    try:
        out = subprocess.run(
            ["lsblk", "-Jpo", "NAME,FSTYPE,LABEL,SIZE"],
            capture_output=True, text=True, check=True,
        )
        import json
        data = json.loads(out.stdout)
        for dev in data.get("blockdevices", []):
            for part in dev.get("children", []):
                fstype = part.get("fstype") or ""
                if fstype in ("ext4", "btrfs", "xfs", "f2fs"):
                    name = part.get("name", "")
                    label = part.get("label") or ""
                    size = part.get("size") or ""
                    systems.append({
                        "device": name,
                        "fstype": fstype,
                        "label": label,
                        "size": size,
                    })
    except (subprocess.CalledProcessError, Exception):
        pass
    return systems


class RepairPage(BasePage):
    def __init__(self, window):
        super().__init__(window, "Repair Options", tag="repair")

        tools_group = Adw.PreferencesGroup()
        tools_group.set_title("Tools")

        tools = [
            ("utilities-terminal-symbolic", "Terminal", "Open a shell", self._on_terminal),
            ("web-browser-symbolic", "Web Browser", "Open Firefox", self._on_browser),
            ("drive-harddisk-symbolic", "GParted", "Partition editor", self._on_gparted),
        ]

        for icon_name, title, subtitle, callback in tools:
            row = Adw.ActionRow()
            row.set_title(title)
            row.set_subtitle(subtitle)
            row.set_activatable(True)
            row.connect("activated", lambda _, cb=callback: cb())
            icon = Gtk.Image.new_from_icon_name(icon_name)
            icon.set_pixel_size(24)
            row.add_prefix(icon)
            arrow = Gtk.Image.new_from_icon_name("go-next-symbolic")
            row.add_suffix(arrow)
            tools_group.add(row)

        self.content.append(tools_group)

        # Chroot section
        self.chroot_group = Adw.PreferencesGroup()
        self.chroot_group.set_title("Enter Existing Installation")
        self.chroot_group.set_description("Mount a root partition and chroot into it.")
        self.chroot_group.set_visible(False)

        self.content.append(self.chroot_group)

        self.no_systems_label = Gtk.Label(label="No existing installations detected.")
        self.no_systems_label.add_css_class("dim-label")
        self.no_systems_label.set_visible(False)
        self.content.append(self.no_systems_label)

        self.add_nav_buttons(next_label="Back to Installer", show_back=False)

    def _on_next(self):
        self.window.go_back()

    def on_enter(self):
        # Clear old chroot rows
        child = self.chroot_group.get_first_child()
        while child:
            next_child = child.get_next_sibling()
            self.chroot_group.remove(child)
            child = next_child

        systems = _detect_installed_systems()
        if systems:
            self.chroot_group.set_visible(True)
            self.no_systems_label.set_visible(False)
            for sys in systems:
                row = Adw.ActionRow()
                row.set_title(f"{sys['device']}")
                row.set_subtitle(f"{sys['fstype']}  {sys['label']}  {sys['size']}")
                row.set_activatable(True)
                row.connect("activated", lambda _, d=sys['device']: self._on_chroot(d))
                icon = Gtk.Image.new_from_icon_name("computer-symbolic")
                icon.set_pixel_size(24)
                row.add_prefix(icon)
                arrow = Gtk.Image.new_from_icon_name("go-next-symbolic")
                row.add_suffix(arrow)
                self.chroot_group.add(row)
        else:
            self.chroot_group.set_visible(False)
            self.no_systems_label.set_visible(True)

    def _on_terminal(self):
        subprocess.Popen(["gnome-terminal", "--"])

    def _on_browser(self):
        subprocess.Popen(["firefox"])

    def _on_gparted(self):
        subprocess.Popen(["gparted"])

    def _on_chroot(self, device):
        subprocess.Popen(["gnome-terminal", "--", "-e", "bash", "-c",
            f"echo 'Mounting {device} to /mnt...'; "
            f"sudo mount {device} /mnt 2>/dev/null; "
            f"if [ -d /mnt/etc ]; then "
            f"  echo 'Entering chroot...'; echo; "
            f"  sudo nixos-enter --root /mnt; "
            f"else "
            f"  echo 'Not a valid root partition.'; "
            f"fi; "
            f"echo; echo 'Press Enter to exit.'; read",
        ])
