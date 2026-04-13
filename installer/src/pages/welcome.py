import os
import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk, GdkPixbuf
from pages.base_page import BasePage


def _find_logo():
    src_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    prefix = os.path.dirname(os.path.dirname(src_dir))
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

        # Logo (smaller to save space)
        logo_path = _find_logo()
        if logo_path:
            pixbuf = GdkPixbuf.Pixbuf.new_from_file_at_scale(logo_path, 96, 96, True)
            texture = Gtk.Image.new_from_pixbuf(pixbuf)
            texture.set_pixel_size(96)
            texture.set_halign(Gtk.Align.CENTER)
            self.content.append(texture)

        title = Gtk.Label(label="Welcome to Bingux")
        title.add_css_class("title-1")
        title.set_halign(Gtk.Align.CENTER)
        self.content.append(title)

        subtitle = Gtk.Label(label="A modern NixOS-based Linux distribution")
        subtitle.add_css_class("dim-label")
        subtitle.set_halign(Gtk.Align.CENTER)
        subtitle.set_margin_bottom(8)
        self.content.append(subtitle)

        # Info cards (compact)
        cards = [
            ("drive-harddisk-symbolic", "Install fresh or from your own config repository"),
            ("network-wireless-symbolic", "An internet connection is required"),
            ("dialog-password-symbolic", "Sign in to GitHub or GitLab if your config is private"),
        ]

        for icon_name, text in cards:
            row = Adw.ActionRow()
            row.set_title(text)
            icon = Gtk.Image.new_from_icon_name(icon_name)
            icon.set_pixel_size(20)
            row.add_prefix(icon)
            self.content.append(row)

        # Buttons side by side
        btn_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        btn_box.set_halign(Gtk.Align.CENTER)
        btn_box.set_margin_top(12)

        start_btn = Gtk.Button(label="Get Started")
        start_btn.add_css_class("pill")
        start_btn.add_css_class("suggested-action")
        start_btn.connect("clicked", lambda _: self.window.go_next())
        btn_box.append(start_btn)

        repair_btn = Gtk.Button(label="Repair Options")
        repair_btn.add_css_class("pill")
        repair_btn.connect("clicked", self._on_repair)
        btn_box.append(repair_btn)

        self.content.append(btn_box)

    def _on_repair(self, _btn):
        repair_page = self.window.repair_page
        repair_page.on_enter()
        self.window.nav_view.push(repair_page)
