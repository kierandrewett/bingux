import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk
from pages.base_page import BasePage


class SummaryPage(BasePage):
    def __init__(self, window):
        super().__init__(window, "Summary", tag="summary")
        self.summary_group = Adw.PreferencesGroup()
        self.summary_group.set_title("Review Installation")
        self.summary_group.set_description(
            "Please review your choices before proceeding. "
            "This will erase data on the selected partitions."
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
        # Clear old rows
        while True:
            child = self.summary_group.get_first_child()
            if child is None:
                break
            self.summary_group.remove(child)

        s = self.state
        rows = [
            ("Host", s.selected_host),
            ("Repository", s.repo_url),
            ("Disk", s.selected_disk),
            ("EFI", s.efi_partition),
            ("Root", f"{s.root_partition}  \u2192  {s.filesystem}"),
        ]

        if s.home_partition:
            rows.append(("Home", s.home_partition))
        if s.swap_partition:
            rows.append(("Swap", s.swap_partition))
        if s.encrypt_root:
            rows.append(("Encryption", "LUKS2 (root)"))
        if s.encrypt_home:
            rows.append(("Encryption", "LUKS2 (home)"))
        if s.username:
            rows.append(("User", s.username))

        for title, value in rows:
            row = Adw.ActionRow()
            row.set_title(title)
            row.set_subtitle(value)
            self.summary_group.add(row)
