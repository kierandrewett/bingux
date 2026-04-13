import os
import sys


def has_display():
    return bool(os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY"))


def main():
    if has_display():
        import gi
        gi.require_version("Gtk", "4.0")
        gi.require_version("Adw", "1")
        from application import BinguxInstallerApp
        app = BinguxInstallerApp()
        return app.run(sys.argv)
    else:
        from tui import run_tui
        return run_tui()


if __name__ == "__main__":
    sys.exit(main())
