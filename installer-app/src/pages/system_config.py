import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk
from pages.base_page import BasePage


class SystemConfigPage(BasePage):
    def __init__(self, window):
        super().__init__(window, "System Configuration", tag="system-config")

        # Hostname
        host_group = Adw.PreferencesGroup()
        host_group.set_title("System")

        self.hostname_row = Adw.EntryRow(title="Hostname")
        self.hostname_row.set_text("bingux")
        host_group.add(self.hostname_row)

        self.content.append(host_group)

        # Profile
        profile_group = Adw.PreferencesGroup()
        profile_group.set_title("System Profile")

        self.profiles = ["workstation", "laptop", "generic"]
        self.profile_descriptions = {
            "workstation": "Desktop PC \u2014 disables sleep/suspend",
            "laptop": "Laptop \u2014 power management, lid switch handling",
            "generic": "Minimal \u2014 no hardware-specific tweaks",
        }

        check_group = None
        self.profile_checks = {}
        for profile in self.profiles:
            row = Adw.ActionRow()
            row.set_title(profile.capitalize())
            row.set_subtitle(self.profile_descriptions[profile])

            check = Gtk.CheckButton()
            if check_group is None:
                check_group = check
                check.set_active(True)
            else:
                check.set_group(check_group)
            check.connect("toggled", self._on_profile_toggled, profile)
            row.add_prefix(check)
            row.set_activatable_widget(check)
            profile_group.add(row)
            self.profile_checks[profile] = check

        self.content.append(profile_group)

        # Desktop
        desktop_group = Adw.PreferencesGroup()
        desktop_group.set_title("Desktop Environment")

        self.desktop_options = [
            ("gnome", "GNOME (Bingux)", "Full Bingux experience \u2014 extensions, blur, rounded corners"),
            ("gnome-default", "GNOME (Default)", "Stock GNOME desktop without Bingux extensions"),
            ("kde", "KDE Plasma", "Feature-rich desktop with Plasma 6 and Wayland"),
            ("xfce", "XFCE", "Lightweight traditional desktop"),
        ]

        desk_check_group = None
        self.desktop_checks = {}
        for key, name, desc in self.desktop_options:
            row = Adw.ActionRow()
            row.set_title(name)
            row.set_subtitle(desc)

            check = Gtk.CheckButton()
            if desk_check_group is None:
                desk_check_group = check
                check.set_active(True)
            else:
                check.set_group(desk_check_group)
            check.connect("toggled", self._on_desktop_toggled, key)
            row.add_prefix(check)
            row.set_activatable_widget(check)
            desktop_group.add(row)
            self.desktop_checks[key] = check

        self.content.append(desktop_group)

        self.error_label = Gtk.Label()
        self.error_label.add_css_class("error")
        self.content.append(self.error_label)

        self.add_nav_buttons()

    def should_show(self):
        return self.state.install_type == "fresh"

    def _on_profile_toggled(self, check, profile):
        if check.get_active():
            self.state.profile = profile

    def _on_desktop_toggled(self, check, desktop):
        if check.get_active():
            self.state.desktop = desktop

    def validate(self):
        hostname = self.hostname_row.get_text().strip()
        if not hostname:
            self.error_label.set_text("Hostname is required.")
            return False
        if " " in hostname:
            self.error_label.set_text("Hostname cannot contain spaces.")
            return False
        self.state.hostname = hostname
        self.state.selected_host = hostname
        return True
