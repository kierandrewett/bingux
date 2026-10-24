"""CommandError messages carry the first part of the command's stderr."""

from bingux_installer.runner import CommandError


def test_message_includes_collapsed_stderr() -> None:
    err = CommandError(["mount", "/dev/vda3", "/x"], 32, "mount: bad\n  type 'swap'\n")
    assert str(err) == "mount exited with status 32: mount: bad type 'swap'"


def test_message_without_stderr_has_no_detail() -> None:
    assert str(CommandError(["blkid"], 2)) == "blkid exited with status 2"


def test_message_truncates_long_stderr() -> None:
    err = CommandError(["x"], 1, "a" * 1000)
    assert len(str(err)) == len("x exited with status 1: ") + 300
