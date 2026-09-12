#!/usr/bin/env python3
"""Launch a desktop entry through GIO, including its optional new-window action."""

import argparse
import json
import tempfile
from pathlib import Path
import sys
import os
import signal
import time
import subprocess
import gi

gi.require_version("Gio", "2.0")
gi.require_version("GioUnix", "2.0")
from gi.repository import Gio, GioUnix, GLib  # noqa: E402 - Select GI versions before importing their modules.


FEEDBACK_PATH = None
LAST_ERROR = None
LAUNCH_ACCEPT_TIMEOUT = 5.0
HOST_MANAGER_TIMEOUT = 5.0


class LaunchTimeout(RuntimeError):
    pass


def call_with_timeout(callback):
    """Bound a synchronous desktop launch so one broken app cannot wedge launchers."""

    def alarm(_signum, _frame):
        raise LaunchTimeout("The desktop entry did not respond while starting. It may be blocked by a full filesystem.")

    previous_handler = signal.getsignal(signal.SIGALRM)
    signal.signal(signal.SIGALRM, alarm)
    previous_timer = signal.setitimer(signal.ITIMER_REAL, LAUNCH_ACCEPT_TIMEOUT)
    try:
        return callback()
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous_handler)
        if previous_timer[0] > 0:
            signal.setitimer(signal.ITIMER_REAL, previous_timer[0], previous_timer[1])


def report_failure(name, message, notify):
    global LAST_ERROR
    LAST_ERROR = {"name": name, "message": message}
    print("BINGUX_LAUNCH_ERROR " + json.dumps(LAST_ERROR), flush=True)
    print(f"Could not launch {name}: {message}", file=sys.stderr, flush=True)
    if notify:
        try:
            bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
            bus.call_sync(
                "org.freedesktop.Notifications",
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications",
                "Notify",
                GLib.Variant(
                    "(susssasa{sv}i)",
                    ("Bingux", 0, "dialog-error", f"Could not open {name}", message[:1000], [], {}, -1),
                ),
                None,
                Gio.DBusCallFlags.NONE,
                3000,
                None,
            )
        except GLib.Error as error:
            print(f"Launch error notification failed: {error.message}", file=sys.stderr)
    return 1


def launch_and_watch(entry, context):
    children = []
    accepted = call_with_timeout(
        lambda: entry.launch_uris_as_manager_with_fds(
            [],
            context,
            GLib.SpawnFlags.SEARCH_PATH | GLib.SpawnFlags.DO_NOT_REAP_CHILD,
            None,
            None,
            lambda _app, pid, _data: children.append(pid),
            None,
            -1,
            -1,
            -1,
        )
    )
    if not accepted:
        raise RuntimeError("Application launch was rejected")
    # Some launchers hand off to an existing instance and exit successfully.
    # Only a non-zero exit or signal is a confirmed failure.
    deadline = time.monotonic() + 4
    while children and time.monotonic() < deadline:
        for pid in children[:]:
            done, status = os.waitpid(pid, os.WNOHANG)
            if not done:
                continue
            children.remove(pid)
            code = os.waitstatus_to_exitcode(status)
            if code:
                reason = (
                    f"The application exited with code {code}."
                    if code > 0
                    else f"The application stopped with signal {-code}."
                )
                raise RuntimeError(reason + " Diagnostic output is in the user journal (bingux-app-launch).")
        if children:
            time.sleep(0.05)


