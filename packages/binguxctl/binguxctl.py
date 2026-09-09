#!/usr/bin/env python3
"""Control the running Bingux shell through its public Quickshell IPC."""
import argparse
import json
import math
from pathlib import Path
import os
import re
import shutil
import subprocess
import sys
import time


def parser():
    result = argparse.ArgumentParser(prog="binguxctl", description=__doc__)
    result.add_argument("--quickshell", default=os.environ.get("BINGUX_QUICKSHELL", "qs"), help="Quickshell executable")
    result.add_argument("--any-display", action="store_true", default=os.environ.get("BINGUX_ANY_DISPLAY") == "1", help="Match the selected shell across display connections")
    selection = result.add_mutually_exclusive_group()
    selection.add_argument("--path", help="Running shell configuration path")
    selection.add_argument("--config", help="Running shell configuration name (default: bingux)")
    commands = result.add_subparsers(dest="command", required=True)
    commands.add_parser("status", help="Print shell state as JSON")
    commands.add_parser("reload", help="Reload the shell configuration")
    commands.add_parser("list", help="List all public IPC targets and methods")
    capture = commands.add_parser("capture", help="Show, trigger or stop capture")
    capture.add_argument("action", nargs="?", default="open", choices=["open", "toggle", "take", "stop", "cancel", "status", "configure", "options"])
    capture.add_argument("--mode", choices=["screenshot", "recording"], default="")
    capture.add_argument("--target", choices=["region", "screen", "window"], default="")
    capture.add_argument("--fps", type=int, choices=[15, 30, 60])
    capture.add_argument("--max-height", type=int, choices=[0, 720, 1080, 1440, 2160])
    capture.add_argument("--delay", type=int, choices=[0, 1, 2, 3, 5, 10])
    capture.add_argument("--quality", choices=["compact", "balanced", "high"])
    capture.add_argument("--audio", choices=["none", "system", "microphone", "both"])
    capture.add_argument("--format", choices=["png", "jpeg"])
    capture.add_argument("--encoder", choices=["auto", "cpu"])
    capture.add_argument("--backend", choices=["auto", "portal"])
    capture.add_argument("--output", help="Output directory")
    capture.add_argument("--cursor", action=argparse.BooleanOptionalAction, default=None)
    capture.add_argument("--copy", action=argparse.BooleanOptionalAction, default=None)
    capture.add_argument("--region", nargs=4, type=int, metavar=("X", "Y", "WIDTH", "HEIGHT"))
    audio = commands.add_parser("audio", help="Control default output volume or microphone gain")
    audio.add_argument("action", nargs="?", default="status", choices=["status", "volume", "up", "down", "mute", "unmute", "toggle", "devices", "select"])
    audio.add_argument("value", nargs="?", help="Percentage or device ID for select")
    audio.add_argument("--input", action="store_true", help="Control the default microphone")
    for name, actions in (("network", ["status", "list", "refresh", "connect", "disconnect"]),
                          ("vpn", ["status", "list", "connect", "disconnect"]),
                          ("bluetooth", ["status", "list", "on", "off", "toggle", "scan", "connect", "disconnect"]),
                          ("power", ["status", "set"]),
                          ("night-light", ["status", "on", "off", "toggle"]),
                          ("awake", ["status", "on", "off", "toggle"])):
        control = commands.add_parser(name, help=f"Control {name}")
        control.add_argument("action", nargs="?", default="status", choices=actions)
        control.add_argument("value", nargs="?")
    apps = commands.add_parser("apps", help="List or launch installed applications")
    apps.add_argument("action", nargs="?", default="list", choices=["list", "launch"])
    apps.add_argument("value", nargs="?", help="Search text for list, exact desktop ID for launch")
    apps.add_argument("--new-window", action="store_true", help="Request the app's new-window action")
    media = commands.add_parser("media", help="Control an MPRIS player")
    media.add_argument("action", nargs="?", default="status", choices=["list", "status", "play", "pause", "toggle", "next", "previous", "seek"])
    media.add_argument("value", nargs="?", type=float, help="Absolute position in seconds for seek")
    media.add_argument("--player", default="", help="Exact player ID from media list")
    for name, actions in (("dock", ["list", "pin", "unpin", "move", "launch", "activate"]),
                          ("windows", ["list", "activate", "minimize", "restore", "close"])):
        control = commands.add_parser(name, help=f"Control {name} by ID")
        control.add_argument("action", nargs="?", default="list", choices=actions)
        control.add_argument("id", nargs="?")
        if name == "dock": control.add_argument("position", nargs="?", type=int)
    search = commands.add_parser("search", help="Control search or set its query")
    search.add_argument("action", nargs="?", default="open", choices=["open", "close", "toggle", "status", "query"])
    search.add_argument("text", nargs="*")
    for name in ("settings", "calendar", "controls", "metrics", "keyboard", "notifications"):
        command = commands.add_parser(name, help=f"Control {name}")
        actions = ["open", "close", "toggle", "status"]
        if name == "controls": actions += ["page", "list", "show", "hide"]
        if name == "keyboard": actions += ["next", "previous", "list", "select"]
        if name == "notifications": actions += ["list", "dismiss", "clear", "invoke"]
        command.add_argument("action", nargs="?", default="toggle", choices=actions)
        if name == "controls":
            command.add_argument("value", nargs="?", help="Page name or control ID")
            command.add_argument("--input", action="store_true", help="Open the audio input page")
        if name == "keyboard":
            command.add_argument("type", nargs="?", help="Source type from keyboard list")
            command.add_argument("id", nargs="?", help="Source ID from keyboard list")
        if name == "notifications":
            command.add_argument("id", nargs="?")
            command.add_argument("action_id", nargs="?")
    emoji = commands.add_parser("emoji", help="Control the emoji picker")
    emoji.add_argument("action", nargs="?", default="open", choices=["open", "close", "status"])
    dnd = commands.add_parser("dnd", help="Set Do Not Disturb")
    dnd.add_argument("action", nargs="?", default="status", choices=["on", "off", "toggle", "status"])
    sidebar = commands.add_parser("sidebar", help="Control the sidebar")
    sidebar.add_argument("action", nargs="?", default="toggle", choices=["open", "close", "toggle", "popout", "dock", "select", "edge", "status"])
    sidebar.add_argument("value", nargs="?")
    for name, actions in (("privacy", ["status"]), ("switcher", ["status", "close"])):
        command = commands.add_parser(name, help=f"Inspect {name}")
        command.add_argument("action", nargs="?", default="status", choices=actions)
    ipc = commands.add_parser("ipc", help="Call any public IPC method directly")
    ipc.add_argument("target")
    ipc.add_argument("method")
    ipc.add_argument("arguments", nargs=argparse.REMAINDER)
    return result


