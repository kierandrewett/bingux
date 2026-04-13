#!/usr/bin/env python3
"""bgx — Bingux package manager."""

import os
import subprocess
import sys
import threading
import time


_USER = os.environ.get('USER', 'root')
_unfree_accepted = False
VOLATILE_PROFILE = f"/tmp/bgx-session-{_USER}-packages"
PERMANENT_PROFILE = f"/nix/var/nix/profiles/per-user/{_USER}/bgx/packages"

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
    info = {"name": pkg, "version": "", "description": "", "size": "", "size_bytes": 0, "unfree": False, "license": ""}
    try:
        r = run(["nix", "eval", "--raw", f"nixpkgs#{pkg}.version"],
                capture_output=True, text=True, timeout=15)
        if r.returncode == 0:
            info["version"] = r.stdout.strip()
        elif "unfree" in (r.stderr or "").lower():
            info["unfree"] = True
            info["license"] = "unfree"
    except subprocess.TimeoutExpired:
        pass
    try:
        r = run(["nix", "eval", "--raw", f"nixpkgs#{pkg}.meta.license.spdxId"],
                capture_output=True, text=True, timeout=15)
        if r.returncode == 0 and r.stdout.strip():
            info["license"] = r.stdout.strip()
        else:
            r = run(["nix", "eval", "--raw", f"nixpkgs#{pkg}.meta.license.shortName"],
                    capture_output=True, text=True, timeout=15)
            if r.returncode == 0 and r.stdout.strip():
                info["license"] = r.stdout.strip()
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


MIN_LIC = 8


def _calc_cols(infos, show_size=True, show_license=False):
    """Calculate dynamic column widths based on content."""
    tw = _term_width()
    cn = max((len(i["name"]) for i in infos), default=0)
    cv = max((len(i.get("version") or "-") for i in infos), default=0)
    cs = max((len(i.get("size") or "-") for i in infos), default=0) if show_size else 0
    cl = max((len(i.get("license") or "-") for i in infos), default=0) if show_license else 0

    cn = max(cn, MIN_NAME) + 2
    cv = max(cv, MIN_VER) + 2
    cs = (max(cs, MIN_SIZE) + 2) if show_size else 0
    cl = (max(cl, MIN_LIC) + 2) if show_license else 0

    cn = min(cn, int(tw * 0.4))
    cd = max(tw - 4 - cn - cv - cs - cl - 4, MIN_DESC)
    return cn, cv, cs, cl, cd


def _fmt_row(name, version, size, description, cols, name_color=WHITE, license=""):
    cn, cv, cs, cl, cd = cols
    n = name[:cn-1].ljust(cn) if len(name) >= cn else name.ljust(cn)
    v = (version or "-")[:cv-1].ljust(cv) if len(version or "-") >= cv else (version or "-").ljust(cv)
    desc = description
    if len(desc) > cd:
        desc = desc[:cd-1] + "\u2026"
    parts = f"    {name_color}{n}{RESET} {WHITE}{v}{RESET}"
    if cs:
        s = (size or "-")[:cs-1].ljust(cs) if len(size or "-") >= cs else (size or "-").ljust(cs)
        parts += f" {GRAY}{s}{RESET}"
    if cl:
        lic = (license or "-")[:cl-1].ljust(cl) if len(license or "-") >= cl else (license or "-").ljust(cl)
        lic_color = WARN if license == "unfree" else DARK
        parts += f" {lic_color}{lic}{RESET}"
    parts += f" {DARK}{desc}{RESET}"
    return parts


def _print_table(label, label_color, infos, name_color=WHITE, show_size=True, show_license=False):
    has_license = show_license and any(i.get("license") for i in infos)
    cols = _calc_cols(infos, show_size=show_size, show_license=has_license)
    cn, cv, cs, cl, cd = cols
    print(f"  {label_color}\u276f{RESET} {WHITE}{label}{RESET}")
    line_w = _term_width() - 6
    header = f"    {DARK}{'Package'.ljust(cn)} {'Version'.ljust(cv)}"
    if cs:
        header += f" {'Size'.ljust(cs)}"
    if cl:
        header += f" {'License'.ljust(cl)}"
    header += f" Description{RESET}"
    print(header)
    print(f"    {DARK}{'\u2500' * line_w}{RESET}")
    for info in infos:
        print(_fmt_row(info["name"], info.get("version", ""), info.get("size", ""), info.get("description", ""), cols, name_color, info.get("license", "")))
    print()


