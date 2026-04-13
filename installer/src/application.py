import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gio
from window import BinguxInstallerWindow


class BinguxInstallerApp(Adw.Application):
    def __init__(self):
        super().__init__(
            application_id="dev.drewett.BinguxInstaller",
            flags=Gio.ApplicationFlags.FLAGS_NONE,
        )

    def do_activate(self):
        win = self.props.active_window
        if not win:
            win = BinguxInstallerWindow(application=self)
        win.present()
