import os
import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk, GdkPixbuf
from pages.base_page import BasePage


def _find_logo():
    """Find the bingux logo relative to the install prefix."""
    # Installed path: $out/share/bingux-installer/logo.png
    src_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    prefix = os.path.dirname(os.path.dirname(src_dir))  # $out
    for path in [
        os.path.join(prefix, "share", "bingux-installer", "logo.png"),
        "/etc/bingus.png",
    ]:
        if os.path.isfile(path):
            return path
    return None


class WelcomePage(BasePage):
    def __init__(self, window):
        super().__init__(window, "Welcome", tag="welcome")

        # Logo
        logo_path = _find_logo()
        if logo_path:
            pixbuf = GdkPixbuf.Pixbuf.new_from_file_at_scale(logo_path, 128, 128, True)
            texture = Gtk.Image.new_from_pixbuf(pixbuf)
            texture.set_pixel_size(128)
            texture.set_halign(Gtk.Align.CENTER)
            texture.set_margin_bottom(8)
            self.content.append(texture)

        title = Gtk.Label(label="Welcome to Bingux")
        title.add_css_class("title-1")
        title.set_halign(Gtk.Align.CENTER)
        self.content.append(title)

        subtitle = Gtk.Label(label="A modern NixOS-based Linux distribution")
        subtitle.add_css_class("dim-label")
        subtitle.set_halign(Gtk.Align.CENTER)
        subtitle.set_margin_bottom(16)
        self.content.append(subtitle)

        # Info cards
        cards = [
            ("drive-harddisk-symbolic", "Install fresh or from your own NixOS config repository"),
            ("network-wireless-symbolic", "An internet connection is required"),
            ("dialog-password-symbolic", "Sign in to GitHub or GitLab if your config is private"),
        ]

        for icon_name, text in cards:
            row = Adw.ActionRow()
            row.set_title(text)
            icon = Gtk.Image.new_from_icon_name(icon_name)
            icon.set_pixel_size(24)
            row.add_prefix(icon)
            self.content.append(row)

        self.add_nav_buttons(next_label="Get Started", show_back=False)

        # Repair options
        repair_btn = Gtk.Button(label="Repair Options")
        repair_btn.add_css_class("pill")
        repair_btn.add_css_class("flat")
        repair_btn.set_halign(Gtk.Align.CENTER)
        repair_btn.connect("clicked", self._on_repair)
        self.content.append(repair_btn)

    def _on_repair(self, _btn):
        repair_page = self.window.repair_page
        repair_page.on_enter()
        self.window.nav_view.push(repair_page)
