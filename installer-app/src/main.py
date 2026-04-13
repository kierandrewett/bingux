import sys
import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Gio
from application import BinguxInstallerApp


def main():
    app = BinguxInstallerApp()
    return app.run(sys.argv)


if __name__ == "__main__":
    sys.exit(main())
