"""Pure text helpers used by configure.py (no filesystem, no chroot)."""

from __future__ import annotations

from bingux_installer import configure, layout


def test_enable_locale_gen_uncomments_existing_line() -> None:
    text = "#en_GB.UTF-8 UTF-8\n#en_US.UTF-8 UTF-8\n"
    assert configure.enable_locale_gen(text, "en_GB.UTF-8").splitlines() == [
        "en_GB.UTF-8 UTF-8", "#en_US.UTF-8 UTF-8"]


def test_enable_locale_gen_appends_when_absent() -> None:
    assert configure.enable_locale_gen("#en_US.UTF-8 UTF-8\n", "de_DE.UTF-8") == (
        "#en_US.UTF-8 UTF-8\nde_DE.UTF-8 UTF-8\n")


def test_rewrite_hooks_replaces_or_appends() -> None:
    text = "MODULES=()\nHOOKS=(base udev)\n"
    out = configure.rewrite_hooks(text, ("base", "systemd", "filesystems"))
    assert out == "MODULES=()\nHOOKS=(base systemd filesystems)\n"
    assert configure.rewrite_hooks("MODULES=()\n", ("base",)) == "MODULES=()\nHOOKS=(base)\n"


def test_user_groups_depend_on_admin() -> None:
    assert configure.user_groups(True) == "wheel,video,audio,input"
    assert configure.user_groups(False) == "video,audio,input"


def test_locale_hostname_and_vconsole_renderers() -> None:
    assert configure.render_hostname("bingux") == "bingux\n"
    assert configure.render_locale_conf("en_GB.UTF-8") == "LANG=en_GB.UTF-8\n"
    assert configure.render_vconsole("uk") == "KEYMAP=uk\n"


def test_restore_greeter_config_replaces_live_config(tmp_path) -> None:
    packaged = tmp_path / configure.GREETD_PACKAGED
    packaged.parent.mkdir(parents=True)
    packaged.write_text('[default_session]\ncommand = "bingux-greeter-session"\n', encoding="utf-8")
    live = tmp_path / configure.GREETD_CONFIG
    live.parent.mkdir(parents=True)
    live.write_text('[initial_session]\ncommand = "bingux-live-session installer"\n', encoding="utf-8")

    configure.restore_greeter_config(tmp_path)

    assert live.read_text(encoding="utf-8") == packaged.read_text(encoding="utf-8")
    assert "initial_session" not in live.read_text(encoding="utf-8")


def test_restore_greeter_config_fails_without_packaged_file(tmp_path) -> None:
    import pytest
    from bingux_installer.protocol import InstallerError

    with pytest.raises(InstallerError):
        configure.restore_greeter_config(tmp_path)


def _autologin_target(tmp_path):  # noqa: ANN001,ANN202
    session = tmp_path / configure.GREETD_GNOBLIN_SESSION
    session.parent.mkdir(parents=True)
    session.write_text("[Desktop Entry]\nName=Gnoblin\nExec=/usr/lib/gnoblin/bin/gnoblin\n"
                       "DesktopNames=Gnoblin;\n", encoding="utf-8")
    live = tmp_path / configure.GREETD_CONFIG
    live.parent.mkdir(parents=True)
    live.write_text('[default_session]\ncommand = "bingux-greeter-session"\nuser = "greeter"\n',
                    encoding="utf-8")
    return live


def test_write_autologin_adds_initial_session_for_the_user(tmp_path) -> None:  # noqa: ANN001
    import tomllib

    live = _autologin_target(tmp_path)
    configure.write_autologin(tmp_path, _plan(autologin="kieran"))

    config = tomllib.loads(live.read_text(encoding="utf-8"))
    assert config["default_session"] == {"command": "bingux-greeter-session", "user": "greeter"}
    assert config["initial_session"]["user"] == "kieran"
    assert config["initial_session"]["command"] == (
        "env XDG_SESSION_TYPE=wayland XDG_SESSION_DESKTOP=gnoblin XDG_CURRENT_DESKTOP=Gnoblin "
        "/usr/lib/gnoblin/bin/gnoblin")


def test_write_autologin_none_leaves_the_greeter_config_alone(tmp_path) -> None:  # noqa: ANN001
    live = _autologin_target(tmp_path)
    before = live.read_text(encoding="utf-8")
    configure.write_autologin(tmp_path, _plan(autologin=None))
    assert live.read_text(encoding="utf-8") == before


def test_write_autologin_fails_without_the_gnoblin_session(tmp_path) -> None:  # noqa: ANN001
    import pytest
    from bingux_installer.protocol import InstallerError

    _autologin_target(tmp_path)
    (tmp_path / configure.GREETD_GNOBLIN_SESSION).unlink()
    with pytest.raises(InstallerError):
        configure.write_autologin(tmp_path, _plan(autologin="kieran"))


def test_rewrite_modules_replaces_or_appends() -> None:
    assert configure.rewrite_modules("MODULES=()\nHOOKS=(base)\n", ("xfs",)) == (
        "MODULES=(xfs)\nHOOKS=(base)\n")
    assert configure.rewrite_modules("HOOKS=(base)\n", ("xfs",)) == "HOOKS=(base)\nMODULES=(xfs)\n"


