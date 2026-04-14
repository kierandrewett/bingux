#!/usr/bin/env python3
"""bgx — Bingux package manager."""

import os
import re
import subprocess
import sys
import threading
import time


_USER = os.environ.get('USER', 'root')
_unfree_accepted = False
VOLATILE_PROFILE = f"/tmp/bgx-session-{_USER}-packages"
PERMANENT_PROFILE = f"/nix/var/nix/profiles/per-user/{_USER}/bgx/packages"

WHITE = "\033[97m"
GRAY = "\033[37m"
DARK = "\033[90m"
BOLD = "\033[1m"
DIM = "\033[2m"
RESET = "\033[0m"
ACCENT = "\033[38;5;111m"
SUCCESS = "\033[38;5;114m"
WARN = "\033[38;5;180m"
FAIL = "\033[38;5;174m"

MIN_NAME = 16
MIN_VER = 10
MIN_SIZE = 10
MIN_DESC = 20
MIN_LIC = 8
VERSION = "0.2.0"


# ── Spinner ──

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
            sys.stdout.write(f"\r\033[K  {DARK}{frame}{RESET} {GRAY}{self.msg}{RESET}")
            sys.stdout.flush()
            i += 1
            time.sleep(0.08)


# ── Nix helpers ──

def _nix_profile_install_cmd():
    """Detect whether nix uses 'add' (newer) or 'install' (older)."""
    r = subprocess.run(["nix", "profile", "add", "--help"], capture_output=True, text=True)
    if r.returncode == 0 and "not a recognised command" not in (r.stderr or ""):
        return "add"
    return "install"


_PROFILE_INSTALL = None


def _profile_install():
    global _PROFILE_INSTALL
    if _PROFILE_INSTALL is None:
        _PROFILE_INSTALL = _nix_profile_install_cmd()
    return _PROFILE_INSTALL


def run(cmd, **kwargs):
    return subprocess.run(cmd, **kwargs)


def _term_width():
    try:
        return os.get_terminal_size().columns
    except OSError:
        return 80


def format_size(nbytes):
    for unit in ("B", "KiB", "MiB", "GiB"):
        if nbytes < 1024:
            return f"{nbytes:.2f} {unit}"
        nbytes /= 1024
    return f"{nbytes:.2f} TiB"


def _nix_install_streaming(cmd, env, pkg):
    """Run a nix profile add command with streaming progress.

    Returns (success, stderr_text, dl_size, progress_lines, fetched_count).
    """
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    sp = Spinner(f"{pkg}: evaluating...")
    sp.start()

    stderr_lines = []
    progress_lines = []
    state = {"fetched": 0, "total": 0, "dl_size": ""}

    def reader():
        while True:
            raw = proc.stderr.readline()
            if not raw:
                break
            try:
                line = raw.decode("utf-8", errors="replace").strip()
            except AttributeError:
                line = raw.strip()
            if not line:
                continue
            stderr_lines.append(line)

            m = re.match(r"these (\d+) paths will be fetched \((.+?) download", line)
            if m:
                state["total"] = int(m.group(1))
                state["dl_size"] = m.group(2)
                sp.msg = f"{pkg}: fetching {state['total']} paths ({state['dl_size']})..."
                continue

            if "copying path" in line:
                state["fetched"] += 1
                m2 = re.search(r"copying path '/nix/store/[a-z0-9]+-(.+?)'", line)
                name = m2.group(1) if m2 else "..."
                t = state["total"]
                f = state["fetched"]
                sp.msg = f"{pkg}: [{f}/{t}] {name}" if t else f"{pkg}: fetching {name}"
                progress_lines.append(name)
            elif "building" in line.lower():
                sp.msg = f"{pkg}: building..."
            elif "evaluating" in line.lower():
                sp.msg = f"{pkg}: evaluating..."

    t = threading.Thread(target=reader, daemon=True)
    t.start()
    proc.wait()
    sp._stop = True
    t.join(timeout=2)

    ok = proc.returncode == 0
    if ok:
        if state["dl_size"]:
            sp.stop(f"{SUCCESS}\u2713{RESET} {WHITE}{pkg}{RESET} {DARK}(downloaded {state['dl_size']}){RESET}")
        elif state["fetched"]:
            sp.stop(f"{SUCCESS}\u2713{RESET} {WHITE}{pkg}{RESET} {DARK}({state['fetched']} paths fetched){RESET}")
        else:
            sp.stop(f"{SUCCESS}\u2713{RESET} {WHITE}{pkg}{RESET} {DARK}(cached){RESET}")
        if progress_lines:
            # Show last 5 fetched paths
            shown = progress_lines[-5:]
            for i, name in enumerate(shown):
                if i < len(shown) - 1:
                    print(f"    {DARK}\u2502 {name}{RESET}")
                else:
                    print(f"    {DARK}\u2570 {state['fetched']} paths fetched (last: {name}){RESET}")
    else:
        sp.stop(f"{FAIL}\u2717{RESET} {WHITE}{pkg}{RESET}")

    return ok, "\n".join(stderr_lines)


