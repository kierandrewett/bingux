"""Pure text helpers used by configure.py (no filesystem, no chroot)."""

from __future__ import annotations

from bingux_installer import configure


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