def show_transaction(installs, removes, save=False):
    if not installs and not removes:
        return True

    print()

    if installs:
        mode = "permanently" if save else "for this session"
        _print_table(f"Installing {DARK}({mode}){RESET}", SUCCESS, installs, name_color=SUCCESS, show_license=True)

    if removes:
        neg_removes = []
        for info in removes:
            r = dict(info)
            if r.get("size"):
                r["size"] = f"-{r['size']}"
            neg_removes.append(r)
        _print_table("Removing", WARN, neg_removes, name_color=FAIL, show_license=True)

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


def _is_in_profile(pkg, profile):
    """Check if a package is installed in a specific profile."""
    r = run(["nix", "profile", "list", "--profile", profile],
            capture_output=True, text=True)
    if r.returncode != 0:
        return False
    # Match package name precisely (not just substring)
    for line in r.stdout.split("\n"):
        if f"#{pkg}" in line or f"-{pkg}-" in line or line.strip().endswith(f"-{pkg}"):
            return True
    return False


def _is_installed(pkg):
    """Check if a package is installed in either profile."""
    return _is_in_profile(pkg, VOLATILE_PROFILE) or _is_in_profile(pkg, PERMANENT_PROFILE)


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

    # Handle unfree packages
    global _unfree_accepted
    unfree = [i for i in infos if i.get("unfree")]
    if unfree:
        is_nixos = os.path.exists("/etc/nixos")
        for i in unfree:
            print(f"  {WARN}\u276f{RESET} {WHITE}{i['name']}{RESET} {DARK}has an unfree license.{RESET}")
        if is_nixos:
            print(f"    {DARK}Enable unfree in your NixOS config: nixpkgs.config.allowUnfree = true;{RESET}")
            infos = [i for i in infos if not i.get("unfree")]
            if not infos:
                return False
        elif _unfree_accepted:
            for i in unfree:
                i["_allow_unfree"] = True
        else:
            if not confirm(f"  {DARK}Install unfree packages anyway?{RESET} [{GRAY}y/N{RESET}] "):
                infos = [i for i in infos if not i.get("unfree")]
                if not infos:
                    print(f"  {DARK}Aborted.{RESET}")
                    return False
            else:
                _unfree_accepted = True
                for i in unfree:
                    i["_allow_unfree"] = True

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

    # Build a lookup for unfree packages
    unfree_pkgs = {i["name"] for i in infos if i.get("_allow_unfree")}

    failed = 0
    for pkg in pkgs:
        cmd = ["nix", "profile", "add", "--profile", profile]
        env = None
        if pkg in unfree_pkgs:
            cmd.append("--impure")
            env = {**os.environ, "NIXPKGS_ALLOW_UNFREE": "1"}
        cmd.append(f"nixpkgs#{pkg}")

        # Stream progress from nix stderr
        import re as _re
        proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
        sp = Spinner(f"Installing {pkg}...")
        sp.start()

        stderr_lines = []
        progress_lines = []
        fetched = 0
        total_fetch = 0

        def _read_progress():
            nonlocal fetched, total_fetch
            for line in proc.stderr:
                line = line.strip()
                if not line:
                    continue
                stderr_lines.append(line)

                # Count total paths to fetch
                m = _re.match(r"these (\d+) paths will be fetched", line)
                if m:
                    total_fetch = int(m.group(1))
                    continue

                if "copying path" in line:
                    fetched += 1
                    m2 = _re.search(r"copying path '.*-([^/']+)'", line)
                    name = m2.group(1) if m2 else "..."
                    if total_fetch:
                        sp.msg = f"{pkg}: [{fetched}/{total_fetch}] {name}"
                    else:
                        sp.msg = f"{pkg}: fetching {name}"
                    progress_lines.append(f"    {DARK}\u2502 [{fetched}/{total_fetch or '?'}] {name}{RESET}")
                elif "building" in line.lower():
                    sp.msg = f"{pkg}: building..."
                elif "evaluating" in line.lower():
                    sp.msg = f"{pkg}: evaluating..."

        progress_thread = threading.Thread(target=_read_progress, daemon=True)
        progress_thread.start()
        proc.wait()
        sp._stop = True
        progress_thread.join(timeout=1)
        r_code = proc.returncode
        r_stderr = "\n".join(stderr_lines)
        if r_code == 0:
            sp.stop(f"{SUCCESS}\u2713{RESET} {WHITE}{pkg}{RESET}")
            if progress_lines:
                for pl in progress_lines[:-1]:
                    print(pl)
                print(f"    {DARK}\u2570 {fetched} paths fetched{RESET}")
        else:
            output = r_stderr.strip()
            sp.stop(f"{FAIL}\u2717{RESET} {WHITE}{pkg}{RESET}")

            if "unfree" in output.lower():
                if os.path.exists("/etc/nixos"):
                    print(f"    {DARK}\u2502 This package has an unfree license.{RESET}")
                    print(f"    {DARK}\u2570 Add to your NixOS config: nixpkgs.config.allowUnfree = true;{RESET}")
                else:
                    # Non-NixOS: retry with unfree, overwrite the ✗ line
                    sys.stdout.write(f"\033[A\r\033[K")
                    retry_env = {**os.environ, "NIXPKGS_ALLOW_UNFREE": "1"}
                    retry_cmd = ["nix", "profile", "add", "--impure", "--profile", profile, f"nixpkgs#{pkg}"]
                    proc2 = subprocess.Popen(retry_cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=retry_env)
                    sp2 = Spinner(f"{pkg}: retrying (unfree)...")
                    sp2.start()

                    def _read_retry():
                        for line in proc2.stderr:
                            line = line.strip()
                            if not line:
                                continue
                            if "copying path" in line:
                                m = _re.search(r"copying path '.*-([^/']+)'", line)
                                if m:
                                    sp2.msg = f"{pkg}: fetching {m.group(1)}"
                            elif "building" in line.lower():
                                sp2.msg = f"{pkg}: building..."
                            elif "downloading" in line.lower():
                                sp2.msg = f"{pkg}: downloading..."

                    rt = threading.Thread(target=_read_retry, daemon=True)
                    rt.start()
                    proc2.wait()
                    sp2._stop = True
                    rt.join(timeout=1)
                    if proc2.returncode == 0:
                        sp2.stop(f"{SUCCESS}\u2713{RESET} {WHITE}{pkg}{RESET} {DARK}(unfree){RESET}")
                        continue
                    sp2.stop(f"{FAIL}\u2717{RESET} {WHITE}{pkg}{RESET}")
                    print(f"    {DARK}\u2570 Failed to install unfree package.{RESET}")
            elif "does not provide" in output:
                print(f"    {DARK}\u2570 Package not found in nixpkgs.{RESET}")
            else:
                # Show last few meaningful lines
                lines = [l for l in output.split("\n") if l.strip() and not l.strip().startswith("…")
                         and not l.strip().startswith("at ") and not l.strip().startswith("(stack")]
                for i, line in enumerate(lines[-3:]):
                    if i < len(lines[-3:]) - 1:
                        print(f"    {DARK}\u2502 {line.strip()}{RESET}")
                    else:
                        print(f"    {DARK}\u2570 {line.strip()}{RESET}")
            failed += 1

    if failed == 0 and len(pkgs) > 0:
        print(f"\n  {DARK}All packages installed.{RESET}\n")
    elif failed:
        print(f"\n  {FAIL}{failed} failed.{RESET}\n")

    return failed == 0


