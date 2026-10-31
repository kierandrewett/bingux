"""The bubblewrap command line keeps a session apart from the host. Nothing is started here."""

from pathlib import Path

import pytest

from bxdesktop.sandbox import SandboxSpec, bwrap_argv, nsenter_argv

pytestmark = pytest.mark.fast


def spec(**changes) -> SandboxSpec:
    values = dict(
        root=Path("/dev/shm/bingux-desktop-x"),
        uid=1000,
        home="/home/someone",
        env={"HOME": "/home/someone", "PATH": "/usr/bin"},
        read_only=(Path("/srv/checkout"),),
        command=("bash", "/tmp/init.sh"),
    )
    values.update(changes)
    return SandboxSpec(**values)


def pairs(argv: list[str], flag: str) -> list[tuple[str, ...]]:
    """The arguments that follow each occurrence of `flag`."""
    width = {"--bind": 2, "--ro-bind": 2, "--dev-bind": 2, "--tmpfs": 1, "--setenv": 2, "--symlink": 2}[flag]
    return [tuple(argv[i + 1 : i + 1 + width]) for i, value in enumerate(argv) if value == flag]


def test_every_namespace_is_unshared_and_the_environment_is_cleared():
    argv = bwrap_argv(spec())
    assert "--unshare-all" in argv  # user, mount, PID, IPC, UTS, cgroup and network
    assert "--share-net" not in argv
    assert "--clearenv" in argv
    assert "--die-with-parent" in argv and "--new-session" in argv


def test_the_host_is_visible_only_through_usr_etc_and_the_named_read_only_paths():
    argv = bwrap_argv(spec())
    read_only = {source for source, _ in pairs(argv, "--ro-bind")}
    assert read_only == {"/usr", "/etc", "/srv/checkout"}
    # The only writable host paths are directories of the run's temporary root.
    writable = {source for source, _ in pairs(argv, "--bind")}
    assert writable == {f"/dev/shm/bingux-desktop-x/{name}" for name in ("run", "tmp", "logs", "home")}


def test_no_host_autostart_entry_is_visible():
    argv = bwrap_argv(spec())
    assert ("/etc/xdg",) in pairs(argv, "--tmpfs")
    assert argv.index("--tmpfs") > argv.index("/etc")  # the empty tmpfs is mounted over the read-only /etc


def test_the_runtime_directory_and_home_are_the_runs_own():
    argv = bwrap_argv(spec(uid=4242))
    mounts = dict(pairs(argv, "--bind"))
    assert mounts["/dev/shm/bingux-desktop-x/run"] == "/run/user/4242"
    assert mounts["/dev/shm/bingux-desktop-x/home"] == "/home/someone"


def test_gpu_devices_are_off_unless_asked_for():
    assert "/dev/dri" not in bwrap_argv(spec())
    assert ("/dev/dri", "/dev/dri") in pairs(bwrap_argv(spec(dri=True)), "--dev-bind")


def test_the_environment_is_exactly_the_one_given():
    argv = bwrap_argv(spec())
    assert dict(pairs(argv, "--setenv")) == {"HOME": "/home/someone", "PATH": "/usr/bin"}


def test_the_command_comes_last_after_a_separator():
    argv = bwrap_argv(spec())
    assert argv[-3:] == ["--", "bash", "/tmp/init.sh"]


def test_nsenter_keeps_credentials_and_enters_every_namespace_the_sandbox_made():
    argv = nsenter_argv(77, ["id"])
    for flag in ("--user", "--preserve-credentials", "--mount", "--pid", "--net", "--ipc", "--uts", "-r"):
        assert flag in argv
    assert argv[argv.index("--") + 1 :] == ["id"]
    assert not any(a.startswith("--wd") for a in argv)  # --wd leaves a working directory getcwd cannot resolve


def test_a_working_directory_is_set_inside_the_sandbox_not_by_nsenter():
    argv = nsenter_argv(77, ["pwd"], cwd="/tmp")
    assert argv[argv.index("--") + 1 :][-2:] == ["/tmp", "pwd"]
