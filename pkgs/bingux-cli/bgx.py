#!/usr/bin/env python3
"""bgx — Bingux package manager."""

import os
import subprocess
import sys
import threading
import time


VOLATILE_PROFILE = f"/nix/var/nix/profiles/per-user/{os.environ.get('USER', 'root')}/bgx-volatile"
PERMANENT_PROFILE = os.path.expanduser("~/.local/state/nix/profiles/profile")

# Colors — mostly grays with white for emphasis
WHITE = "\033[97m"
GRAY = "\033[37m"
DARK = "\033[90m"
BOLD = "\033[1m"
DIM = "\033[2m"
RESET = "\033[0m"
ACCENT = "\033[38;5;111m"   # soft blue
SUCCESS = "\033[38;5;114m"  # soft green
WARN = "\033[38;5;180m"     # soft amber
FAIL = "\033[38;5;174m"     # soft red

COL_NAME = 28
COL_VER = 14
COL_SIZE = 14
VERSION = "0.2.0"


class Spinner:
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
            sys.stdout.write(f"\r  {DARK}{frame}{RESET} {GRAY}{self.msg}{RESET}")
            sys.stdout.flush()
            i += 1
            time.sleep(0.08)


def run(cmd, **kwargs):
    return subprocess.run(cmd, **kwargs)


def format_size(nbytes):
    for unit in ("B", "KB", "MB", "GB"):
        if nbytes < 1024:
            return f"{nbytes:.1f} {unit}"
        nbytes /= 1024
    return f"{nbytes:.1f} TB"


def pkg_info(pkg):
    import json
    info = {"name": pkg, "version": "", "description": "", "size": ""}
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
    try:
        import re
        r = run(["nix", "path-info", "-S", f"nixpkgs#{pkg}"],
                capture_output=True, text=True, timeout=30)
        # Parse stderr for "X.XX MiB download, Y.YY MiB unpacked"
        for line in (r.stderr or "").split("\n"):
            m = re.search(r"([\d.]+)\s+([KMGT]iB)\s+download,\s+([\d.]+)\s+([KMGT]iB)\s+unpacked", line)
            if m:
                info["size"] = f"{m.group(3)} {m.group(4)}"
                break
        # Fallback: parse stdout for store path size
        if not info["size"] and r.stdout:
            parts = r.stdout.strip().split()
            if len(parts) >= 2:
                try:
                    info["size"] = format_size(int(parts[-1]))
                except ValueError:
                    pass
    except (subprocess.TimeoutExpired, Exception):
        pass
    return info


def confirm():
    try:
        ans = input(f"  {DARK}Proceed?{RESET} {GRAY}[y/N]{RESET} ")
        return ans.strip().lower() in ("y", "yes")
    except (EOFError, KeyboardInterrupt):
        print()
        return False


def _term_width():
    try:
        return os.get_terminal_size().columns
    except OSError:
        return 80


def pkg_row(name, version="", size="", description=""):
    n = name.ljust(COL_NAME)
    v = (version or "-").ljust(COL_VER)
    s = (size or "-").ljust(COL_SIZE)
    prefix_len = 4 + COL_NAME + 1 + COL_VER + 1 + COL_SIZE + 1
    max_desc = _term_width() - prefix_len - 2
    if max_desc > 0 and len(description) > max_desc:
        description = description[:max_desc - 1] + "\u2026"
    return f"    {WHITE}{n} {RESET}{WHITE}{v} {RESET}{GRAY}{s} {RESET}{DARK}{description}{RESET}"


def _print_table(label, label_color, infos):
    print(f"  {label_color}\u25b8{RESET} {WHITE}{label}{RESET}")
    line_w = _term_width() - 6
    print(f"    {DARK}{'Package'.ljust(COL_NAME)} {'Version'.ljust(COL_VER)} {'Size'.ljust(COL_SIZE)} Description{RESET}")
    print(f"    {DARK}{'\u2500' * line_w}{RESET}")
    for info in infos:
        print(pkg_row(info["name"], info["version"], info.get("size", ""), info["description"]))
    print()


def show_transaction(installs, removes, save=False):
    if not installs and not removes:
        return True

    print()

    if installs:
        mode = "permanently" if save else "for this session"
        _print_table(f"Installing {DARK}({mode}){RESET}", ACCENT, installs)

    if removes:
        _print_table("Removing", WARN, removes)

    ni = len(installs)
    nr = len(removes)
    print(f"  {GRAY}Summary: {SUCCESS}+{ni}{RESET}{DARK}/{RESET}{FAIL}-{nr}{RESET}{DARK}/{RESET}{WARN}~0{RESET}")
    print()

    return confirm()