# ── Package info ──

UNFREE_ENV = {**os.environ, "NIXPKGS_ALLOW_UNFREE": "1"}


def _eval_pkg(expr, timeout=15):
    """Eval a nix expression, retrying with unfree if needed."""
    r = run(["nix", "eval", "--raw", expr], capture_output=True, text=True, timeout=timeout)
    if r.returncode == 0:
        return True, r.stdout.strip(), False
    if "unfree" in (r.stderr or "").lower():
        r2 = run(["nix", "eval", "--impure", "--raw", expr],
                 capture_output=True, text=True, timeout=timeout, env=UNFREE_ENV)
        if r2.returncode == 0:
            return True, r2.stdout.strip(), True
        return False, "", True
    return False, "", False


def pkg_info(pkg):
    info = {"name": pkg, "version": "", "description": "", "size": "", "size_bytes": 0, "disk": "", "unfree": False, "license": ""}
    try:
        ok, val, is_unfree = _eval_pkg(f"nixpkgs#{pkg}.version")
        if ok:
            info["version"] = val
        if is_unfree:
            info["unfree"] = True
            info["license"] = "unfree"
    except subprocess.TimeoutExpired:
        pass
    try:
        ok, val, _ = _eval_pkg(f"nixpkgs#{pkg}.meta.description")
        if ok:
            info["description"] = val
    except subprocess.TimeoutExpired:
        pass
    try:
        ok, val, _ = _eval_pkg(f"nixpkgs#{pkg}.meta.license.spdxId")
        if ok and val:
            info["license"] = val
        else:
            ok, val, _ = _eval_pkg(f"nixpkgs#{pkg}.meta.license.shortName")
            if ok and val:
                info["license"] = val
    except subprocess.TimeoutExpired:
        pass
    try:
        # Use nix build --dry-run to get download/unpacked size without fetching
        cmd = ["nix", "build", "--dry-run", f"nixpkgs#{pkg}"]
        r = run(cmd, capture_output=True, text=True, timeout=30)
        if r.returncode != 0:
            r = run(["nix", "build", "--impure", "--dry-run", f"nixpkgs#{pkg}"],
                    capture_output=True, text=True, timeout=30, env=UNFREE_ENV)
        for line in (r.stderr or "").split("\n"):
            m = re.search(r"([\d.]+)\s+([KMGT]iB)\s+download,\s+([\d.]+)\s+([KMGT]iB)\s+unpacked", line)
            if m:
                units = {"KiB": 1024, "MiB": 1024**2, "GiB": 1024**3, "TiB": 1024**4}
                info["size_bytes"] = int(float(m.group(1)) * units.get(m.group(2), 1))
                info["size"] = f"{m.group(1)} {m.group(2)}"
                info["disk"] = f"{m.group(3)} {m.group(4)}"
                break
        # Fallback: query installed store path for disk size
        if not info["disk"]:
            for pcmd in [["nix", "path-info", f"nixpkgs#{pkg}"],
                         ["nix", "path-info", "--impure", f"nixpkgs#{pkg}"]]:
                pr = run(pcmd, capture_output=True, text=True, timeout=15,
                         env=UNFREE_ENV if "--impure" in pcmd else None)
                if pr.returncode == 0 and pr.stdout.strip():
                    spath = pr.stdout.strip().split()[0]
                    sr = run(["nix-store", "--query", "--size", spath],
                             capture_output=True, text=True, timeout=10)
                    if sr.returncode == 0 and sr.stdout.strip():
                        try:
                            nb = int(sr.stdout.strip())
                            info["disk"] = format_size(nb)
                            if not info["size_bytes"]:
                                info["size_bytes"] = nb
                                info["size"] = info["disk"]
                        except ValueError:
                            pass
                    break
    except (subprocess.TimeoutExpired, Exception):
        pass
    return info


