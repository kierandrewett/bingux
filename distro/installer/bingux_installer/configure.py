"""Configure the installed target: live-only strip, identity, locale, users, initramfs.

Every chroot command is run through the Runner with stdin for secrets; nothing secret
ever appears on argv. Pure text renderers are separate so tests can check them.
"""

from __future__ import annotations

import configparser
import glob
import json
import os
import re
import shlex
import shutil
from collections.abc import Sequence
from pathlib import Path

from . import layout
from .plan import Plan
from .protocol import InstallerError, redact
from .runner import CommandError, Runner

# Identical to distro/packages/bingux-os/bingux-os-release (keep in sync).
OS_RELEASE = """NAME="Bingux OS"
PRETTY_NAME="Bingux OS"
ID=bingux
ID_LIKE=arch
BUILD_ID=rolling
ANSI_COLOR="38;2;23;147;209"
HOME_URL="https://github.com/kierandrewett/bingux"
DOCUMENTATION_URL="https://github.com/kierandrewett/bingux"
SUPPORT_URL="https://github.com/kierandrewett/bingux/issues"
BUG_REPORT_URL="https://github.com/kierandrewett/bingux/issues"
LOGO=archlinux-logo
VERSION_ID=0.1.0
VERSION="0.1.0"
"""

LIVE_FILES = (
    "root/.automated_script.sh",
    "root/.zlogin",
    "usr/local/bin/bingux-os-install",
    "usr/local/bin/bingux-os-firstboot",
    "usr/local/bin/Installation_guide",
    "usr/local/bin/choose-mirror",
    "usr/local/bin/livecd-sound",
    "etc/ssh/sshd_config.d/10-archiso.conf",
    "etc/systemd/resolved.conf.d/archiso.conf",
    "etc/mkinitcpio.conf.d/archiso.conf",
    "etc/motd",
    "etc/systemd/system/choose-mirror.service",
    "etc/systemd/system/livecd-talk.service",
    "etc/systemd/system/livecd-alsa-unmuter.service",
    "etc/systemd/system/bingux-os-firstboot.service",
)
LIVE_TREES = (
    "root/.gnupg",
    "root/.ssh",
    "etc/systemd/system/getty@tty1.service.d",
    "etc/systemd/system/serial-getty@ttyS0.service.d",
    "opt/bingux-os/repo",
    "var/lib/bingux/greeter/live",
)
LIVE_WANTS_UNITS = (
    "choose-mirror.service",
    "livecd-talk.service",
    "livecd-alsa-unmuter.service",
    "bingux-os-firstboot.service",
    "bingux-installer.socket",
    "sshd.service",
    "sshd.socket",
)
LIVE_USER = "live"
PAYLOAD_DEFAULT_GROUPS = ("video", "audio", "input")
ADMIN_GROUP = "wheel"
SUDOERS_FILE = "etc/sudoers.d/10-bingux-wheel"
SUDOERS_TEXT = "%wheel ALL=(ALL:ALL) ALL\n"
LIVE_ONLY_PACKAGES = ("archinstall", "mkinitcpio-archiso", "bingux-installer", "python-blivet")
GREETD_CONFIG = "etc/greetd/config.toml"
GREETD_PACKAGED = "usr/share/bingux/greeter/greetd.toml"
GREETD_GNOBLIN_SESSION = "usr/share/wayland-sessions/gnoblin.desktop"
PLYMOUTH_HOOK = "usr/lib/initcpio/install/plymouth"
HOOKS_RE = re.compile(r"^HOOKS=.*$", re.MULTILINE)
ZRAM_CONF = "etc/systemd/zram-generator.conf"
TIMESYNCD_UNIT = "systemd-timesyncd.service"
MODULES_RE = re.compile(r"^MODULES=.*$", re.MULTILINE)


# ---- pure renderers -------------------------------------------------------------

def render_hostname(hostname: str) -> str:
    return hostname + "\n"


def render_locale_conf(locale: str) -> str:
    return f"LANG={locale}\n"


def render_vconsole(keymap: str) -> str:
    return f"KEYMAP={keymap}\n"


def enable_locale_gen(text: str, locale: str) -> str:
    """Uncomment `locale UTF-8` in locale.gen, or append it when absent."""
    charset = locale.split(".", 1)[1] if "." in locale else "UTF-8"
    wanted = f"{locale} {charset}"
    lines = text.splitlines()
    found = False
    out: list[str] = []
    for line in lines:
        bare = line.lstrip("#").strip()
        if bare == wanted:
            out.append(wanted)
            found = True
        else:
            out.append(line)
    if not found:
        out.append(wanted)
    return "\n".join(out) + "\n"


def rewrite_hooks(text: str, hooks: Sequence[str]) -> str:
    line = layout.hooks_line(tuple(hooks))
    if HOOKS_RE.search(text):
        return HOOKS_RE.sub(line, text, count=1)
    return text.rstrip("\n") + "\n" + line + "\n"


