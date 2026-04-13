import gi
import subprocess

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk
from pages.base_page import BasePage


class CompletePage(BasePage):
    def __init__(self, window):
        super().__init__(window, "Complete", tag="complete")

        status = Adw.StatusPage()
        status.set_icon_name("emblem-ok-symbolic")
        status.set_title("Installation Complete!")
        status.set_description(
            "Bingux has been installed successfully.\n\n"
            "Remove the installation media and reboot to start using your new system."
        )
        self.content.append(status)

        btn_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        btn_box.set_halign(Gtk.Align.CENTER)

        reboot_btn = Gtk.Button(label="Reboot Now")
        reboot_btn.add_css_class("pill")
        reboot_btn.add_css_class("suggested-action")
        reboot_btn.connect("clicked", self._on_reboot)
        btn_box.append(reboot_btn)

        close_btn = Gtk.Button(label="Close")
        close_btn.add_css_class("pill")
        close_btn.connect("clicked", lambda _: self.window.close())
        btn_box.append(close_btn)

        self.content.append(btn_box)

    def _on_reboot(self, _btn):
        try:
            subprocess.run(["reboot"])
        except FileNotFoundError:
            pass
