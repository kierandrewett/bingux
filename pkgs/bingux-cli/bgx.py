#!/usr/bin/env python3
"""bgx — Bingux package manager."""

import os
import subprocess
import sys
import threading
import time


_USER = os.environ.get('USER', 'root')
VOLATILE_PROFILE = f"/tmp/bgx-{_USER}"
PERMANENT_PROFILE = f"/nix/var/nix/profiles/per-user/{_USER}/bgx-saved"

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

MIN_NAME = 16
MIN_VER = 10
MIN_SIZE = 10
MIN_DESC = 20
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
    for unit in ("B", "KiB", "MiB", "GiB"):
        if nbytes < 1024:
            return f"{nbytes:.2f} {unit}"
        nbytes /= 1024
    return f"{nbytes:.2f} TiB"


def pkg_info(pkg):
    import json
    info = {"name": pkg, "version": "", "description": "", "size": "", "size_bytes": 0}
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
                units = {"KiB": 1024, "MiB": 1024**2, "GiB": 1024**3, "TiB": 1024**4}
                info["size_bytes"] = int(float(m.group(3)) * units.get(m.group(4), 1))
                info["size"] = format_size(info["size_bytes"])
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


def _calc_cols(infos, show_size=True):
    """Calculate dynamic column widths based on content."""
    tw = _term_width()
    cn = max((len(i["name"]) for i in infos), default=0)
    cv = max((len(i.get("version") or "-") for i in infos), default=0)
    cs = max((len(i.get("size") or "-") for i in infos), default=0) if show_size else 0

    cn = max(cn, MIN_NAME) + 2
    cv = max(cv, MIN_VER) + 2
    cs = (max(cs, MIN_SIZE) + 2) if show_size else 0

    # Cap name at 40% of terminal
    cn = min(cn, int(tw * 0.4))
    cd = max(tw - 4 - cn - cv - cs - 3, MIN_DESC)
    return cn, cv, cs, cd


def _fmt_row(name, version, size, description, cols, name_color=WHITE):
    cn, cv, cs, cd = cols
    n = name[:cn-1].ljust(cn) if len(name) >= cn else name.ljust(cn)
    v = (version or "-")[:cv-1].ljust(cv) if len(version or "-") >= cv else (version or "-").ljust(cv)
    desc = description
    if len(desc) > cd:
        desc = desc[:cd-1] + "\u2026"
    if cs:
        s = (size or "-")[:cs-1].ljust(cs) if len(size or "-") >= cs else (size or "-").ljust(cs)
        return f"    {name_color}{n}{RESET} {WHITE}{v}{RESET} {GRAY}{s}{RESET} {DARK}{desc}{RESET}"
    else:
        return f"    {name_color}{n}{RESET} {WHITE}{v}{RESET} {DARK}{desc}{RESET}"


def _print_table(label, label_color, infos, name_color=WHITE, show_size=True):
    cols = _calc_cols(infos, show_size=show_size)
    cn, cv, cs, cd = cols
    print(f"  {label_color}\u276f{RESET} {WHITE}{label}{RESET}")
    line_w = _term_width() - 6
    if show_size:
        print(f"    {DARK}{'Package'.ljust(cn)} {'Version'.ljust(cv)} {'Size'.ljust(cs)} Description{RESET}")
    else:
        print(f"    {DARK}{'Package'.ljust(cn)} {'Version'.ljust(cv)} Description{RESET}")
    print(f"    {DARK}{'\u2500' * line_w}{RESET}")
    for info in infos:
        print(_fmt_row(info["name"], info.get("version", ""), info.get("size", ""), info.get("description", ""), cols, name_color))
    print()


def show_transaction(installs, removes, save=False):
    if not installs and not removes:
        return True

    print()

    if installs:
        mode = "permanently" if save else "for this session"
        _print_table(f"Installing {DARK}({mode}){RESET}", SUCCESS, installs, name_color=SUCCESS)

    if removes:
        neg_removes = []
        for info in removes:
            r = dict(info)
            if r.get("size"):
                r["size"] = f"-{r['size']}"
            neg_removes.append(r)
        _print_table("Removing", WARN, neg_removes, name_color=FAIL)

    ni = len(installs)
    nr = len(removes)
    summary = f"  {GRAY}Summary: {SUCCESS}+{ni}{RESET}{DARK}/{RESET}{FAIL}-{nr}{RESET}{DARK}/{RESET}{WARN}~0{RESET}"

    size_parts = []
    if installs:
        total_add = sum(i.get("size_bytes", 0) for i in installs)
        if total_add:
            size_parts.append(f"{SUCCESS}+{format_size(total_add)}{RESET}")
    if removes:
        total_rm = sum(i.get("size_bytes", 0) for i in removes)
        if total_rm:
            size_parts.append(f"{FAIL}-{format_size(total_rm)}{RESET}")
    if size_parts:
        summary += f"  {DARK}({' '.join(size_parts)}{DARK}){RESET}"

    print(summary)
    print()

    return confirm()


def _is_installed(pkg):
    """Check if a package is installed in either profile."""
    for profile in (VOLATILE_PROFILE, PERMANENT_PROFILE):
        r = run(["nix", "profile", "list", "--profile", profile],
                capture_output=True, text=True)
        if r.returncode == 0 and pkg in r.stdout:
            return True
    return False


def _ensure_profile_dir(profile):
    """Ensure the parent directory for a nix profile exists."""
    d = os.path.dirname(profile)
    if d and not os.path.isdir(d):
        os.makedirs(d, mode=0o755, exist_ok=True)


