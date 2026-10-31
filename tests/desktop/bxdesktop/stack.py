"""Find what the isolated desktop runs: Gnoblin, Quickshell, the Bingux install and the checkout.

A `Stack` is a set of paths, resolved once. Nothing here starts a process
except one `bash` that sources Gnoblin's own environment script, so the
library path and data directories come from Gnoblin, not from a copy of its
logic here.

What is taken from where:

* the shell's QML, the Lua module (`gnoblin/bingux.lua`) and the helper
  scripts come from this checkout, so a test exercises the change under review;
* the compositor comes from the installed Gnoblin prefix;
* Quickshell, the native QML plugins (`Bingux.Text`, `Bingux.Effects`,
  `Bingux.Wayland`), `bingux-statusd`, `bingux-config` and the apps come from an
  installed Bingux prefix, or from the Cargo target directory for the Rust
  programs. These are built artefacts of a different commit when the prefix is
  old; `Stack.describe()` reports their paths so a run says what it used.

Every path can be set by an environment variable (`StackUnavailable` names the
missing piece and the variable that sets it), so CI and a developer machine use
the same code with different values.
"""

from __future__ import annotations

import os
import shutil
import subprocess
from dataclasses import dataclass, field
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]

# The environment names a Gnoblin prefix sets for its own tools. They are read
# from `gnoblin_env_apply`, never written here.
GNOBLIN_ENV_KEYS = (
    "GNOBLIN_PREFIX",
    "GNOBLIN_LIBDIR",
    "LD_LIBRARY_PATH",
    "XDG_DATA_DIRS",
    "XDG_CURRENT_DESKTOP",
)

DEFAULT_GNOBLIN_PREFIXES = ("/usr/local/lib/gnoblin", "/usr/lib/gnoblin", "/usr/lib64/gnoblin")
DEFAULT_BINGUX_PREFIXES = ("build/test-prefix", "~/.local/share/bingux", "/usr/local", "/usr")


class StackUnavailable(RuntimeError):
    """A piece of the stack is missing. The message says which and how to point at one."""


def _first_existing(candidates, test) -> Path | None:
    for candidate in candidates:
        path = Path(os.path.expanduser(str(candidate)))
        if not path.is_absolute():
            path = REPO / path
        if test(path):
            return path
    return None


def _which(name: str) -> Path | None:
    found = shutil.which(name)
    return Path(found) if found else None