# ── Profile checks ──

def _list_profile_packages(profile):
    """Get list of package names in a profile."""
    r = run(["nix", "profile", "list", "--profile", profile],
            capture_output=True, text=True)
    if r.returncode != 0 or not r.stdout.strip():
        return []
    pkgs = []
    for line in r.stdout.split("\n"):
        clean = line.strip()
        if clean.startswith("Name:"):
            name = clean.split(":", 1)[1].strip()
            # Strip ANSI codes
            name = re.sub(r"\033\[[0-9;]*m", "", name).strip()
            if name:
                pkgs.append(name)
    return pkgs


def _is_in_profile(pkg, profile):
    return pkg in _list_profile_packages(profile)


def _is_installed(pkg):
    return _is_in_profile(pkg, VOLATILE_PROFILE) or _is_in_profile(pkg, PERMANENT_PROFILE)


def _auto_gc():
    """Wipe old profile generations, update desktop db, and garbage collect.
    Returns freed size string or empty string."""
    # Update desktop database so new apps appear in GNOME menu
    for profile in (VOLATILE_PROFILE, PERMANENT_PROFILE):
        apps_dir = f"{profile}/share/applications"
        if os.path.isdir(apps_dir):
            try:
                run(["update-desktop-database", apps_dir], capture_output=True)
            except FileNotFoundError:
                pass

    sp = Spinner("Cleaning up...")
    sp.start()
    # Wipe old profile generations so gc can actually free paths
    for profile in (VOLATILE_PROFILE, PERMANENT_PROFILE):
        run(["nix", "profile", "wipe-history", "--profile", profile], capture_output=True)
    r = run(["nix", "store", "gc"], capture_output=True, text=True)
    freed = ""
    freed_size = ""
    if r.returncode == 0 and r.stderr:
        for line in r.stderr.split("\n"):
            m = re.search(r"([\d.]+)\s+(\S+)\s+freed", line)
            if m:
                freed = line.strip()
                freed_size = f"{m.group(1)} {m.group(2)}"
                break
    if freed:
        sp.stop(f"{DARK}{freed}{RESET}")
    else:
        sp.stop(f"{DARK}Done.{RESET}")
    return freed_size


def _ensure_profile_dir(profile):
    d = os.path.dirname(profile)
    if d and not os.path.isdir(d):
        os.makedirs(d, mode=0o755, exist_ok=True)


# ── Table formatting ──

def _calc_cols(infos, show_size=True, show_license=False):
    tw = _term_width()
    cn = max((len(i["name"]) for i in infos), default=0)
    cv = max((len(i.get("version") or "-") for i in infos), default=0)
    cs = max((len(i.get("size") or "-") for i in infos), default=0) if show_size else 0
    cdisk = max((len(i.get("disk") or "-") for i in infos), default=0) if show_size else 0
    cl = max((len(i.get("license") or "-") for i in infos), default=0) if show_license else 0

    cn = max(cn, MIN_NAME) + 2
    cv = max(cv, MIN_VER) + 2
    cs = (max(cs, MIN_SIZE) + 2) if show_size else 0
    cdisk = (max(cdisk, MIN_SIZE) + 2) if show_size else 0
    cl = (max(cl, MIN_LIC) + 2) if show_license else 0

    cn = min(cn, int(tw * 0.4))
    cd = max(tw - 4 - cn - cv - cs - cdisk - cl - 5, MIN_DESC)
    return cn, cv, cs, cdisk, cl, cd