def do_remove(pkgs, skip_confirm=False, profile_filter=None):
    if profile_filter == "session":
        profiles = [(VOLATILE_PROFILE, "session")]
    elif profile_filter == "permanent":
        profiles = [(PERMANENT_PROFILE, "permanent")]
    else:
        profiles = [(VOLATILE_PROFILE, "session"), (PERMANENT_PROFILE, "permanent")]

    # Check for not installed in the target profiles
    check_profiles = [p for p, _ in profiles]
    not_installed = [p for p in pkgs if not any(_is_in_profile(p, pr) for pr in check_profiles)]
    if not_installed:
        for p in not_installed:
            if profile_filter == "session":
                print(f"  {FAIL}\u2717{RESET} {WHITE}{p}{RESET} {DARK}is not installed in this session.{RESET}")
                if _is_in_profile(p, PERMANENT_PROFILE):
                    print(f"    {DARK}\u2570 Did you mean: bgx --{p} or bgx remove -p {p}{RESET}")
            elif profile_filter == "permanent":
                print(f"  {FAIL}\u2717{RESET} {WHITE}{p}{RESET} {DARK}is not installed to this installation.{RESET}")
                if _is_in_profile(p, VOLATILE_PROFILE):
                    print(f"    {DARK}\u2570 Did you mean: bgx -{p} or bgx remove -s {p}{RESET}")
            else:
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
        for profile, _ in profiles:
            # Check if actually in this profile before trying to remove
            r = run(["nix", "profile", "list", "--profile", profile],
                    capture_output=True, text=True)
            if r.returncode == 0 and pkg in r.stdout:
                r2 = run(
                    ["nix", "profile", "remove", "--profile", profile, f".*{pkg}.*"],
                    capture_output=True,
                )
                if r2.returncode == 0:
                    removed = True

        if removed:
            print(f"  {SUCCESS}\u2713{RESET} {WHITE}{pkg}{RESET}")
        else:
            print(f"  {FAIL}\u2717{RESET} {WHITE}{pkg}{RESET} {DARK}not installed{RESET}", file=sys.stderr)
            failed += 1

    return failed == 0