class RecordingRunner:
    """Records argv and stdin; returns a fixed hash for openssl and empty output otherwise."""

    def __init__(self, on_run=None) -> None:  # noqa: ANN001
        self.calls: list[tuple[list[str], str | None]] = []
        self.on_run = on_run

    def run(self, argv, *, check=True, stdin=None):  # noqa: ANN001, ANN201
        from bingux_installer.runner import CmdResult

        self.calls.append((list(argv), stdin))
        if self.on_run is not None:
            self.on_run(argv)
        digest = "$6$fake$hash" if argv[:2] == ["openssl", "passwd"] else ""
        return CmdResult(0, digest, "")

    def stream(self, argv, on_line, **kwargs):  # noqa: ANN001, ANN003
        raise AssertionError("stream not expected")


def _plan(**changes):  # noqa: ANN202
    from dataclasses import replace

    from bingux_installer.plan import validate_plan

    raw = {"version": 2, "disk": "/dev/vda", "confirm_erase": "/dev/vda",
           "timezone": "Europe/London", "locale": "en_GB", "keymap": "uk", "hostname": "bingux",
           "users": [{"name": "kieran", "password": "correct horse", "admin": True}]}
    plan = validate_plan(raw, booted_firmware="uefi", timezone_exists=lambda _: True,
                         locale_supported=lambda _: True)
    return replace(plan, **changes)


def test_set_ntp_enables_or_disables_timesyncd(tmp_path) -> None:  # noqa: ANN001
    runner = RecordingRunner()
    configure.set_ntp(runner, tmp_path, True)
    configure.set_ntp(runner, tmp_path, False)
    assert [argv for argv, _ in runner.calls] == [
        ["chroot", str(tmp_path), "systemctl", "enable", "systemd-timesyncd.service"],
        ["chroot", str(tmp_path), "systemctl", "disable", "systemd-timesyncd.service"],
    ]


def test_write_swap_zram_writes_generator_conf(tmp_path) -> None:  # noqa: ANN001
    runner = RecordingRunner()
    configure.write_swap(runner, tmp_path, _plan(swap="zram"))
    assert (tmp_path / configure.ZRAM_CONF).read_text() == layout.zram_conf()
    assert runner.calls == []


def test_write_swap_none_removes_stale_zram_conf(tmp_path) -> None:  # noqa: ANN001
    stale = tmp_path / configure.ZRAM_CONF
    stale.parent.mkdir(parents=True)
    stale.write_text("[zram0]\n")
    configure.write_swap(RecordingRunner(), tmp_path, _plan(swap="none"))
    assert not stale.exists()


def test_write_swap_file_ext4_uses_fallocate_and_mkswap(tmp_path) -> None:  # noqa: ANN001
    def make(argv):  # noqa: ANN001, ANN202
        if argv[0] == "fallocate":
            (tmp_path / "swapfile").write_bytes(b"")

    runner = RecordingRunner(on_run=make)
    configure.write_swap(runner, tmp_path, _plan(swap="file", swap_bytes=2 * 1024**3))
    argvs = [argv for argv, _ in runner.calls]
    assert argvs == [
        ["fallocate", "-l", str(2 * 1024**3), str(tmp_path / "swapfile")],
        ["mkswap", str(tmp_path / "swapfile")],
    ]
    assert (tmp_path / "swapfile").stat().st_mode & 0o777 == 0o600


def test_write_swap_file_btrfs_uses_mkswapfile(tmp_path) -> None:  # noqa: ANN001
    runner = RecordingRunner()
    configure.write_swap(runner, tmp_path, _plan(filesystem="btrfs", swap="file",
                                                 swap_bytes=2 * 1024**3))
    assert [argv for argv, _ in runner.calls] == [
        ["btrfs", "filesystem", "mkswapfile", "--size", str(2 * 1024**3), str(tmp_path / "swapfile")],
    ]


def test_write_crypttab_writes_etc_file(tmp_path) -> None:  # noqa: ANN001
    configure.write_crypttab(tmp_path, "swap-line\n")
    assert (tmp_path / "etc/crypttab").read_text() == "swap-line\n"


def test_create_users_sets_root_password_when_given(tmp_path) -> None:  # noqa: ANN001
    runner = RecordingRunner()
    configure.create_users(runner, tmp_path, _plan(root_password="root pass word"))
    chpasswd = [stdin for argv, stdin in runner.calls if argv[:2] == ["chroot", str(tmp_path)]
                and "chpasswd" in argv]
    assert chpasswd == ["kieran:$6$fake$hash\nroot:$6$fake$hash\n"]
    assert not any("passwd" in argv and "-l" in argv for argv, _ in runner.calls)


def test_seed_greeter_snapshots_exports_each_account_as_itself(tmp_path) -> None:  # noqa: ANN001
    runner = RecordingRunner()
    configure.seed_greeter_snapshots(runner, tmp_path, _plan())
    snapshots = tmp_path / "var/lib/bingux/greeter"
    assert snapshots.is_dir()
    assert snapshots.stat().st_mode & 0o7777 == 0o1777
    assert [argv for argv, _ in runner.calls] == [
        ["chroot", str(tmp_path), "runuser", "-u", "kieran", "--", "env",
         "HOME=/home/kieran", "bingux-session", "export"],
    ]


def test_create_users_locks_root_without_root_password(tmp_path) -> None:  # noqa: ANN001
    runner = RecordingRunner()
    configure.create_users(runner, tmp_path, _plan())
    assert [argv[-2:] for argv, _ in runner.calls if "passwd" in argv and "-l" in argv] == [
        ["-l", "root"]]