def launch_local(args):
    identity = args.desktop_id
    if "/" in identity or not identity:
        raise ValueError("Expected a desktop entry ID")
    # Quickshell removes the file extension, including when the application
    # ID itself ends in .desktop (for example org.telegram.desktop).
    candidates = [identity + ".desktop"]
    if identity.endswith(".desktop"):
        candidates.insert(0, identity)
    entry = None
    for candidate in candidates:
        try:
            entry = GioUnix.DesktopAppInfo.new(candidate)
        except TypeError:
            continue
        if entry is not None:
            break
    if entry is None:
        directories = [GLib.get_user_data_dir(), *GLib.get_system_data_dirs()]
        exists = any(
            (Path(directory) / "applications" / candidate).is_file()
            for directory in directories
            for candidate in candidates
        )
        message = (
            "The desktop entry exists, but it is invalid or its executable is unavailable."
            if exists
            else "The installed desktop entry could not be found."
        )
        return report_failure(identity, message, args.notify_errors)
    if args.report_timeout:
        return report_failure(
            entry.get_display_name(),
            "No application window appeared before the launch timeout. The application may still be starting or running in the background.",
            True,
        )
    try:
        context = Gio.AppLaunchContext()
        if args.new_window and "new-window" in entry.list_actions():
            call_with_timeout(lambda: entry.launch_action("new-window", context))
        else:
            launch_and_watch(entry, context)
    except (GLib.Error, RuntimeError) as error:
        return report_failure(entry.get_display_name(), str(error), args.notify_errors)
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--feedback-file", help=argparse.SUPPRESS)
    parser.add_argument("--dock-feedback", action="store_true")
    parser.add_argument("--host-launch", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--report-timeout", action="store_true")
    parser.add_argument("--notify-errors", action="store_true")
    parser.add_argument("--new-window", action="store_true")
    parser.add_argument("desktop_id")
    args = parser.parse_args(argv)
    global FEEDBACK_PATH
    FEEDBACK_PATH = args.feedback_file
    # A development runtime may use a private user namespace. Flatpak must
    # start from the host user manager, not inherit that nested namespace.
    if not args.host_launch and (args.notify_errors or args.dock_feedback):
        command = [
            "systemd-run",
            "--user",
            "--collect",
            "--quiet",
            "--property=ExitType=cgroup",
            "--property=SyslogIdentifier=bingux-app-launch",
            "--",
            sys.executable,
            os.path.realpath(__file__),
            "--host-launch",
            *(argv if argv is not None else sys.argv[1:]),
        ]
        if args.dock_feedback:
            # A private result file survives the host namespace boundary without
            # tying application stdout or lifetime to the shell's process pipes.
            with tempfile.TemporaryDirectory(
                prefix="bingux-launch-", dir=os.environ.get("XDG_RUNTIME_DIR")
            ) as directory:
                feedback = Path(directory) / "result.json"
                command[command.index("--host-launch") + 1 : command.index("--host-launch") + 1] = [
                    "--feedback-file",
                    str(feedback),
                ]
                try:
                    result = subprocess.run(command, capture_output=True, text=True, timeout=HOST_MANAGER_TIMEOUT)
                except subprocess.TimeoutExpired:
                    return report_failure(
                        args.desktop_id,
                        "The host application launcher did not respond. It may be blocked by a full filesystem.",
                        False,
                    )
                if result.returncode:
                    # If the user manager is still refusing transient units after
                    # space was reclaimed, launch in the current session. This
                    # keeps ordinary native apps usable without requiring logout.
                    return launch_local(args)
                deadline = time.monotonic() + 8
                while time.monotonic() < deadline:
                    if feedback.exists():
                        outcome = json.loads(feedback.read_text())
                        if outcome["error"]:
                            print("BINGUX_LAUNCH_ERROR " + json.dumps(outcome["error"]), flush=True)
                        return outcome["code"]
                    time.sleep(0.05)
                return report_failure(
                    args.desktop_id, "The application launcher did not respond. The app may still be starting.", False
                )
        try:
            result = subprocess.run(command, capture_output=True, text=True, timeout=HOST_MANAGER_TIMEOUT)
        except subprocess.TimeoutExpired:
            return report_failure(
                args.desktop_id,
                "The host application launcher did not respond. It may be blocked by a full filesystem.",
                args.notify_errors,
            )
        if result.returncode:
            return launch_local(args)
        return 0
    try:
        return launch_local(args)
    except ValueError as error:
        parser.error(str(error))


if __name__ == "__main__":
    code = main()
    if FEEDBACK_PATH:
        path = Path(FEEDBACK_PATH)
        try:
            temporary = path.with_suffix(".tmp")
            temporary.write_text(json.dumps({"code": code, "error": LAST_ERROR}))
            temporary.replace(path)
        except OSError as error:
            print(f"Could not return launch result: {error}", file=sys.stderr)
    raise SystemExit(code)