def invocation(args, cli):
    command, action = args.command, getattr(args, "action", None)
    if command == "list": return ["show"]
    if command in ("status", "reload"): call = ["shell", command]
    elif command == "capture":
        options = {key: getattr(args, key) for key in ("fps", "delay", "quality", "audio", "format", "encoder", "backend", "cursor", "copy") if getattr(args, key) is not None}
        if args.mode: options["kind"] = args.mode
        if args.target: options["target"] = args.target
        if args.max_height is not None: options["maxHeight"] = args.max_height
        if args.output is not None: options["directory"] = str(Path(args.output).expanduser().absolute())
        if args.region is not None:
            if min(args.region[:2]) < 0 or min(args.region[2:]) < 2: cli.error("Invalid capture region")
            options["region"] = dict(zip(("x", "y", "width", "height"), args.region))
        if action not in ("open", "configure") and options: cli.error("Capture options require open or configure")
        if action == "configure":
            if not options: cli.error("capture configure requires options; use capture options to inspect them")
            call = ["capture", "configure", json.dumps(options)]
        elif action == "open" and any(key not in ("kind", "target") for key in options):
            call = ["capture", "openOptions", json.dumps(options)]
        else: call = ["capture", "show", args.mode, args.target] if action == "open" else ["capture", {"toggle": "open"}.get(action, action)]
    elif command == "apps":
        if action == "launch" and not args.value: cli.error("apps launch requires a desktop ID from apps list")
        if args.new_window and action != "launch": cli.error("--new-window requires apps launch")
        call = ["actions", "app", "new-window" if args.new_window else action,
                (args.value or "") if action == "launch" else "", (args.value or "") if action == "list" else ""]
    elif command == "controls":
        needs_value = action in ("page", "show", "hide")
        if needs_value != (args.value is not None): cli.error("This controls action " + ("requires a value" if needs_value else "takes no value"))
        if args.input and (action != "page" or args.value != "audio"): cli.error("--input requires controls page audio")
        if action == "page":
            if args.value not in ("network", "bluetooth", "audio", "display", "vpn", "power", "customise"): cli.error("Unknown control-centre page")
            call = ["actions", "page", args.value, str(args.input).lower()]
        elif action in ("list", "show", "hide"): call = ["actions", "control", action, args.value or ""]
        else: call = ["shell", "panel", "controls", action]
    elif command == "keyboard":
        if action == "select":
            if not args.type or not args.id: cli.error("keyboard select requires a source type and ID from keyboard list")
        elif args.type is not None or args.id is not None: cli.error("Only keyboard select takes a source type and ID")
        if action in ("list", "select"): call = ["actions", "keyboard", action, args.type or "", args.id or ""]
        elif action in ("next", "previous"): call = ["shell", "keyboardNext" if action == "next" else "keyboardPrevious"]
        else: call = ["shell", "panel", "keyboard", action]
    elif command == "audio" and action in ("devices", "select"):
        if (action == "select") != (args.value is not None): cli.error("A device ID is required only for audio select")
        call = ["actions", "device", "list" if action == "devices" else "select", str(args.input).lower(), args.value or ""]
    elif command in ("network", "bluetooth", "vpn", "power", "night-light", "awake"):
        needs_value = action in ("connect", "disconnect", "scan", "set")
        if needs_value != (args.value is not None): cli.error("This action " + ("requires a value" if needs_value else "takes no value"))
        if command == "bluetooth":
            if action == "scan" and args.value not in ("on", "off"): cli.error("bluetooth scan requires on or off")
            if action in ("connect", "disconnect") and not re.fullmatch(r"(?:[0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}", args.value): cli.error("Use a Bluetooth device address from bluetooth list")
        if command == "network" and needs_value and not re.fullmatch(r"[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}", args.value): cli.error("Use a saved connection UUID from network list")
        if command in ("power", "night-light", "awake"):
            call = ["actions", "service", command, action, args.value or ""]
        else: call = ["actions", command, "status" if command == "network" and action == "list" else action, args.value or ""]
    elif command == "audio":
        numeric = action in ("volume", "up", "down")
        if action == "volume" and args.value is None: cli.error("audio volume requires a percentage")
        if not numeric and args.value is not None: cli.error("This audio action takes no value")
        try: value = float(args.value) if args.value is not None else 5 if numeric else 0
        except ValueError: cli.error("Volume must be a number")
        if not math.isfinite(value) or not 0 <= value <= 100: cli.error("Volume must be between 0 and 100")
        call = ["actions", "audio", action, str(args.input).lower(), str(value)]
    elif command == "media":
        if action == "seek":
            if args.value is None or not math.isfinite(args.value) or args.value < 0: cli.error("media seek requires non-negative seconds")
        elif args.value is not None: cli.error("Only media seek accepts a position")
        call = ["actions", "media", action, args.player, str(args.value or 0)]
    elif command in ("dock", "windows"):
        if (action == "list") != (args.id is None): cli.error("An item ID is required for actions and omitted for list")
        if command == "dock":
            if action == "move" and (args.position is None or args.position < 0): cli.error("dock move requires a zero-based position")
            if action != "move" and args.position is not None: cli.error("Only dock move takes a position")
        call = ["actions", "dock" if command == "dock" else "window", action, args.id or ""]
        if command == "dock": call.append(str(args.position or 0))
    elif command == "notifications" and action in ("list", "dismiss", "clear", "invoke"):
        if (action in ("dismiss", "invoke")) != (args.id is not None): cli.error("Notification ID required only for dismiss or invoke")
        if (action == "invoke") != (args.action_id is not None): cli.error("An action ID is required only for invoke")
        call = ["actions", "notification", action, args.id or "", args.action_id or ""]
    elif command == "notifications" and (args.id is not None or args.action_id is not None): cli.error("This notification action takes no ID")
    elif command == "search" and action == "query":
        if not args.text: cli.error("search query requires text")
        call = ["search", "query", " ".join(args.text)]
    elif command == "search" and args.text: cli.error("query text requires search query")
    elif command == "search": call = ["search", action]
    elif command == "dnd": call = ["shell", "dnd", action]
    elif command in ("settings", "search", "calendar", "controls", "metrics", "keyboard", "notifications"):
        call = ["shell", "panel", command, action]
    elif command == "sidebar":
        allowed = {"edge": ["left", "top", "right"], "select": ["terminal", "notes", "monitor", "calendar", "media", "tasks"]}
        if action in allowed:
            if args.value not in allowed[action]: cli.error(f"sidebar {action} requires one of: {', '.join(allowed[action])}")
        elif args.value is not None: cli.error(f"sidebar {action} takes no argument")
        call = ["sidebar", "hide" if action == "close" else action] + ([args.value] if args.value else [])
    elif command == "ipc": call = [args.target, args.method, *args.arguments]
    else: call = [command, action]
    return ["call", "--", *call]


