"""Exercise GIO desktop launch semantics without altering installed applications."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

launcher = Path(__file__).resolve().parents[1] / "shell/bingux/launch-application.py"


class ApplicationLaunch(unittest.TestCase):
    def test_missing_and_path_ids_return_clean_errors(self):
        for identity in ("bingux-no-such-test-app-984a5", "../invalid"):
            result = subprocess.run([sys.executable, str(launcher), identity], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn("Traceback", result.stderr)

    def test_desktop_action_and_field_codes(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            apps = directory / "applications"
            apps.mkdir()
            record = directory / "launched.json"
            entrypoint = directory / "entry.py"
            entrypoint.write_text("import json, os, sys\nfrom pathlib import Path\n"
                + f"Path({str(record)!r}).write_text(json.dumps({{'args': sys.argv[1:], 'cwd': os.getcwd()}}))\n")
            desktop = apps / "bingux-launch-test.desktop"
            desktop.write_text(f"[Desktop Entry]\nType=Application\nName=Test\nPath={directory}\n"
                + f"Exec={sys.executable} {entrypoint} normal %k\nActions=new-window;\n"
                + f"[Desktop Action new-window]\nName=New\nExec={sys.executable} {entrypoint} new-window %k\n")
            env = os.environ | {"XDG_DATA_HOME": temporary, "XDG_DATA_DIRS": temporary}
            for flags, expected in (([], "normal"), (["--new-window"], "new-window")):
                record.unlink(missing_ok=True)
                result = subprocess.run([sys.executable, str(launcher), *flags, "bingux-launch-test"], env=env, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                deadline = time.monotonic() + 3
                while not record.exists() and time.monotonic() < deadline: time.sleep(.01)
                self.assertTrue(record.exists(), result.stderr)
                state = json.loads(record.read_text())
                self.assertEqual(state, {"args": [expected, str(desktop)], "cwd": temporary})


if __name__ == "__main__": unittest.main()
