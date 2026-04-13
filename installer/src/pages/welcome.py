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
        super().__init__(window, "Welcome to Bingux", tag="welcome", show_title=True)

        # Logo
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

        # Info cards
        cards_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        cards_box.set_halign(Gtk.Align.CENTER)
        cards_box.set_homogeneous(True)

        cards = [
            ("drive-harddisk-symbolic", "Flexible", "Install fresh or from\nyour own config repo"),
            ("network-wireless-symbolic", "Online", "Internet connection\nrequired to install"),
            ("dialog-password-symbolic", "Private Repos", "Sign in to GitHub\nor GitLab if needed"),
        ]

        for icon_name, heading, desc in cards:
            card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
            card.add_css_class("card")
            card.set_size_request(150, -1)
            card.set_valign(Gtk.Align.START)

            inner = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
            inner.set_margin_top(16)
            inner.set_margin_bottom(16)
            inner.set_margin_start(12)
            inner.set_margin_end(12)

            icon = Gtk.Image.new_from_icon_name(icon_name)
            icon.set_pixel_size(28)
            icon.add_css_class("accent")
            inner.append(icon)

            label = Gtk.Label(label=heading)
            label.add_css_class("heading")
            inner.append(label)

            detail = Gtk.Label(label=desc)
            detail.add_css_class("dim-label")
            detail.add_css_class("caption")
            detail.set_justify(Gtk.Justification.CENTER)
            inner.append(detail)

            card.append(inner)
            cards_box.append(card)

        self.content.append(cards_box)

        # Bottom bar: Repair (left), Get Started (right)
        bar = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)
        bar.set_margin_start(24)
        bar.set_margin_end(24)
        bar.set_margin_top(8)
        bar.set_margin_bottom(12)

        repair_btn = Gtk.Button(label="Repair Options")
        repair_btn.add_css_class("pill")
        repair_btn.connect("clicked", self._on_repair)
        bar.append(repair_btn)

        spacer = Gtk.Box()
        spacer.set_hexpand(True)
        bar.append(spacer)

        start_btn = Gtk.Button(label="Get Started")
        start_btn.add_css_class("pill")
        start_btn.add_css_class("suggested-action")
        start_btn.connect("clicked", lambda _: self.window.go_next())
        bar.append(start_btn)

        self.toolbar_view.add_bottom_bar(bar)

    def _on_repair(self, _btn):
        repair_page = self.window.repair_page
        repair_page.on_enter()
        self.window.nav_view.push(repair_page)