def do_install(pkgs, save=False, skip_confirm=False):
    profile = PERMANENT_PROFILE if save else VOLATILE_PROFILE

    sp = Spinner("Resolving packages...")
    sp.start()
    infos = [pkg_info(p) for p in pkgs]
    count = len(infos)
    sp.stop(f"{DARK}Resolved {count} {'package' if count == 1 else 'packages'}.{RESET}")

    if not skip_confirm and not show_transaction(infos, [], save=save):
        print(f"  {DARK}Aborted.{RESET}")
        return False

    failed = 0
    for pkg in pkgs:
        sp = Spinner(f"Installing {pkg}...")
        sp.start()
        r = run(["nix", "profile", "install", "--profile", profile, f"nixpkgs#{pkg}"],
                capture_output=True)
        if r.returncode == 0:
            sp.stop(f"{SUCCESS}\u2713{RESET} {WHITE}{pkg}{RESET}")
        else:
            sp.stop(f"{FAIL}\u2717{RESET} {WHITE}{pkg}{RESET}")
            failed += 1

    if failed == 0 and len(pkgs) > 0:
        print(f"\n  {DARK}All packages installed.{RESET}\n")
    elif failed:
        print(f"\n  {FAIL}{failed} failed.{RESET}\n")

    return failed == 0


def do_remove(pkgs, skip_confirm=False):
    if not skip_confirm:
        sp = Spinner("Resolving packages...")
        sp.start()
        infos = [pkg_info(p) for p in pkgs]
        count = len(infos)
        sp.stop(f"{DARK}Resolved {count} {'package' if count == 1 else 'packages'}.{RESET}")

        if not show_transaction([], infos):
            print(f"  {DARK}Aborted.{RESET}")
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
                print(f"  {SUCCESS}\u2713{RESET} {WHITE}{pkg}{RESET} {DARK}({label}){RESET}")
                removed = True

        if not removed:
            print(f"  {FAIL}\u2717{RESET} {WHITE}{pkg}{RESET} {DARK}not installed{RESET}", file=sys.stderr)
            failed += 1

    return failed == 0


def do_search(query):
    run(["nix", "search", "nixpkgs", query])


def do_list():
    for profile, label, marker in [
        (VOLATILE_PROFILE, "Session", WARN),
        (PERMANENT_PROFILE, "Permanent", SUCCESS),
    ]:
        print(f"\n  {marker}\u25cf{RESET} {WHITE}{label}{RESET}")
        r = run(["nix", "profile", "list", "--profile", profile], capture_output=True, text=True)
        if r.returncode == 0 and r.stdout.strip():
            for line in r.stdout.strip().split("\n"):
                print(f"    {GRAY}{line}{RESET}")
        else:
            print(f"    {DARK}(none){RESET}")
    print()


def run_prefix_mode(args):
    installs, saves, removes = [], [], []

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
            print(f"  {FAIL}\u2717{RESET} Unknown: {arg}", file=sys.stderr)
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
    cmd, rest = args[0], args[1:]

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
            print(f"  {DARK}Package name required.{RESET}", file=sys.stderr)
            sys.exit(1)
        if not do_install(pkgs, save=save, skip_confirm=yes):
            sys.exit(1)

    elif cmd in ("remove", "uninstall", "rm", "r"):
        yes = "-y" in rest or "--yes" in rest
        pkgs = [a for a in rest if a not in ("-y", "--yes")]
        if not pkgs:
            print(f"  {DARK}Package name required.{RESET}", file=sys.stderr)
            sys.exit(1)
        if not do_remove(pkgs, skip_confirm=yes):
            sys.exit(1)

    elif cmd in ("search", "s", "q"):
        if not rest:
            print(f"  {DARK}Search query required.{RESET}", file=sys.stderr)
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
  {WHITE}{BOLD}bgx{RESET} {DARK}v{VERSION} — Bingux package manager{RESET}

  {WHITE}Quick syntax:{RESET}
    {GRAY}{"bgx +firefox".ljust(C1)}{DARK}Install for this session{RESET}
    {GRAY}{"bgx ++firefox".ljust(C1)}{DARK}Install permanently{RESET}
    {GRAY}{"bgx -firefox".ljust(C1)}{DARK}Remove{RESET}
    {GRAY}{"bgx +pkg1 +pkg2 -pkg3".ljust(C1)}{DARK}Batch operations{RESET}
    {GRAY}{"bgx ?query".ljust(C1)}{DARK}Search{RESET}

  {WHITE}Commands:{RESET}
    {GRAY}{"install, add, a".ljust(C1)}{DARK}Install packages{RESET}
    {GRAY}{"remove, rm, r".ljust(C1)}{DARK}Remove packages{RESET}
    {GRAY}{"search, s, q".ljust(C1)}{DARK}Search nixpkgs{RESET}
    {GRAY}{"list, ls".ljust(C1)}{DARK}List installed packages{RESET}
    {GRAY}{"help".ljust(C1)}{DARK}Show this help{RESET}

  {WHITE}Flags:{RESET}
    {GRAY}{"-s, --save".ljust(C1)}{DARK}Install permanently (persists after reboot){RESET}
    {GRAY}{"-y, --yes".ljust(C1)}{DARK}Skip confirmation prompt{RESET}
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
