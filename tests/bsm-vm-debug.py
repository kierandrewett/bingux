#!/usr/bin/env python3
"""Boot the image with the bsm test harness's two displays and run shell commands in the guest.

usage: tests/bsm-vm-debug.py [--inject DIR] [--keys TEXT] [--wait SECONDS] [--shot PATH] COMMAND...

A debugging aid for failures found by tests/bsm-qemu.py. Boots the newest ISO
exactly as that test does (bingux.bsm, two heads, optional 9p injection of
tests/bsm-vm-inject.py's tree, applied the way the test applies it), waits
`--wait` seconds (default 25), optionally sends QEMU key presses (`--keys abc`),
saves a screenshot of display 0 (`--shot`), and runs each COMMAND on the guest's
serial root shell, printing its output. Working files stay under /tmp/bsm-vm.
"""

import argparse
from pathlib import Path
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))

from bsm_harness import BsmVm  # noqa: E402
from distro_vm import find_iso  # noqa: E402


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--inject")
    parser.add_argument("--iso")
    parser.add_argument("--keys", default="", help="key presses; a backslash-n in the text is Enter")
    parser.add_argument("--after", type=float, default=0, help="seconds to wait after the keys, before the commands")
    parser.add_argument("--wait", type=float, default=25)
    parser.add_argument("--shot")
    parser.add_argument("--user", action="append", default=[], help="create this account (password = name) before bsm starts")
    parser.add_argument("commands", nargs="*")
    args = parser.parse_args()
    base = Path("/tmp/bsm-vm")
    base.mkdir(exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="dbg-", dir=base))
    vm = BsmVm(work, iso=find_iso(args.iso), record=work / "rec", inject=args.inject)
    vm.start()
    try:
        if not vm.start_shell():
            print("no serial shell")
            return 1
        for name in args.user:
            vm.run(f"id {name} >/dev/null 2>&1 || (useradd -m {name} && echo {name}:{name} | chpasswd)", 30)
        if args.inject:
            print(
                vm.run(
                    "mkdir -p /mnt/bsm-inject && modprobe 9pnet_virtio; modprobe 9p; "
                    "mount -t 9p -o trans=virtio,version=9p2000.L,ro bsminject /mnt/bsm-inject "
                    "&& sh /mnt/bsm-inject/apply.sh 2>&1 | tail -5",
                    300,
                )
            )
        time.sleep(args.wait)
        for char in args.keys.replace("\\n", "\n"):
            vm.key({" ": "spc", "\n": "ret"}.get(char, char))
            time.sleep(0.3)
        time.sleep(1.5 + args.after)
        if args.shot:
            from PIL import Image

            Image.fromarray(vm.frame()).save(args.shot)
        for command in args.commands:
            print("$", command)
            print(vm.run(command, 120))
    finally:
        vm.stop()
    return 0


if __name__ == "__main__":
    sys.exit(main())
