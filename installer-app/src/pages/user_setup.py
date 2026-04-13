import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk
from pages.base_page import BasePage


class UserSetupPage(BasePage):
    def __init__(self, window):
        super().__init__(window, "User Account", tag="user")

        group = Adw.PreferencesGroup()
        group.set_title("User Account")
        group.set_description("Set up your user account.")

        self.user_row = Adw.EntryRow(title="Username")
        group.add(self.user_row)

        self.pass_row = Adw.PasswordEntryRow(title="Password")
        group.add(self.pass_row)

        self.pass_confirm_row = Adw.PasswordEntryRow(title="Confirm Password")
        group.add(self.pass_confirm_row)

        self.content.append(group)

        self.error_label = Gtk.Label()
        self.error_label.add_css_class("error")
        self.content.append(self.error_label)

        self.add_nav_buttons()

    def should_show(self):
        # Fresh install always needs user setup; repository mode makes it optional
        return True

    def validate(self):
        self.error_label.set_text("")
        username = self.user_row.get_text().strip()
        p1 = self.pass_row.get_text()
        p2 = self.pass_confirm_row.get_text()

        if self.state.install_type == "fresh" and not username:
            self.error_label.set_text("Username is required for a fresh install.")
            return False

        if self.state.install_type == "fresh" and not p1:
            self.error_label.set_text("Password is required for a fresh install.")
            return False

        if p1 and p1 != p2:
            self.error_label.set_text("Passwords do not match.")
            return False

        self.state.username = username
        self.state.password = p1
        return True
