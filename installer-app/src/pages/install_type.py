import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk
from pages.base_page import BasePage


class InstallTypePage(BasePage):
    def __init__(self, window):
        super().__init__(window, "Installation Type", tag="install-type")

        group = Adw.PreferencesGroup()
        group.set_title("How would you like to install?")

        # Fresh install
        self.fresh_row = Adw.ActionRow()
        self.fresh_row.set_title("Fresh Install")
        self.fresh_row.set_subtitle(
            "Set up Bingux from scratch \u2014 configure hostname, user, and desktop graphically."
        )
        self.fresh_check = Gtk.CheckButton()
        self.fresh_check.set_active(True)
        self.fresh_row.add_prefix(self.fresh_check)
        self.fresh_row.set_activatable_widget(self.fresh_check)
        group.add(self.fresh_row)

        # From repository
        self.repo_row = Adw.ActionRow()
        self.repo_row.set_title("From Repository")
        self.repo_row.set_subtitle(
            "Use an existing NixOS config repository from GitHub."
        )
        self.repo_check = Gtk.CheckButton()
        self.repo_check.set_group(self.fresh_check)
        self.repo_row.add_prefix(self.repo_check)
        self.repo_row.set_activatable_widget(self.repo_check)
        group.add(self.repo_row)

        self.content.append(group)
        self.add_nav_buttons()

    def validate(self):
        self.state.install_type = "repository" if self.repo_check.get_active() else "fresh"
        return True