def do_info(pkg):
    """Show detailed package info."""
    sp = Spinner(f"Fetching info for {pkg}...")
    sp.start()
    info = pkg_info(pkg)

    # Get homepage + full license name
    homepage = ""
    license_full = ""
    try:
        r = run(["nix", "eval", "--raw", f"nixpkgs#{pkg}.meta.homepage"],
                capture_output=True, text=True, timeout=15)
        if r.returncode == 0:
            homepage = r.stdout.strip()
    except subprocess.TimeoutExpired:
        pass
    try:
        r = run(["nix", "eval", "--raw", f"nixpkgs#{pkg}.meta.license.fullName"],
                capture_output=True, text=True, timeout=15)
        if r.returncode == 0:
            license_full = r.stdout.strip()
    except subprocess.TimeoutExpired:
        pass

    sp.stop(f"{DARK}Done.{RESET}")

    if not info["version"] and not info["description"] and not info.get("unfree"):
        print(f"  {FAIL}\u2717{RESET} {WHITE}{pkg}{RESET} {DARK}not found in nixpkgs.{RESET}")
        return

    print(f"\n  {WHITE}{BOLD}{info['name']}{RESET}")
    print()
    rows = [
        ("Version", info["version"] or "-"),
        ("License", license_full or info.get("license") or "-"),
        ("Size", info.get("size") or "-"),
        ("Homepage", homepage or "-"),
        ("Attribute", f"nixpkgs#{pkg}"),
    ]
    label_w = 12
    for label, val in rows:
        print(f"    {GRAY}{label.ljust(label_w)}{RESET} {WHITE}{val}{RESET}")

    if info.get("description"):
        print(f"\n    {DARK}{info['description']}{RESET}")

    if info.get("unfree"):
        print(f"\n    {WARN}This package has an unfree license.{RESET}")

    installed = _is_installed(pkg)
    if installed:
        print(f"\n    {SUCCESS}Installed{RESET}")
    print()


def do_search(query, sort="relevance"):
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
    if sort == "name":
        results = sorted(filtered, key=lambda r: r["name"].lower())
    elif sort == "version":
        results = sorted(filtered, key=lambda r: r.get("version", ""), reverse=True)
    else:
        # relevance — keep nix's original order
        results = filtered

    if not results:
        print(f"  {DARK}No results found.{RESET}")
        return

    _print_table(f"Results for '{query}' {DARK}(sorted by {sort}){RESET}", ACCENT, results, name_color=WHITE, show_size=False)
    print(f"  {DARK}{len(results)} results \u2022 sorted by {sort}{RESET}")


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

    removes_volatile = []
    removes_permanent = []

    for arg in args:
        if arg in ("-y", "--yes"):
            yes = True
        elif arg.startswith("++"):
            saves.append(arg[2:])
        elif arg.startswith("+"):
            installs.append(arg[1:])
        elif arg.startswith("--") and not arg.startswith("---") and len(arg) > 2 and arg[2] != "-":
            removes_permanent.append(arg[2:])
        elif arg.startswith("-") and len(arg) > 1:
            removes_volatile.append(arg[1:])
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
    if removes_volatile:
        r = do_remove(removes_volatile, profile_filter="session", skip_confirm=yes)
        ok = r and ok
        changed = changed or r
    if removes_permanent:
        r = do_remove(removes_permanent, profile_filter="permanent", skip_confirm=yes)
        ok = r and ok
        changed = changed or r
    if not changed:
        print(f"  {DARK}No changes were made.{RESET}")
    if not ok:
        sys.exit(1)


