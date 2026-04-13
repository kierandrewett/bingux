import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk
from pages.base_page import BasePage


class EncryptionPage(BasePage):
    def __init__(self, window):
        super().__init__(window, "Encryption", tag="encryption")

        group = Adw.PreferencesGroup()
        group.set_title("Disk Encryption")
        group.set_description("Optionally encrypt your partitions with LUKS2.")

        self.encrypt_root_row = Adw.SwitchRow(title="Encrypt Root")
        self.encrypt_root_row.set_subtitle("Encrypts the root partition with LUKS2")
        self.encrypt_root_row.connect("notify::active", self._on_encrypt_toggled)
        group.add(self.encrypt_root_row)

        self.encrypt_home_row = Adw.SwitchRow(title="Encrypt Home")
        self.encrypt_home_row.set_subtitle("Encrypts the home partition with LUKS2")
        self.encrypt_home_row.set_sensitive(False)
        group.add(self.encrypt_home_row)

        self.content.append(group)

        self.pass_group = Adw.PreferencesGroup()
        self.pass_group.set_title("LUKS Passphrase")
        self.pass_group.set_visible(False)

        self.pass_row = Adw.PasswordEntryRow(title="Passphrase")
        self.pass_group.add(self.pass_row)

        self.pass_confirm_row = Adw.PasswordEntryRow(title="Confirm Passphrase")
        self.pass_group.add(self.pass_confirm_row)

        self.content.append(self.pass_group)

        self.error_label = Gtk.Label()
        self.error_label.add_css_class("error")
        self.content.append(self.error_label)

        self.add_nav_buttons()

    def on_enter(self):
        has_home = bool(self.state.home_partition)
        self.encrypt_home_row.set_sensitive(has_home)
        if not has_home:
            self.encrypt_home_row.set_active(False)

    def _on_encrypt_toggled(self, row, _pspec):
        needs_pass = row.get_active() or self.encrypt_home_row.get_active()
        self.pass_group.set_visible(needs_pass)

    def validate(self):
        self.error_label.set_text("")
        self.state.encrypt_root = self.encrypt_root_row.get_active()
        self.state.encrypt_home = self.encrypt_home_row.get_active()

        if self.state.encrypt_root or self.state.encrypt_home:
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