def _fmt_row(name, version, size, description, cols, name_color=WHITE, license="", disk=""):
    cn, cv, cs, cdisk, cl, cd = cols
    n = name[:cn-1].ljust(cn) if len(name) >= cn else name.ljust(cn)
    v = (version or "-")[:cv-1].ljust(cv) if len(version or "-") >= cv else (version or "-").ljust(cv)
    desc = description
    if len(desc) > cd:
        desc = desc[:cd-1] + "\u2026"
    parts = f"    {name_color}{n}{RESET} {WHITE}{v}{RESET}"
    if cs:
        s = (size or "-").ljust(cs)
        parts += f" {GRAY}{s}{RESET}"
    if cdisk:
        d = (disk or "-").ljust(cdisk)
        parts += f" {GRAY}{d}{RESET}"
    if cl:
        lic = (license or "-").ljust(cl)
        lic_color = WARN if license == "unfree" else DARK
        parts += f" {lic_color}{lic}{RESET}"
    parts += f" {DARK}{desc}{RESET}"
    return parts


def _print_table(label, label_color, infos, name_color=WHITE, show_size=True, show_license=False):
    has_license = show_license and any(i.get("license") for i in infos)
    cols = _calc_cols(infos, show_size=show_size, show_license=has_license)
    cn, cv, cs, cdisk, cl, cd = cols
    print(f"  {label_color}\u276f{RESET} {WHITE}{label}{RESET}")
    line_w = _term_width() - 6
    header = f"    {DARK}{'Package'.ljust(cn)} {'Version'.ljust(cv)}"
    if cs:
        header += f" {'Download'.ljust(cs)}"
    if cdisk:
        header += f" {'Disk'.ljust(cdisk)}"
    if cl:
        header += f" {'License'.ljust(cl)}"
    header += f" Description{RESET}"
    print(header)
    print(f"    {DARK}{'\u2500' * line_w}{RESET}")
    for info in infos:
        print(_fmt_row(info["name"], info.get("version", ""), info.get("size", ""), info.get("description", ""), cols, name_color, info.get("license", ""), info.get("disk", "")))
    print()


def confirm(prompt=f"  {DARK}Proceed?{RESET} [{GRAY}y/N{RESET}] "):
    try:
        return input(prompt).strip().lower() in ("y", "yes")
    except (EOFError, KeyboardInterrupt):
        print()
        return False


def show_transaction(installs, removes, save=False, remove_filter=None):
    if not installs and not removes:
        return True

    print()

    if installs:
        mode = "permanently" if save else "for this session"
        _print_table(f"Installing {DARK}({mode}){RESET}", SUCCESS, installs, name_color=SUCCESS, show_license=True)

    if removes:
        if remove_filter == "session":
            rm_label = f"Removing {DARK}(from this session){RESET}"
        elif remove_filter == "permanent":
            rm_label = f"Removing {DARK}(from this installation){RESET}"
        else:
            rm_label = "Removing"
        _print_table(rm_label, WARN, removes, name_color=FAIL, show_license=True)

    ni = len(installs)
    nr = len(removes)
    summary = f"  {GRAY}Summary: {SUCCESS}+{ni}{RESET}{DARK}/{RESET}{FAIL}-{nr}{RESET}{DARK}/{RESET}{WARN}~0{RESET}"

    if installs:
        total_add = sum(i.get("size_bytes", 0) for i in installs)
        if total_add:
            summary += f"  {DARK}({SUCCESS}{format_size(total_add)}{DARK}){RESET}"

    print(summary)
    print()

    return confirm()


# ── Commands ──