def run_subcommand_mode(args):
    cmd, rest = args[0], args[1:]

    if "-h" in rest or "--help" in rest:
        subcmd_help = {
            "install": f"  {BOLD}bgx install{RESET} {DARK}[-p/--permanent] [-y] <packages...>{RESET}\n\n    Install packages for this session. Use -p to install permanently.",
            "add": f"  {BOLD}bgx add{RESET} {DARK}[-p/--permanent] [-y] <packages...>{RESET}\n\n    Alias for install.",
            "remove": f"  {BOLD}bgx remove{RESET} {DARK}[-p/--permanent] [-y] <packages...>{RESET}\n\n    Remove packages from this session. Use -p for permanent.",
            "rm": f"  {BOLD}bgx rm{RESET} {DARK}[-p/--permanent] [-y] <packages...>{RESET}\n\n    Alias for remove.",
            "search": f"  {BOLD}bgx search{RESET} {DARK}[--name/--version/--relevance] <query>{RESET}\n\n    Search nixpkgs. Default sort: relevance.",
            "info": f"  {BOLD}bgx info{RESET} {DARK}<package>{RESET}\n\n    Show detailed package information.",
            "list": f"  {BOLD}bgx list{RESET}\n\n    List installed packages (session + permanent).",
        }
        aliases = {"a": "install", "add": "install", "uninstall": "remove", "r": "remove", "s": "search", "q": "search", "i": "info", "ls": "list"}
        key = aliases.get(cmd, cmd)
        print(subcmd_help.get(key, f"  No help for '{cmd}'."))
        return

    if cmd in ("install", "add", "a"):
        save = False
        yes = False
        pkgs = []
        for arg in rest:
            if arg in ("-s", "--save", "-p", "--permanent"):
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
        yes = False
        pfilter = "session"
        pkgs = []
        for arg in rest:
            if arg in ("-y", "--yes"):
                yes = True
            elif arg in ("-p", "--permanent", "--save"):
                pfilter = "permanent"
            else:
                pkgs.append(arg)
        if not pkgs:
            print(f"  {DARK}Package name required.{RESET}", file=sys.stderr)
            sys.exit(1)
        if not do_remove(pkgs, skip_confirm=yes, profile_filter=pfilter):
            sys.exit(1)

    elif cmd in ("info", "i"):
        if not rest:
            print(f"  {DARK}Package name required.{RESET}", file=sys.stderr)
            sys.exit(1)
        do_info(rest[0])

    elif cmd in ("search", "s", "q", "?"):
        sort = "relevance"
        terms = []
        for arg in rest:
            if arg.startswith("--sort="):
                sort = arg.split("=", 1)[1]
            elif arg in ("--name", "--alphabetical"):
                sort = "name"
            elif arg == "--version":
                sort = "version"
            elif arg == "--relevance":
                sort = "relevance"
            else:
                terms.append(arg)
        if not terms:
            print(f"  {DARK}Search query required.{RESET}", file=sys.stderr)
            sys.exit(1)
        do_search(" ".join(terms), sort=sort)

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
    {GRAY}{"bgx -firefox".ljust(C1)}{DARK}Remove from session{RESET}
    {GRAY}{"bgx --firefox".ljust(C1)}{DARK}Remove permanently{RESET}
    {GRAY}{"bgx +pkg1 ++pkg2 -pkg3".ljust(C1)}{DARK}Batch operations{RESET}
    {GRAY}{"bgx ?query".ljust(C1)}{DARK}Search{RESET}

  {WHITE}Commands:{RESET}
    {GRAY}{"install, add, a".ljust(C1)}{DARK}Install packages{RESET}
    {GRAY}{"remove, rm, r".ljust(C1)}{DARK}Remove from session (-p for permanent){RESET}
    {GRAY}{"info, i".ljust(C1)}{DARK}Show package details{RESET}
    {GRAY}{"search, s, q".ljust(C1)}{DARK}Search nixpkgs (--name, --version, --relevance){RESET}
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
    try:
        main()
    except KeyboardInterrupt:
        print(f"\n  {DARK}Aborted.{RESET}")
        sys.exit(130)
