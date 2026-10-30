#!/usr/bin/env python3
"""Run tests/bsm-greeter-client.qml: BsmAuthClient and BsmGreeterModel against crafted bsm messages.

No socket, compositor or display is needed (offscreen Qt). The QML is copied
next to a scratch directory with its imports pointed at this checkout, because
Quickshell only resolves relative imports inside the folder of the entry file.
"""

import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
QS = os.environ.get("QS_TEST_BIN", str(ROOT / "build/bingux-shell/quickshell/src/bingux-shell"))


def main():
    if not Path(QS).exists():
        print("missing bingux-shell binary: set QS_TEST_BIN")
        return 1
    with tempfile.TemporaryDirectory(prefix="bsm-greeter-client-") as scratch:
        home = Path(scratch)
        qml = home / "bsm-greeter-client.qml"
        qml.write_text(
            (ROOT / "tests/bsm-greeter-client.qml").read_text().replace("../shell/bingux", (ROOT / "shell/bingux").as_uri())
        )
        environment = os.environ | {
            "HOME": str(home),
            "XDG_RUNTIME_DIR": str(home),
            "QT_QPA_PLATFORM": "offscreen",
            "BINGUX_SOFTWARE_RENDERING": "1",
        }
        # Unset on purpose: Quickshell.env gives null, and the default must apply.
        environment.pop("BSM_GREETER_CACHE", None)
        environment.pop("BSM_CONTROL_SOCKET", None)
        environment.pop("WAYLAND_DISPLAY", None)
        result = subprocess.run([QS, "-p", str(qml)], env=environment, capture_output=True, text=True, timeout=90)
    output = result.stdout + result.stderr
    for line in output.splitlines():
        if "BSM_CLIENT_" in line or " ERROR" in line or "TypeError" in line:
            print(line)
    ok = "BSM_CLIENT_PASSED" in output and "BSM_CLIENT_FAILED" not in output
    print(("ok   " if ok else "FAIL ") + "bsm greeter client and model")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
