"""Command line: python3 -m vmdrive smoke --iso PATH --firmware uefi|bios --out DIR

Boots the ISO with one 4 GiB raw disk, runs a few commands on the serial shell,
takes a screenshot, powers off, and writes transcript.log, commands.jsonl,
results.json and screen.png into DIR.
"""

import argparse
import re
import sys
from pathlib import Path

from .checks import Checks
from .vm import LiveVm

DISK_BYTES = 4 * 1024**3
SMOKE_COMMANDS = ("uname -r", "lsblk -b -d -o NAME,SIZE", "cat /etc/os-release | head -3")


def smoke(args):
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    checks = Checks(out)
    vm = LiveVm(out)
    try:
        disk = vm.scratch / "disk0.raw"
        vm.start(iso=args.iso, disks=[(disk, DISK_BYTES)], firmware=args.firmware, ram_mb=args.ram_mb,
                 cpus=args.cpus, extra_disks_format="raw")
        if checks.expect(vm.wait_for_shell(timeout=args.timeout), f"{args.firmware}: root shell after autologin"):
            rc, uname = vm.run(SMOKE_COMMANDS[0], timeout=60)
            checks.expect(rc == 0 and uname.strip(), "uname -r exits 0 with a kernel release", f"rc={rc} out={uname!r}")
            rc, lsblk = vm.run(SMOKE_COMMANDS[1], timeout=60)
            disk_ok = re.search(rf"^vda\s+{DISK_BYTES}\s*$", lsblk, re.M) is not None
            checks.expect(rc == 0 and disk_ok, "lsblk shows vda at 4 GiB", f"rc={rc} out={lsblk!r}")
            rc, release = vm.run(SMOKE_COMMANDS[2], timeout=60)
            checks.expect(rc == 0 and "NAME=" in release, "os-release readable", f"rc={rc} out={release!r}")
            vm.screenshot(out / "screen.png")
            checks.expect((out / "screen.png").stat().st_size > 0, "screenshot written")
        vm.poweroff()
        checks.expect(not vm.alive(), "guest powered off")
    except Exception as error:  # the run is reported as failed, artifacts are still saved
        checks.expect(False, "driver completed without error", repr(error))
    finally:
        vm.close()
        checks.save()
    return 0 if checks.ok else 1


def main(argv=None):
    parser = argparse.ArgumentParser(prog="vmdrive", description=__doc__.split("\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    smoke_parser = sub.add_parser("smoke", help="boot the ISO and run the smoke checks")
    smoke_parser.add_argument("--iso", required=True, help="live ISO to boot as -cdrom")
    smoke_parser.add_argument("--firmware", choices=("uefi", "bios"), default="uefi")
    smoke_parser.add_argument("--out", required=True, help="artifact directory")
    smoke_parser.add_argument("--timeout", type=int, default=300, help="seconds to wait for the shell")
    smoke_parser.add_argument("--ram-mb", type=int, default=4096)
    smoke_parser.add_argument("--cpus", type=int, default=4)
    args = parser.parse_args(argv)
    if args.command == "smoke":
        return smoke(args)
    return 2


if __name__ == "__main__":
    sys.exit(main())