def rewrite_modules(text: str, modules: Sequence[str]) -> str:
    line = layout.modules_line(tuple(modules))
    if MODULES_RE.search(text):
        return MODULES_RE.sub(line, text, count=1)
    return text.rstrip("\n") + "\n" + line + "\n"


def sudoers_needed(plan: Plan) -> bool:
    return plan.has_admin


def user_groups(admin: bool) -> str:
    groups = list(PAYLOAD_DEFAULT_GROUPS)
    if admin:
        groups.insert(0, ADMIN_GROUP)
    return ",".join(groups)


# ---- filesystem helpers ---------------------------------------------------------

def _remove(path: Path) -> None:
    if path.is_symlink() or path.is_file():
        path.unlink()
    elif path.is_dir():
        shutil.rmtree(path)


def _write(path: Path, text: str, mode: int = 0o644) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.is_symlink():
        path.unlink()
    path.write_text(text, encoding="utf-8")
    os.chmod(path, mode)


def strip_live(target: Path) -> None:
    """Remove live-only files, trees and enable links from the target. Idempotent."""
    for rel in LIVE_FILES + LIVE_TREES:
        _remove(target / rel)
    for unit in LIVE_WANTS_UNITS:
        for link in glob.glob(str(target / "etc/systemd/system/*.wants" / unit)):
            _remove(Path(link))
    for rel in glob.glob(str(target / "etc/ssh/ssh_host_*")):
        _remove(Path(rel))
    (target / "etc/machine-id").parent.mkdir(parents=True, exist_ok=True)
    (target / "etc/machine-id").write_text("", encoding="utf-8")


# ---- chroot helper --------------------------------------------------------------

def _chroot(runner: Runner, target: Path, argv: Sequence[str], *, stdin: str | None = None,
            check: bool = True, secrets: Sequence[str] = ()):
    try:
        return runner.run(["chroot", str(target), *argv], stdin=stdin, check=check)
    except CommandError as exc:
        detail = redact(exc.stderr.strip(), secrets)[-400:]
        raise InstallerError(
            "config_failed", f"{argv[0]} failed in the target: {detail or 'no output'}") from exc


def _installed_packages(runner: Runner, target: Path, names: Sequence[str]) -> list[str]:
    present = []
    for name in names:
        result = _chroot(runner, target, ["pacman", "-Q", name], check=False)
        if result.returncode == 0:
            present.append(name)
    return present


# ---- orchestration --------------------------------------------------------------

def restore_greeter_config(target: Path) -> None:
    """Replace the live greetd config with the packaged greeter configuration.

    The live image's config starts the live session; the installed system must
    get the copy bingux ships in /usr/share/bingux/greeter/greetd.toml."""
    source = target / GREETD_PACKAGED
    if not source.is_file():
        raise InstallerError("config_failed", f"{GREETD_PACKAGED} is missing from the target")
    dest = target / GREETD_CONFIG
    dest.parent.mkdir(parents=True, exist_ok=True)
    if dest.is_symlink():
        dest.unlink()
    shutil.copyfile(source, dest)
    os.chmod(dest, 0o644)


def write_autologin(target: Path, plan: Plan) -> None:
    """Start the first user's Gnoblin session at boot instead of the greeter.

    The packaged configuration stays the default session, so logging out still shows the
    greeter. The initial session runs the Exec of the Gnoblin wayland-sessions entry with
    the login environment minus BINGUX_CONTINUING: that flag makes a session keep the
    greeter's wallpaper and bar, and no greeter screen is shown before an autologin."""
    if plan.autologin is None:
        return
    session = target / GREETD_GNOBLIN_SESSION
    if not session.is_file():
        raise InstallerError("config_failed", f"{GREETD_GNOBLIN_SESSION} is missing from the target")
    entry = configparser.ConfigParser(interpolation=None)
    try:
        entry.read_string(session.read_text(encoding="utf-8"))
        desktop = entry["Desktop Entry"]
        exec_line = desktop["Exec"].strip()
    except (configparser.Error, KeyError) as exc:
        raise InstallerError("config_failed", f"{GREETD_GNOBLIN_SESSION} has no Exec line") from exc
    environment = ["XDG_SESSION_TYPE=wayland",
                   f"XDG_SESSION_DESKTOP={Path(GREETD_GNOBLIN_SESSION).stem}"]
    desktop_names = desktop.get("DesktopNames", "").rstrip(";").strip()
    if desktop_names:
        environment.append(f"XDG_CURRENT_DESKTOP={desktop_names}")
    command = shlex.join(["env", *environment]) + " " + exec_line
    config = target / GREETD_CONFIG
    block = (f"\n[initial_session]\ncommand = {json.dumps(command)}\n"
             f"user = {json.dumps(plan.autologin)}\n")
    _write(config, config.read_text(encoding="utf-8").rstrip("\n") + "\n" + block)


