import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk
from pages.base_page import BasePage


class SummaryPage(BasePage):
    def __init__(self, window):
        super().__init__(window, "Summary", tag="summary")
        self._rows = []
        self.summary_group = Adw.PreferencesGroup()
        self.summary_group.set_title("Review Installation")
        self.summary_group.set_description(
            "Please review your choices before proceeding."
        )
        self.content.append(self.summary_group)

        self.warning = Gtk.Label()
        self.warning.set_markup(
            '<span foreground="red" weight="bold">'
            "WARNING: This will format and erase data on the selected partitions!"
            "</span>"
        )
        self.warning.set_wrap(True)
        self.content.append(self.warning)

        self.add_nav_buttons(next_label="Install")

    def on_enter(self):
        # Remove previously added rows
        for row in self._rows:
            self.summary_group.remove(row)
        self._rows.clear()

        s = self.state
        entries = []

        if s.install_type == "fresh":
            entries.append(("Host", s.selected_host))
            entries.append(("Profile", getattr(s, "profile", "")))
            entries.append(("Desktop", getattr(s, "desktop", "")))
        else:
            entries.append(("Host", s.selected_host))
            entries.append(("Repository", s.repo_url))

        entries.append(("Disk", s.selected_disk))
        if getattr(s, "disk_mode", "wipe") == "wipe":
            entries.append(("Mode", "Erase entire disk"))
        else:
            entries.append(("EFI", s.efi_partition))
            entries.append(("Root", s.root_partition))
            if s.home_partition:
                entries.append(("Home", s.home_partition))
            if s.swap_partition:
                entries.append(("Swap", s.swap_partition))

        entries.append(("Filesystem", s.filesystem))
        if s.encrypt_root:
            entries.append(("Encryption", "LUKS2"))
        if s.username:
            entries.append(("User", s.username))

        for title, value in entries:
            if value:
                row = Adw.ActionRow()
                row.set_title(title)
                row.set_subtitle(str(value))
                self.summary_group.add(row)
                self._rows.append(row)
