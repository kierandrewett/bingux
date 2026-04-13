#!/usr/bin/env python3
"""bgx — Bingux package manager."""

import os
import subprocess
import sys
import threading
import time


VOLATILE_PROFILE = f"/nix/var/nix/profiles/per-user/{os.environ.get('USER', 'root')}/bgx-volatile"
PERMANENT_PROFILE = os.path.expanduser("~/.local/state/nix/profiles/profile")

BOLD = "\033[1m"
DIM = "\033[2m"
RESET = "\033[0m"

COL_NAME = 28
COL_VER = 14

VERSION = "0.2.0"


class Spinner:
    """Braille dot spinner for async operations."""
    FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]

    def __init__(self, msg):
        self.msg = msg
        self._stop = False
        self._thread = None

    def start(self):
        self._stop = False
        self._thread = threading.Thread(target=self._spin, daemon=True)
        self._thread.start()

    def stop(self, result=""):
        self._stop = True
        if self._thread:
            self._thread.join()
        sys.stdout.write(f"\r\033[K  {result}\n")
        sys.stdout.flush()

    def _spin(self):
        i = 0
        while not self._stop:
            frame = self.FRAMES[i % len(self.FRAMES)]
            sys.stdout.write(f"\r  {frame} {self.msg}")
            sys.stdout.flush()
            i += 1
            time.sleep(0.08)


def run(cmd, **kwargs):
    return subprocess.run(cmd, **kwargs)


def pkg_info(pkg):
    info = {"name": pkg, "version": "", "description": ""}
    try:
        r = run(["nix", "eval", "--raw", f"nixpkgs#{pkg}.version"],
                capture_output=True, text=True, timeout=15)
        if r.returncode == 0:
            info["version"] = r.stdout.strip()
    except subprocess.TimeoutExpired:
        pass
    try:
        r = run(["nix", "eval", "--raw", f"nixpkgs#{pkg}.meta.description"],
                capture_output=True, text=True, timeout=15)
        if r.returncode == 0:
            info["description"] = r.stdout.strip()
    except subprocess.TimeoutExpired:
        pass
    return info


def confirm(prompt="  Proceed? [y/N] "):
    try:
        return input(prompt).strip().lower() in ("y", "yes")
    except (EOFError, KeyboardInterrupt):
        print()
        return False


def show_transaction(installs, removes, save=False):
    if not installs and not removes:
        return True

    if installs:
        mode = "permanently" if save else "for this session"
        print(f"\n  Installing ({mode}):")
        print(f"  {DIM}{'Package'.ljust(COL_NAME)} {'Version'.ljust(COL_VER)} Description{RESET}")
        for info in installs:
            n = info["name"].ljust(COL_NAME)
            v = (info["version"] or "").ljust(COL_VER)
            d = info["description"]
            print(f"  {BOLD}{n}{RESET} {v} {DIM}{d}{RESET}")

    if removes:
        print(f"\n  Removing:")
        for pkg in removes:
            print(f"  {pkg}")

    parts = []
    if installs:
        parts.append(f"{len(installs)} to install")
    if removes:
        parts.append(f"{len(removes)} to remove")
    print(f"\n  {DIM}{', '.join(parts)}{RESET}\n")

    return confirm()


def do_install(pkgs, save=False, skip_confirm=False):
    profile = PERMANENT_PROFILE if save else VOLATILE_PROFILE

    sp = Spinner("Resolving packages...")
    sp.start()
    infos = [pkg_info(p) for p in pkgs]
    sp.stop("Done.")

    if not skip_confirm and not show_transaction(infos, [], save=save):
        print("  Aborted.")
        return False

    failed = 0
    for pkg in pkgs:
        sp = Spinner(f"Installing {pkg}...")
        sp.start()
        r = run(["nix", "profile", "install", "--profile", profile, f"nixpkgs#{pkg}"],
                capture_output=True)
        if r.returncode == 0:
            sp.stop(f"\u2713 {pkg}")
        else:
            sp.stop(f"\u2717 {pkg}")
            failed += 1

    return failed == 0


def do_remove(pkgs, skip_confirm=False):
    if not skip_confirm and not show_transaction([], pkgs):
        print("  Aborted.")
        return False

    failed = 0
    for pkg in pkgs:
        removed = False
        for profile, label in [
            (VOLATILE_PROFILE, "session"),
            (PERMANENT_PROFILE, "permanent"),
        ]:
            r = run(
                ["nix", "profile", "remove", "--profile", profile, f".*{pkg}.*"],
                capture_output=True,
            )
            if r.returncode == 0:
                print(f"  \u2713 {pkg} ({label})")
                removed = True

        if not removed:
            print(f"  \u2717 {pkg} not installed", file=sys.stderr)
            failed += 1

    return failed == 0


