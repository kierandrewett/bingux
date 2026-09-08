"""Opt-in GTK/AT-SPI integration check; opens and closes only our test picker."""
import json
import subprocess
import threading
import time
from pathlib import Path
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk, GLib

command = ["qs", "ipc", "--any-display", "-p",
           str(Path(__file__).resolve().parents[1] / "shell/bingux"), "call", "emoji"]


def status():
    return json.loads(subprocess.check_output(command + ["status"], text=True, timeout=3))


assert not status()["visible"], "Leave an existing picker untouched"
window = Gtk.Window(title="Bingux caret placement test")
entry = Gtk.Entry(text="Caret placement test")
window.add(entry)
window.show_all()
entry.grab_focus()
entry.set_position(-1)
passed = []


def exercise():
    try:
        time.sleep(.5)
        subprocess.run(command + ["open"], check=True, timeout=3)
        time.sleep(.4)
        result = status()
        assert result["visible"] and result["anchor"] == "caret", result
        subprocess.run(["grim", "/tmp/bingux-emoji-caret.png"], check=True, timeout=3)
        print("PASS native GTK caret placement:", result["x"], result["y"], flush=True)
        passed.append(True)
    finally:
        subprocess.run(command + ["close"], timeout=3)
        GLib.idle_add(Gtk.main_quit)


worker = threading.Thread(target=exercise)
worker.start()
Gtk.main()
worker.join()
window.destroy()
raise SystemExit(0 if passed else 1)
