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