def do_install(pkgs, save=False, skip_confirm=False):
    global _unfree_accepted
    profile = PERMANENT_PROFILE if save else VOLATILE_PROFILE
    _ensure_profile_dir(profile)

    already = [p for p in pkgs if _is_installed(p)]
    if already:
        for p in already:
            where = []
            if _is_in_profile(p, VOLATILE_PROFILE):
                where.append("session")
            if _is_in_profile(p, PERMANENT_PROFILE):
                where.append("permanent")
            print(f"  {WARN}\u276f{RESET} {WHITE}{p}{RESET} {DARK}is already installed ({', '.join(where)}).{RESET}")
        pkgs = [p for p in pkgs if p not in already]
        if not pkgs:
            return True

    sp = Spinner("Resolving packages...")
    sp.start()
    infos = [pkg_info(p) for p in pkgs]
    sp.stop(f"{DARK}Resolved {len(infos)} {'package' if len(infos) == 1 else 'packages'}.{RESET}")

    # Handle unfree
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

    # Check for not found
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

    unfree_pkgs = {i["name"] for i in infos if i.get("_allow_unfree")}
    pkgs = [i["name"] for i in infos]

    failed = 0
    for pkg in pkgs:
        cmd = ["nix", "profile", _profile_install(), "--log-format", "bar-with-logs", "--profile", profile]
        env = None
        if pkg in unfree_pkgs:
            cmd.append("--impure")
            env = {**os.environ, "NIXPKGS_ALLOW_UNFREE": "1"}
        cmd.append(f"nixpkgs#{pkg}")

        ok, stderr = _nix_install_streaming(cmd, env, pkg)
        if ok:
            continue

        # Handle errors
        if "unfree" in stderr.lower():
            if os.path.exists("/etc/nixos"):
                print(f"    {DARK}\u2502 This package has an unfree license.{RESET}")
                print(f"    {DARK}\u2570 Add to your NixOS config: nixpkgs.config.allowUnfree = true;{RESET}")
            else:
                sys.stdout.write(f"\033[A\r\033[K")
                retry_cmd = ["nix", "profile", _profile_install(), "--log-format", "bar-with-logs", "--impure", "--profile", profile, f"nixpkgs#{pkg}"]
                retry_env = {**os.environ, "NIXPKGS_ALLOW_UNFREE": "1"}
                ok2, _ = _nix_install_streaming(retry_cmd, retry_env, pkg)
                if ok2:
                    continue
                print(f"    {DARK}\u2570 Failed to install unfree package.{RESET}")
        elif "does not provide" in stderr:
            print(f"    {DARK}\u2570 Package not found in nixpkgs.{RESET}")
        else:
            lines = [l for l in stderr.split("\n") if l.strip() and not l.strip().startswith("…")
                     and not l.strip().startswith("at ") and not l.strip().startswith("(stack")]
            for i, line in enumerate(lines[-3:]):
                if i < len(lines[-3:]) - 1:
                    print(f"    {DARK}\u2502 {line.strip()}{RESET}")
                else:
                    print(f"    {DARK}\u2570 {line.strip()}{RESET}")
        failed += 1

    if failed == 0 and len(pkgs) > 0:
        print(f"\n  {DARK}All packages installed.{RESET}")
    elif failed:
        print(f"\n  {FAIL}{failed} failed.{RESET}")

    _auto_gc()
    return failed == 0