def strip_live_and_packages(runner: Runner, target: Path) -> None:
    strip_live(target)
    restore_greeter_config(target)
    installed = _installed_packages(runner, target, LIVE_ONLY_PACKAGES)
    if installed:
        _chroot(runner, target, ["pacman", "-Rns", "--noconfirm", *installed])
    _chroot(runner, target, ["userdel", "-r", LIVE_USER], check=False)


def write_identity(target: Path, plan: Plan, *, fstab_text: str, os_release: str = OS_RELEASE) -> None:
    _write(target / "etc/hostname", render_hostname(plan.hostname))
    _write(target / "etc/fstab", fstab_text)
    _write(target / "etc/os-release", os_release)
    localtime = target / "etc/localtime"
    if localtime.is_symlink() or localtime.exists():
        localtime.unlink()
    localtime.parent.mkdir(parents=True, exist_ok=True)
    os.symlink(f"/usr/share/zoneinfo/{plan.timezone}", localtime)


def write_locale(runner: Runner, target: Path, plan: Plan) -> None:
    gen = target / "etc/locale.gen"
    current = gen.read_text(encoding="utf-8") if gen.exists() else ""
    _write(gen, enable_locale_gen(current, plan.locale))
    _chroot(runner, target, ["locale-gen"])
    _write(target / "etc/locale.conf", render_locale_conf(plan.locale))
    _write(target / "etc/vconsole.conf", render_vconsole(plan.keymap))


def _hash_password(runner: Runner, password: str) -> str:
    try:
        digest = runner.run(["openssl", "passwd", "-6", "-stdin"],
                            stdin=password + "\n").stdout.strip()
    except CommandError as exc:
        raise InstallerError("config_failed", "could not hash a password") from exc
    if not digest:
        raise InstallerError("config_failed", "password hashing produced no output")
    return digest


def create_users(runner: Runner, target: Path, plan: Plan) -> None:
    secrets = plan.secrets()
    lines = []
    for user in plan.users:
        comment = user.full_name or user.name
        _chroot(runner, target, ["useradd", "-m", "-G", user_groups(user.admin),
                                 "-s", "/bin/bash", "-c", comment, user.name],
                secrets=secrets)
        lines.append(f"{user.name}:{_hash_password(runner, user.password)}\n")
    if plan.root_password is not None:
        lines.append(f"root:{_hash_password(runner, plan.root_password)}\n")
    _chroot(runner, target, ["chpasswd", "-e"], stdin="".join(lines), secrets=secrets)
    if plan.root_password is None:
        _chroot(runner, target, ["passwd", "-l", "root"])


def set_ntp(runner: Runner, target: Path, enabled: bool) -> None:
    verb = "enable" if enabled else "disable"
    _chroot(runner, target, ["systemctl", verb, TIMESYNCD_UNIT])


def create_swapfile(runner: Runner, target: Path, size_bytes: int, filesystem: str) -> None:
    """Create /swapfile in the target. btrfs needs its own helper: a plain file gets CoW."""
    path = target / layout.SWAPFILE.lstrip("/")
    try:
        if filesystem == "btrfs":
            runner.run(["btrfs", "filesystem", "mkswapfile", "--size", str(size_bytes), str(path)])
        else:
            runner.run(["fallocate", "-l", str(size_bytes), str(path)])
            os.chmod(path, 0o600)
            runner.run(["mkswap", str(path)])
    except CommandError as exc:
        raise InstallerError("config_failed", "could not create the swap file") from exc


def write_swap(runner: Runner, target: Path, plan: Plan) -> None:
    """zram gets a generator config; file gets /swapfile; partition and none write nothing."""
    zram = target / ZRAM_CONF
    if plan.swap == "zram":
        _write(zram, layout.zram_conf())
    elif zram.exists() or zram.is_symlink():
        _remove(zram)
    if plan.swap == "file":
        create_swapfile(runner, target, plan.swap_bytes, plan.filesystem)


def write_crypttab(target: Path, text: str) -> None:
    _write(target / "etc/crypttab", text)


def write_sudoers(target: Path, plan: Plan) -> None:
    if sudoers_needed(plan):
        _write(target / SUDOERS_FILE, SUDOERS_TEXT, 0o440)


def write_initramfs(runner: Runner, target: Path, plan: Plan) -> None:
    conf = target / "etc/mkinitcpio.conf"
    text = conf.read_text(encoding="utf-8") if conf.exists() else ""
    plymouth = (target / PLYMOUTH_HOOK).exists()
    hooks = layout.mkinitcpio_hooks(encrypted=plan.encrypted, plymouth=plymouth)
    text = rewrite_hooks(text, hooks)
    if plan.filesystem == "xfs":
        text = rewrite_modules(text, ("xfs",))
    _write(conf, text)
    _chroot(runner, target, ["mkinitcpio", "-P"])
