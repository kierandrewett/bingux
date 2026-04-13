import subprocess
import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk
from pages.base_page import BasePage


class CompletePage(BasePage):
    def __init__(self, window):
        super().__init__(window, "Complete", tag="complete")

        # Big checkmark
        check = Gtk.Label(label="\u2713")
        check.set_halign(Gtk.Align.CENTER)
        css = Gtk.CssProvider()
        css.load_from_string("""
            label {
                font-size: 64px;
                color: @success_color;
            }
        """)
        check.get_style_context().add_provider(css, 800)
        self.content.append(check)

        title = Gtk.Label(label="Installation Complete!")
        title.add_css_class("title-1")
        title.set_halign(Gtk.Align.CENTER)
        self.content.append(title)

        desc = Gtk.Label(label="Bingux has been installed successfully.\nRemove the installation media and reboot to start using your new system.")
        desc.add_css_class("dim-label")
        desc.set_halign(Gtk.Align.CENTER)
        desc.set_justify(Gtk.Justification.CENTER)
        desc.set_wrap(True)
        self.content.append(desc)

        # Bottom bar: View Log (left), Reboot (right)
        bar = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)
        bar.set_margin_start(24)
        bar.set_margin_end(24)
        bar.set_margin_top(8)
        bar.set_margin_bottom(12)

        log_btn = Gtk.Button(label="View Log")
        log_btn.add_css_class("pill")
        log_btn.connect("clicked", self._on_view_log)
        bar.append(log_btn)

        spacer = Gtk.Box()
        spacer.set_hexpand(True)
        bar.append(spacer)

        reboot_btn = Gtk.Button(label="Reboot Now")
        reboot_btn.add_css_class("pill")
        reboot_btn.add_css_class("suggested-action")
        reboot_btn.connect("clicked", self._on_reboot)
        bar.append(reboot_btn)

        self.toolbar_view.add_bottom_bar(bar)

    def _on_view_log(self, _btn):
        try:
            subprocess.Popen(["gnome-text-editor", "/tmp/bingux-install-latest.log"])
        except FileNotFoundError:
            subprocess.Popen(["xdg-open", "/tmp/bingux-install-latest.log"])

    def _on_reboot(self, _btn):
        try:
            subprocess.run(["reboot"])
        except FileNotFoundError:
            pass