def execute(command, run=subprocess.run, sleep=time.sleep, capture=False):
    for attempt in range(12):
        result = run(command, capture_output=True, text=True, timeout=10)
        output = (result.stdout + result.stderr).strip()
        if output == "Not ready to accept queries yet." and attempt < 11:
            sleep(.1)
            continue
        # Never repeat an uncertain result: a toggle or capture may have executed.
        if any(line.lstrip().startswith("ERROR quickshell.ipc:") for line in output.splitlines()): raise RuntimeError(output)
        if result.returncode or output.startswith("No running instances") or output in ("Not ready to accept queries yet.", "Function not found.", "Target not found."):
            raise RuntimeError(output or f"Quickshell exited with status {result.returncode}")
        try:
            response = json.loads(result.stdout)
        except json.JSONDecodeError:
            response = None
        if isinstance(response, dict) and response.get("ok") is False:
            raise RuntimeError(response.get("error", "Shell rejected the command"))
        if capture:
            if not isinstance(response, dict): raise RuntimeError("Expected a JSON response from the shell")
            return response
        if result.stdout: print(result.stdout, end="" if result.stdout.endswith("\n") else "\n")
        if result.stderr: print(result.stderr, file=sys.stderr, end="")
        return 0
    raise RuntimeError("Shell is still reloading")


