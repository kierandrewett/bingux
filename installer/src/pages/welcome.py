import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk
from pages.base_page import BasePage


class WelcomePage(BasePage):
    def __init__(self, window):
        super().__init__(window, "Welcome", tag="welcome")

        status = Adw.StatusPage()
        status.set_icon_name("system-software-install-symbolic")
        status.set_title("Welcome to Bingux")
        status.set_description(
            "This wizard will guide you through installing Bingux on your computer.\n\n"
            "You will need:\n"
            "  \u2022  A GitHub account with access to your NixOS config repository\n"
            "  \u2022  An internet connection\n"
            "  \u2022  A disk to install to"
        )
        self.content.append(status)

        self.add_nav_buttons(next_label="Get Started", show_back=False)