def do_install(pkgs, save=False, skip_confirm=False):
    profile = PERMANENT_PROFILE if save else VOLATILE_PROFILE
    _ensure_profile_dir(profile)

    # Check for already installed
    already = [p for p in pkgs if _is_installed(p)]
    if already:
        for p in already:
            print(f"  {WARN}\u276f{RESET} {WHITE}{p}{RESET} {DARK}is already installed.{RESET}")
        pkgs = [p for p in pkgs if p not in already]
        if not pkgs:
            return True

    sp = Spinner("Resolving packages...")
    sp.start()
    infos = [pkg_info(p) for p in pkgs]
    sp.stop(f"{DARK}Resolved {len(infos)} {'package' if len(infos) == 1 else 'packages'}.{RESET}")

    # Check for packages that don't exist in nixpkgs
    not_found = [i for i in infos if not i["version"] and not i["description"]]
    if not_found:
        for i in not_found:
            print(f"  {FAIL}\u2717{RESET} {WHITE}{i['name']}{RESET} {DARK}not found in nixpkgs. Try 'bgx ?{i['name']}' to search.{RESET}")
        infos = [i for i in infos if i not in not_found]
        if not infos:
            return False

    if not skip_confirm and not show_transaction(infos, [], save=save):
        print(f"  {DARK}Aborted.{RESET}")
        return False

    failed = 0
    for pkg in pkgs:
        sp = Spinner(f"Installing {pkg}...")
        sp.start()
        r = run(["nix", "profile", "add", "--profile", profile, f"nixpkgs#{pkg}"],
                capture_output=True, text=True)
        if r.returncode == 0:
            sp.stop(f"{SUCCESS}\u2713{RESET} {WHITE}{pkg}{RESET}")
        else:
            output = (r.stderr or r.stdout or "").strip()
            # Get the most useful error line (skip blank/trace lines)
            lines = [l for l in output.split("\n") if l.strip() and not l.strip().startswith("…")]
            summary = lines[-1].strip() if lines else "unknown error"
            sp.stop(f"{FAIL}\u2717{RESET} {WHITE}{pkg}{RESET}")
            for i, line in enumerate(lines[-5:]):
                if i < len(lines[-5:]) - 1:
                    print(f"    {DARK}\u2502 {line.strip()}{RESET}")
                else:
                    print(f"    {DARK}\u2570 {line.strip()}{RESET}")
            failed += 1

    if failed == 0 and len(pkgs) > 0:
        print(f"\n  {DARK}All packages installed.{RESET}\n")
    elif failed:
        print(f"\n  {FAIL}{failed} failed.{RESET}\n")

    return failed == 0


def do_remove(pkgs, skip_confirm=False):
    # Check for not installed
    not_installed = [p for p in pkgs if not _is_installed(p)]
    if not_installed:
        for p in not_installed:
            print(f"  {FAIL}\u2717{RESET} {WHITE}{p}{RESET} {DARK}is not installed.{RESET}")
        pkgs = [p for p in pkgs if p not in not_installed]
        if not pkgs:
            return len(not_installed) == 0

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
    import re
    sp = Spinner(f"Searching for '{query}'...")
    sp.start()
    r = run(["nix", "search", "nixpkgs", query], capture_output=True, text=True)
    sp.stop(f"{DARK}Search complete.{RESET}")

    if r.returncode != 0 or not r.stdout.strip():
        print(f"  {DARK}No results found.{RESET}")
        return

    # Parse nix search output
    ansi_re = re.compile(r"\033\[[0-9;]*m")
    lines = r.stdout.strip().split("\n")
    results = []
    current = None

    # Package paths to skip (deeply nested SDK/internal packages)
    skip_prefixes = ()

    for line in lines:
        clean = ansi_re.sub("", line).strip()
        if clean.startswith("* "):
            if current:
                results.append(current)
            rest = clean[2:]
            # Format: legacyPackages.x86_64-linux.pkgname (version)
            m = re.match(r"legacyPackages\.\S+?\.(.+?)\s*\(([^)]*)\)", rest)
            if m:
                attr = m.group(1)
                # Skip deeply nested internal packages
                if any(attr.startswith(p) for p in skip_prefixes):
                    current = None
                    continue
                # Use last segment as display name, but keep full path if nested
                parts = attr.split(".")
                name = parts[-1] if len(parts) <= 2 else attr
                current = {"name": name, "version": m.group(2), "description": ""}
            else:
                current = None
        elif current and clean:
            current["description"] = clean

    if current:
        results.append(current)

    # Filter junk: no version, versions that look like filenames, duplicates
    seen = set()
    filtered = []
    for r in results:
        v = r.get("version", "")
        if not v:
            continue
        if ".zip" in v or ".tar" in v or ".iso" in v:
            continue
        key = r["name"]
        if key in seen:
            continue
        seen.add(key)
        filtered.append(r)
    results = sorted(filtered, key=lambda r: r["name"].lower())

    if not results:
        print(f"  {DARK}No results found.{RESET}")
        return

    _print_table(f"Results for '{query}'", ACCENT, results, name_color=WHITE, show_size=False)
    print(f"  {DARK}{len(results)} results.{RESET}")


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
    yes = False

    for arg in args:
        if arg in ("-y", "--yes"):
            yes = True
        elif arg.startswith("++"):
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
    changed = False
    if installs:
        r = do_install(installs, save=False, skip_confirm=yes)
        ok = r and ok
        changed = changed or r
    if saves:
        r = do_install(saves, save=True, skip_confirm=yes)
        ok = r and ok
        changed = changed or r
    if removes:
        r = do_remove(removes, skip_confirm=yes)
        ok = r and ok
        changed = changed or r
    if not changed:
        print(f"  {DARK}No changes were made.{RESET}")
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

    elif cmd in ("search", "s", "q", "?"):
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