def refresh_network(prefix, query=execute, sleep=time.sleep, monotonic=time.monotonic):
    query([*prefix, "call", "--", "actions", "network", "refresh", ""], capture=True)
    deadline = monotonic() + 15
    while True:
        state = query([*prefix, "call", "--", "actions", "network", "status", ""], capture=True)
        if not state.get("busy"):
            if state.get("error"): raise RuntimeError(state["error"])
            if state.get("ready"):
                return state
        if monotonic() >= deadline: raise RuntimeError("Network refresh timed out; use network status to inspect progress")
        sleep(.1)


def main(argv=None):
    cli = parser()
    args = cli.parse_args(argv)
    if args.command == "settings":
        launcher = os.environ.get("BINGUX_SETTINGS_APP") or shutil.which("bingux-settings")
        if launcher:
            return subprocess.call([launcher, args.action])
    call = invocation(args, cli)
    path = args.path or (None if args.config else os.environ.get("BINGUX_CONFIG_PATH"))
    target = args.target if args.command == "ipc" else args.command
    if target in ("search", "switcher"):
        # Invoke the companion directly, even when the desktop is frozen.
        if path:
            base = Path(path).expanduser()
            if base.suffix == ".qml": base = base.parent
        else:
            base = Path(os.environ.get("XDG_CONFIG_HOME", str(Path.home() / ".config"))) / "quickshell" / (args.config or os.environ.get("BINGUX_CONFIG_NAME", "bingux"))
        path = str(base / ("SearchShell.qml" if target == "search" else "SwitcherShell.qml"))
    selection = ["--path", path] if path else ["--config", args.config or os.environ.get("BINGUX_CONFIG_NAME", "bingux")]
    try:
        prefix = [args.quickshell, "ipc", *(["--any-display"] if args.any_display else []), *selection]
        if args.command == "network" and args.action in ("list", "connect", "disconnect"):
            state = refresh_network(prefix)
            if args.action == "list":
                print(json.dumps(state))
                return 0
        return execute([*prefix, *call])
    except (OSError, subprocess.TimeoutExpired, RuntimeError) as error:
        print(f"binguxctl: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