def do_remove(pkgs, skip_confirm=False, profile_filter=None):
    if profile_filter == "session":
        profiles = [(VOLATILE_PROFILE, "session")]
    elif profile_filter == "permanent":
        profiles = [(PERMANENT_PROFILE, "permanent")]
    else:
        profiles = [(VOLATILE_PROFILE, "session"), (PERMANENT_PROFILE, "permanent")]

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

    sp = Spinner("Resolving packages...")
    sp.start()
    infos = [pkg_info(p) for p in pkgs]
    sp.stop(f"{DARK}Resolved {len(infos)} {'package' if len(infos) == 1 else 'packages'}.{RESET}")

    if not skip_confirm:
        if not show_transaction([], infos, remove_filter=profile_filter):
            print(f"  {DARK}Aborted.{RESET}")
            return False

    total_freed = sum(i.get("size_bytes", 0) for i in infos)

    failed = 0
    for pkg in pkgs:
        removed = False
        for profile, _ in profiles:
            if not _is_in_profile(pkg, profile):
                continue
            run(["nix", "profile", "remove", "--profile", profile, pkg], capture_output=True)
            if not _is_in_profile(pkg, profile):
                removed = True
            else:
                print(f"  {FAIL}\u2717{RESET} {WHITE}{pkg}{RESET} {DARK}failed to remove{RESET}", file=sys.stderr)
                failed += 1

        if removed:
            print(f"  {SUCCESS}\u2713{RESET} {WHITE}{pkg}{RESET}")
        elif failed == 0:
            print(f"  {FAIL}\u2717{RESET} {WHITE}{pkg}{RESET} {DARK}not installed{RESET}", file=sys.stderr)
            failed += 1

    removed_count = len(pkgs) - failed
    if removed_count > 0:
        freed = _auto_gc()
        if freed:
            print(f"\n  {DARK}{removed_count} removed. Freed {freed}.{RESET}")
        else:
            print(f"\n  {DARK}{removed_count} removed.{RESET}")
        return failed == 0

    _auto_gc()
    return failed == 0


def do_info(pkg):
    sp = Spinner(f"Fetching info for {pkg}...")
    sp.start()
    info = pkg_info(pkg)

    homepage = ""
    license_full = ""
    try:
        ok, val, _ = _eval_pkg(f"nixpkgs#{pkg}.meta.homepage")
        if ok:
            homepage = val
    except subprocess.TimeoutExpired:
        pass
    try:
        ok, val, _ = _eval_pkg(f"nixpkgs#{pkg}.meta.license.fullName")
        if ok:
            license_full = val
    except subprocess.TimeoutExpired:
        pass

    sp.stop(f"{DARK}Done.{RESET}")

    if not info["version"] and not info["description"] and not info.get("unfree"):
        print(f"  {FAIL}\u2717{RESET} {WHITE}{pkg}{RESET} {DARK}not found in nixpkgs.{RESET}")
        return

    print(f"\n  {WHITE}{BOLD}{info['name']}{RESET}")
    print()
    label_w = 12
    for label, val in [
        ("Version", info["version"] or "-"),
        ("License", license_full or info.get("license") or "-"),
        ("Size", info.get("size") or "-"),
        ("Homepage", homepage or "-"),
        ("Attribute", f"nixpkgs#{pkg}"),
    ]:
        print(f"    {GRAY}{label.ljust(label_w)}{RESET} {WHITE}{val}{RESET}")

    if info.get("description"):
        print(f"\n    {DARK}{info['description']}{RESET}")
    if info.get("unfree"):
        print(f"\n    {WARN}This package has an unfree license.{RESET}")
    if _is_installed(pkg):
        print(f"\n    {SUCCESS}Installed{RESET}")
    print()


def do_search(query, sort="relevance"):
    sp = Spinner(f"Searching for '{query}'...")
    sp.start()
    r = run(["nix", "search", "nixpkgs", query], capture_output=True, text=True)
    sp.stop(f"{DARK}Search complete.{RESET}")

    if r.returncode != 0 or not r.stdout.strip():
        print(f"  {DARK}No results found.{RESET}")
        return

    ansi_re = re.compile(r"\033\[[0-9;]*m")
    lines = r.stdout.strip().split("\n")
    results = []
    current = None

    for line in lines:
        clean = ansi_re.sub("", line).strip()
        if clean.startswith("* "):
            if current:
                results.append(current)
            rest = clean[2:]
            m = re.match(r"legacyPackages\.\S+?\.(.+?)\s*\(([^)]*)\)", rest)
            if m:
                attr = m.group(1)
                parts = attr.split(".")
                name = parts[-1] if len(parts) <= 2 else attr
                current = {"name": name, "version": m.group(2), "description": ""}
            else:
                current = None
        elif current and clean:
            current["description"] = clean

    if current:
        results.append(current)

    seen = set()
    filtered = []
    for r in results:
        v = r.get("version", "")
        if not v or ".zip" in v or ".tar" in v or ".iso" in v:
            continue
        if r["name"] in seen:
            continue
        seen.add(r["name"])
        filtered.append(r)

    if sort == "name":
        results = sorted(filtered, key=lambda r: r["name"].lower())
    elif sort == "version":
        results = sorted(filtered, key=lambda r: r.get("version", ""), reverse=True)
    else:
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


