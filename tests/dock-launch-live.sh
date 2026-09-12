#!/usr/bin/env bash
# Opens and closes a real isolated desktop application to check launch completion.
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
helper="$(mktemp)"
app_fixture="$(mktemp -d)"
trap 'rm -f "$helper"; rm -rf "$app_fixture"' EXIT
cat >"$app_fixture/bingux-dock-launch-app.py" <<'PY'
#!/usr/bin/env python3
import gi
gi.require_version("Gtk", "4.0")
from gi.repository import Gtk

class LaunchProbe(Gtk.Application):
    def __init__(self):
        super().__init__(application_id="com.example.BinguxDockTest")

    def do_activate(self):
        window = Gtk.ApplicationWindow(application=self)
        window.set_title("Bingux dock launch probe")
        window.set_default_size(320, 180)
        window.present()

LaunchProbe().run()
PY
chmod 700 "$app_fixture/bingux-dock-launch-app.py"
mkdir -p "$XDG_DATA_HOME/applications"
cat >"$XDG_DATA_HOME/applications/com.example.BinguxDockTest.desktop" <<EOF
[Desktop Entry]
Name=Bingux Dock Launch Probe
Exec=$app_fixture/bingux-dock-launch-app.py
Icon=application-x-executable
Terminal=false
Type=Application
Actions=new-window;

[Desktop Action new-window]
Name=New Window
Exec=$app_fixture/bingux-dock-launch-app.py
EOF
cat >"$helper" <<'SH'
#!/bin/sh
exec python3 "$BINGUX_LAUNCH_TEST_SOURCE" --host-launch "$@"
SH
chmod +x "$helper"
export BINGUX_LAUNCH_TEST_SOURCE="$repo_dir/shell/bingux/launch-application.py"
export BINGUX_DOCK_LAUNCH_TEST_ID=com.example.BinguxDockTest
export BINGUX_APP_LAUNCHER_HELPER="$helper" BINGUX_NOTES_NATIVE=1
exec_runner="$repo_dir/tests/sidebar-notes.sh"
"$exec_runner" dock-launch-live