@dataclass(frozen=True)
class Stack:
    repo: Path
    gnoblin_prefix: Path
    gnoblin_env: dict[str, str]
    quickshell: Path
    shell_dir: Path
    bingux_prefix: Path | None
    cargo_target: Path | None
    tools: dict[str, Path] = field(default_factory=dict)

    # -- discovery -----------------------------------------------------------

    @classmethod
    def discover(cls, environ: dict[str, str] | None = None) -> Stack:
        env = dict(os.environ if environ is None else environ)
        missing: list[str] = []

        gnoblin = env.get("GNOBLIN_PREFIX")
        gnoblin_prefix = (
            Path(gnoblin)
            if gnoblin
            else _first_existing(DEFAULT_GNOBLIN_PREFIXES, lambda p: (p / "bin" / "gnoblin").is_file())
        )
        if gnoblin_prefix is None or not (gnoblin_prefix / "bin" / "gnoblin").is_file():
            missing.append("Gnoblin (set GNOBLIN_PREFIX to a prefix with bin/gnoblin and libexec/gnoblin-env.sh)")
        elif not (gnoblin_prefix / "libexec" / "gnoblin-env.sh").is_file():
            missing.append(f"{gnoblin_prefix}/libexec/gnoblin-env.sh (a Gnoblin prefix without its environment script)")

        prefix_setting = env.get("BINGUX_TEST_PREFIX")
        bingux_prefix = (
            Path(prefix_setting)
            if prefix_setting
            else _first_existing(DEFAULT_BINGUX_PREFIXES, lambda p: (p / "lib" / "bingux" / "qml").is_dir())
        )

        quickshell = None
        for candidate in (
            env.get("BINGUX_QUICKSHELL"),
            bingux_prefix and bingux_prefix / "bin" / "bingux-shell",
            Path(os.path.expanduser("~/.local/share/gnoblin/quickshell-fedora/bin/quickshell")),
            _which("gnoblin-quickshell"),
            _which("quickshell"),
            _which("qs"),
        ):
            if candidate and Path(candidate).is_file() and os.access(candidate, os.X_OK):
                quickshell = Path(candidate)
                break
        if quickshell is None:
            missing.append("Quickshell (set BINGUX_QUICKSHELL)")

        if bingux_prefix is None or not (bingux_prefix / "lib" / "bingux" / "qml").is_dir():
            missing.append(
                "a Bingux prefix with lib/bingux/qml (the native QML plugins; set BINGUX_TEST_PREFIX, "
                "or run `make install-user` to create one)"
            )

        shell_dir = REPO / "shell" / "bingux"
        if not (shell_dir / "shell.qml").is_file():
            missing.append(f"{shell_dir}/shell.qml")

        tools = {}
        for name in ("bwrap", "nsenter", "dbus-daemon", "grim", "gdbus"):
            found = _which(name)
            if found is None:
                missing.append(f"`{name}` on PATH")
            else:
                tools[name] = found

        if missing:
            raise StackUnavailable("the isolated desktop needs: " + "; ".join(missing))
        assert gnoblin_prefix and quickshell
        target = env.get("BINGUX_TEST_CARGO_TARGET") or env.get("CARGO_TARGET_DIR")
        cargo_target = Path(target) if target else _first_existing(("build/cargo",), Path.is_dir)
        return cls(
            repo=REPO,
            gnoblin_prefix=gnoblin_prefix,
            gnoblin_env=_gnoblin_environment(gnoblin_prefix),
            quickshell=quickshell,
            shell_dir=shell_dir,
            bingux_prefix=bingux_prefix,
            cargo_target=cargo_target,
            tools=tools,
        )

    # -- lookups -------------------------------------------------------------

    def binaries(self, name: str) -> list[Path]:
        """Every build of a Bingux program, best first: BINGUX_<NAME>_BIN, the Cargo target (release, then debug), the prefix."""
        found: list[Path] = []
        override = os.environ.get("BINGUX_" + name.removeprefix("bingux-").replace("-", "_").upper() + "_BIN")
        candidates = [Path(override)] if override else []
        if self.cargo_target:
            candidates += [self.cargo_target / "release" / name, self.cargo_target / "debug" / name]
        if self.bingux_prefix:
            candidates.append(self.bingux_prefix / "bin" / name)
        for candidate in candidates:
            if candidate.is_file() and os.access(candidate, os.X_OK) and candidate not in found:
                found.append(candidate)
        return found

    def binary(self, name: str) -> Path | None:
        """The first build of a Bingux program, or None."""
        builds = self.binaries(name)
        return builds[0] if builds else None

    def snapshot_binary(self, name: str, workdir: Path) -> Path | None:
        """The first build of `name` made with `--features snapshot`, or None.

        Only that build honours the BINGUX_SETTINGS_FAKE_* variables that replace the machine's network,
        displays and devices with fixed data, so a desktop test that needs them must use it too.
        """
        for binary in self.binaries(name):
            workdir.mkdir(parents=True, exist_ok=True)
            env = {"PATH": "/usr/bin:/bin", "GSETTINGS_BACKEND": "memory", "XDG_CACHE_HOME": str(workdir / "cache")}
            env["BINGUX_SETTINGS_SNAPSHOT_SETTLE_MS"] = "300"
            done = subprocess.run(
                [str(binary), "--snapshot", str(workdir), "mouse"], env=env, capture_output=True, text=True, timeout=120
            )
            if done.returncode == 0 and list(workdir.glob("*.png")):
                return binary
            workdir = workdir.with_name(workdir.name + "x")
        return None

    def describe(self) -> dict[str, str]:
        """What this run used, for the artefact directory and for a bug report."""
        return {
            "repo": str(self.repo),
            "gnoblin": str(self.gnoblin_prefix),
            "quickshell": str(self.quickshell),
            "shell_dir": str(self.shell_dir),
            "bingux_prefix": str(self.bingux_prefix),
            "cargo_target": str(self.cargo_target),
            **{
                f"bin:{name}": str(self.binary(name)) for name in ("bingux-statusd", "bingux-settings", "bingux-config")
            },
        }


def _gnoblin_environment(prefix: Path) -> dict[str, str]:
    """The variables Gnoblin's `gnoblin_env_apply` exports, read from a clean shell."""
    script = prefix / "libexec" / "gnoblin-env.sh"
    if not script.is_file():
        return {}
    program = 'source "$1" && gnoblin_env_apply "$2" >/dev/null && env -0'
    done = subprocess.run(
        ["bash", "-c", program, "bash", str(script), str(prefix)],
        env={"PATH": "/usr/bin:/bin", "HOME": "/nonexistent"},
        capture_output=True,
        check=False,
    )
    if done.returncode != 0:
        raise StackUnavailable(f"{script} failed: {done.stderr.decode(errors='replace').strip()}")
    values = dict(item.split("=", 1) for item in done.stdout.decode().split("\0") if "=" in item)
    return {key: values[key] for key in GNOBLIN_ENV_KEYS if key in values}