# ── Routing ──

def run_prefix_mode(args):
    installs, saves, removes_volatile, removes_permanent = [], [], [], []
    yes = False

    for arg in args:
        if arg in ("-y", "--yes"):
            yes = True
        elif arg.startswith("++"):
            saves.append(arg[2:])
        elif arg.startswith("+"):
            installs.append(arg[1:])
        elif arg.startswith("--") and not arg.startswith("---") and len(arg) > 2:
            pkg = arg[2:]
            if pkg == "*":
                removes_permanent.extend(_list_profile_packages(PERMANENT_PROFILE))
            else:
                removes_permanent.append(pkg)
        elif arg.startswith("-") and len(arg) > 1:
            pkg = arg[1:]
            if pkg == "*":
                removes_volatile.extend(_list_profile_packages(VOLATILE_PROFILE))
            else:
                removes_volatile.append(pkg)
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

    # Help for any subcommand
    if "-h" in rest or "--help" in rest:
        subcmd_help = {
            "install": f"  {BOLD}bgx install{RESET} {DARK}[-p/--permanent] [-y] <packages...>{RESET}\n\n    Install packages for this session. Use -p to install permanently.",
            "remove": f"  {BOLD}bgx remove{RESET} {DARK}[-p/--permanent] [-y] <packages...>{RESET}\n\n    Remove packages from this session. Use -p for permanent.",
            "search": f"  {BOLD}bgx search{RESET} {DARK}[--name/--version/--relevance] <query>{RESET}\n\n    Search nixpkgs. Default sort: relevance.",
            "info": f"  {BOLD}bgx info{RESET} {DARK}<package>{RESET}\n\n    Show detailed package information.",
            "list": f"  {BOLD}bgx list{RESET}\n\n    List installed packages (session + permanent).",
        }
        aliases = {"a": "install", "add": "install", "uninstall": "remove", "rm": "remove", "r": "remove", "s": "search", "q": "search", "i": "info", "ls": "list"}
        print(subcmd_help.get(aliases.get(cmd, cmd), f"  No help for '{cmd}'."))
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
                if arg == "*":
                    profile = PERMANENT_PROFILE if pfilter == "permanent" else VOLATILE_PROFILE
                    pkgs.extend(_list_profile_packages(profile))
                else:
                    pkgs.append(arg)
        if not pkgs:
            print(f"  {DARK}No packages to remove.{RESET}")
            return
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
    {GRAY}{"install, add, a".ljust(C1)}{DARK}Install for this session (-p for permanent){RESET}
    {GRAY}{"remove, rm, r".ljust(C1)}{DARK}Remove from session (-p for permanent){RESET}
    {GRAY}{"info, i".ljust(C1)}{DARK}Show package details{RESET}
    {GRAY}{"search, s, q".ljust(C1)}{DARK}Search nixpkgs (--name, --version, --relevance){RESET}
    {GRAY}{"list, ls".ljust(C1)}{DARK}List installed packages{RESET}
    {GRAY}{"help".ljust(C1)}{DARK}Show this help{RESET}

  {WHITE}Flags:{RESET}
    {GRAY}{"-p, --permanent".ljust(C1)}{DARK}Install/remove permanently (persists after reboot){RESET}
    {GRAY}{"-y, --yes".ljust(C1)}{DARK}Skip confirmation prompt{RESET}
    {GRAY}{"-h, --help".ljust(C1)}{DARK}Show help for a subcommand{RESET}
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