def do_search(query):
    run(["nix", "search", "nixpkgs", query])


def do_list():
    for profile, label in [
        (VOLATILE_PROFILE, "Session"),
        (PERMANENT_PROFILE, "Permanent"),
    ]:
        print(f"\n  {BOLD}{label}{RESET}")
        r = run(["nix", "profile", "list", "--profile", profile], capture_output=True, text=True)
        if r.returncode == 0 and r.stdout.strip():
            for line in r.stdout.strip().split("\n"):
                print(f"    {line}")
        else:
            print(f"    {DIM}(none){RESET}")
    print()


def run_prefix_mode(args):
    installs = []
    saves = []
    removes = []

    for arg in args:
        if arg.startswith("++"):
            saves.append(arg[2:])
        elif arg.startswith("+"):
            installs.append(arg[1:])
        elif arg.startswith("-"):
            removes.append(arg[1:])
        elif arg.startswith("?"):
            do_search(arg[1:])
            return
        else:
            print(f"  \u2717 Unknown: {arg}", file=sys.stderr)
            sys.exit(1)

    ok = True
    if installs:
        ok = do_install(installs, save=False) and ok
    if saves:
        ok = do_install(saves, save=True) and ok
    if removes:
        ok = do_remove(removes) and ok

    if not ok:
        sys.exit(1)


def run_subcommand_mode(args):
    cmd = args[0]
    rest = args[1:]

    if cmd in ("install", "add", "a"):
        save = False
        yes = False
        pkgs = []
        for arg in rest:
            if arg in ("-s", "--save"):
                save = True
            elif arg in ("-y", "--yes"):
                yes = True
            else:
                pkgs.append(arg)
        if not pkgs:
            print("  Package name required.", file=sys.stderr)
            sys.exit(1)
        if not do_install(pkgs, save=save, skip_confirm=yes):
            sys.exit(1)

    elif cmd in ("remove", "uninstall", "rm", "r"):
        yes = "-y" in rest or "--yes" in rest
        pkgs = [a for a in rest if a not in ("-y", "--yes")]
        if not pkgs:
            print("  Package name required.", file=sys.stderr)
            sys.exit(1)
        if not do_remove(pkgs, skip_confirm=yes):
            sys.exit(1)

    elif cmd in ("search", "s", "q"):
        if not rest:
            print("  Search query required.", file=sys.stderr)
            sys.exit(1)
        do_search(" ".join(rest))

    elif cmd in ("list", "ls"):
        do_list()

    elif cmd in ("help", "--help", "-h"):
        print_usage()

    else:
        print_usage()
        sys.exit(1)


C1 = 36


def print_usage():
    print(f"""
  {BOLD}bgx{RESET} {DIM}v{VERSION} — Bingux package manager{RESET}

  {BOLD}Quick syntax:{RESET}
    {"bgx +firefox".ljust(C1)}{DIM}Install for this session{RESET}
    {"bgx ++firefox".ljust(C1)}{DIM}Install permanently{RESET}
    {"bgx -firefox".ljust(C1)}{DIM}Remove{RESET}
    {"bgx +pkg1 +pkg2 -pkg3".ljust(C1)}{DIM}Batch operations{RESET}
    {"bgx ?query".ljust(C1)}{DIM}Search{RESET}

  {BOLD}Commands:{RESET}
    {"install, add, a".ljust(C1)}{DIM}Install packages{RESET}
    {"remove, rm, r".ljust(C1)}{DIM}Remove packages{RESET}
    {"search, s, q".ljust(C1)}{DIM}Search nixpkgs{RESET}
    {"list, ls".ljust(C1)}{DIM}List installed packages{RESET}
    {"help".ljust(C1)}{DIM}Show this help{RESET}

  {BOLD}Flags:{RESET}
    {"-s, --save".ljust(C1)}{DIM}Install permanently (persists after reboot){RESET}
    {"-y, --yes".ljust(C1)}{DIM}Skip confirmation prompt{RESET}
""")


def main():
    args = sys.argv[1:]

    if not args:
        print_usage()
        return

    if args[0][0] in ("+", "-", "?"):
        run_prefix_mode(args)
    else:
        run_subcommand_mode(args)


if __name__ == "__main__":
    main()
