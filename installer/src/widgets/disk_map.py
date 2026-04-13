import gi

gi.require_version("Gtk", "4.0")

from gi.repository import Gtk, Gdk


# Colors for partition roles
ROLE_COLORS = {
    "efi": "#3584e4",
    "root": "#33d17a",
    "home": "#ff7800",
    "swap": "#e5a50a",
    "other": "#9a9996",
    "free": "#3d3846",
}


def format_size(size_bytes):
    if size_bytes is None or size_bytes == 0:
        return "?"
    size = int(size_bytes)
    for unit in ["B", "KB", "MB", "GB", "TB"]:
        if size < 1024:
            return f"{size:.1f} {unit}"
        size /= 1024
    return f"{size:.1f} PB"


class DiskMap(Gtk.Box):
    """Graphical partition map showing proportional colored segments."""

    def __init__(self):
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        self.bar_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=2)
        self.bar_box.set_size_request(-1, 40)
        self.bar_box.add_css_class("card")
        self.append(self.bar_box)

        self.legend_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        self.legend_box.set_halign(Gtk.Align.CENTER)
        self.append(self.legend_box)

        self._segments = []

    def set_partitions(self, partitions):
        """Set partition data.

        partitions: list of dicts with keys:
            name: str (e.g. "/dev/sda1")
            size: int (bytes)
            role: str ("efi", "root", "home", "swap", "other")
            label: str (display label, e.g. "EFI (1 GB)")
        """
        # Clear
        for seg in self._segments:
            self.bar_box.remove(seg)
        self._segments.clear()
        child = self.legend_box.get_first_child()
        while child:
            next_child = child.get_next_sibling()
            self.legend_box.remove(child)
            child = next_child

        if not partitions:
            return

        total = sum(p.get("size", 0) for p in partitions) or 1

        for part in partitions:
            size = part.get("size", 0)
            role = part.get("role", "other")
            label = part.get("label", part.get("name", ""))
            color = ROLE_COLORS.get(role, ROLE_COLORS["other"])
            fraction = max(size / total, 0.03)  # min 3% so tiny partitions are visible

            # Bar segment
            seg = Gtk.Box()
            seg.set_hexpand_set(True)
            seg.set_size_request(int(fraction * 500), 40)
            provider = Gtk.CssProvider()
            provider.load_from_string(
                f"box {{ background-color: {color}; border-radius: 4px; }}"
            )
            seg.get_style_context().add_provider(provider, 800)
            self.bar_box.append(seg)
            self._segments.append(seg)

            # Legend entry
            legend = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=4)
            dot = Gtk.Box()
            dot.set_size_request(12, 12)
            dot.set_valign(Gtk.Align.CENTER)
            dot_provider = Gtk.CssProvider()
            dot_provider.load_from_string(
                f"box {{ background-color: {color}; border-radius: 6px; }}"
            )
            dot.get_style_context().add_provider(dot_provider, 800)
            legend.append(dot)

            text = Gtk.Label(label=label)
            text.add_css_class("caption")
            legend.append(text)

            self.legend_box.append(legend)

    def set_from_lsblk(self, partitions, roles=None):
        """Build from lsblk partition list with optional role mapping.

        roles: dict mapping device name -> role string
            e.g. {"/dev/sda1": "efi", "/dev/sda2": "root"}
        """
        roles = roles or {}
        data = []
        for p in partitions:
            name = p.get("name", "")
            size = int(p.get("size") or 0)
            fstype = p.get("fstype") or ""
            role = roles.get(name, "other")

            # Auto-detect role from filesystem/partition type
            if role == "other":
                parttype = (p.get("parttype") or "").lower()
                if parttype == "c12a7328-f81f-11d2-ba4b-00a0c93ec93b" or fstype == "vfat":
                    role = "efi"
                elif fstype == "swap" or parttype == "0657fd6d-a4ab-43c4-84e5-0933c84b4f4f":
                    role = "swap"

            size_str = format_size(size)
            short_name = name.split("/")[-1]
            label = f"{short_name} ({size_str})"
            if role != "other":
                label = f"{role.upper()} ({size_str})"

            data.append({"name": name, "size": size, "role": role, "label": label})

        self.set_partitions(data)

    def set_from_state(self, state):
        """Build from installer state (for summary page)."""
        from backend.disks import list_partitions
        parts = list_partitions(state.selected_disk)
        roles = {}
        if state.efi_partition:
            roles[state.efi_partition] = "efi"
        if state.root_partition:
            roles[state.root_partition] = "root"
        if state.home_partition:
            roles[state.home_partition] = "home"
        if state.swap_partition:
            roles[state.swap_partition] = "swap"
        self.set_from_lsblk(parts, roles)

    def set_wipe_preview(self, disk_size):
        """Show preview for a full-disk wipe (1GB EFI + rest root)."""
        efi_size = 1 * 1024 ** 3
        root_size = max(int(disk_size) - efi_size, efi_size)
        self.set_partitions([
            {"name": "EFI", "size": efi_size, "role": "efi",
             "label": f"EFI ({format_size(efi_size)})"},
            {"name": "Root", "size": root_size, "role": "root",
             "label": f"Root ({format_size(root_size)})"},
        ])
